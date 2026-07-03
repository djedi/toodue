use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::ApiUser;
use crate::error::{ApiError, ApiResult};
use crate::models::Task;
use crate::routes::projects::{project_json, require_member, CreateProject};
use crate::routes::tasks::{CreateTask, ListQuery, UpdateTask, TASK_COLS};
use crate::AppState;

pub async fn me(ApiUser(user): ApiUser) -> Json<Value> {
    Json(json!({ "id": user.id, "email": user.email, "name": user.name }))
}

pub async fn list_projects(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
) -> ApiResult<Json<Value>> {
    crate::routes::projects::list(State(st), crate::auth::AuthUser(user)).await
}

pub async fn create_project(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Json(b): Json<CreateProject>,
) -> ApiResult<Json<Value>> {
    crate::routes::projects::create(State(st), crate::auth::AuthUser(user), Json(b)).await
}

pub async fn list_tasks(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Task>>> {
    crate::routes::tasks::list(State(st), crate::auth::AuthUser(user), Query(q)).await
}

pub async fn create_task(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Json(b): Json<CreateTask>,
) -> ApiResult<Json<Task>> {
    crate::routes::tasks::create(State(st), crate::auth::AuthUser(user), Json(b)).await
}

pub async fn update_task(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
    Json(b): Json<UpdateTask>,
) -> ApiResult<Json<Task>> {
    crate::routes::tasks::update(State(st), crate::auth::AuthUser(user), Path(id), Json(b)).await
}

pub async fn delete_task(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    crate::routes::tasks::remove(State(st), crate::auth::AuthUser(user), Path(id)).await
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn search_tasks(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<Vec<Task>>> {
    let needle = q.q.trim();
    if needle.is_empty() {
        return Err(ApiError::bad_request("q is required"));
    }
    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks t WHERE t.project_id IN \
         (SELECT project_id FROM project_members WHERE user_id = ?) \
         AND t.completed_at IS NULL AND (t.name LIKE ? OR t.description LIKE ?) \
         ORDER BY t.updated_at DESC LIMIT ?"
    );
    let pattern = format!("%{needle}%");
    let tasks = sqlx::query_as::<_, Task>(&sql)
        .bind(user.id)
        .bind(&pattern)
        .bind(&pattern)
        .bind(q.limit.unwrap_or(25).clamp(1, 100))
        .fetch_all(&st.db)
        .await?;
    Ok(Json(tasks))
}

pub async fn task_detail(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    crate::routes::tasks::detail(State(st), crate::auth::AuthUser(user), Path(id)).await
}

pub async fn project_detail(
    State(st): State<AppState>,
    ApiUser(user): ApiUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    require_member(&st.db, user.id, id).await?;
    Ok(Json(project_json(&st.db, id).await?))
}
