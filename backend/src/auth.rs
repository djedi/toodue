use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::extract::{FromRef, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderName, Method};
use axum::response::AppendHeaders;
use axum::Json;
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::{ApiError, ApiResult};
use crate::models::User;
use crate::AppState;

const COOKIE_NAME: &str = "toodue_session";
const SESSION_DAYS: i64 = 60;

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn now_iso() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            part.trim()
                .strip_prefix(concat!("toodue_session", "="))
                .map(str::to_string)
        })
}

#[derive(Clone)]
pub struct AuthUser(pub User);

#[derive(Clone)]
pub struct ApiUser(pub User);

pub(crate) async fn user_from_bearer(
    st: &AppState,
    headers: &HeaderMap,
) -> ApiResult<Option<User>> {
    let Some(token) = bearer_token(headers) else {
        return Ok(None);
    };
    let token_hash = hash_token(&token);
    let user = sqlx::query_as::<_, User>(&*crate::db::sql(
        "SELECT u.id, u.email, u.name FROM api_keys k \
         JOIN users u ON u.id = k.user_id WHERE k.token_hash = ?",
    ))
    .bind(&token_hash)
    .fetch_optional(&st.db.pool)
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    sqlx::query(&*crate::db::sql(
        "UPDATE api_keys SET last_used_at = ? WHERE token_hash = ?",
    ))
    .bind(now_iso())
    .bind(&token_hash)
    .execute(&st.db.pool)
    .await?;
    Ok(Some(user))
}

/// Native (Capacitor) clients can't use the cookie cross-origin, so they send
/// their session token as a bearer header. EventSource can't set headers, so
/// the in-app SSE stream also accepts it as `?access_token=`.
fn native_session_token(parts: &Parts) -> Option<String> {
    if let Some(token) = bearer_token(&parts.headers) {
        return Some(token);
    }
    if parts.method != Method::GET || !parts.uri.path().ends_with("/events") {
        return None;
    }
    query_param(parts, "access_token")
}

fn query_param(parts: &Parts, name: &str) -> Option<String> {
    parts.uri.query()?.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == name && !v.is_empty() && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| v.to_string())
    })
}

/// Short-lived, single-use tokens that let a native client open one API URL
/// (an attachment download, the Google connect redirect) in the system
/// browser without handing it the session token. token -> (user, expiry, path).
pub type LinkTokens = Arc<Mutex<HashMap<String, (i64, i64, String)>>>;

const LINK_TOKEN_SECS: i64 = 60;

fn link_path_allowed(path: &str) -> bool {
    path == "/google/connect"
        || path
            .strip_prefix("/attachments/")
            .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

#[derive(Deserialize)]
pub struct LinkTokenBody {
    pub path: String,
}

pub async fn link_token(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(b): Json<LinkTokenBody>,
) -> ApiResult<Json<serde_json::Value>> {
    if !link_path_allowed(&b.path) {
        return Err(ApiError::bad_request("unsupported link path"));
    }
    let token = random_token();
    let now = Utc::now().timestamp();
    let mut tokens = st.link_tokens.lock().unwrap();
    tokens.retain(|_, (_, exp, _)| *exp > now);
    tokens.insert(token.clone(), (user.id, now + LINK_TOKEN_SECS, b.path));
    Ok(Json(serde_json::json!({ "token": token })))
}

fn take_link_token(st: &AppState, parts: &Parts) -> Option<i64> {
    if parts.method != Method::GET {
        return None;
    }
    let token = query_param(parts, "link_token")?;
    let (user_id, exp, path) = st.link_tokens.lock().unwrap().remove(&token)?;
    // Nested routers see the path without the /api prefix.
    let req_path = parts.uri.path();
    let req_path = req_path.strip_prefix("/api").unwrap_or(req_path);
    (exp > Utc::now().timestamp() && req_path == path).then_some(user_id)
}

fn wants_native_session(headers: &HeaderMap) -> bool {
    headers
        .get("x-toodue-client")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "native")
}

async fn user_from_session(st: &AppState, token: &str) -> ApiResult<Option<User>> {
    let user = sqlx::query_as::<_, User>(&*crate::db::sql(
        "SELECT u.id, u.email, u.name FROM sessions s \
         JOIN users u ON u.id = s.user_id \
         WHERE s.token = ? AND s.expires_at > ?",
    ))
    .bind(token)
    .bind(now_iso())
    .fetch_optional(&st.db.pool)
    .await?;
    Ok(user)
}

async fn user_from_cookie(st: &AppState, headers: &HeaderMap) -> ApiResult<Option<User>> {
    let Some(token) = cookie_token(headers) else {
        return Ok(None);
    };
    let user = sqlx::query_as::<_, User>(&*crate::db::sql(
        "SELECT u.id, u.email, u.name FROM sessions s \
         JOIN users u ON u.id = s.user_id \
         WHERE s.token = ? AND s.expires_at > ?",
    ))
    .bind(&token)
    .bind(now_iso())
    .fetch_optional(&st.db.pool)
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    Ok(Some(user))
}

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let st = AppState::from_ref(state);
        if let Some(token) = native_session_token(parts) {
            if let Some(user) = user_from_session(&st, &token).await? {
                return Ok(AuthUser(user));
            }
        }
        if let Some(user_id) = take_link_token(&st, parts) {
            let user = sqlx::query_as::<_, User>(&*crate::db::sql(
                "SELECT id, email, name FROM users WHERE id = ?",
            ))
            .bind(user_id)
            .fetch_optional(&st.db.pool)
            .await?;
            if let Some(user) = user {
                return Ok(AuthUser(user));
            }
        }
        if bearer_token(&parts.headers).is_some() {
            if let Some(user) = user_from_bearer(&st, &parts.headers).await? {
                return Ok(AuthUser(user));
            }
        }
        if let Some(user) = user_from_cookie(&st, &parts.headers).await? {
            return Ok(AuthUser(user));
        }
        Err(ApiError::unauthorized())
    }
}

impl<S> FromRequestParts<S> for ApiUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let st = AppState::from_ref(state);
        let user = user_from_bearer(&st, &parts.headers)
            .await?
            .ok_or_else(ApiError::unauthorized)?;
        Ok(ApiUser(user))
    }
}

#[derive(Serialize)]
pub struct SessionBody {
    #[serde(flatten)]
    user: User,
    /// Only returned to native clients, which can't use the HttpOnly cookie.
    #[serde(skip_serializing_if = "Option::is_none")]
    session_token: Option<String>,
}

type SessionResponse = (AppendHeaders<[(HeaderName, String); 1]>, Json<SessionBody>);

async fn start_session(st: &AppState, user_id: i64, native: bool) -> ApiResult<SessionResponse> {
    let token = random_token();
    let expires = (Utc::now() + Duration::days(SESSION_DAYS))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    sqlx::query(&*crate::db::sql(
        "INSERT INTO sessions (token, user_id, expires_at) VALUES (?, ?, ?)",
    ))
    .bind(&token)
    .bind(user_id)
    .bind(&expires)
    .execute(&st.db.pool)
    .await?;
    let user = sqlx::query_as::<_, User>(&*crate::db::sql(
        "SELECT id, email, name FROM users WHERE id = ?",
    ))
    .bind(user_id)
    .fetch_one(&st.db.pool)
    .await?;
    let cookie = format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        SESSION_DAYS * 24 * 3600
    );
    let session_token = native.then(|| token.clone());
    Ok((
        AppendHeaders([(header::SET_COOKIE, cookie)]),
        Json(SessionBody {
            user,
            session_token,
        }),
    ))
}

#[derive(Deserialize)]
pub struct RegisterBody {
    pub name: String,
    pub email: String,
    pub password: String,
}

pub async fn register(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<RegisterBody>,
) -> ApiResult<SessionResponse> {
    let email = b.email.trim().to_lowercase();
    let name = b.name.trim().to_string();
    if name.is_empty() || !email.contains('@') {
        return Err(ApiError::bad_request(
            "a name and a valid email are required",
        ));
    }
    if b.password.len() < 8 {
        return Err(ApiError::bad_request(
            "password must be at least 8 characters",
        ));
    }
    let existing: Option<(i64,)> =
        sqlx::query_as(&*crate::db::sql("SELECT id FROM users WHERE email = ?"))
            .bind(&email)
            .fetch_optional(&st.db.pool)
            .await?;
    if existing.is_some() {
        return Err(ApiError::bad_request(
            "an account with that email already exists",
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(b.password.as_bytes(), &salt)
        .map_err(|_| ApiError::internal("could not hash password"))?
        .to_string();

    let (user_id,): (i64,) = sqlx::query_as(&*crate::db::sql(
        "INSERT INTO users (email, name, password_hash, ics_token) VALUES (?, ?, ?, ?) RETURNING id",
    ))
    .bind(&email)
    .bind(&name)
    .bind(&hash)
    .bind(random_token())
    .fetch_one(&st.db.pool)
    .await?;

    let (project_id,): (i64,) = sqlx::query_as(&*crate::db::sql(
        "INSERT INTO projects (name, color, owner_id, is_inbox) VALUES ('Inbox', 'slate', ?, 1) RETURNING id",
    ))
    .bind(user_id)
    .fetch_one(&st.db.pool)
    .await?;
    sqlx::query(&*crate::db::sql(
        "INSERT INTO project_members (project_id, user_id, role) VALUES (?, ?, 'owner')",
    ))
    .bind(project_id)
    .bind(user_id)
    .execute(&st.db.pool)
    .await?;

    start_session(&st, user_id, wants_native_session(&headers)).await
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
}

pub async fn login(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<LoginBody>,
) -> ApiResult<SessionResponse> {
    let email = b.email.trim().to_lowercase();
    let row: Option<(i64, String)> = sqlx::query_as(&*crate::db::sql(
        "SELECT id, password_hash FROM users WHERE email = ?",
    ))
    .bind(&email)
    .fetch_optional(&st.db.pool)
    .await?;
    let (user_id, hash) = row.ok_or_else(|| ApiError::bad_request("invalid email or password"))?;
    let parsed =
        PasswordHash::new(&hash).map_err(|_| ApiError::internal("corrupt password hash"))?;
    Argon2::default()
        .verify_password(b.password.as_bytes(), &parsed)
        .map_err(|_| ApiError::bad_request("invalid email or password"))?;
    start_session(&st, user_id, wants_native_session(&headers)).await
}

pub async fn logout(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<AppendHeaders<[(HeaderName, String); 1]>> {
    for token in [cookie_token(&headers), bearer_token(&headers)]
        .into_iter()
        .flatten()
    {
        sqlx::query(&*crate::db::sql("DELETE FROM sessions WHERE token = ?"))
            .bind(&token)
            .execute(&st.db.pool)
            .await?;
    }
    let cookie = format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    Ok(AppendHeaders([(header::SET_COOKIE, cookie)]))
}

pub async fn me(AuthUser(user): AuthUser) -> Json<User> {
    Json(user)
}

#[derive(Deserialize)]
pub struct DeleteAccountBody {
    pub password: String,
}

/// Permanently deletes the signed-in account (App Store guideline 5.1.1(v)).
/// The password is re-checked so a stolen session can't wipe an account.
pub async fn delete_account(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(b): Json<DeleteAccountBody>,
) -> ApiResult<AppendHeaders<[(HeaderName, String); 1]>> {
    let (hash,): (String,) = sqlx::query_as(&*crate::db::sql(
        "SELECT password_hash FROM users WHERE id = ?",
    ))
    .bind(user.id)
    .fetch_one(&st.db.pool)
    .await?;
    let parsed =
        PasswordHash::new(&hash).map_err(|_| ApiError::internal("corrupt password hash"))?;
    Argon2::default()
        .verify_password(b.password.as_bytes(), &parsed)
        .map_err(|_| ApiError::bad_request("incorrect password"))?;

    crate::gcal::teardown_for_user(&st, user.id).await;
    let files = delete_user_data(&st.db.pool, user.id).await?;
    let dir = st.data_dir.join("attachments");
    for name in files {
        let _ = tokio::fs::remove_file(dir.join(name)).await;
    }
    tracing::info!("deleted account {}", user.id);
    let cookie = format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    Ok(AppendHeaders([(header::SET_COOKIE, cookie)]))
}

/// Removes a user and everything they own in one transaction. Tasks they
/// created in other people's projects stay with those projects, reassigned to
/// the project owner. Returns attachment file names to unlink afterwards.
pub(crate) async fn delete_user_data(
    pool: &sqlx::AnyPool,
    user_id: i64,
) -> Result<Vec<String>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let files: Vec<(String,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT a.stored_name FROM attachments a JOIN tasks t ON t.id = a.task_id \
         JOIN projects p ON p.id = t.project_id \
         WHERE a.user_id = ? OR p.owner_id = ?",
    ))
    .bind(user_id)
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query(&*crate::db::sql(
        "UPDATE tasks SET creator_id = (SELECT p.owner_id FROM projects p WHERE p.id = tasks.project_id) \
         WHERE creator_id = ? AND project_id IN (SELECT id FROM projects WHERE owner_id <> ?)",
    ))
    .bind(user_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(&*crate::db::sql("DELETE FROM projects WHERE owner_id = ?"))
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(&*crate::db::sql("DELETE FROM users WHERE id = ?"))
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(files.into_iter().map(|r| r.0).collect())
}

#[cfg(test)]
mod tests {
    use super::link_path_allowed;

    #[tokio::test]
    async fn delete_user_data_removes_owned_data_and_keeps_shared_tasks() {
        use sqlx::any::{AnyConnectOptions, AnyPoolOptions};
        sqlx::any::install_default_drivers();
        let path = std::env::temp_dir().join(format!(
            "toodue-delete-account-{}-{}.db",
            std::process::id(),
            super::random_token()
        ));
        let opts: AnyConnectOptions = format!("sqlite://{}?mode=rwc", path.display())
            .parse()
            .unwrap();
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .after_connect(|c, _| {
                Box::pin(async move {
                    sqlx::query("PRAGMA foreign_keys = ON").execute(c).await?;
                    Ok(())
                })
            })
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("schema.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(
            "INSERT INTO users (id, email, name, password_hash, ics_token) VALUES \
               (1, 'gone@x.com', 'Gone', 'h', 't1'), (2, 'stay@x.com', 'Stay', 'h', 't2');
             INSERT INTO projects (id, name, owner_id) VALUES (10, 'Mine', 1), (20, 'Theirs', 2);
             INSERT INTO project_members (project_id, user_id) VALUES (10, 1), (20, 2), (20, 1);
             INSERT INTO tasks (id, project_id, creator_id, name) VALUES \
               (100, 10, 1, 'mine'), (200, 20, 1, 'mine in theirs'), (201, 20, 2, 'theirs');
             INSERT INTO attachments (task_id, user_id, filename, stored_name, mime, size) VALUES \
               (100, 1, 'a', 'own-file', 'text/plain', 1), (201, 2, 'b', 'their-file', 'text/plain', 1);
             INSERT INTO sessions (token, user_id, expires_at) VALUES ('s', 1, 'x');",
        )
        .execute(&pool)
        .await
        .unwrap();

        let files = super::delete_user_data(&pool, 1).await.unwrap();
        assert_eq!(files, ["own-file"]);

        let count = |q: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, i64>(q)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
            }
        };
        assert_eq!(count("SELECT COUNT(*) FROM users WHERE id = 1").await, 0);
        assert_eq!(count("SELECT COUNT(*) FROM sessions").await, 0);
        assert_eq!(
            count("SELECT COUNT(*) FROM projects WHERE id = 10").await,
            0
        );
        assert_eq!(count("SELECT COUNT(*) FROM tasks WHERE id = 100").await, 0);
        assert_eq!(
            count("SELECT creator_id FROM tasks WHERE id = 200").await,
            2
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM project_members WHERE user_id = 1").await,
            0
        );
        assert_eq!(count("SELECT COUNT(*) FROM attachments").await, 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn link_paths_are_restricted() {
        assert!(link_path_allowed("/google/connect"));
        assert!(link_path_allowed("/attachments/42"));
        assert!(!link_path_allowed("/attachments/"));
        assert!(!link_path_allowed("/attachments/42/../x"));
        assert!(!link_path_allowed("/tasks"));
    }
}
