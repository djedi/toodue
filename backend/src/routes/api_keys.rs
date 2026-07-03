use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth::{hash_token, random_token, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::AppState;

#[derive(Serialize, sqlx::FromRow)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub name: String,
    pub prefix: String,
    pub last_used_at: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct CreatedApiKey {
    pub api_key: ApiKeyInfo,
    pub key: String,
}

#[derive(Deserialize)]
pub struct CreateApiKey {
    pub name: String,
}

pub async fn list(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Vec<ApiKeyInfo>>> {
    let keys = sqlx::query_as::<_, ApiKeyInfo>(
        "SELECT id, name, prefix, last_used_at, created_at FROM api_keys WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(user.id)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(keys))
}

pub async fn create(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(b): Json<CreateApiKey>,
) -> ApiResult<Json<CreatedApiKey>> {
    let name = b.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("API key name is required"));
    }
    let key = format!("tdue_{}", random_token());
    let prefix = key.chars().take(12).collect::<String>();
    let token_hash = hash_token(&key);
    let id =
        sqlx::query("INSERT INTO api_keys (user_id, name, prefix, token_hash) VALUES (?, ?, ?, ?)")
            .bind(user.id)
            .bind(name)
            .bind(&prefix)
            .bind(&token_hash)
            .execute(&st.db)
            .await?
            .last_insert_rowid();
    let api_key = sqlx::query_as::<_, ApiKeyInfo>(
        "SELECT id, name, prefix, last_used_at, created_at FROM api_keys WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(CreatedApiKey { api_key, key }))
}

pub async fn remove(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let result = sqlx::query("DELETE FROM api_keys WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user.id)
        .execute(&st.db)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
