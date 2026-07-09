use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::models::Task;
use crate::routes::projects::{project_json, require_member};
use crate::routes::tasks::TASK_COLS;
use crate::AppState;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TemplateTask {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parent_index: Option<usize>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub due_time: Option<String>,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default = "default_priority")]
    pub priority: i64,
}

fn default_priority() -> i64 {
    4
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProjectTemplate {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub color: String,
    pub is_builtin: bool,
    pub task_count: usize,
    pub tasks: Vec<TemplateTask>,
}

#[derive(Clone, Debug)]
pub struct BuiltInTemplate {
    pub slug: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub color: &'static str,
    pub tasks: Vec<TemplateTask>,
}

impl BuiltInTemplate {
    fn as_project_template(&self) -> ProjectTemplate {
        ProjectTemplate {
            id: format!("builtin:{}", self.slug),
            slug: self.slug.to_string(),
            name: self.name.to_string(),
            description: self.description.to_string(),
            color: self.color.to_string(),
            is_builtin: true,
            task_count: self.tasks.len(),
            tasks: self.tasks.clone(),
        }
    }
}

fn task(name: &str) -> TemplateTask {
    TemplateTask {
        name: name.to_string(),
        description: String::new(),
        parent_index: None,
        due_date: None,
        due_time: None,
        deadline: None,
        priority: 4,
    }
}

pub fn built_in_templates() -> Vec<BuiltInTemplate> {
    vec![BuiltInTemplate {
        slug: "packing-list",
        name: "Packing List",
        description:
            "A reusable travel packing checklist so you do not rebuild your suitcase from vibes.",
        color: "sky",
        tasks: vec![
            task("Wallet / ID"),
            task("Keys"),
            task("Phone charger"),
            task("Laptop / tablet charger"),
            task("Medications"),
            task("Toiletries"),
            task("Toothbrush and toothpaste"),
            task("Underwear"),
            task("Socks"),
            task("Shirts"),
            task("Pants / shorts"),
            task("Sleepwear"),
            task("Shoes"),
            task("Jacket / hoodie"),
            task("Laundry bag"),
            task("Snacks / water bottle"),
        ],
    }]
}

pub fn normalize_template_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.chars().take(120).collect())
    }
}

pub async fn list(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Vec<ProjectTemplate>>> {
    let mut out: Vec<ProjectTemplate> = built_in_templates()
        .into_iter()
        .map(|t| t.as_project_template())
        .collect();

    let rows = sqlx::query_as::<_, (i64, String, String, String, String)>(
        &*crate::db::sql(
            "SELECT id, name, description, color, tasks_json FROM project_templates WHERE owner_id = ? ORDER BY name",
        ),
    )
    .bind(user.id)
    .fetch_all(&st.db.pool)
    .await?;

    for (id, name, description, color, tasks_json) in rows {
        let tasks: Vec<TemplateTask> = serde_json::from_str(&tasks_json).unwrap_or_default();
        out.push(ProjectTemplate {
            id: format!("custom:{id}"),
            slug: format!("custom-{id}"),
            name,
            description,
            color,
            is_builtin: false,
            task_count: tasks.len(),
            tasks,
        });
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct CreateTemplate {
    pub project_id: i64,
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

pub async fn create(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(b): Json<CreateTemplate>,
) -> ApiResult<Json<ProjectTemplate>> {
    require_member(&st.db.pool, user.id, b.project_id).await?;
    let project: (String, String) = sqlx::query_as(&*crate::db::sql(
        "SELECT name, color FROM projects WHERE id = ?",
    ))
    .bind(b.project_id)
    .fetch_one(&st.db.pool)
    .await?;

    let name = normalize_template_name(b.name.as_deref().unwrap_or(&project.0))
        .ok_or_else(|| ApiError::bad_request("template name is required"))?;
    let description = b
        .description
        .unwrap_or_else(|| format!("Saved from {}", project.0))
        .trim()
        .chars()
        .take(240)
        .collect::<String>();

    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks t WHERE t.project_id = ? AND t.completed_at IS NULL ORDER BY t.sort_order, t.id"
    );
    let tasks = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(b.project_id)
        .fetch_all(&st.db.pool)
        .await?;
    let templates = task_templates_from_tasks(&tasks);
    let tasks_json = serde_json::to_string(&templates).unwrap();

    let (id,): (i64,) = sqlx::query_as(&*crate::db::sql(
        "INSERT INTO project_templates (owner_id, name, description, color, tasks_json) VALUES (?, ?, ?, ?, ?) RETURNING id",
    ))
    .bind(user.id)
    .bind(&name)
    .bind(&description)
    .bind(&project.1)
    .bind(&tasks_json)
    .fetch_one(&st.db.pool)
    .await?;

    Ok(Json(ProjectTemplate {
        id: format!("custom:{id}"),
        slug: format!("custom-{id}"),
        name,
        description,
        color: project.1,
        is_builtin: false,
        task_count: templates.len(),
        tasks: templates,
    }))
}

fn task_templates_from_tasks(tasks: &[Task]) -> Vec<TemplateTask> {
    let index_by_id: HashMap<i64, usize> =
        tasks.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
    tasks
        .iter()
        .map(|t| TemplateTask {
            name: t.name.clone(),
            description: t.description.clone(),
            parent_index: t.parent_id.and_then(|id| index_by_id.get(&id).copied()),
            due_date: t.due_date.clone(),
            due_time: t.due_time.clone(),
            deadline: t.deadline.clone(),
            priority: t.priority,
        })
        .collect()
}

#[derive(Deserialize)]
pub struct ImportTemplate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub parent_id: Option<i64>,
}

pub async fn import_template(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(b): Json<ImportTemplate>,
) -> ApiResult<Json<Value>> {
    if let Some(parent_id) = b.parent_id {
        require_member(&st.db.pool, user.id, parent_id).await?;
    }
    let template = load_template(&st, user.id, &id).await?;
    let project_name = normalize_template_name(b.name.as_deref().unwrap_or(&template.name))
        .ok_or_else(|| ApiError::bad_request("project name is required"))?;

    let (project_id,): (i64,) = sqlx::query_as(&*crate::db::sql(
        "INSERT INTO projects (name, color, parent_id, owner_id, sort_order) VALUES (?, ?, ?, ?, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM projects)) RETURNING id",
    ))
    .bind(&project_name)
    .bind(&template.color)
    .bind(b.parent_id)
    .bind(user.id)
    .fetch_one(&st.db.pool)
    .await?;
    sqlx::query(&*crate::db::sql(
        "INSERT INTO project_members (project_id, user_id, role) VALUES (?, ?, 'owner')",
    ))
    .bind(project_id)
    .bind(user.id)
    .execute(&st.db.pool)
    .await?;

    let mut created_task_ids: Vec<i64> = Vec::with_capacity(template.tasks.len());
    for task in &template.tasks {
        let parent_id = task
            .parent_index
            .and_then(|i| created_task_ids.get(i).copied());
        let (task_id,): (i64,) = sqlx::query_as(&*crate::db::sql(
            "INSERT INTO tasks (project_id, parent_id, creator_id, name, description, due_date, due_time, deadline, priority, sort_order) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM tasks WHERE project_id = ?)) RETURNING id",
        ))
        .bind(project_id)
        .bind(parent_id)
        .bind(user.id)
        .bind(task.name.trim())
        .bind(task.description.trim())
        .bind(&task.due_date)
        .bind(&task.due_time)
        .bind(&task.deadline)
        .bind(task.priority.clamp(1, 4))
        .bind(project_id)
        .fetch_one(&st.db.pool)
        .await?;
        created_task_ids.push(task_id);
    }

    let project = project_json(&st.db.pool, project_id).await?;
    st.hub
        .publish(vec![user.id], "project.upsert", project.clone());
    st.hub.publish(
        vec![user.id],
        "tasks.refresh",
        json!({ "project_id": project_id }),
    );
    Ok(Json(
        json!({ "project": project, "task_count": created_task_ids.len() }),
    ))
}

async fn load_template(st: &AppState, user_id: i64, id: &str) -> ApiResult<ProjectTemplate> {
    if let Some(slug) = id
        .strip_prefix("builtin:")
        .or(Some(id))
        .filter(|s| !s.starts_with("custom:"))
    {
        if let Some(t) = built_in_templates().into_iter().find(|t| t.slug == slug) {
            return Ok(t.as_project_template());
        }
    }
    let raw_id = id
        .strip_prefix("custom:")
        .unwrap_or(id)
        .parse::<i64>()
        .map_err(|_| ApiError::not_found())?;
    let row: Option<(i64, String, String, String, String)> = sqlx::query_as(&*crate::db::sql(
        "SELECT id, name, description, color, tasks_json FROM project_templates WHERE id = ? AND owner_id = ?",
    ))
    .bind(raw_id)
    .bind(user_id)
    .fetch_optional(&st.db.pool)
    .await?;
    let (id, name, description, color, tasks_json) = row.ok_or_else(ApiError::not_found)?;
    let tasks: Vec<TemplateTask> = serde_json::from_str(&tasks_json).unwrap_or_default();
    Ok(ProjectTemplate {
        id: format!("custom:{id}"),
        slug: format!("custom-{id}"),
        name,
        description,
        color,
        is_builtin: false,
        task_count: tasks.len(),
        tasks,
    })
}

pub async fn remove(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> ApiResult<Json<Value>> {
    let result = sqlx::query(&*crate::db::sql(
        "DELETE FROM project_templates WHERE id = ? AND owner_id = ?",
    ))
    .bind(id)
    .bind(user.id)
    .execute(&st.db.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_list_template_is_available() {
        let templates = built_in_templates();
        let packing = templates.iter().find(|t| t.slug == "packing-list").unwrap();
        assert_eq!(packing.name, "Packing List");
        assert_eq!(packing.color, "sky");
        assert!(packing.tasks.iter().any(|t| t.name == "Phone charger"));
        assert!(packing.tasks.iter().any(|t| t.name == "Toiletries"));
        assert!(packing.tasks.len() >= 12);
    }

    #[test]
    fn custom_template_names_are_normalized() {
        assert_eq!(
            normalize_template_name("  Weekend Trip  ").unwrap(),
            "Weekend Trip"
        );
        assert!(normalize_template_name("   ").is_none());
    }
}
