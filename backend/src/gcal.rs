//! Two-way Google Calendar sync.
//!
//! Outbound: dated, incomplete tasks are mirrored as events in a dedicated
//! "TooDue" calendar for every connected project member. Inbound: a watch
//! channel posts to /api/google/webhook when the user edits the calendar, and
//! date/time changes flow back onto the task (dates only — titles and other
//! fields stay owned by TooDue).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Redirect;
use axum::Json;
use chrono::{Duration, NaiveDate, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{now_iso, random_token, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::events::project_recipients;
use crate::models::Task;
use crate::routes::tasks::TASK_COLS;
use crate::AppState;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const CAL_API: &str = "https://www.googleapis.com/calendar/v3";

/// Pending OAuth `state` params: state -> (user_id, expires_unix).
pub type OauthStates = Arc<Mutex<HashMap<String, (i64, i64)>>>;

type GRes<T> = Result<T, String>;

fn estr<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

struct Config {
    client_id: String,
    client_secret: String,
    public_url: String,
}

fn config() -> Option<Config> {
    Some(Config {
        client_id: std::env::var("GOOGLE_CLIENT_ID")
            .ok()
            .filter(|s| !s.is_empty())?,
        client_secret: std::env::var("GOOGLE_CLIENT_SECRET")
            .ok()
            .filter(|s| !s.is_empty())?,
        public_url: std::env::var("PUBLIC_URL")
            .ok()
            .filter(|s| !s.is_empty())?
            .trim_end_matches('/')
            .to_string(),
    })
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn iso_in(secs: i64) -> String {
    (Utc::now() + Duration::seconds(secs))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/* ---------- OAuth endpoints ---------- */

pub async fn status(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Value>> {
    let row: Option<(String,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT calendar_id FROM google_accounts WHERE user_id = ?",
    ))
    .bind(user.id)
    .fetch_optional(&st.db.pool)
    .await?;
    Ok(Json(json!({
        "configured": config().is_some(),
        "connected": row.is_some(),
    })))
}

pub async fn connect(State(st): State<AppState>, AuthUser(user): AuthUser) -> ApiResult<Redirect> {
    let cfg = config().ok_or_else(|| {
        ApiError::bad_request("Google Calendar sync is not configured on this server")
    })?;
    let state_token = random_token();
    {
        let mut states = st.oauth_states.lock().unwrap();
        let now = Utc::now().timestamp();
        states.retain(|_, (_, exp)| *exp > now);
        states.insert(state_token.clone(), (user.id, now + 600));
    }
    let redirect_uri = format!("{}/api/google/callback", cfg.public_url);
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth\
         ?client_id={}&redirect_uri={}&response_type=code&scope={}\
         &access_type=offline&prompt={}&state={state_token}",
        urlencode(&cfg.client_id),
        urlencode(&redirect_uri),
        urlencode("https://www.googleapis.com/auth/calendar"),
        urlencode("select_account consent"),
    );
    Ok(Redirect::temporary(&url))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
}

pub async fn callback(
    State(st): State<AppState>,
    Query(q): Query<CallbackQuery>,
) -> ApiResult<Redirect> {
    let cfg = config().ok_or_else(|| {
        ApiError::bad_request("Google Calendar sync is not configured on this server")
    })?;
    let (Some(code), Some(state_token)) = (q.code, q.state) else {
        // User cancelled at the consent screen.
        return Ok(Redirect::temporary("/"));
    };
    let user_id = {
        let mut states = st.oauth_states.lock().unwrap();
        match states.remove(&state_token) {
            Some((uid, exp)) if exp > Utc::now().timestamp() => uid,
            _ => {
                return Err(ApiError::bad_request(
                    "sign-in expired — go back to TooDue and connect again",
                ))
            }
        }
    };

    let redirect_uri = format!("{}/api/google/callback", cfg.public_url);
    let resp: Value = st
        .http
        .post(TOKEN_URL)
        .form(&[
            ("code", code.as_str()),
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("Google token exchange failed: {e}")))?
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("Google token exchange failed: {e}")))?;

    let access = resp["access_token"]
        .as_str()
        .ok_or_else(|| ApiError::bad_request("Google did not return an access token"))?
        .to_string();
    let refresh = resp["refresh_token"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if refresh.is_empty() {
        return Err(ApiError::bad_request(
            "Google did not return a refresh token — remove TooDue at myaccount.google.com/permissions and connect again",
        ));
    }
    let expires_at = iso_in(resp["expires_in"].as_i64().unwrap_or(3600) - 60);

    // Reconnecting: clean up the previous calendar and channel first.
    let _ = teardown(&st, user_id).await;

    let primary: Value = st
        .http
        .get(format!("{CAL_API}/calendars/primary"))
        .bearer_auth(&access)
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("Google Calendar unreachable: {e}")))?
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("Google Calendar unreachable: {e}")))?;
    let tz = primary["timeZone"].as_str().unwrap_or("UTC").to_string();

    let cal: Value = st
        .http
        .post(format!("{CAL_API}/calendars"))
        .bearer_auth(&access)
        .json(&json!({ "summary": "TooDue", "timeZone": tz }))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("could not create calendar: {e}")))?
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("could not create calendar: {e}")))?;
    let calendar_id = cal["id"]
        .as_str()
        .ok_or_else(|| ApiError::internal(format!("could not create calendar: {cal}")))?
        .to_string();

    sqlx::query(&*crate::db::sql(
        "INSERT INTO google_accounts (user_id, access_token, refresh_token, token_expires_at, calendar_id, time_zone) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(user_id) DO UPDATE SET \
           access_token = excluded.access_token, refresh_token = excluded.refresh_token, \
           token_expires_at = excluded.token_expires_at, calendar_id = excluded.calendar_id, \
           time_zone = excluded.time_zone, channel_id = NULL, resource_id = NULL, \
           channel_expires_at = NULL, sync_token = NULL",
    ))
    .bind(user_id)
    .bind(&access)
    .bind(&refresh)
    .bind(&expires_at)
    .bind(&calendar_id)
    .bind(&tz)
    .execute(&st.db.pool)
    .await?;

    let st2 = st.clone();
    tokio::spawn(async move {
        if let Err(e) = full_sync(&st2, user_id).await {
            tracing::warn!("gcal initial sync for user {user_id}: {e}");
        }
        if let Err(e) = start_watch(&st2, user_id).await {
            tracing::warn!("gcal watch for user {user_id}: {e}");
        }
        if let Err(e) = prime_sync_token(&st2, user_id).await {
            tracing::warn!("gcal sync token for user {user_id}: {e}");
        }
    });

    Ok(Redirect::temporary("/"))
}

/// Best-effort removal of the user's channel and TooDue calendar at Google.
async fn teardown(st: &AppState, user_id: i64) -> GRes<()> {
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(&*crate::db::sql(
        "SELECT calendar_id, channel_id, resource_id FROM google_accounts WHERE user_id = ?",
    ))
    .bind(user_id)
    .fetch_optional(&st.db.pool)
    .await
    .map_err(estr)?;
    let Some((calendar_id, channel_id, resource_id)) = row else {
        return Ok(());
    };
    let Ok((token, _, _)) = access_token(st, user_id).await else {
        return Ok(());
    };
    if let (Some(cid), Some(rid)) = (channel_id, resource_id) {
        let _ = st
            .http
            .post(format!("{CAL_API}/channels/stop"))
            .bearer_auth(&token)
            .json(&json!({ "id": cid, "resourceId": rid }))
            .send()
            .await;
    }
    let _ = st
        .http
        .delete(format!("{CAL_API}/calendars/{}", urlencode(&calendar_id)))
        .bearer_auth(&token)
        .send()
        .await;
    Ok(())
}

pub async fn disconnect(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Value>> {
    let _ = teardown(&st, user.id).await;
    sqlx::query(&*crate::db::sql(
        "DELETE FROM gcal_events WHERE user_id = ?",
    ))
    .bind(user.id)
    .execute(&st.db.pool)
    .await?;
    sqlx::query(&*crate::db::sql(
        "DELETE FROM google_accounts WHERE user_id = ?",
    ))
    .bind(user.id)
    .execute(&st.db.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

/* ---------- token handling ---------- */

/// Returns a valid (access_token, calendar_id, time_zone), refreshing if needed.
async fn access_token(st: &AppState, user_id: i64) -> GRes<(String, String, String)> {
    let row: Option<(String, String, String, String, String)> = sqlx::query_as(&*crate::db::sql(
        "SELECT access_token, refresh_token, token_expires_at, calendar_id, time_zone \
         FROM google_accounts WHERE user_id = ?",
    ))
    .bind(user_id)
    .fetch_optional(&st.db.pool)
    .await
    .map_err(estr)?;
    let (access, refresh, expires, cal, tz) = row.ok_or("not connected")?;
    if expires > now_iso() {
        return Ok((access, cal, tz));
    }
    let cfg = config().ok_or("not configured")?;
    let resp: Value = st
        .http
        .post(TOKEN_URL)
        .form(&[
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(estr)?
        .json()
        .await
        .map_err(estr)?;
    let access = resp["access_token"]
        .as_str()
        .ok_or_else(|| format!("token refresh failed: {resp}"))?
        .to_string();
    let new_expires = iso_in(resp["expires_in"].as_i64().unwrap_or(3600) - 60);
    sqlx::query(&*crate::db::sql(
        "UPDATE google_accounts SET access_token = ?, token_expires_at = ? WHERE user_id = ?",
    ))
    .bind(&access)
    .bind(&new_expires)
    .bind(user_id)
    .execute(&st.db.pool)
    .await
    .map_err(estr)?;
    Ok((access, cal, tz))
}

/* ---------- outbound sync (tasks -> events) ---------- */

fn event_body(task: &Task, tz: &str) -> GRes<Value> {
    let date = task.due_date.as_deref().ok_or("task has no date")?;
    let (start, end) = if let Some(t) = &task.due_time {
        let start_dt = NaiveDateTime::parse_from_str(&format!("{date} {t}"), "%Y-%m-%d %H:%M")
            .map_err(estr)?;
        let end_dt = start_dt + Duration::hours(1);
        (
            json!({ "dateTime": start_dt.format("%Y-%m-%dT%H:%M:00").to_string(), "timeZone": tz }),
            json!({ "dateTime": end_dt.format("%Y-%m-%dT%H:%M:00").to_string(), "timeZone": tz }),
        )
    } else {
        let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(estr)?;
        (
            json!({ "date": date }),
            json!({ "date": (d + Duration::days(1)).format("%Y-%m-%d").to_string() }),
        )
    };
    Ok(json!({
        "summary": task.name,
        "description": task.description,
        "start": start,
        "end": end,
        "extendedProperties": { "private": { "toodue_task_id": task.id.to_string() } },
    }))
}

async fn upsert_event(st: &AppState, user_id: i64, task: &Task) -> GRes<()> {
    let (token, cal, tz) = access_token(st, user_id).await?;
    let body = event_body(task, &tz)?;
    let existing: Option<(String,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT event_id FROM gcal_events WHERE user_id = ? AND task_id = ?",
    ))
    .bind(user_id)
    .bind(task.id)
    .fetch_optional(&st.db.pool)
    .await
    .map_err(estr)?;
    if let Some((event_id,)) = existing {
        let resp = st
            .http
            .patch(format!(
                "{CAL_API}/calendars/{}/events/{}",
                urlencode(&cal),
                urlencode(&event_id)
            ))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(estr)?;
        match resp.status().as_u16() {
            200..=299 => return Ok(()),
            404 | 410 => {} // event was deleted in Google; recreate it below
            s => return Err(format!("event update failed ({s})")),
        }
    }
    let resp = st
        .http
        .post(format!("{CAL_API}/calendars/{}/events", urlencode(&cal)))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .map_err(estr)?;
    if !resp.status().is_success() {
        return Err(format!("event insert failed ({})", resp.status()));
    }
    let v: Value = resp.json().await.map_err(estr)?;
    let event_id = v["id"].as_str().ok_or("event insert returned no id")?;
    sqlx::query(&*crate::db::sql(
        "INSERT OR REPLACE INTO gcal_events (user_id, task_id, event_id) VALUES (?, ?, ?)",
    ))
    .bind(user_id)
    .bind(task.id)
    .bind(event_id)
    .execute(&st.db.pool)
    .await
    .map_err(estr)?;
    Ok(())
}

async fn delete_event(st: &AppState, user_id: i64, task_id: i64) -> GRes<()> {
    let existing: Option<(String,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT event_id FROM gcal_events WHERE user_id = ? AND task_id = ?",
    ))
    .bind(user_id)
    .bind(task_id)
    .fetch_optional(&st.db.pool)
    .await
    .map_err(estr)?;
    if let Some((event_id,)) = existing {
        if let Ok((token, cal, _)) = access_token(st, user_id).await {
            let _ = st
                .http
                .delete(format!(
                    "{CAL_API}/calendars/{}/events/{}",
                    urlencode(&cal),
                    urlencode(&event_id)
                ))
                .bearer_auth(&token)
                .send()
                .await;
        }
        sqlx::query(&*crate::db::sql(
            "DELETE FROM gcal_events WHERE user_id = ? AND task_id = ?",
        ))
        .bind(user_id)
        .bind(task_id)
        .execute(&st.db.pool)
        .await
        .map_err(estr)?;
    }
    Ok(())
}

/// Reconciles one task with every connected member's calendar. Handles
/// deleted, completed, undated, and moved tasks by diffing desired state
/// against the mapping table.
async fn task_sync(st: &AppState, task_id: i64) -> GRes<()> {
    let sql = format!("SELECT {TASK_COLS} FROM tasks t WHERE t.id = ?");
    let task = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(task_id)
        .fetch_optional(&st.db.pool)
        .await
        .map_err(estr)?;

    let desired_users: Vec<i64> = match &task {
        Some(t) if t.due_date.is_some() && t.completed_at.is_none() => sqlx::query_as::<_, (i64,)>(
            "SELECT ga.user_id FROM google_accounts ga \
                 JOIN project_members m ON m.user_id = ga.user_id WHERE m.project_id = ?",
        )
        .bind(t.project_id)
        .fetch_all(&st.db.pool)
        .await
        .map_err(estr)?
        .into_iter()
        .map(|r| r.0)
        .collect(),
        _ => Vec::new(),
    };

    let mapped: Vec<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT user_id FROM gcal_events WHERE task_id = ?",
    ))
    .bind(task_id)
    .fetch_all(&st.db.pool)
    .await
    .map_err(estr)?;
    for (uid,) in &mapped {
        if !desired_users.contains(uid) {
            if let Err(e) = delete_event(st, *uid, task_id).await {
                tracing::warn!("gcal delete event (user {uid}, task {task_id}): {e}");
            }
        }
    }
    if let Some(t) = &task {
        for uid in &desired_users {
            if let Err(e) = upsert_event(st, *uid, t).await {
                tracing::warn!("gcal upsert event (user {uid}, task {task_id}): {e}");
            }
        }
    }
    Ok(())
}

pub fn spawn_task_sync(st: &AppState, task_id: i64) {
    if config().is_none() {
        return;
    }
    let st = st.clone();
    tokio::spawn(async move {
        if let Err(e) = task_sync(&st, task_id).await {
            tracing::warn!("gcal sync task {task_id}: {e}");
        }
    });
}

/// Re-syncs every dated task in a project (share / member changes).
pub fn spawn_project_sync(st: &AppState, project_id: i64) {
    if config().is_none() {
        return;
    }
    let st = st.clone();
    tokio::spawn(async move {
        let ids: Vec<(i64,)> = sqlx::query_as(&*crate::db::sql(
            "SELECT id FROM tasks WHERE project_id = ? AND due_date IS NOT NULL",
        ))
        .bind(project_id)
        .fetch_all(&st.db.pool)
        .await
        .unwrap_or_default();
        for (id,) in ids {
            if let Err(e) = task_sync(&st, id).await {
                tracing::warn!("gcal sync task {id}: {e}");
            }
        }
    });
}

/// Cleans up events whose tasks no longer exist (bulk deletes, project deletes).
pub fn spawn_orphan_cleanup(st: &AppState) {
    if config().is_none() {
        return;
    }
    let st = st.clone();
    tokio::spawn(async move {
        let ids: Vec<(i64,)> = sqlx::query_as(&*crate::db::sql(
            "SELECT DISTINCT task_id FROM gcal_events WHERE task_id NOT IN (SELECT id FROM tasks)",
        ))
        .fetch_all(&st.db.pool)
        .await
        .unwrap_or_default();
        for (id,) in ids {
            if let Err(e) = task_sync(&st, id).await {
                tracing::warn!("gcal cleanup task {id}: {e}");
            }
        }
    });
}

pub fn spawn_full_sync(st: &AppState, user_id: i64) {
    if config().is_none() {
        return;
    }
    let st = st.clone();
    tokio::spawn(async move {
        if let Err(e) = full_sync(&st, user_id).await {
            tracing::warn!("gcal full sync for user {user_id}: {e}");
        }
    });
}

async fn full_sync(st: &AppState, user_id: i64) -> GRes<()> {
    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks t WHERE t.completed_at IS NULL AND t.due_date IS NOT NULL \
         AND t.project_id IN (SELECT project_id FROM project_members WHERE user_id = ?)"
    );
    let tasks = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(user_id)
        .fetch_all(&st.db.pool)
        .await
        .map_err(estr)?;
    let desired: HashSet<i64> = tasks.iter().map(|t| t.id).collect();

    let mapped: Vec<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT task_id FROM gcal_events WHERE user_id = ?",
    ))
    .bind(user_id)
    .fetch_all(&st.db.pool)
    .await
    .map_err(estr)?;
    for (tid,) in mapped {
        if !desired.contains(&tid) {
            let _ = delete_event(st, user_id, tid).await;
        }
    }
    for t in &tasks {
        if let Err(e) = upsert_event(st, user_id, t).await {
            tracing::warn!("gcal upsert (user {user_id}, task {}): {e}", t.id);
        }
    }
    Ok(())
}

/* ---------- inbound sync (events -> tasks) ---------- */

async fn start_watch(st: &AppState, user_id: i64) -> GRes<()> {
    let cfg = config().ok_or("not configured")?;
    if !cfg.public_url.starts_with("https://") {
        // Google requires an HTTPS webhook; outbound-only sync in local dev.
        return Ok(());
    }
    let (token, cal, _) = access_token(st, user_id).await?;
    let channel_id = random_token();
    let resp: Value = st
        .http
        .post(format!(
            "{CAL_API}/calendars/{}/events/watch",
            urlencode(&cal)
        ))
        .bearer_auth(&token)
        .json(&json!({
            "id": channel_id,
            "type": "web_hook",
            "address": format!("{}/api/google/webhook", cfg.public_url),
        }))
        .send()
        .await
        .map_err(estr)?
        .json()
        .await
        .map_err(estr)?;
    let resource_id = resp["resourceId"]
        .as_str()
        .ok_or_else(|| format!("watch failed: {resp}"))?;
    let expires = resp["expiration"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| resp["expiration"].as_i64())
        .and_then(chrono::DateTime::from_timestamp_millis)
        .map(|d| d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string());
    sqlx::query(&*crate::db::sql(
        "UPDATE google_accounts SET channel_id = ?, resource_id = ?, channel_expires_at = ? WHERE user_id = ?",
    ))
    .bind(&channel_id)
    .bind(resource_id)
    .bind(&expires)
    .bind(user_id)
    .execute(&st.db.pool)
    .await
    .map_err(estr)?;
    Ok(())
}

/// Pages through the event list solely to obtain a fresh nextSyncToken.
async fn prime_sync_token(st: &AppState, user_id: i64) -> GRes<()> {
    let (token, cal, _) = access_token(st, user_id).await?;
    let mut page_token: Option<String> = None;
    loop {
        let mut url = format!(
            "{CAL_API}/calendars/{}/events?maxResults=250&showDeleted=true",
            urlencode(&cal)
        );
        if let Some(p) = &page_token {
            url += &format!("&pageToken={}", urlencode(p));
        }
        let v: Value = st
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(estr)?
            .json()
            .await
            .map_err(estr)?;
        if let Some(next) = v["nextPageToken"].as_str() {
            page_token = Some(next.to_string());
            continue;
        }
        let sync = v["nextSyncToken"].as_str().ok_or("no sync token")?;
        sqlx::query(&*crate::db::sql(
            "UPDATE google_accounts SET sync_token = ? WHERE user_id = ?",
        ))
        .bind(sync)
        .bind(user_id)
        .execute(&st.db.pool)
        .await
        .map_err(estr)?;
        return Ok(());
    }
}

pub async fn webhook(State(st): State<AppState>, headers: HeaderMap) -> StatusCode {
    let channel = headers
        .get("x-goog-channel-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let resource_state = headers
        .get("x-goog-resource-state")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    if channel.is_empty() || resource_state == "sync" {
        return StatusCode::OK;
    }
    let row: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT user_id FROM google_accounts WHERE channel_id = ?",
    ))
    .bind(&channel)
    .fetch_optional(&st.db.pool)
    .await
    .ok()
    .flatten();
    if let Some((user_id,)) = row {
        let st = st.clone();
        tokio::spawn(async move {
            if let Err(e) = incremental_sync(&st, user_id).await {
                tracing::warn!("gcal incremental sync for user {user_id}: {e}");
            }
        });
    }
    StatusCode::OK
}

async fn incremental_sync(st: &AppState, user_id: i64) -> GRes<()> {
    let (token, cal, _) = access_token(st, user_id).await?;
    let (sync_token,): (Option<String>,) = sqlx::query_as(&*crate::db::sql(
        "SELECT sync_token FROM google_accounts WHERE user_id = ?",
    ))
    .bind(user_id)
    .fetch_optional(&st.db.pool)
    .await
    .map_err(estr)?
    .ok_or("not connected")?;
    let Some(sync) = sync_token else {
        return prime_sync_token(st, user_id).await;
    };

    let mut page_token: Option<String> = None;
    loop {
        let mut url = format!(
            "{CAL_API}/calendars/{}/events?maxResults=250",
            urlencode(&cal)
        );
        if let Some(p) = &page_token {
            url += &format!("&pageToken={}", urlencode(p));
        } else {
            url += &format!("&syncToken={}", urlencode(&sync));
        }
        let resp = st
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(estr)?;
        if resp.status().as_u16() == 410 {
            // Sync token expired; re-prime and catch changes on the next ping.
            sqlx::query(&*crate::db::sql(
                "UPDATE google_accounts SET sync_token = NULL WHERE user_id = ?",
            ))
            .bind(user_id)
            .execute(&st.db.pool)
            .await
            .map_err(estr)?;
            return prime_sync_token(st, user_id).await;
        }
        let v: Value = resp.json().await.map_err(estr)?;
        let empty = Vec::new();
        for item in v["items"].as_array().unwrap_or(&empty) {
            if let Err(e) = apply_event(st, user_id, item).await {
                tracing::warn!("gcal apply event for user {user_id}: {e}");
            }
        }
        if let Some(next) = v["nextPageToken"].as_str() {
            page_token = Some(next.to_string());
            continue;
        }
        if let Some(ns) = v["nextSyncToken"].as_str() {
            sqlx::query(&*crate::db::sql(
                "UPDATE google_accounts SET sync_token = ? WHERE user_id = ?",
            ))
            .bind(ns)
            .bind(user_id)
            .execute(&st.db.pool)
            .await
            .map_err(estr)?;
        }
        return Ok(());
    }
}

/// Applies a changed Google event back onto its task (date/time only).
async fn apply_event(st: &AppState, user_id: i64, ev: &Value) -> GRes<()> {
    if ev["status"].as_str() == Some("cancelled") {
        return Ok(());
    }
    let Some(task_id) = ev["extendedProperties"]["private"]["toodue_task_id"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok())
    else {
        return Ok(()); // not one of ours
    };
    let sql = format!("SELECT {TASK_COLS} FROM tasks t WHERE t.id = ?");
    let Some(task) = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(task_id)
        .fetch_optional(&st.db.pool)
        .await
        .map_err(estr)?
    else {
        return Ok(());
    };
    let member: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT 1 FROM project_members WHERE project_id = ? AND user_id = ?",
    ))
    .bind(task.project_id)
    .bind(user_id)
    .fetch_optional(&st.db.pool)
    .await
    .map_err(estr)?;
    if member.is_none() {
        return Ok(());
    }

    let (new_date, new_time) = if let Some(d) = ev["start"]["date"].as_str() {
        (Some(d.to_string()), None)
    } else if let Some(dt) = ev["start"]["dateTime"].as_str() {
        if dt.len() < 16 {
            return Ok(());
        }
        // Take the wall-clock part; events live in the user's own timezone.
        (Some(dt[..10].to_string()), Some(dt[11..16].to_string()))
    } else {
        return Ok(());
    };

    if task.due_date == new_date && task.due_time == new_time {
        return Ok(());
    }

    let now = now_iso();
    sqlx::query(&*crate::db::sql(
        "UPDATE tasks SET due_date = ?, due_time = ?, updated_at = ? WHERE id = ?",
    ))
    .bind(&new_date)
    .bind(&new_time)
    .bind(&now)
    .bind(task_id)
    .execute(&st.db.pool)
    .await
    .map_err(estr)?;

    let fresh = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(task_id)
        .fetch_one(&st.db.pool)
        .await
        .map_err(estr)?;
    let recipients = project_recipients(&st.db.pool, fresh.project_id).await;
    st.hub.publish(
        recipients,
        "task.upsert",
        serde_json::to_value(&fresh).map_err(estr)?,
    );
    // Propagate to the other members' calendars too (no-op for the editor).
    task_sync(st, task_id).await?;
    Ok(())
}

/* ---------- channel renewal ---------- */

/// Runs periodically: renews watch channels expiring within a day and primes
/// missing sync tokens.
pub async fn renew_channels(st: &AppState) {
    if config().is_none() {
        return;
    }
    let rows: Vec<(i64, Option<String>, Option<String>)> = sqlx::query_as(&*crate::db::sql(
        "SELECT user_id, channel_expires_at, sync_token FROM google_accounts",
    ))
    .fetch_all(&st.db.pool)
    .await
    .unwrap_or_default();
    for (user_id, expires, sync_token) in rows {
        let needs_renewal = expires.map(|e| e < iso_in(24 * 3600)).unwrap_or(true);
        if needs_renewal {
            if let Err(e) = start_watch(st, user_id).await {
                tracing::warn!("gcal channel renewal for user {user_id}: {e}");
            }
        }
        if sync_token.is_none() {
            if let Err(e) = prime_sync_token(st, user_id).await {
                tracing::warn!("gcal sync token for user {user_id}: {e}");
            }
        }
    }
}
