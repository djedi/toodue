use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{Duration, NaiveDate, NaiveTime, Utc};
use serde_json::json;

use crate::auth::{random_token, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::AppState;

fn ics_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn all_day_event(out: &mut String, uid: &str, stamp: &str, date: &str, summary: &str, desc: &str) {
    let Ok(d) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
        return;
    };
    let end = d + Duration::days(1);
    out.push_str("BEGIN:VEVENT\r\n");
    out.push_str(&format!("UID:{uid}\r\n"));
    out.push_str(&format!("DTSTAMP:{stamp}\r\n"));
    out.push_str(&format!("DTSTART;VALUE=DATE:{}\r\n", d.format("%Y%m%d")));
    out.push_str(&format!("DTEND;VALUE=DATE:{}\r\n", end.format("%Y%m%d")));
    out.push_str(&format!("SUMMARY:{}\r\n", ics_escape(summary)));
    if !desc.is_empty() {
        out.push_str(&format!("DESCRIPTION:{}\r\n", ics_escape(desc)));
    }
    out.push_str("END:VEVENT\r\n");
}

fn timed_event(
    out: &mut String,
    uid: &str,
    stamp: &str,
    date: &str,
    time: &str,
    summary: &str,
    desc: &str,
) {
    let (Ok(d), Ok(t)) = (
        NaiveDate::parse_from_str(date, "%Y-%m-%d"),
        NaiveTime::parse_from_str(time, "%H:%M"),
    ) else {
        return;
    };
    let start = d.and_time(t);
    let end = start + Duration::hours(1);
    out.push_str("BEGIN:VEVENT\r\n");
    out.push_str(&format!("UID:{uid}\r\n"));
    out.push_str(&format!("DTSTAMP:{stamp}\r\n"));
    out.push_str(&format!("DTSTART:{}\r\n", start.format("%Y%m%dT%H%M%S")));
    out.push_str(&format!("DTEND:{}\r\n", end.format("%Y%m%dT%H%M%S")));
    out.push_str(&format!("SUMMARY:{}\r\n", ics_escape(summary)));
    if !desc.is_empty() {
        out.push_str(&format!("DESCRIPTION:{}\r\n", ics_escape(desc)));
    }
    out.push_str("END:VEVENT\r\n");
}

#[derive(sqlx::FromRow)]
struct IcsTask {
    id: i64,
    name: String,
    description: String,
    due_date: Option<String>,
    due_time: Option<String>,
    deadline: Option<String>,
}

pub async fn feed(State(st): State<AppState>, Path(token): Path<String>) -> ApiResult<Response> {
    let token = token.trim_end_matches(".ics");
    let user: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE ics_token = ?")
        .bind(token)
        .fetch_optional(&st.db)
        .await?;
    let (user_id,) = user.ok_or_else(ApiError::not_found)?;

    let tasks = sqlx::query_as::<_, IcsTask>(
        "SELECT t.id, t.name, t.description, t.due_date, t.due_time, t.deadline FROM tasks t \
         WHERE t.completed_at IS NULL \
           AND (t.due_date IS NOT NULL OR t.deadline IS NOT NULL) \
           AND t.project_id IN (SELECT project_id FROM project_members WHERE user_id = ?)",
    )
    .bind(user_id)
    .fetch_all(&st.db)
    .await?;

    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut out = String::from(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//TooDue//TooDue//EN\r\nCALSCALE:GREGORIAN\r\nX-WR-CALNAME:TooDue\r\n",
    );
    for t in &tasks {
        if let Some(date) = &t.due_date {
            let uid = format!("task-{}-due@toodue", t.id);
            match &t.due_time {
                Some(time) => {
                    timed_event(&mut out, &uid, &stamp, date, time, &t.name, &t.description)
                }
                None => all_day_event(&mut out, &uid, &stamp, date, &t.name, &t.description),
            }
        }
        if let Some(date) = &t.deadline {
            let uid = format!("task-{}-deadline@toodue", t.id);
            let summary = format!("Deadline: {}", t.name);
            all_day_event(&mut out, &uid, &stamp, date, &summary, &t.description);
        }
    }
    out.push_str("END:VCALENDAR\r\n");

    Ok((
        [(header::CONTENT_TYPE, "text/calendar; charset=utf-8")],
        out,
    )
        .into_response())
}

pub async fn my_url(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let (token,): (String,) = sqlx::query_as("SELECT ics_token FROM users WHERE id = ?")
        .bind(user.id)
        .fetch_one(&st.db)
        .await?;
    Ok(Json(json!({ "url": format!("/api/calendar/{token}.ics") })))
}

pub async fn rotate(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<serde_json::Value>> {
    let token = random_token();
    sqlx::query("UPDATE users SET ics_token = ? WHERE id = ?")
        .bind(&token)
        .bind(user.id)
        .execute(&st.db)
        .await?;
    Ok(Json(json!({ "url": format!("/api/calendar/{token}.ics") })))
}
