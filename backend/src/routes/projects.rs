use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::AnyPool;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::events::project_recipients;
use crate::models::{Member, Project};
use crate::AppState;

const PROJECT_COLS: &str = "p.*, \
    (SELECT COUNT(*) FROM tasks t WHERE t.project_id = p.id AND t.completed_at IS NULL) AS active_count";

pub async fn require_member(db: &AnyPool, user_id: i64, project_id: i64) -> ApiResult<()> {
    let row: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT 1 FROM project_members WHERE project_id = ? AND user_id = ?",
    ))
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    row.map(|_| ()).ok_or_else(ApiError::forbidden)
}

async fn members_of(db: &AnyPool, project_id: i64) -> ApiResult<Vec<Member>> {
    Ok(sqlx::query_as::<_, Member>(&*crate::db::sql(
        "SELECT u.id, u.name, u.email, m.role, m.project_id FROM project_members m \
         JOIN users u ON u.id = m.user_id WHERE m.project_id = ? ORDER BY m.role DESC, u.name",
    ))
    .bind(project_id)
    .fetch_all(db)
    .await?)
}

pub async fn project_json(db: &AnyPool, project_id: i64) -> ApiResult<Value> {
    let sql = format!("SELECT {PROJECT_COLS} FROM projects p WHERE p.id = ?");
    let project = sqlx::query_as::<_, Project>(&*crate::db::sql(&sql))
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
    let projects = sqlx::query_as::<_, Project>(&*crate::db::sql(&sql))
        .bind(user.id)
        .fetch_all(&st.db.pool)
        .await?;

    let mut out = Vec::with_capacity(projects.len());
    for p in &projects {
        let members = members_of(&st.db.pool, p.id).await?;
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
        require_member(&st.db.pool, user.id, parent_id).await?;
    }
    let color = b.color.unwrap_or_else(|| "slate".into());
    let (id,): (i64,) = sqlx::query_as(&*crate::db::sql(
        "INSERT INTO projects (name, color, parent_id, owner_id, sort_order) \
         VALUES (?, ?, ?, ?, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM projects)) RETURNING id",
    ))
    .bind(&name)
    .bind(&color)
    .bind(b.parent_id)
    .bind(user.id)
    .fetch_one(&st.db.pool)
    .await?;
    sqlx::query(&*crate::db::sql(
        "INSERT INTO project_members (project_id, user_id, role) VALUES (?, ?, 'owner')",
    ))
    .bind(id)
    .bind(user.id)
    .execute(&st.db.pool)
    .await?;

    let v = project_json(&st.db.pool, id).await?;
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
    require_member(&st.db.pool, user.id, id).await?;

    let current_sql = format!("SELECT {PROJECT_COLS} FROM projects p WHERE p.id = ?");
    let current = sqlx::query_as::<_, Project>(&*crate::db::sql(&current_sql))
        .bind(id)
        .fetch_optional(&st.db.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;

    let new_name = if let Some(name) = &b.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::bad_request("project name cannot be empty"));
        }
        name.to_string()
    } else {
        current.name
    };
    let new_color = b.color.unwrap_or(current.color);
    let new_parent_id = if let Some(parent_id) = &b.parent_id {
        if let Some(pid) = parent_id {
            if *pid == id {
                return Err(ApiError::bad_request("a project cannot be its own parent"));
            }
            require_member(&st.db.pool, user.id, *pid).await?;
            if is_descendant(&st.db.pool, id, *pid).await? {
                return Err(ApiError::bad_request(
                    "that would create a loop of nested projects",
                ));
            }
        }
        *parent_id
    } else {
        current.parent_id
    };

    sqlx::query(&*crate::db::sql(
        "UPDATE projects SET name = ?, color = ?, parent_id = ? WHERE id = ?",
    ))
    .bind(new_name)
    .bind(new_color)
    .bind(new_parent_id)
    .bind(id)
    .execute(&st.db.pool)
    .await?;

    let v = project_json(&st.db.pool, id).await?;
    let recipients = project_recipients(&st.db.pool, id).await;
    st.hub.publish(recipients, "project.upsert", v.clone());
    Ok(Json(v))
}

/// True if `node` sits anywhere inside `ancestor`'s subtree (walking parents up).
async fn is_descendant(db: &AnyPool, ancestor: i64, node: i64) -> ApiResult<bool> {
    let mut cur = node;
    for _ in 0..100 {
        let row: Option<(Option<i64>,)> = sqlx::query_as(&*crate::db::sql(
            "SELECT parent_id FROM projects WHERE id = ?",
        ))
        .bind(cur)
        .fetch_optional(db)
        .await?;
        match row.and_then(|r| r.0) {
            Some(p) if p == ancestor => return Ok(true),
            Some(p) => cur = p,
            None => return Ok(false),
        }
    }
    Ok(true) // absurdly deep chain — treat as a cycle
}

#[derive(Deserialize)]
pub struct ReorderItem {
    pub id: i64,
    #[serde(default)]
    pub parent_id: Option<i64>,
    pub sort_order: i64,
}

#[derive(Deserialize)]
pub struct ReorderBody {
    pub items: Vec<ReorderItem>,
}

/// Applies a full drag-and-drop rearrangement: new parent and sort order for
/// every project the client sends, atomically.
pub async fn reorder(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(b): Json<ReorderBody>,
) -> ApiResult<Json<Value>> {
    if b.items.is_empty() {
        return Ok(Json(json!({ "ok": true })));
    }
    if b.items.len() > 500 {
        return Err(ApiError::bad_request("too many projects in one reorder"));
    }
    let ids: HashSet<i64> = b.items.iter().map(|i| i.id).collect();
    if ids.len() != b.items.len() {
        return Err(ApiError::bad_request("duplicate project ids"));
    }
    for item in &b.items {
        require_member(&st.db.pool, user.id, item.id).await?;
        let (is_inbox,): (i64,) = sqlx::query_as(&*crate::db::sql(
            "SELECT is_inbox FROM projects WHERE id = ?",
        ))
        .bind(item.id)
        .fetch_one(&st.db.pool)
        .await?;
        if is_inbox != 0 {
            return Err(ApiError::bad_request("the inbox cannot be moved"));
        }
        if let Some(p) = item.parent_id {
            if p == item.id {
                return Err(ApiError::bad_request("a project cannot be its own parent"));
            }
            require_member(&st.db.pool, user.id, p).await?;
        }
    }

    // Cycle check against the proposed parents, falling back to current DB
    // parents for projects outside the payload.
    let proposed: HashMap<i64, Option<i64>> = b.items.iter().map(|i| (i.id, i.parent_id)).collect();
    for item in &b.items {
        let mut seen = HashSet::from([item.id]);
        let mut cur = item.parent_id;
        while let Some(p) = cur {
            if !seen.insert(p) {
                return Err(ApiError::bad_request(
                    "that would create a loop of nested projects",
                ));
            }
            cur = match proposed.get(&p) {
                Some(v) => *v,
                None => sqlx::query_as::<_, (Option<i64>,)>(
                    "SELECT parent_id FROM projects WHERE id = ?",
                )
                .bind(p)
                .fetch_optional(&st.db.pool)
                .await?
                .and_then(|r| r.0),
            };
        }
    }

    let mut tx = st.db.pool.begin().await?;
    for item in &b.items {
        sqlx::query(&*crate::db::sql(
            "UPDATE projects SET parent_id = ?, sort_order = ? WHERE id = ?",
        ))
        .bind(item.parent_id)
        .bind(item.sort_order)
        .bind(item.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    let mut recipients = Vec::new();
    for id in &ids {
        recipients.extend(project_recipients(&st.db.pool, *id).await);
    }
    recipients.sort_unstable();
    recipients.dedup();
    st.hub.publish(recipients, "projects.refresh", json!({}));
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let row: Option<(i64, i64)> = sqlx::query_as(&*crate::db::sql(
        "SELECT owner_id, is_inbox FROM projects WHERE id = ?",
    ))
    .bind(id)
    .fetch_optional(&st.db.pool)
    .await?;
    let (owner_id, is_inbox) = row.ok_or_else(ApiError::not_found)?;
    if owner_id != user.id {
        return Err(ApiError::forbidden());
    }
    if is_inbox != 0 {
        return Err(ApiError::bad_request("the inbox cannot be deleted"));
    }
    let recipients = project_recipients(&st.db.pool, id).await;
    sqlx::query(&*crate::db::sql("DELETE FROM projects WHERE id = ?"))
        .bind(id)
        .execute(&st.db.pool)
        .await?;
    st.hub
        .publish(recipients, "project.remove", json!({ "id": id }));
    crate::gcal::spawn_orphan_cleanup(&st);
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ShareBody {
    pub email: String,
}

#[derive(Deserialize)]
pub struct BulkShareBody {
    pub email: String,
    pub project_ids: Vec<i64>,
}

#[derive(Serialize)]
pub struct BulkShareSkip {
    pub id: i64,
    pub reason: String,
}

pub async fn share(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    Json(b): Json<ShareBody>,
) -> ApiResult<Json<Value>> {
    require_member(&st.db.pool, user.id, id).await?;
    let row: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT is_inbox FROM projects WHERE id = ?",
    ))
    .bind(id)
    .fetch_optional(&st.db.pool)
    .await?;
    if row.ok_or_else(ApiError::not_found)?.0 != 0 {
        return Err(ApiError::bad_request("the inbox cannot be shared"));
    }

    let email = b.email.trim().to_lowercase();
    let target: Option<(i64,)> =
        sqlx::query_as(&*crate::db::sql("SELECT id FROM users WHERE email = ?"))
            .bind(&email)
            .fetch_optional(&st.db.pool)
            .await?;
    let (target_id,) = target.ok_or_else(|| {
        ApiError::bad_request("no TooDue account with that email — ask them to sign up first")
    })?;

    sqlx::query(&*crate::db::sql(
        "INSERT OR IGNORE INTO project_members (project_id, user_id, role) VALUES (?, ?, 'member')",
    ))
    .bind(id)
    .bind(target_id)
    .execute(&st.db.pool)
    .await?;

    let v = project_json(&st.db.pool, id).await?;
    let recipients = project_recipients(&st.db.pool, id).await;
    st.hub.publish(recipients, "project.upsert", v.clone());
    crate::gcal::spawn_project_sync(&st, id);
    Ok(Json(v))
}

pub async fn share_bulk(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(b): Json<BulkShareBody>,
) -> ApiResult<Json<Value>> {
    let email = b.email.trim().to_lowercase();
    if email.is_empty() {
        return Err(ApiError::bad_request("email is required"));
    }
    if b.project_ids.is_empty() {
        return Err(ApiError::bad_request(
            "choose at least one project to share",
        ));
    }
    if b.project_ids.len() > 500 {
        return Err(ApiError::bad_request("too many projects in one share"));
    }

    let target: Option<(i64,)> =
        sqlx::query_as(&*crate::db::sql("SELECT id FROM users WHERE email = ?"))
            .bind(&email)
            .fetch_optional(&st.db.pool)
            .await?;
    let (target_id,) = target.ok_or_else(|| {
        ApiError::bad_request("no TooDue account with that email — ask them to sign up first")
    })?;

    let mut shared = Vec::new();
    let mut already_shared = Vec::new();
    let mut skipped = Vec::new();

    for id in HashSet::<i64>::from_iter(b.project_ids) {
        let row: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
            "SELECT is_inbox FROM projects WHERE id = ?",
        ))
        .bind(id)
        .fetch_optional(&st.db.pool)
        .await?;
        let Some((is_inbox,)) = row else {
            skipped.push(BulkShareSkip {
                id,
                reason: "project not found".into(),
            });
            continue;
        };
        if is_inbox != 0 {
            skipped.push(BulkShareSkip {
                id,
                reason: "the inbox cannot be shared".into(),
            });
            continue;
        }
        if require_member(&st.db.pool, user.id, id).await.is_err() {
            skipped.push(BulkShareSkip {
                id,
                reason: "you don't have access to that project".into(),
            });
            continue;
        }

        let exists: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
            "SELECT 1 FROM project_members WHERE project_id = ? AND user_id = ?",
        ))
        .bind(id)
        .bind(target_id)
        .fetch_optional(&st.db.pool)
        .await?;
        if exists.is_some() {
            already_shared.push(id);
            continue;
        }

        sqlx::query(&*crate::db::sql(
            "INSERT INTO project_members (project_id, user_id, role) VALUES (?, ?, 'member')",
        ))
        .bind(id)
        .bind(target_id)
        .execute(&st.db.pool)
        .await?;
        shared.push(id);
    }

    let mut projects = Vec::new();
    for id in shared.iter().chain(already_shared.iter()) {
        if let Ok(v) = project_json(&st.db.pool, *id).await {
            projects.push(v.clone());
            let recipients = project_recipients(&st.db.pool, *id).await;
            st.hub.publish(recipients, "project.upsert", v);
        }
    }
    for id in &shared {
        crate::gcal::spawn_project_sync(&st, *id);
    }

    Ok(Json(json!({
        "shared": shared,
        "already_shared": already_shared,
        "skipped": skipped,
        "projects": projects,
    })))
}

pub async fn remove_member(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path((id, member_id)): Path<(i64, i64)>,
) -> ApiResult<Json<Value>> {
    let row: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT owner_id FROM projects WHERE id = ?",
    ))
    .bind(id)
    .fetch_optional(&st.db.pool)
    .await?;
    let (owner_id,) = row.ok_or_else(ApiError::not_found)?;
    if member_id == owner_id {
        return Err(ApiError::bad_request("the project owner cannot be removed"));
    }
    if user.id != owner_id && user.id != member_id {
        return Err(ApiError::forbidden());
    }
    require_member(&st.db.pool, member_id, id)
        .await
        .map_err(|_| ApiError::not_found())?;

    sqlx::query(&*crate::db::sql(
        "DELETE FROM project_members WHERE project_id = ? AND user_id = ?",
    ))
    .bind(id)
    .bind(member_id)
    .execute(&st.db.pool)
    .await?;

    st.hub
        .publish(vec![member_id], "project.remove", json!({ "id": id }));
    let v = project_json(&st.db.pool, id).await?;
    let recipients = project_recipients(&st.db.pool, id).await;
    st.hub.publish(recipients, "project.upsert", v.clone());
    crate::gcal::spawn_project_sync(&st, id);
    Ok(Json(v))
}
