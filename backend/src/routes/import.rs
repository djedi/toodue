use std::io::{Cursor, Read};

use axum::extract::{Multipart, State};
use axum::Json;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Utc};
use serde_json::{json, Value};
use sqlx::AnyPool;

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::routes::projects::project_json;
use crate::AppState;

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

#[derive(Default)]
struct ParsedDate {
    date: Option<String>,
    time: Option<String>,
    recurring: bool,
}

/// Best-effort parse of Todoist's exported date strings: "Jun 29", "29 Jun 2027",
/// "Jun 29 09:00", "2026-06-29", or recurring rules like "every 27th".
/// Years are omitted by Todoist when the date is in the current year.
fn parse_todoist_date(raw: &str, today: NaiveDate) -> ParsedDate {
    let s = raw.trim();
    if s.is_empty() {
        return ParsedDate::default();
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return ParsedDate {
            date: Some(d.to_string()),
            ..Default::default()
        };
    }
    let lower = s.to_lowercase();
    if lower.starts_with("every") {
        // Recurrence isn't supported; schedule the next occurrence for
        // day-of-month rules ("every 27th") and leave the rest undated.
        let day = lower.split_whitespace().find_map(|t| {
            t.trim_end_matches(|c: char| c.is_alphabetic())
                .parse::<u32>()
                .ok()
        });
        if let Some(day) = day {
            let mut d = today;
            for _ in 0..62 {
                if d.day() == day {
                    return ParsedDate {
                        date: Some(d.to_string()),
                        time: None,
                        recurring: true,
                    };
                }
                d += Duration::days(1);
            }
        }
        return ParsedDate {
            recurring: true,
            ..Default::default()
        };
    }

    let (mut month, mut day, mut year, mut time) = (None, None, None, None);
    for tok in lower.split_whitespace() {
        let t = tok.trim_matches(',');
        if let Some(m) = MONTHS.iter().position(|m| t.starts_with(m)) {
            month = Some(m as u32 + 1);
            continue;
        }
        if t.contains(':') {
            if NaiveTime::parse_from_str(t, "%H:%M").is_ok() {
                time = Some(t.to_string());
            }
            continue;
        }
        let digits = t
            .trim_end_matches("st")
            .trim_end_matches("nd")
            .trim_end_matches("rd")
            .trim_end_matches("th");
        if let Ok(n) = digits.parse::<u32>() {
            if n >= 1000 {
                year = Some(n as i32);
            } else if day.is_none() {
                day = Some(n);
            }
        }
    }
    if let (Some(m), Some(d)) = (month, day) {
        if let Some(nd) = NaiveDate::from_ymd_opt(year.unwrap_or(today.year()), m, d) {
            return ParsedDate {
                date: Some(nd.to_string()),
                time,
                recurring: false,
            };
        }
    }
    ParsedDate::default()
}

/// "🛒 Groceries [6CrfQx5qH23VhMmR].csv" → "🛒 Groceries"
fn project_name_from_filename(filename: &str) -> String {
    let base = filename.rsplit('/').next().unwrap_or(filename);
    let base = base.strip_suffix(".csv").unwrap_or(base);
    match base.rfind(" [") {
        Some(i) => base[..i].trim().to_string(),
        None => base.trim().to_string(),
    }
}

async fn find_or_create_project(
    db: &AnyPool,
    user_id: i64,
    name: &str,
    created: &mut usize,
) -> ApiResult<i64> {
    // Todoist's Inbox maps onto the TooDue inbox.
    if name == "Inbox" {
        let row: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
            "SELECT id FROM projects WHERE owner_id = ? AND is_inbox = 1",
        ))
        .bind(user_id)
        .fetch_optional(db)
        .await?;
        if let Some((id,)) = row {
            return Ok(id);
        }
    }
    // Re-importing reuses a same-named project instead of duplicating it.
    let row: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT id FROM projects WHERE owner_id = ? AND is_inbox = 0 AND name = ? LIMIT 1",
    ))
    .bind(user_id)
    .bind(name)
    .fetch_optional(db)
    .await?;
    if let Some((id,)) = row {
        return Ok(id);
    }
    let (id,): (i64,) = sqlx::query_as(&*crate::db::sql(
        "INSERT INTO projects (name, color, owner_id, sort_order) \
         VALUES (?, 'slate', ?, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM projects)) RETURNING id",
    ))
    .bind(name)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    sqlx::query(&*crate::db::sql(
        "INSERT INTO project_members (project_id, user_id, role) VALUES (?, ?, 'owner')",
    ))
    .bind(id)
    .bind(user_id)
    .execute(db)
    .await?;
    *created += 1;
    Ok(id)
}

pub async fn todoist(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    mut multipart: Multipart,
) -> ApiResult<Json<Value>> {
    let mut bytes = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::bad_request("invalid upload"))?
    {
        if field.name() == Some("file") {
            bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|_| ApiError::bad_request("upload failed or file too large"))?,
            );
            break;
        }
    }
    let bytes =
        bytes.ok_or_else(|| ApiError::bad_request("expected a multipart field named \"file\""))?;

    // Extract everything up front: ZipFile is not Send, so it can't be held
    // across the database awaits below.
    let mut csv_files: Vec<(String, String)> = Vec::new();
    {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|_| ApiError::bad_request("that doesn't look like a Todoist backup (.zip)"))?;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|_| ApiError::bad_request("corrupt zip entry"))?;
            // Decode names as UTF-8 ourselves: Todoist uses emoji-heavy names
            // and the cp437 fallback would mangle them.
            let filename = String::from_utf8_lossy(&file.name_raw().to_vec()).into_owned();
            if file.is_dir() || !filename.to_lowercase().ends_with(".csv") {
                continue;
            }
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|_| ApiError::bad_request("zip entry is not valid UTF-8 CSV"))?;
            csv_files.push((filename, content));
        }
    }

    let today = Utc::now().date_naive();
    let (mut projects_created, mut tasks, mut comments, mut sections, mut recurring) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut touched_projects: Vec<i64> = Vec::new();

    for (filename, content) in &csv_files {
        let content = content.strip_prefix('\u{feff}').unwrap_or(content);

        let project_name = project_name_from_filename(filename);
        if project_name.is_empty() {
            continue;
        }

        let mut rdr = csv::ReaderBuilder::new()
            .flexible(true)
            .from_reader(content.as_bytes());
        let headers = rdr
            .headers()
            .map_err(|_| ApiError::bad_request("invalid CSV in backup"))?
            .clone();
        let col = |name: &str| headers.iter().position(|h| h.eq_ignore_ascii_case(name));
        let (Some(c_type), Some(c_content)) = (col("TYPE"), col("CONTENT")) else {
            continue; // not a Todoist project export
        };
        let c_desc = col("DESCRIPTION");
        let c_priority = col("PRIORITY");
        let c_indent = col("INDENT");
        let c_date = col("DATE");
        let c_deadline = col("DEADLINE");

        let project_id =
            find_or_create_project(&st.db.pool, user.id, &project_name, &mut projects_created)
                .await?;
        touched_projects.push(project_id);

        let mut sort_order: i64 = sqlx::query_as::<_, (i64,)>(
            "SELECT COALESCE(MAX(sort_order), 0) FROM tasks WHERE project_id = ?",
        )
        .bind(project_id)
        .fetch_one(&st.db.pool)
        .await?
        .0;
        let mut last_top_task: Option<i64> = None;
        let mut last_task: Option<i64> = None;

        for record in rdr.records() {
            let Ok(record) = record else { continue };
            let get = |idx: Option<usize>| idx.and_then(|i| record.get(i)).unwrap_or("").trim();
            let content = get(Some(c_content));
            match get(Some(c_type)) {
                "task" => {
                    if content.is_empty() {
                        continue;
                    }
                    let indent: i64 = get(c_indent).parse().unwrap_or(1);
                    let priority: i64 = get(c_priority).parse().unwrap_or(4).clamp(1, 4);
                    let due = parse_todoist_date(get(c_date), today);
                    if due.recurring {
                        recurring += 1;
                    }
                    let deadline = parse_todoist_date(get(c_deadline), today);
                    // TooDue nests one level; deeper Todoist indents attach to
                    // the nearest top-level task so nothing gets hidden.
                    let parent_id = if indent > 1 { last_top_task } else { None };
                    sort_order += 1;
                    let (id,): (i64,) = sqlx::query_as(&*crate::db::sql(
                        "INSERT INTO tasks (project_id, parent_id, creator_id, name, description, \
                         due_date, due_time, deadline, priority, sort_order) \
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
                    ))
                    .bind(project_id)
                    .bind(parent_id)
                    .bind(user.id)
                    .bind(content)
                    .bind(get(c_desc))
                    .bind(&due.date)
                    .bind(due.date.is_some().then_some(due.time).flatten())
                    .bind(&deadline.date)
                    .bind(priority)
                    .bind(sort_order)
                    .fetch_one(&st.db.pool)
                    .await?;
                    tasks += 1;
                    if indent <= 1 {
                        last_top_task = Some(id);
                    }
                    last_task = Some(id);
                }
                "note" => {
                    // Notes follow the task they belong to and become comments.
                    if let Some(task_id) = last_task {
                        if !content.is_empty() {
                            sqlx::query(&*crate::db::sql(
                                "INSERT INTO comments (task_id, user_id, body) VALUES (?, ?, ?)",
                            ))
                            .bind(task_id)
                            .bind(user.id)
                            .bind(content)
                            .execute(&st.db.pool)
                            .await?;
                            comments += 1;
                        }
                    }
                }
                "section" => sections += 1,
                _ => {} // meta rows, separators
            }
        }
    }

    touched_projects.sort_unstable();
    touched_projects.dedup();
    for pid in &touched_projects {
        if let Ok(v) = project_json(&st.db.pool, *pid).await {
            st.hub.publish(vec![user.id], "project.upsert", v);
        }
    }
    st.hub.publish(vec![user.id], "tasks.refresh", json!({}));
    crate::gcal::spawn_full_sync(&st, user.id);

    Ok(Json(json!({
        "projects": touched_projects.len(),
        "projects_created": projects_created,
        "tasks": tasks,
        "comments": comments,
        "sections_flattened": sections,
        "recurring_converted": recurring,
    })))
}
