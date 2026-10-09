//! Remote MCP server (Streamable HTTP transport, stateless, JSON responses).
//!
//! `POST /mcp` accepts JSON-RPC messages from MCP clients. Requests are
//! authenticated with an OAuth access token issued by `crate::oauth` (scoped to
//! what the user approved) or with a TooDue API key (full access). Tools reuse
//! the REST handlers so permissions, validation, real-time events, and Google
//! Calendar sync behave exactly like the app.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{Duration, NaiveDate, Utc};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{bearer_token, user_from_bearer, AuthUser};
use crate::error::ApiError;
use crate::models::{Task, User};
use crate::oauth::{self, SCOPES};
use crate::routes::{comments, projects, tasks};
use crate::AppState;

const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "TooDue is the user's to-do list. Projects contain tasks; every user has an \
Inbox project (is_inbox = 1) that new tasks go to when no project is given. Tasks can have sub-tasks \
(parent_id), a due date (YYYY-MM-DD) with optional time (HH:MM), a deadline, a repeat rule (daily, \
weekly, monthly, yearly), and a priority from 1 (P1, most urgent) to 4 (P4, default). Use \
toodue_agenda to see what is overdue or due soon, toodue_list_projects to find project ids, and \
toodue_search_tasks to find a task by name before updating it. Pass the user's local date as `today` \
when you know it. Confirm with the user before deleting tasks.";

struct Caller {
    user: User,
    scopes: Vec<&'static str>,
}

fn unauthorized(headers: &HeaderMap, message: &str) -> Response {
    let challenge = format!(
        "Bearer resource_metadata=\"{}\", scope=\"{}\"",
        oauth::resource_metadata_url(headers),
        SCOPES.join(" ")
    );
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, challenge)],
        Json(json!({ "error": message })),
    )
        .into_response()
}

async fn authenticate(st: &AppState, headers: &HeaderMap) -> Result<Caller, Response> {
    let Some(token) = bearer_token(headers) else {
        return Err(unauthorized(headers, "authorization required"));
    };
    let internal = |_| (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
    if token.starts_with("tdue_at_") {
        return match oauth::access_from_token(st, &token)
            .await
            .map_err(internal)?
        {
            Some(a) => Ok(Caller {
                user: a.user,
                scopes: a.scopes,
            }),
            None => Err(unauthorized(headers, "access token is invalid or expired")),
        };
    }
    match user_from_bearer(st, headers).await {
        Ok(Some(user)) => Ok(Caller {
            user,
            scopes: SCOPES.to_vec(),
        }),
        _ => Err(unauthorized(headers, "invalid API key")),
    }
}

pub async fn handle_get() -> Response {
    // Stateless server: no server-initiated SSE stream.
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, "POST")],
        "TooDue MCP uses POST (Streamable HTTP, JSON responses)",
    )
        .into_response()
}

pub async fn handle_post(
    State(st): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let caller = match authenticate(&st, &headers).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    if let Some(v) = headers
        .get("mcp-protocol-version")
        .and_then(|v| v.to_str().ok())
    {
        if !PROTOCOL_VERSIONS.contains(&v) {
            return (
                StatusCode::BAD_REQUEST,
                Json(rpc_error(
                    Value::Null,
                    -32600,
                    &format!("unsupported MCP-Protocol-Version {v}"),
                )),
            )
                .into_response();
        }
    }
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(rpc_error(Value::Null, -32700, "parse error")),
            )
                .into_response()
        }
    };
    let replies = match msg {
        Value::Array(batch) if !batch.is_empty() => {
            let mut out = Vec::new();
            for m in batch {
                if let Some(r) = dispatch(&st, &caller, m).await {
                    out.push(r);
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(Value::Array(out))
            }
        }
        m => dispatch(&st, &caller, m).await,
    };
    match replies {
        Some(r) => Json(r).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Returns `None` for notifications and client responses.
async fn dispatch(st: &AppState, caller: &Caller, msg: Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        // A response to something we never sent, or garbage with an id.
        return msg
            .get("result")
            .or_else(|| msg.get("error"))
            .is_none()
            .then(|| rpc_error(id, -32600, "invalid request"));
    };
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    Some(match method {
        "initialize" => {
            let requested = params["protocolVersion"].as_str().unwrap_or("");
            let version = if PROTOCOL_VERSIONS.contains(&requested) {
                requested
            } else {
                PROTOCOL_VERSIONS[0]
            };
            rpc_result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": {
                        "name": "toodue",
                        "title": "TooDue",
                        "version": env!("CARGO_PKG_VERSION"),
                        "websiteUrl": "https://toodue.com",
                    },
                    "instructions": INSTRUCTIONS,
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => {
            let tools: Vec<Value> = tool_defs()
                .into_iter()
                .filter(|(scope, _)| caller.scopes.contains(scope))
                .map(|(_, def)| def)
                .collect();
            rpc_result(id, json!({ "tools": tools }))
        }
        "tools/call" => {
            let Some(name) = params["name"].as_str() else {
                return Some(rpc_error(id, -32602, "tool name is required"));
            };
            let Some(scope) = tool_defs()
                .into_iter()
                .find(|(_, d)| d["name"] == name)
                .map(|(s, _)| s)
            else {
                return Some(rpc_error(id, -32602, &format!("unknown tool: {name}")));
            };
            if !caller.scopes.contains(&scope) {
                return Some(rpc_result(
                    id,
                    tool_error(&format!(
                        "This connection doesn't have the \"{scope}\" permission. The user can \
                         reconnect TooDue and allow it."
                    )),
                ));
            }
            let args = match params.get("arguments") {
                Some(Value::Object(_)) => params["arguments"].clone(),
                _ => json!({}),
            };
            let result = match call_tool(st, &caller.user, name, args).await {
                Ok(data) => json!({
                    "content": [{ "type": "text", "text": serde_json::to_string_pretty(&data).unwrap_or_default() }],
                }),
                Err(message) => tool_error(&message),
            };
            rpc_result(id, result)
        }
        "resources/list" => rpc_result(id, json!({ "resources": [] })),
        "prompts/list" => rpc_result(id, json!({ "prompts": [] })),
        _ => rpc_error(id, -32601, &format!("method not found: {method}")),
    })
}

fn tool_error(message: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}

/* ---------- tools ---------- */

fn tool(
    name: &str,
    title: &str,
    description: &str,
    schema: Value,
    read_only: bool,
    destructive: bool,
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": schema,
        "annotations": {
            "title": title,
            "readOnlyHint": read_only,
            "destructiveHint": destructive,
            "idempotentHint": read_only,
            "openWorldHint": false,
        },
    })
}

fn task_fields() -> Value {
    json!({
        "name": { "type": "string", "description": "Task title" },
        "description": { "type": "string", "description": "Notes (plain text or Markdown)" },
        "due_date": { "type": ["string", "null"], "description": "Due date, YYYY-MM-DD. null clears it." },
        "due_time": { "type": ["string", "null"], "description": "Due time, HH:MM (24h). Requires due_date." },
        "deadline": { "type": ["string", "null"], "description": "Hard deadline, YYYY-MM-DD" },
        "repeat_rule": { "type": ["string", "null"], "enum": ["daily", "weekly", "monthly", "yearly", null], "description": "Repeat schedule. Requires due_date." },
        "priority": { "type": "integer", "minimum": 1, "maximum": 4, "description": "1 = P1 (most urgent) … 4 = P4 (default)" },
    })
}

fn id_schema(desc: &str) -> Value {
    json!({ "type": "object", "required": ["id"], "properties": { "id": { "type": "integer", "description": desc } } })
}

/// Every tool with the scope it requires.
fn tool_defs() -> Vec<(&'static str, Value)> {
    let mut create_props = task_fields();
    create_props["project_id"] =
        json!({ "type": "integer", "description": "Project to add to. Defaults to the Inbox." });
    create_props["parent_id"] =
        json!({ "type": "integer", "description": "Make this a sub-task of another task" });
    let mut update_props = task_fields();
    update_props["id"] = json!({ "type": "integer", "description": "Task id" });
    update_props["project_id"] =
        json!({ "type": "integer", "description": "Move the task to this project" });
    update_props["completed"] = json!({ "type": "boolean" });

    vec![
        ("read", tool("toodue_me", "Who am I", "Return the TooDue account this connection acts as.",
            json!({ "type": "object", "properties": {} }), true, false)),
        ("read", tool("toodue_list_projects", "List projects",
            "List the user's projects (including shared ones) with ids, parent ids, and active task counts.",
            json!({ "type": "object", "properties": {} }), true, false)),
        ("read", tool("toodue_get_project", "Get project",
            "Get one project with its members.", id_schema("Project id"), true, false)),
        ("read", tool("toodue_agenda", "Agenda",
            "Incomplete tasks that are overdue or due within the next `days` days, ordered by date. The best starting point for \"what do I need to do?\"",
            json!({ "type": "object", "properties": {
                "days": { "type": "integer", "minimum": 0, "maximum": 60, "default": 7, "description": "How many days ahead to include (0 = today only)" },
                "today": { "type": "string", "description": "The user's local date, YYYY-MM-DD. Defaults to the server's UTC date." }
            } }), true, false)),
        ("read", tool("toodue_list_tasks", "List tasks",
            "List incomplete tasks, optionally limited to one project. With completed=true (requires project_id) returns recently completed tasks instead.",
            json!({ "type": "object", "properties": {
                "project_id": { "type": "integer" },
                "completed": { "type": "boolean", "default": false }
            } }), true, false)),
        ("read", tool("toodue_search_tasks", "Search tasks",
            "Search incomplete tasks whose name or description contains the text.",
            json!({ "type": "object", "required": ["q"], "properties": {
                "q": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 25 }
            } }), true, false)),
        ("read", tool("toodue_get_task", "Get task",
            "Get a task with its sub-tasks, comments, and attachment list.", id_schema("Task id"), true, false)),
        ("write", tool("toodue_create_task", "Create task", "Create a task or sub-task.",
            json!({ "type": "object", "required": ["name"], "properties": create_props }), false, false)),
        ("write", tool("toodue_update_task", "Update task",
            "Change a task. Only the fields you pass are changed; pass null to clear a date.",
            json!({ "type": "object", "required": ["id"], "properties": update_props }), false, false)),
        ("write", tool("toodue_complete_task", "Complete task",
            "Mark a task complete (or incomplete with completed=false). Completing a repeating task schedules its next occurrence.",
            json!({ "type": "object", "required": ["id"], "properties": {
                "id": { "type": "integer" },
                "completed": { "type": "boolean", "default": true }
            } }), false, false)),
        ("write", tool("toodue_add_comment", "Add comment", "Add a comment to a task.",
            json!({ "type": "object", "required": ["task_id", "body"], "properties": {
                "task_id": { "type": "integer" },
                "body": { "type": "string" }
            } }), false, false)),
        ("write", tool("toodue_create_project", "Create project", "Create a project, optionally nested under another.",
            json!({ "type": "object", "required": ["name"], "properties": {
                "name": { "type": "string" },
                "parent_id": { "type": "integer" },
                "color": { "type": "string", "description": "Color name, e.g. slate, red, orange, amber, green, teal, sky, blue, violet, pink" }
            } }), false, false)),
        ("delete", tool("toodue_delete_task", "Delete task",
            "Permanently delete a task and its sub-tasks. Prefer completing tasks; confirm with the user first.",
            id_schema("Task id"), false, true)),
    ]
}

fn parse<T: DeserializeOwned>(args: Value) -> Result<T, String> {
    serde_json::from_value(args).map_err(|e| format!("invalid arguments: {e}"))
}

fn api_err(e: ApiError) -> String {
    e.1
}

fn to_value<T: serde::Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct IdArg {
    id: i64,
}

#[derive(Deserialize)]
struct AgendaArgs {
    #[serde(default)]
    days: Option<i64>,
    #[serde(default)]
    today: Option<String>,
}

async fn call_tool(
    st: &AppState,
    user: &User,
    name: &str,
    mut args: Value,
) -> Result<Value, String> {
    let s = || State(st.clone());
    let u = || AuthUser(user.clone());
    match name {
        "toodue_me" => to_value(user),
        "toodue_list_projects" => Ok(projects::list(s(), u()).await.map_err(api_err)?.0),
        "toodue_get_project" => {
            let a: IdArg = parse(args)?;
            projects::require_member(&st.db.pool, user.id, a.id)
                .await
                .map_err(api_err)?;
            projects::project_json(&st.db.pool, a.id)
                .await
                .map_err(api_err)
        }
        "toodue_agenda" => agenda(st, user, parse(args)?).await,
        "toodue_list_tasks" => {
            let q: tasks::ListQuery = parse(args)?;
            if q.completed == Some(true) && q.project_id.is_none() {
                return Err("completed=true requires project_id".into());
            }
            to_value(tasks::list(s(), u(), Query(q)).await.map_err(api_err)?.0)
        }
        "toodue_search_tasks" => {
            let q: tasks::SearchQuery = parse(args)?;
            to_value(tasks::search(s(), u(), Query(q)).await.map_err(api_err)?.0)
        }
        "toodue_get_task" => {
            let a: IdArg = parse(args)?;
            Ok(tasks::detail(s(), u(), Path(a.id))
                .await
                .map_err(api_err)?
                .0)
        }
        "toodue_create_task" => {
            if args.get("project_id").is_none_or(Value::is_null)
                && args.get("parent_id").is_none_or(Value::is_null)
            {
                let (inbox,): (i64,) = sqlx::query_as(&*crate::db::sql(
                    "SELECT id FROM projects WHERE owner_id = ? AND is_inbox = 1 ORDER BY id LIMIT 1",
                ))
                .bind(user.id)
                .fetch_one(&st.db.pool)
                .await
                .map_err(|_| "could not find your Inbox; pass project_id".to_string())?;
                args["project_id"] = json!(inbox);
            }
            let b: tasks::CreateTask = parse(args)?;
            to_value(
                tasks::create(s(), u(), axum::Json(b))
                    .await
                    .map_err(api_err)?
                    .0,
            )
        }
        "toodue_update_task" => {
            let id = args
                .get("id")
                .and_then(Value::as_i64)
                .ok_or("id is required")?;
            if let Some(obj) = args.as_object_mut() {
                obj.remove("id");
            }
            let b: tasks::UpdateTask = parse(args)?;
            to_value(
                tasks::update(s(), u(), Path(id), axum::Json(b))
                    .await
                    .map_err(api_err)?
                    .0,
            )
        }
        "toodue_complete_task" => {
            let a: IdArg = parse(args.clone())?;
            let completed = args
                .get("completed")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let b: tasks::UpdateTask = parse(json!({ "completed": completed }))?;
            to_value(
                tasks::update(s(), u(), Path(a.id), axum::Json(b))
                    .await
                    .map_err(api_err)?
                    .0,
            )
        }
        "toodue_add_comment" => {
            let task_id = args
                .get("task_id")
                .and_then(Value::as_i64)
                .ok_or("task_id is required")?;
            let b: comments::CreateComment = parse(args)?;
            to_value(
                comments::create(s(), u(), Path(task_id), axum::Json(b))
                    .await
                    .map_err(api_err)?
                    .0,
            )
        }
        "toodue_create_project" => {
            let b: projects::CreateProject = parse(args)?;
            Ok(projects::create(s(), u(), axum::Json(b))
                .await
                .map_err(api_err)?
                .0)
        }
        "toodue_delete_task" => {
            let a: IdArg = parse(args)?;
            Ok(tasks::remove(s(), u(), Path(a.id))
                .await
                .map_err(api_err)?
                .0)
        }
        _ => Err(format!("unknown tool: {name}")),
    }
}

async fn agenda(st: &AppState, user: &User, a: AgendaArgs) -> Result<Value, String> {
    let today = match a.today.as_deref() {
        Some(d) => NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map_err(|_| "today must be YYYY-MM-DD".to_string())?,
        None => Utc::now().date_naive(),
    };
    let days = a.days.unwrap_or(7).clamp(0, 60);
    let until = today + Duration::days(days);
    let sql = format!(
        "SELECT {} FROM tasks t WHERE t.completed_at IS NULL AND t.due_date IS NOT NULL \
         AND t.due_date <= ? \
         AND t.project_id IN (SELECT project_id FROM project_members WHERE user_id = ?) \
         ORDER BY t.due_date, t.due_time IS NULL, t.due_time, t.priority, t.id LIMIT 500",
        tasks::TASK_COLS
    );
    let rows = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(until.format("%Y-%m-%d").to_string())
        .bind(user.id)
        .fetch_all(&st.db.pool)
        .await
        .map_err(|e| {
            tracing::error!("database error: {e}");
            "internal error".to_string()
        })?;
    let today_s = today.format("%Y-%m-%d").to_string();
    let (overdue, upcoming): (Vec<Task>, Vec<Task>) = rows
        .into_iter()
        .partition(|t| t.due_date.as_deref().is_some_and(|d| d < today_s.as_str()));
    Ok(json!({
        "today": today_s,
        "through": until.format("%Y-%m-%d").to_string(),
        "overdue": overdue,
        "due": upcoming,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_known_scope_and_unique_name() {
        let defs = tool_defs();
        let mut names: Vec<&str> = defs
            .iter()
            .map(|(_, d)| d["name"].as_str().unwrap())
            .collect();
        assert!(defs.iter().all(|(s, _)| SCOPES.contains(s)));
        names.sort();
        let len = names.len();
        names.dedup();
        assert_eq!(names.len(), len);
    }

    #[test]
    fn delete_is_the_only_destructive_tool() {
        for (scope, def) in tool_defs() {
            assert_eq!(
                def["annotations"]["destructiveHint"] == true,
                scope == "delete"
            );
            assert_eq!(def["annotations"]["readOnlyHint"] == true, scope == "read");
        }
    }
}
