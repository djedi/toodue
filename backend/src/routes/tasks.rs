use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{NaiveDate, NaiveTime};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;

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

pub async fn fetch_task(db: &SqlitePool, id: i64) -> ApiResult<Task> {
    let sql = format!("SELECT {TASK_COLS} FROM tasks t WHERE t.id = ?");
    sqlx::query_as::<_, Task>(&sql)
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
    let recipients = project_recipients(&st.db, task.project_id).await;
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
        require_member(&st.db, user.id, project_id).await?;
        if q.completed.unwrap_or(false) {
            let sql = format!(
                "SELECT {TASK_COLS} FROM tasks t WHERE t.project_id = ? AND t.completed_at IS NOT NULL \
                 ORDER BY t.completed_at DESC LIMIT 200"
            );
            sqlx::query_as::<_, Task>(&sql)
                .bind(project_id)
                .fetch_all(&st.db)
                .await?
        } else {
            let sql = format!(
                "SELECT {TASK_COLS} FROM tasks t WHERE t.project_id = ? AND t.completed_at IS NULL \
                 ORDER BY t.sort_order, t.id"
            );
            sqlx::query_as::<_, Task>(&sql)
                .bind(project_id)
                .fetch_all(&st.db)
                .await?
        }
    } else {
        let sql = format!(
            "SELECT {TASK_COLS} FROM tasks t WHERE t.completed_at IS NULL \
             AND t.project_id IN (SELECT project_id FROM project_members WHERE user_id = ?) \
             ORDER BY t.sort_order, t.id"
        );
        sqlx::query_as::<_, Task>(&sql)
            .bind(user.id)
            .fetch_all(&st.db)
            .await?
    };
    Ok(Json(tasks))
}

pub async fn detail(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let task = fetch_task(&st.db, id).await?;
    require_member(&st.db, user.id, task.project_id).await?;

    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks t WHERE t.parent_id = ? \
         ORDER BY t.completed_at IS NOT NULL, t.sort_order, t.id"
    );
    let subtasks = sqlx::query_as::<_, Task>(&sql)
        .bind(id)
        .fetch_all(&st.db)
        .await?;

    let comments = sqlx::query_as::<_, Comment>(
        "SELECT c.id, c.task_id, c.user_id, u.name AS user_name, c.body, c.created_at \
         FROM comments c JOIN users u ON u.id = c.user_id WHERE c.task_id = ? ORDER BY c.created_at",
    )
    .bind(id)
    .fetch_all(&st.db)
    .await?;

    let attachments = sqlx::query_as::<_, Attachment>(
        "SELECT id, task_id, user_id, filename, mime, size, created_at \
         FROM attachments WHERE task_id = ? ORDER BY created_at",
    )
    .bind(id)
    .fetch_all(&st.db)
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
        Some(pid) => Some(fetch_task(&st.db, pid).await?),
        None => None,
    };
    let project_id = match &parent {
        Some(p) => p.project_id,
        None => b
            .project_id
            .ok_or_else(|| ApiError::bad_request("project_id is required"))?,
    };
    require_member(&st.db, user.id, project_id).await?;

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

    let id = sqlx::query(
        "INSERT INTO tasks (project_id, parent_id, creator_id, name, description, due_date, due_time, deadline, priority, sort_order) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM tasks WHERE project_id = ?))",
    )
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
    .execute(&st.db)
    .await?
    .last_insert_rowid();

    let task = fetch_task(&st.db, id).await?;
    publish_task(&st, &task).await;
    if let Some(p) = &parent {
        if let Ok(fresh) = fetch_task(&st.db, p.id).await {
            publish_task(&st, &fresh).await;
        }
    }
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
    let task = fetch_task(&st.db, id).await?;
    require_member(&st.db, user.id, task.project_id).await?;

    let mut moved_from: Option<i64> = None;
    if let Some(new_pid) = b.project_id {
        if new_pid != task.project_id {
            require_member(&st.db, user.id, new_pid).await?;
            moved_from = Some(task.project_id);
        }
    }

    let now = now_iso();
    let mut qb = sqlx::QueryBuilder::new("UPDATE tasks SET updated_at = ");
    qb.push_bind(&now);
    if let Some(name) = &b.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::bad_request("task name cannot be empty"));
        }
        qb.push(", name = ").push_bind(name.to_string());
    }
    if let Some(description) = &b.description {
        qb.push(", description = ").push_bind(description);
    }
    if let Some(due_date) = &b.due_date {
        if let Some(d) = due_date {
            validate_date(d)?;
        }
        qb.push(", due_date = ").push_bind(due_date.clone());
        if due_date.is_none() {
            qb.push(", due_time = NULL");
        }
    }
    if let Some(due_time) = &b.due_time {
        if let Some(t) = due_time {
            validate_time(t)?;
        }
        qb.push(", due_time = ").push_bind(due_time.clone());
    }
    if let Some(deadline) = &b.deadline {
        if let Some(d) = deadline {
            validate_date(d)?;
        }
        qb.push(", deadline = ").push_bind(deadline.clone());
    }
    if let Some(priority) = b.priority {
        qb.push(", priority = ").push_bind(priority.clamp(1, 4));
    }
    let completing = b.completed == Some(true) && task.completed_at.is_none();
    if let Some(completed) = b.completed {
        if completed {
            qb.push(", completed_at = COALESCE(completed_at, ")
                .push_bind(&now)
                .push(")");
        } else {
            qb.push(", completed_at = NULL");
        }
    }
    if let Some(new_pid) = b.project_id {
        qb.push(", project_id = ").push_bind(new_pid);
    }
    qb.push(" WHERE id = ").push_bind(id);
    qb.build().execute(&st.db).await?;

    // Completing a parent completes its remaining subtasks, like Todoist.
    let mut completed_subtasks: Vec<i64> = Vec::new();
    if completing {
        let rows: Vec<(i64,)> =
            sqlx::query_as("SELECT id FROM tasks WHERE parent_id = ? AND completed_at IS NULL")
                .bind(id)
                .fetch_all(&st.db)
                .await?;
        completed_subtasks = rows.into_iter().map(|r| r.0).collect();
        sqlx::query("UPDATE tasks SET completed_at = ?, updated_at = ? WHERE parent_id = ? AND completed_at IS NULL")
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(&st.db)
            .await?;
    }

    // Subtasks follow their parent to a new project.
    if moved_from.is_some() {
        sqlx::query("UPDATE tasks SET project_id = (SELECT project_id FROM tasks WHERE id = ?) WHERE parent_id = ?")
            .bind(id)
            .bind(id)
            .execute(&st.db)
            .await?;
    }

    let task = fetch_task(&st.db, id).await?;

    if let Some(old_pid) = moved_from {
        // A cross-project move changes visibility for two member sets; a full
        // refetch on both sides is simpler than diffing them.
        let mut recipients = project_recipients(&st.db, old_pid).await;
        recipients.extend(project_recipients(&st.db, task.project_id).await);
        recipients.sort_unstable();
        recipients.dedup();
        st.hub.publish(recipients, "tasks.refresh", json!({}));
    } else {
        publish_task(&st, &task).await;
        for sid in completed_subtasks {
            if let Ok(sub) = fetch_task(&st.db, sid).await {
                publish_task(&st, &sub).await;
            }
        }
        if b.completed.is_some() {
            if let Some(pid) = task.parent_id {
                if let Ok(parent) = fetch_task(&st.db, pid).await {
                    publish_task(&st, &parent).await;
                }
            }
        }
    }
    Ok(Json(task))
}

pub async fn remove(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let task = fetch_task(&st.db, id).await?;
    require_member(&st.db, user.id, task.project_id).await?;
    let recipients = project_recipients(&st.db, task.project_id).await;
    sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(id)
        .execute(&st.db)
        .await?;
    st.hub
        .publish(recipients, "task.remove", json!({ "id": id }));
    if let Some(pid) = task.parent_id {
        if let Ok(parent) = fetch_task(&st.db, pid).await {
            publish_task(&st, &parent).await;
        }
    }
    Ok(Json(json!({ "ok": true })))
}
