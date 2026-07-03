use axum::extract::{Multipart, Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::{random_token, AuthUser};
use crate::error::{ApiError, ApiResult};
use crate::events::project_recipients;
use crate::models::Attachment;
use crate::routes::projects::require_member;
use crate::routes::tasks::fetch_task;
use crate::AppState;

async fn fetch_attachment(db: &sqlx::AnyPool, id: i64) -> ApiResult<(Attachment, String)> {
    let row: Option<(i64, i64, i64, String, String, String, i64, String)> =
        sqlx::query_as(&*crate::db::sql(
            "SELECT id, task_id, user_id, filename, stored_name, mime, size, created_at \
         FROM attachments WHERE id = ?",
        ))
        .bind(id)
        .fetch_optional(db)
        .await?;
    let (id, task_id, user_id, filename, stored_name, mime, size, created_at) =
        row.ok_or_else(ApiError::not_found)?;
    Ok((
        Attachment {
            id,
            task_id,
            user_id,
            filename,
            mime,
            size,
            created_at,
        },
        stored_name,
    ))
}

async fn publish_attachment_change(st: &AppState, task_id: i64) {
    if let Ok(task) = fetch_task(&st.db.pool, task_id).await {
        let recipients = project_recipients(&st.db.pool, task.project_id).await;
        st.hub.publish(
            recipients,
            "task.upsert",
            serde_json::to_value(&task).unwrap(),
        );
    }
}

pub async fn upload(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(task_id): Path<i64>,
    mut multipart: Multipart,
) -> ApiResult<Json<Attachment>> {
    let task = fetch_task(&st.db.pool, task_id).await?;
    require_member(&st.db.pool, user.id, task.project_id).await?;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::bad_request("invalid upload"))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let filename = field.file_name().unwrap_or("file").to_string();
        let mime = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();
        let data = field
            .bytes()
            .await
            .map_err(|_| ApiError::bad_request("upload failed or file too large"))?;

        let stored_name = random_token();
        tokio::fs::write(st.data_dir.join("attachments").join(&stored_name), &data).await?;

        let (id,): (i64,) = sqlx::query_as(&*crate::db::sql(
            "INSERT INTO attachments (task_id, user_id, filename, stored_name, mime, size) \
             VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
        ))
        .bind(task_id)
        .bind(user.id)
        .bind(&filename)
        .bind(&stored_name)
        .bind(&mime)
        .bind(data.len() as i64)
        .fetch_one(&st.db.pool)
        .await?;

        let (attachment, _) = fetch_attachment(&st.db.pool, id).await?;
        let recipients = project_recipients(&st.db.pool, task.project_id).await;
        st.hub.publish(
            recipients,
            "attachment.new",
            serde_json::to_value(&attachment).unwrap(),
        );
        publish_attachment_change(&st, task_id).await;
        return Ok(Json(attachment));
    }
    Err(ApiError::bad_request(
        "expected a multipart field named \"file\"",
    ))
}

pub async fn download(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let (attachment, stored_name) = fetch_attachment(&st.db.pool, id).await?;
    let task = fetch_task(&st.db.pool, attachment.task_id).await?;
    require_member(&st.db.pool, user.id, task.project_id).await?;

    let bytes = tokio::fs::read(st.data_dir.join("attachments").join(&stored_name))
        .await
        .map_err(|_| ApiError::not_found())?;

    let safe_name: String = attachment
        .filename
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .filter(|c| *c != '"' && *c != '\\')
        .collect();
    Ok((
        [
            (header::CONTENT_TYPE, attachment.mime.clone()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{safe_name}\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

pub async fn remove(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let (attachment, stored_name) = fetch_attachment(&st.db.pool, id).await?;
    let task = fetch_task(&st.db.pool, attachment.task_id).await?;
    require_member(&st.db.pool, user.id, task.project_id).await?;

    sqlx::query(&*crate::db::sql("DELETE FROM attachments WHERE id = ?"))
        .bind(id)
        .execute(&st.db.pool)
        .await?;
    let _ = tokio::fs::remove_file(st.data_dir.join("attachments").join(&stored_name)).await;

    let recipients = project_recipients(&st.db.pool, task.project_id).await;
    st.hub.publish(
        recipients,
        "attachment.remove",
        json!({ "id": id, "task_id": attachment.task_id }),
    );
    publish_attachment_change(&st, attachment.task_id).await;
    Ok(Json(json!({ "ok": true })))
}
