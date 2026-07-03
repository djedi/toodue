use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::events::project_recipients;
use crate::models::Comment;
use crate::routes::projects::require_member;
use crate::routes::tasks::fetch_task;
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateComment {
    pub body: String,
}

pub async fn create(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(task_id): Path<i64>,
    Json(b): Json<CreateComment>,
) -> ApiResult<Json<Comment>> {
    let body = b.body.trim().to_string();
    if body.is_empty() {
        return Err(ApiError::bad_request("comment cannot be empty"));
    }
    let task = fetch_task(&st.db.pool, task_id).await?;
    require_member(&st.db.pool, user.id, task.project_id).await?;

    let (id,): (i64,) = sqlx::query_as(&*crate::db::sql(
        "INSERT INTO comments (task_id, user_id, body) VALUES (?, ?, ?) RETURNING id",
    ))
    .bind(task_id)
    .bind(user.id)
    .bind(&body)
    .fetch_one(&st.db.pool)
    .await?;

    let comment = sqlx::query_as::<_, Comment>(&*crate::db::sql(
        "SELECT c.id, c.task_id, c.user_id, u.name AS user_name, c.body, c.created_at \
         FROM comments c JOIN users u ON u.id = c.user_id WHERE c.id = ?",
    ))
    .bind(id)
    .fetch_one(&st.db.pool)
    .await?;

    let recipients = project_recipients(&st.db.pool, task.project_id).await;
    st.hub.publish(
        recipients.clone(),
        "comment.new",
        serde_json::to_value(&comment).unwrap(),
    );
    if let Ok(fresh) = fetch_task(&st.db.pool, task_id).await {
        st.hub.publish(
            recipients,
            "task.upsert",
            serde_json::to_value(&fresh).unwrap(),
        );
    }
    Ok(Json(comment))
}

pub async fn remove(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let row: Option<(i64, i64)> = sqlx::query_as(&*crate::db::sql(
        "SELECT user_id, task_id FROM comments WHERE id = ?",
    ))
    .bind(id)
    .fetch_optional(&st.db.pool)
    .await?;
    let (author_id, task_id) = row.ok_or_else(ApiError::not_found)?;
    if author_id != user.id {
        return Err(ApiError::forbidden());
    }
    let task = fetch_task(&st.db.pool, task_id).await?;
    sqlx::query(&*crate::db::sql("DELETE FROM comments WHERE id = ?"))
        .bind(id)
        .execute(&st.db.pool)
        .await?;

    let recipients = project_recipients(&st.db.pool, task.project_id).await;
    st.hub.publish(
        recipients.clone(),
        "comment.remove",
        json!({ "id": id, "task_id": task_id }),
    );
    if let Ok(fresh) = fetch_task(&st.db.pool, task_id).await {
        st.hub.publish(
            recipients,
            "task.upsert",
            serde_json::to_value(&fresh).unwrap(),
        );
    }
    Ok(Json(json!({ "ok": true })))
}
