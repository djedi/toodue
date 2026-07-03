use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::extract::{FromRef, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderName};
use axum::response::AppendHeaders;
use axum::Json;
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::Deserialize;

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

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let st = AppState::from_ref(state);
        let token = cookie_token(&parts.headers).ok_or_else(ApiError::unauthorized)?;
        let user = sqlx::query_as::<_, User>(
            "SELECT u.id, u.email, u.name FROM sessions s \
             JOIN users u ON u.id = s.user_id \
             WHERE s.token = ? AND s.expires_at > ?",
        )
        .bind(&token)
        .bind(now_iso())
        .fetch_optional(&st.db)
        .await?
        .ok_or_else(ApiError::unauthorized)?;
        Ok(AuthUser(user))
    }
}

type SessionResponse = (AppendHeaders<[(HeaderName, String); 1]>, Json<User>);

async fn start_session(st: &AppState, user_id: i64) -> ApiResult<SessionResponse> {
    let token = random_token();
    let expires = (Utc::now() + Duration::days(SESSION_DAYS))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    sqlx::query("INSERT INTO sessions (token, user_id, expires_at) VALUES (?, ?, ?)")
        .bind(&token)
        .bind(user_id)
        .bind(&expires)
        .execute(&st.db)
        .await?;
    let user = sqlx::query_as::<_, User>("SELECT id, email, name FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_one(&st.db)
        .await?;
    let cookie = format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        SESSION_DAYS * 24 * 3600
    );
    Ok((AppendHeaders([(header::SET_COOKIE, cookie)]), Json(user)))
}

#[derive(Deserialize)]
pub struct RegisterBody {
    pub name: String,
    pub email: String,
    pub password: String,
}

pub async fn register(
    State(st): State<AppState>,
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
    let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind(&email)
        .fetch_optional(&st.db)
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

    let user_id = sqlx::query(
        "INSERT INTO users (email, name, password_hash, ics_token) VALUES (?, ?, ?, ?)",
    )
    .bind(&email)
    .bind(&name)
    .bind(&hash)
    .bind(random_token())
    .execute(&st.db)
    .await?
    .last_insert_rowid();

    let project_id = sqlx::query(
        "INSERT INTO projects (name, color, owner_id, is_inbox) VALUES ('Inbox', 'slate', ?, 1)",
    )
    .bind(user_id)
    .execute(&st.db)
    .await?
    .last_insert_rowid();
    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES (?, ?, 'owner')")
        .bind(project_id)
        .bind(user_id)
        .execute(&st.db)
        .await?;

    start_session(&st, user_id).await
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
}

pub async fn login(
    State(st): State<AppState>,
    Json(b): Json<LoginBody>,
) -> ApiResult<SessionResponse> {
    let email = b.email.trim().to_lowercase();
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT id, password_hash FROM users WHERE email = ?")
            .bind(&email)
            .fetch_optional(&st.db)
            .await?;
    let (user_id, hash) = row.ok_or_else(|| ApiError::bad_request("invalid email or password"))?;
    let parsed =
        PasswordHash::new(&hash).map_err(|_| ApiError::internal("corrupt password hash"))?;
    Argon2::default()
        .verify_password(b.password.as_bytes(), &parsed)
        .map_err(|_| ApiError::bad_request("invalid email or password"))?;
    start_session(&st, user_id).await
}

pub async fn logout(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<AppendHeaders<[(HeaderName, String); 1]>> {
    if let Some(token) = cookie_token(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token = ?")
            .bind(&token)
            .execute(&st.db)
            .await?;
    }
    let cookie = format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    Ok(AppendHeaders([(header::SET_COOKIE, cookie)]))
}

pub async fn me(AuthUser(user): AuthUser) -> Json<User> {
    Json(user)
}
