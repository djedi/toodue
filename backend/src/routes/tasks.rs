use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{NaiveDate, NaiveTime};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::AnyPool;

use crate::auth::{now_iso, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::events::project_recipients;
use crate::models::{Attachment, Comment, Task};
use crate::routes::projects::require_member;
use crate::AppState;

pub const TASK_COLS: &str = "t.*, \
    (SELECT COUNT(*) FROM comments c WHERE c.task_id = t.id) AS comment_count, \
    (SELECT COUNT(*) FROM attachments a WHERE a.task_id = t.id) AS attachment_count, \
    (SELECT COUNT(*) FROM tasks s WHERE s.parent_id = t.id) AS subtask_count, \
    (SELECT COUNT(*) FROM tasks s WHERE s.parent_id = t.id AND s.completed_at IS NOT NULL) AS subtask_done_count";

pub async fn fetch_task(db: &AnyPool, id: i64) -> ApiResult<Task> {
    let sql = format!("SELECT {TASK_COLS} FROM tasks t WHERE t.id = ?");
    sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(ApiError::not_found)
}

fn validate_date(s: &str) -> ApiResult<()> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| ApiError::bad_request("dates must be YYYY-MM-DD"))
}

fn validate_time(s: &str) -> ApiResult<()> {
    NaiveTime::parse_from_str(s, "%H:%M")
        .map(|_| ())
        .map_err(|_| ApiError::bad_request("times must be HH:MM"))
}

async fn publish_task(st: &AppState, task: &Task) {
    let recipients = project_recipients(&st.db.pool, task.project_id).await;
    st.hub.publish(
        recipients,
        "task.upsert",
        serde_json::to_value(task).unwrap(),
    );
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub completed: Option<bool>,
}

pub async fn list(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Task>>> {
    let tasks = if let Some(project_id) = q.project_id {
        require_member(&st.db.pool, user.id, project_id).await?;
        if q.completed.unwrap_or(false) {
            let sql = format!(
                "SELECT {TASK_COLS} FROM tasks t WHERE t.project_id = ? AND t.completed_at IS NOT NULL \
                 ORDER BY t.completed_at DESC LIMIT 200"
            );
            sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
                .bind(project_id)
                .fetch_all(&st.db.pool)
                .await?
        } else {
            let sql = format!(
                "SELECT {TASK_COLS} FROM tasks t WHERE t.project_id = ? AND t.completed_at IS NULL \
                 ORDER BY t.sort_order, t.id"
            );
            sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
                .bind(project_id)
                .fetch_all(&st.db.pool)
                .await?
        }
    } else {
        let sql = format!(
            "SELECT {TASK_COLS} FROM tasks t WHERE t.completed_at IS NULL \
             AND t.project_id IN (SELECT project_id FROM project_members WHERE user_id = ?) \
             ORDER BY t.sort_order, t.id"
        );
        sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
            .bind(user.id)
            .fetch_all(&st.db.pool)
            .await?
    };
    Ok(Json(tasks))
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default)]
    pub limit: Option<i64>,
}

pub async fn search(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
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
    let tasks = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(user.id)
        .bind(&pattern)
        .bind(&pattern)
        .bind(q.limit.unwrap_or(25).clamp(1, 100))
        .fetch_all(&st.db.pool)
        .await?;
    Ok(Json(tasks))
}

pub async fn detail(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let task = fetch_task(&st.db.pool, id).await?;
    require_member(&st.db.pool, user.id, task.project_id).await?;

    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks t WHERE t.parent_id = ? \
         ORDER BY t.completed_at IS NOT NULL, t.sort_order, t.id"
    );
    let subtasks = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(id)
        .fetch_all(&st.db.pool)
        .await?;

    let comments = sqlx::query_as::<_, Comment>(&*crate::db::sql(
        "SELECT c.id, c.task_id, c.user_id, u.name AS user_name, c.body, c.created_at \
         FROM comments c JOIN users u ON u.id = c.user_id WHERE c.task_id = ? ORDER BY c.created_at",
    ))
    .bind(id)
    .fetch_all(&st.db.pool)
    .await?;

    let attachments = sqlx::query_as::<_, Attachment>(&*crate::db::sql(
        "SELECT id, task_id, user_id, filename, mime, size, created_at \
         FROM attachments WHERE task_id = ? ORDER BY created_at",
    ))
    .bind(id)
    .fetch_all(&st.db.pool)
    .await?;

    Ok(Json(json!({
        "task": task,
        "subtasks": subtasks,
        "comments": comments,
        "attachments": attachments,
    })))
}

#[derive(Deserialize)]
pub struct CreateTask {
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub parent_id: Option<i64>,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub due_time: Option<String>,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
}

pub async fn create(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(b): Json<CreateTask>,
) -> ApiResult<Json<Task>> {
    let name = b.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("task name is required"));
    }
    let parent = match b.parent_id {
        Some(pid) => Some(fetch_task(&st.db.pool, pid).await?),
        None => None,
    };
    let project_id = match &parent {
        Some(p) => p.project_id,
        None => b
            .project_id
            .ok_or_else(|| ApiError::bad_request("project_id is required"))?,
    };
    require_member(&st.db.pool, user.id, project_id).await?;

    if let Some(d) = &b.due_date {
        validate_date(d)?;
    }
    if let Some(t) = &b.due_time {
        validate_time(t)?;
    }
    if let Some(d) = &b.deadline {
        validate_date(d)?;
    }
    let priority = b.priority.unwrap_or(4).clamp(1, 4);

    let (id,): (i64,) = sqlx::query_as(&*crate::db::sql(
        "INSERT INTO tasks (project_id, parent_id, creator_id, name, description, due_date, due_time, deadline, priority, sort_order) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM tasks WHERE project_id = ?)) RETURNING id",
    ))
    .bind(project_id)
    .bind(b.parent_id)
    .bind(user.id)
    .bind(&name)
    .bind(b.description.trim())
    .bind(&b.due_date)
    .bind(&b.due_time)
    .bind(&b.deadline)
    .bind(priority)
    .bind(project_id)
    .fetch_one(&st.db.pool)
    .await?;

    let task = fetch_task(&st.db.pool, id).await?;
    publish_task(&st, &task).await;
    if let Some(p) = &parent {
        if let Ok(fresh) = fetch_task(&st.db.pool, p.id).await {
            publish_task(&st, &fresh).await;
        }
    }
    crate::gcal::spawn_task_sync(&st, task.id);
    Ok(Json(task))
}

#[derive(Deserialize)]
pub struct UpdateTask {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub due_date: Option<Option<String>>,
    #[serde(default)]
    pub due_time: Option<Option<String>>,
    #[serde(default)]
    pub deadline: Option<Option<String>>,
    pub priority: Option<i64>,
    pub completed: Option<bool>,
    pub project_id: Option<i64>,
}

pub async fn update(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    Json(b): Json<UpdateTask>,
) -> ApiResult<Json<Task>> {
    let task = fetch_task(&st.db.pool, id).await?;
    require_member(&st.db.pool, user.id, task.project_id).await?;

    let mut moved_from: Option<i64> = None;
    if let Some(new_pid) = b.project_id {
        if new_pid != task.project_id {
            require_member(&st.db.pool, user.id, new_pid).await?;
            moved_from = Some(task.project_id);
        }
    }

    let now = now_iso();
    let mut new_name = task.name.clone();
    if let Some(name) = &b.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::bad_request("task name cannot be empty"));
        }
        new_name = name.to_string();
    }
    let new_description = b
        .description
        .clone()
        .unwrap_or_else(|| task.description.clone());
    let mut new_due_date = task.due_date.clone();
    let mut new_due_time = task.due_time.clone();
    let mut new_deadline = task.deadline.clone();
    let new_priority = b.priority.map(|p| p.clamp(1, 4)).unwrap_or(task.priority);
    let mut new_project_id = task.project_id;

    if let Some(due_date) = &b.due_date {
        if let Some(d) = due_date {
            validate_date(d)?;
        }
        new_due_date = due_date.clone();
        if due_date.is_none() {
            new_due_time = None;
        }
    }
    if let Some(due_time) = &b.due_time {
        if let Some(t) = due_time {
            validate_time(t)?;
        }
        new_due_time = due_time.clone();
    }
    if let Some(deadline) = &b.deadline {
        if let Some(d) = deadline {
            validate_date(d)?;
        }
        new_deadline = deadline.clone();
    }
    let completing = b.completed == Some(true) && task.completed_at.is_none();
    let new_completed_at = match b.completed {
        Some(true) => task.completed_at.clone().or_else(|| Some(now.clone())),
        Some(false) => None,
        None => task.completed_at.clone(),
    };
    if let Some(pid) = b.project_id {
        new_project_id = pid;
    }

    sqlx::query(&*crate::db::sql(
        "UPDATE tasks SET updated_at = ?, name = ?, description = ?, due_date = ?, due_time = ?, deadline = ?, priority = ?, completed_at = ?, project_id = ? WHERE id = ?",
    ))
    .bind(&now)
    .bind(new_name)
    .bind(new_description)
    .bind(new_due_date)
    .bind(new_due_time)
    .bind(new_deadline)
    .bind(new_priority)
    .bind(new_completed_at)
    .bind(new_project_id)
    .bind(id)
    .execute(&st.db.pool)
    .await?;

    // Completing a parent completes its remaining subtasks, like Todoist.
    let mut completed_subtasks: Vec<i64> = Vec::new();
    if completing {
        let rows: Vec<(i64,)> = sqlx::query_as(&*crate::db::sql(
            "SELECT id FROM tasks WHERE parent_id = ? AND completed_at IS NULL",
        ))
        .bind(id)
        .fetch_all(&st.db.pool)
        .await?;
        completed_subtasks = rows.into_iter().map(|r| r.0).collect();
        sqlx::query(&*crate::db::sql("UPDATE tasks SET completed_at = ?, updated_at = ? WHERE parent_id = ? AND completed_at IS NULL"))
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(&st.db.pool)
            .await?;
    }

    // Subtasks follow their parent to a new project.
    if moved_from.is_some() {
        sqlx::query(&*crate::db::sql("UPDATE tasks SET project_id = (SELECT project_id FROM tasks WHERE id = ?) WHERE parent_id = ?"))
            .bind(id)
            .bind(id)
            .execute(&st.db.pool)
            .await?;
    }

    let task = fetch_task(&st.db.pool, id).await?;

    if let Some(old_pid) = moved_from {
        // A cross-project move changes visibility for two member sets; a full
        // refetch on both sides is simpler than diffing them.
        let mut recipients = project_recipients(&st.db.pool, old_pid).await;
        recipients.extend(project_recipients(&st.db.pool, task.project_id).await);
        recipients.sort_unstable();
        recipients.dedup();
        st.hub.publish(recipients, "tasks.refresh", json!({}));
    } else {
        publish_task(&st, &task).await;
        for sid in &completed_subtasks {
            if let Ok(sub) = fetch_task(&st.db.pool, *sid).await {
                publish_task(&st, &sub).await;
            }
        }
        if b.completed.is_some() {
            if let Some(pid) = task.parent_id {
                if let Ok(parent) = fetch_task(&st.db.pool, pid).await {
                    publish_task(&st, &parent).await;
                }
            }
        }
    }

    crate::gcal::spawn_task_sync(&st, id);
    for sid in &completed_subtasks {
        crate::gcal::spawn_task_sync(&st, *sid);
    }
    if moved_from.is_some() {
        let subs: Vec<(i64,)> =
            sqlx::query_as(&*crate::db::sql("SELECT id FROM tasks WHERE parent_id = ?"))
                .bind(id)
                .fetch_all(&st.db.pool)
                .await?;
        for (sid,) in subs {
            crate::gcal::spawn_task_sync(&st, sid);
        }
    }
    Ok(Json(task))
}

pub async fn remove(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let task = fetch_task(&st.db.pool, id).await?;
    require_member(&st.db.pool, user.id, task.project_id).await?;
    let recipients = project_recipients(&st.db.pool, task.project_id).await;
    sqlx::query(&*crate::db::sql("DELETE FROM tasks WHERE id = ?"))
        .bind(id)
        .execute(&st.db.pool)
        .await?;
    st.hub
        .publish(recipients, "task.remove", json!({ "id": id }));
    if let Some(pid) = task.parent_id {
        if let Ok(parent) = fetch_task(&st.db.pool, pid).await {
            publish_task(&st, &parent).await;
        }
    }
    // Removes the deleted task's calendar events, plus any cascade-deleted
    // subtasks' events.
    crate::gcal::spawn_task_sync(&st, id);
    crate::gcal::spawn_orphan_cleanup(&st);
    Ok(Json(json!({ "ok": true })))
}
