use serde::Serialize;
use sqlx::FromRow;

#[derive(Serialize, FromRow, Clone, Debug)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub name: String,
}

#[derive(Serialize, FromRow, Clone, Debug)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub parent_id: Option<i64>,
    pub owner_id: i64,
    pub is_inbox: i64,
    pub sort_order: i64,
    pub created_at: String,
    #[sqlx(default)]
    pub active_count: i64,
}

#[derive(Serialize, FromRow, Clone, Debug)]
pub struct Member {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub role: String,
    pub project_id: i64,
}

#[derive(Serialize, FromRow, Clone, Debug)]
pub struct Task {
    pub id: i64,
    pub project_id: i64,
    pub parent_id: Option<i64>,
    pub creator_id: i64,
    pub name: String,
    pub description: String,
    pub due_date: Option<String>,
    pub due_time: Option<String>,
    pub deadline: Option<String>,
    pub priority: i64,
    pub completed_at: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub updated_at: String,
    #[sqlx(default)]
    pub comment_count: i64,
    #[sqlx(default)]
    pub attachment_count: i64,
    #[sqlx(default)]
    pub subtask_count: i64,
    #[sqlx(default)]
    pub subtask_done_count: i64,
}

#[derive(Serialize, FromRow, Clone, Debug)]
pub struct Comment {
    pub id: i64,
    pub task_id: i64,
    pub user_id: i64,
    pub user_name: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Serialize, FromRow, Clone, Debug)]
pub struct Attachment {
    pub id: i64,
    pub task_id: i64,
    pub user_id: i64,
    pub filename: String,
    pub mime: String,
    pub size: i64,
    pub created_at: String,
}
