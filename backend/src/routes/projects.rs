use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::events::project_recipients;
use crate::models::{Member, Project};
use crate::AppState;

const PROJECT_COLS: &str = "p.*, \
    (SELECT COUNT(*) FROM tasks t WHERE t.project_id = p.id AND t.completed_at IS NULL) AS active_count";

pub async fn require_member(db: &SqlitePool, user_id: i64, project_id: i64) -> ApiResult<()> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM project_members WHERE project_id = ? AND user_id = ?")
            .bind(project_id)
            .bind(user_id)
            .fetch_optional(db)
            .await?;
    row.map(|_| ()).ok_or_else(ApiError::forbidden)
}

async fn members_of(db: &SqlitePool, project_id: i64) -> ApiResult<Vec<Member>> {
    Ok(sqlx::query_as::<_, Member>(
        "SELECT u.id, u.name, u.email, m.role, m.project_id FROM project_members m \
         JOIN users u ON u.id = m.user_id WHERE m.project_id = ? ORDER BY m.role DESC, u.name",
    )
    .bind(project_id)
    .fetch_all(db)
    .await?)
}

pub async fn project_json(db: &SqlitePool, project_id: i64) -> ApiResult<Value> {
    let sql = format!("SELECT {PROJECT_COLS} FROM projects p WHERE p.id = ?");
    let project = sqlx::query_as::<_, Project>(&sql)
        .bind(project_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let members = members_of(db, project_id).await?;
    let mut v = serde_json::to_value(&project).unwrap();
    v["members"] = serde_json::to_value(&members).unwrap();
    Ok(v)
}

pub async fn list(State(st): State<AppState>, AuthUser(user): AuthUser) -> ApiResult<Json<Value>> {
    let sql = format!(
        "SELECT {PROJECT_COLS} FROM projects p \
         JOIN project_members m ON m.project_id = p.id \
         WHERE m.user_id = ? ORDER BY p.is_inbox DESC, p.sort_order, p.id"
    );
    let projects = sqlx::query_as::<_, Project>(&sql)
        .bind(user.id)
        .fetch_all(&st.db)
        .await?;

    let mut out = Vec::with_capacity(projects.len());
    for p in &projects {
        let members = members_of(&st.db, p.id).await?;
        let mut v = serde_json::to_value(p).unwrap();
        v["members"] = serde_json::to_value(&members).unwrap();
        out.push(v);
    }
    Ok(Json(Value::Array(out)))
}

#[derive(Deserialize)]
pub struct CreateProject {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub parent_id: Option<i64>,
}

pub async fn create(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(b): Json<CreateProject>,
) -> ApiResult<Json<Value>> {
    let name = b.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("project name is required"));
    }
    if let Some(parent_id) = b.parent_id {
        require_member(&st.db, user.id, parent_id).await?;
    }
    let color = b.color.unwrap_or_else(|| "slate".into());
    let id = sqlx::query(
        "INSERT INTO projects (name, color, parent_id, owner_id, sort_order) \
         VALUES (?, ?, ?, ?, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM projects))",
    )
    .bind(&name)
    .bind(&color)
    .bind(b.parent_id)
    .bind(user.id)
    .execute(&st.db)
    .await?
    .last_insert_rowid();
    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES (?, ?, 'owner')")
        .bind(id)
        .bind(user.id)
        .execute(&st.db)
        .await?;

    let v = project_json(&st.db, id).await?;
    st.hub.publish(vec![user.id], "project.upsert", v.clone());
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub color: Option<String>,
    #[serde(default)]
    pub parent_id: Option<Option<i64>>,
}

pub async fn update(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    Json(b): Json<UpdateProject>,
) -> ApiResult<Json<Value>> {
    require_member(&st.db, user.id, id).await?;

    let mut qb = sqlx::QueryBuilder::new("UPDATE projects SET id = id");
    if let Some(name) = &b.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::bad_request("project name cannot be empty"));
        }
        qb.push(", name = ").push_bind(name.to_string());
    }
    if let Some(color) = &b.color {
        qb.push(", color = ").push_bind(color);
    }
    if let Some(parent_id) = &b.parent_id {
        if let Some(pid) = parent_id {
            if *pid == id {
                return Err(ApiError::bad_request("a project cannot be its own parent"));
            }
            require_member(&st.db, user.id, *pid).await?;
        }
        qb.push(", parent_id = ").push_bind(*parent_id);
    }
    qb.push(" WHERE id = ").push_bind(id);
    qb.build().execute(&st.db).await?;

    let v = project_json(&st.db, id).await?;
    let recipients = project_recipients(&st.db, id).await;
    st.hub.publish(recipients, "project.upsert", v.clone());
    Ok(Json(v))
}

pub async fn remove(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let row: Option<(i64, i64)> =
        sqlx::query_as("SELECT owner_id, is_inbox FROM projects WHERE id = ?")
            .bind(id)
            .fetch_optional(&st.db)
            .await?;
    let (owner_id, is_inbox) = row.ok_or_else(ApiError::not_found)?;
    if owner_id != user.id {
        return Err(ApiError::forbidden());
    }
    if is_inbox != 0 {
        return Err(ApiError::bad_request("the inbox cannot be deleted"));
    }
    let recipients = project_recipients(&st.db, id).await;
    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(&st.db)
        .await?;
    st.hub
        .publish(recipients, "project.remove", json!({ "id": id }));
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ShareBody {
    pub email: String,
}

pub async fn share(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    Json(b): Json<ShareBody>,
) -> ApiResult<Json<Value>> {
    require_member(&st.db, user.id, id).await?;
    let row: Option<(i64,)> = sqlx::query_as("SELECT is_inbox FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(&st.db)
        .await?;
    if row.ok_or_else(ApiError::not_found)?.0 != 0 {
        return Err(ApiError::bad_request("the inbox cannot be shared"));
    }

    let email = b.email.trim().to_lowercase();
    let target: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind(&email)
        .fetch_optional(&st.db)
        .await?;
    let (target_id,) = target.ok_or_else(|| {
        ApiError::bad_request("no TooDue account with that email — ask them to sign up first")
    })?;

    sqlx::query(
        "INSERT OR IGNORE INTO project_members (project_id, user_id, role) VALUES (?, ?, 'member')",
    )
    .bind(id)
    .bind(target_id)
    .execute(&st.db)
    .await?;

    let v = project_json(&st.db, id).await?;
    let recipients = project_recipients(&st.db, id).await;
    st.hub.publish(recipients, "project.upsert", v.clone());
    Ok(Json(v))
}

pub async fn remove_member(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path((id, member_id)): Path<(i64, i64)>,
) -> ApiResult<Json<Value>> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT owner_id FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(&st.db)
        .await?;
    let (owner_id,) = row.ok_or_else(ApiError::not_found)?;
    if member_id == owner_id {
        return Err(ApiError::bad_request("the project owner cannot be removed"));
    }
    if user.id != owner_id && user.id != member_id {
        return Err(ApiError::forbidden());
    }
    require_member(&st.db, member_id, id)
        .await
        .map_err(|_| ApiError::not_found())?;

    sqlx::query("DELETE FROM project_members WHERE project_id = ? AND user_id = ?")
        .bind(id)
        .bind(member_id)
        .execute(&st.db)
        .await?;

    st.hub
        .publish(vec![member_id], "project.remove", json!({ "id": id }));
    let v = project_json(&st.db, id).await?;
    let recipients = project_recipients(&st.db, id).await;
    st.hub.publish(recipients, "project.upsert", v.clone());
    Ok(Json(v))
}
