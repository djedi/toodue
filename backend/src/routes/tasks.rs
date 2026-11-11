use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{AnyConnection, AnyPool};

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

const REPEAT_RULES: [&str; 4] = ["daily", "weekly", "monthly", "yearly"];

fn validate_repeat_rule(rule: &str) -> ApiResult<()> {
    if REPEAT_RULES.contains(&rule) {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "repeat_rule must be daily, weekly, monthly, or yearly",
        ))
    }
}

fn anchored_month(anchor: NaiveDate, months: i32) -> Option<NaiveDate> {
    let total = anchor.year().checked_mul(12)? + i32::try_from(anchor.month0()).ok()? + months;
    let year = total.div_euclid(12);
    let month = u32::try_from(total.rem_euclid(12) + 1).ok()?;
    let next_month = if month == 12 {
        NaiveDate::from_ymd_opt(year.checked_add(1)?, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)?
    };
    let last_day = (next_month - Duration::days(1)).day();
    NaiveDate::from_ymd_opt(year, month, anchor.day().min(last_day))
}

fn next_repeating_due_date(
    current: NaiveDate,
    anchor: NaiveDate,
    rule: &str,
    completed_on: NaiveDate,
) -> Option<NaiveDate> {
    let threshold = current.max(completed_on);
    match rule {
        "daily" | "weekly" => {
            let step = if rule == "daily" { 1 } else { 7 };
            let elapsed = (threshold - anchor).num_days().max(0);
            let periods = elapsed / step + 1;
            anchor.checked_add_signed(Duration::days(periods * step))
        }
        "monthly" => {
            let mut periods = (threshold.year() - anchor.year()).checked_mul(12)?
                + i32::try_from(threshold.month()).ok()?
                - i32::try_from(anchor.month()).ok()?;
            periods = periods.max(0);
            loop {
                let candidate = anchored_month(anchor, periods)?;
                if candidate > threshold {
                    return Some(candidate);
                }
                periods = periods.checked_add(1)?;
            }
        }
        "yearly" => {
            let mut periods = (threshold.year() - anchor.year()).max(0);
            loop {
                let candidate = anchored_month(anchor, periods.checked_mul(12)?)?;
                if candidate > threshold {
                    return Some(candidate);
                }
                periods = periods.checked_add(1)?;
            }
        }
        _ => None,
    }
}

async fn publish_task(st: &AppState, task: &Task) {
    let recipients = project_recipients(&st.db.pool, task.project_id).await;
    st.hub.publish(
        recipients,
        "task.upsert",
        serde_json::to_value(task).unwrap(),
    );
}

fn shift_optional_date(value: &Option<String>, days: i64) -> Option<String> {
    value
        .as_deref()
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .and_then(|date| date.checked_add_signed(Duration::days(days)))
        .map(|date| date.to_string())
}

async fn ensure_next_occurrence(
    connection: &mut AnyConnection,
    task: &Task,
    completed_on: Option<NaiveDate>,
) -> ApiResult<Vec<Task>> {
    if task.completed_at.is_none() || task.parent_id.is_some() {
        return Ok(Vec::new());
    }
    let (Some(rule), Some(due_date)) = (&task.repeat_rule, &task.due_date) else {
        return Ok(Vec::new());
    };
    let current = NaiveDate::parse_from_str(due_date, "%Y-%m-%d")
        .map_err(|_| ApiError::bad_request("dates must be YYYY-MM-DD"))?;
    let anchor = task
        .repeat_anchor
        .as_deref()
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        .unwrap_or(current);
    let completed_on = completed_on
        .or_else(|| {
            task.completed_at
                .as_deref()
                .and_then(|value| value.get(..10))
                .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
        })
        .unwrap_or_else(|| chrono::Utc::now().date_naive());
    let next_due = next_repeating_due_date(current, anchor, rule, completed_on)
        .ok_or_else(|| ApiError::bad_request("invalid repeat_rule"))?;
    let shift_days = (next_due - current).num_days();
    let next_deadline = shift_optional_date(&task.deadline, shift_days);

    let created: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "INSERT INTO tasks (project_id, creator_id, name, description, due_date, due_time, deadline, repeat_rule, repeat_anchor, repeat_source_id, priority, sort_order) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM tasks WHERE project_id = ?)) \
         ON CONFLICT(repeat_source_id) DO NOTHING RETURNING id",
    ))
    .bind(task.project_id)
    .bind(task.creator_id)
    .bind(&task.name)
    .bind(&task.description)
    .bind(next_due.to_string())
    .bind(&task.due_time)
    .bind(&next_deadline)
    .bind(rule)
    .bind(anchor.to_string())
    .bind(task.id)
    .bind(task.priority)
    .bind(task.project_id)
    .fetch_optional(&mut *connection)
    .await?;

    let next_id = if let Some((id,)) = created {
        id
    } else {
        sqlx::query_as::<_, (i64,)>(&*crate::db::sql(
            "SELECT id FROM tasks WHERE repeat_source_id = ?",
        ))
        .bind(task.id)
        .fetch_one(&mut *connection)
        .await?
        .0
    };

    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks t WHERE t.parent_id = ? ORDER BY t.sort_order, t.id"
    );
    let source_subtasks = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(task.id)
        .fetch_all(&mut *connection)
        .await?;
    for subtask in source_subtasks {
        let due_date = shift_optional_date(&subtask.due_date, shift_days);
        let deadline = shift_optional_date(&subtask.deadline, shift_days);
        sqlx::query(&*crate::db::sql(
            "INSERT INTO tasks (project_id, parent_id, creator_id, name, description, due_date, due_time, deadline, repeat_source_id, priority, sort_order) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(repeat_source_id) DO NOTHING",
        ))
        .bind(task.project_id)
        .bind(next_id)
        .bind(subtask.creator_id)
        .bind(subtask.name)
        .bind(subtask.description)
        .bind(due_date)
        .bind(subtask.due_time)
        .bind(deadline)
        .bind(subtask.id)
        .bind(subtask.priority)
        .bind(subtask.sort_order)
        .execute(&mut *connection)
        .await?;
    }

    let sql = format!("SELECT {TASK_COLS} FROM tasks t WHERE t.id = ?");
    let parent = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(next_id)
        .fetch_one(&mut *connection)
        .await?;
    let mut occurrences = vec![parent];
    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks t WHERE t.parent_id = ? ORDER BY t.sort_order, t.id"
    );
    occurrences.extend(
        sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
            .bind(next_id)
            .fetch_all(&mut *connection)
            .await?,
    );
    Ok(occurrences)
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub completed: Option<bool>,
    /// Completed-task filters (only honoured with `completed=true`): `completed_at`
    /// bounds as RFC 3339 UTC timestamps, `due_date` bounds as YYYY-MM-DD, all inclusive.
    #[serde(default)]
    pub completed_from: Option<String>,
    #[serde(default)]
    pub completed_to: Option<String>,
    #[serde(default)]
    pub due_from: Option<String>,
    #[serde(default)]
    pub due_to: Option<String>,
}

/// Which completed tasks a view shows, and what `DELETE /tasks/completed` purges.
#[derive(Debug, PartialEq)]
pub struct CompletedFilter {
    pub project_id: Option<i64>,
    pub completed_from: Option<String>,
    pub completed_to: Option<String>,
    pub due_from: Option<String>,
    pub due_to: Option<String>,
}

impl CompletedFilter {
    pub fn from_query(q: &ListQuery) -> ApiResult<Self> {
        let date = |value: &Option<String>, name: &str| -> ApiResult<Option<String>> {
            match value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                None => Ok(None),
                Some(v) => NaiveDate::parse_from_str(v, "%Y-%m-%d")
                    .map(|d| Some(d.to_string()))
                    .map_err(|_| ApiError::bad_request(format!("{name} must be YYYY-MM-DD"))),
            }
        };
        let timestamp = |value: &Option<String>, name: &str| -> ApiResult<Option<String>> {
            match value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                None => Ok(None),
                Some(v) => chrono::DateTime::parse_from_rfc3339(v)
                    .map(|t| Some(t.with_timezone(&chrono::Utc).format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()))
                    .map_err(|_| ApiError::bad_request(format!("{name} must be an RFC 3339 timestamp"))),
            }
        };
        Ok(Self {
            project_id: q.project_id,
            completed_from: timestamp(&q.completed_from, "completed_from")?,
            completed_to: timestamp(&q.completed_to, "completed_to")?,
            due_from: date(&q.due_from, "due_from")?,
            due_to: date(&q.due_to, "due_to")?,
        })
    }

    /// SQL conditions on table alias `t` plus their bind values, in order. The first
    /// bind is always the user id for the membership check.
    fn conditions(&self) -> (String, Vec<Bind>) {
        let mut sql = String::from(
            "t.completed_at IS NOT NULL \
             AND t.project_id IN (SELECT project_id FROM project_members WHERE user_id = ?)",
        );
        let mut binds = Vec::new();
        if let Some(pid) = self.project_id {
            sql.push_str(" AND t.project_id = ?");
            binds.push(Bind::Int(pid));
        }
        for (column, op, value) in [
            ("completed_at", ">=", &self.completed_from),
            ("completed_at", "<=", &self.completed_to),
            ("due_date", ">=", &self.due_from),
            ("due_date", "<=", &self.due_to),
        ] {
            if let Some(v) = value {
                sql.push_str(&format!(" AND t.{column} {op} ?"));
                binds.push(Bind::Text(v.clone()));
            }
        }
        (sql, binds)
    }
}

/// A typed bind value: Postgres rejects comparing a bigint column to a text parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum Bind {
    Int(i64),
    Text(String),
}

trait BindAll<'q>: Sized {
    fn bind_all(self, binds: &'q [Bind]) -> Self;
}

impl<'q, O> BindAll<'q> for sqlx::query::QueryAs<'q, sqlx::Any, O, sqlx::any::AnyArguments<'q>> {
    fn bind_all(mut self, binds: &'q [Bind]) -> Self {
        for b in binds {
            self = match b {
                Bind::Int(v) => self.bind(*v),
                Bind::Text(v) => self.bind(v.as_str()),
            };
        }
        self
    }
}

impl<'q> BindAll<'q> for sqlx::query::Query<'q, sqlx::Any, sqlx::any::AnyArguments<'q>> {
    fn bind_all(mut self, binds: &'q [Bind]) -> Self {
        for b in binds {
            self = match b {
                Bind::Int(v) => self.bind(*v),
                Bind::Text(v) => self.bind(v.as_str()),
            };
        }
        self
    }
}

/// Completed tasks the user may see, newest first.
async fn list_completed(
    db: &AnyPool,
    user_id: i64,
    filter: &CompletedFilter,
) -> ApiResult<Vec<Task>> {
    let (conditions, binds) = filter.conditions();
    let sql = format!(
        "SELECT {TASK_COLS} FROM tasks t WHERE {conditions} \
         ORDER BY t.completed_at DESC, t.id DESC LIMIT 200"
    );
    let sql = crate::db::sql(&sql);
    let query = sqlx::query_as::<_, Task>(&sql).bind(user_id).bind_all(&binds);
    Ok(query.fetch_all(db).await?)
}

pub async fn list(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Task>>> {
    if let Some(project_id) = q.project_id {
        require_member(&st.db.pool, user.id, project_id).await?;
    }
    if q.completed.unwrap_or(false) {
        let filter = CompletedFilter::from_query(&q)?;
        return Ok(Json(list_completed(&st.db.pool, user.id, &filter).await?));
    }
    let tasks = if let Some(project_id) = q.project_id {
        let sql = format!(
            "SELECT {TASK_COLS} FROM tasks t WHERE t.project_id = ? AND t.completed_at IS NULL \
             ORDER BY t.sort_order, t.id"
        );
        sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
            .bind(project_id)
            .fetch_all(&st.db.pool)
            .await?
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

/// Deletes the stored files behind attachments after their rows are gone. Failures are
/// logged, not surfaced: the database is already consistent.
fn spawn_attachment_cleanup(st: &AppState, stored_names: Vec<String>) {
    if stored_names.is_empty() {
        return;
    }
    let dir = st.data_dir.join("attachments");
    tokio::spawn(async move {
        for name in stored_names {
            if let Err(e) = tokio::fs::remove_file(dir.join(&name)).await {
                tracing::warn!("remove attachment file {name}: {e}");
            }
        }
    });
}

/// Rows the foreign-key cascade will take under `roots`: (descendant id, root, project id), plus
/// the stored names of every attachment on the roots or beneath them, tagged with the root.
/// On Postgres every row in the subtree is locked first (an attachment insert needs a share
/// lock on its task row, so none can commit into the gap before the cascade); SQLite callers
/// must already hold the writer lock.
struct Subtree {
    descendants: Vec<(i64, i64, i64)>,
    files: Vec<(String, i64)>,
}

async fn collect_subtree(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    postgres: bool,
    roots: &[i64],
) -> ApiResult<Subtree> {
    let marks = vec!["?"; roots.len()].join(", ");
    let cte = format!(
        "WITH RECURSIVE sub(id, root) AS ( \
           SELECT id, id FROM tasks WHERE id IN ({marks}) \
           UNION ALL \
           SELECT t.id, sub.root FROM tasks t JOIN sub ON t.parent_id = sub.id)"
    );
    if postgres {
        let lock_sql = format!("{cte} SELECT s.id FROM tasks s WHERE s.id IN (SELECT id FROM sub) FOR UPDATE");
        let lock_sql = crate::db::sql(&lock_sql);
        let mut q = sqlx::query(&lock_sql);
        for id in roots {
            q = q.bind(*id);
        }
        q.execute(&mut **tx).await?;
    }
    let tree_sql = format!(
        "{cte} SELECT sub.id, sub.root, t.project_id FROM sub JOIN tasks t ON t.id = sub.id \
         WHERE sub.id <> sub.root"
    );
    let tree_sql = crate::db::sql(&tree_sql);
    let mut q = sqlx::query_as::<_, (i64, i64, i64)>(&tree_sql);
    for id in roots {
        q = q.bind(*id);
    }
    let descendants = q.fetch_all(&mut **tx).await?;

    let files_sql = format!(
        "{cte} SELECT a.stored_name, sub.root FROM attachments a JOIN sub ON sub.id = a.task_id"
    );
    let files_sql = crate::db::sql(&files_sql);
    let mut q = sqlx::query_as::<_, (String, i64)>(&files_sql);
    for id in roots {
        q = q.bind(*id);
    }
    let files = q.fetch_all(&mut **tx).await?;
    Ok(Subtree { descendants, files })
}

/// Project ids the user is a member of.
async fn member_projects(
    tx: &mut sqlx::Transaction<'_, sqlx::Any>,
    user_id: i64,
) -> ApiResult<std::collections::HashSet<i64>> {
    let rows: Vec<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT project_id FROM project_members WHERE user_id = ?",
    ))
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Roots whose subtree reaches a project the user can't access. A subtask moved to another
/// project keeps its parent, so the cascade would otherwise delete rows the user can't see.
fn roots_reaching_foreign_projects(
    descendants: &[(i64, i64, i64)],
    allowed: &std::collections::HashSet<i64>,
) -> std::collections::HashSet<i64> {
    descendants
        .iter()
        .filter(|(_, _, pid)| !allowed.contains(pid))
        .map(|(_, root, _)| *root)
        .collect()
}

/// Permanently deletes every completed task matching the filter (and, through the
/// foreign key cascade, their subtasks). Members of the touched projects get a
/// `tasks.remove` with the ids and a `tasks.refresh` so subtask counts catch up.
pub async fn purge_completed(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Value>> {
    if let Some(project_id) = q.project_id {
        require_member(&st.db.pool, user.id, project_id).await?;
    }
    let filter = CompletedFilter::from_query(&q)?;
    let (conditions, binds) = filter.conditions();

    // Materialise the target set under a lock, collect the attachment files of the targets and
    // all their descendants, then delete the targets while re-checking the full predicate so a
    // row moved, reopened or re-dated in the meantime survives untouched. Files are removed
    // only after the transaction commits, and only for roots the DELETE actually returned.
    let mut tx = st.db.pool.begin().await?;
    let postgres = st.db.kind == crate::db::DbKind::Postgres;
    if !postgres {
        // SQLite has no SELECT ... FOR UPDATE: take its single writer slot before reading so
        // the later upgrade can't fail on a stale snapshot.
        let lock_sql = format!(
            "UPDATE tasks SET updated_at = updated_at WHERE id IN (SELECT t.id FROM tasks t WHERE {conditions})"
        );
        let lock_sql = crate::db::sql(&lock_sql);
        sqlx::query(&lock_sql)
            .bind(user.id)
            .bind_all(&binds)
            .execute(&mut *tx)
            .await?;
    }
    let lock_suffix = if postgres { " FOR UPDATE" } else { "" };
    let targets_sql = format!("SELECT t.id FROM tasks t WHERE {conditions}{lock_suffix}");
    let targets_sql = crate::db::sql(&targets_sql);
    let targets: Vec<i64> = sqlx::query_as::<_, (i64,)>(&targets_sql)
        .bind(user.id)
        .bind_all(&binds)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|r| r.0)
        .collect();

    let mut files: Vec<(String, i64)> = Vec::new();
    let mut descendants_of: Vec<(i64, i64, i64)> = Vec::new();
    let mut rows: Vec<(i64, i64)> = Vec::new();
    let allowed = member_projects(&mut tx, user.id).await?;
    let mut skipped = 0usize;
    let mut parent_projects: std::collections::BTreeSet<i64> = Default::default();
    for chunk in targets.chunks(500) {
        let subtree = collect_subtree(&mut tx, postgres, chunk).await?;
        let blocked = roots_reaching_foreign_projects(&subtree.descendants, &allowed);
        skipped += blocked.len();
        let chunk: Vec<i64> = chunk.iter().copied().filter(|id| !blocked.contains(id)).collect();
        if chunk.is_empty() {
            continue;
        }
        let marks = vec!["?"; chunk.len()].join(", ");
        // Parents may sit in another project; their members need a refresh for subtask counts.
        let parents_sql = format!(
            "SELECT DISTINCT p.project_id FROM tasks t JOIN tasks p ON p.id = t.parent_id \
             WHERE t.id IN ({marks})"
        );
        let parents_sql = crate::db::sql(&parents_sql);
        let mut parents_query = sqlx::query_as::<_, (i64,)>(&parents_sql);
        for id in &chunk {
            parents_query = parents_query.bind(*id);
        }
        parent_projects.extend(parents_query.fetch_all(&mut *tx).await?.into_iter().map(|r| r.0));
        files.extend(subtree.files);
        descendants_of.extend(subtree.descendants);

        let delete_sql = format!(
            "DELETE FROM tasks WHERE id IN \
             (SELECT t.id FROM tasks t WHERE t.id IN ({marks}) AND {conditions}) \
             RETURNING id, project_id"
        );
        let delete_sql = crate::db::sql(&delete_sql);
        let mut delete_query = sqlx::query_as::<_, (i64, i64)>(&delete_sql);
        for id in &chunk {
            delete_query = delete_query.bind(*id);
        }
        delete_query = delete_query.bind(user.id).bind_all(&binds);
        rows.extend(delete_query.fetch_all(&mut *tx).await?);
    }
    tx.commit().await?;

    let deleted_set: std::collections::HashSet<i64> = rows.iter().map(|(id, _)| *id).collect();
    let mut stored_names: Vec<String> = files
        .into_iter()
        .filter(|(_, root)| deleted_set.contains(root))
        .map(|(name, _)| name)
        .collect();
    stored_names.sort_unstable();
    stored_names.dedup();
    spawn_attachment_cleanup(&st, stored_names);

    // Everything that went: the targets plus every descendant under a deleted root (subtasks
    // share their root's project). Clients tombstone all of them.
    let mut by_project: std::collections::BTreeMap<i64, Vec<i64>> = Default::default();
    for (id, pid) in &rows {
        by_project.entry(*pid).or_default().push(*id);
    }
    // Descendants are routed by their own project: a moved root takes only its direct
    // children with it, so deeper rows can live elsewhere.
    for (id, root, pid) in descendants_of {
        if deleted_set.contains(&root) && !deleted_set.contains(&id) {
            by_project.entry(pid).or_default().push(id);
        }
    }
    for ids in by_project.values_mut() {
        ids.sort_unstable();
        ids.dedup();
    }
    let ids: Vec<i64> = by_project.values().flatten().copied().collect();

    if !ids.is_empty() {
        let mut all_recipients = Vec::new();
        for (pid, project_ids) in by_project {
            let recipients = project_recipients(&st.db.pool, pid).await;
            all_recipients.extend(recipients.iter().copied());
            // Identities first so clients tombstone the rows, then a refresh for subtask counts.
            st.hub
                .publish(recipients, "tasks.remove", json!({ "ids": project_ids }));
        }
        for pid in parent_projects {
            all_recipients.extend(project_recipients(&st.db.pool, pid).await);
        }
        all_recipients.sort_unstable();
        all_recipients.dedup();
        st.hub.publish(all_recipients, "tasks.refresh", json!({}));
        crate::gcal::spawn_orphan_cleanup(&st);
    }
    // `deleted` counts the matching completed tasks that went, whether deleted directly or by
    // a parent's cascade; `ids` also lists their incomplete cascaded subtasks.
    let gone: std::collections::HashSet<i64> = ids.iter().copied().collect();
    let deleted = targets.iter().filter(|id| gone.contains(id)).count();
    Ok(Json(json!({ "deleted": deleted, "ids": ids, "skipped": skipped })))
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
    pub repeat_rule: Option<String>,
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
    if let Some(rule) = &b.repeat_rule {
        validate_repeat_rule(rule)?;
        if b.due_date.is_none() {
            return Err(ApiError::bad_request("repeating tasks require a due_date"));
        }
        if b.parent_id.is_some() {
            return Err(ApiError::bad_request(
                "subtasks cannot repeat independently",
            ));
        }
    }
    let priority = b.priority.unwrap_or(4).clamp(1, 4);
    let repeat_anchor = b.repeat_rule.as_ref().and(b.due_date.clone());

    let (id,): (i64,) = sqlx::query_as(&*crate::db::sql(
        "INSERT INTO tasks (project_id, parent_id, creator_id, name, description, due_date, due_time, deadline, repeat_rule, repeat_anchor, priority, sort_order) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM tasks WHERE project_id = ?)) RETURNING id",
    ))
    .bind(project_id)
    .bind(b.parent_id)
    .bind(user.id)
    .bind(&name)
    .bind(b.description.trim())
    .bind(&b.due_date)
    .bind(&b.due_time)
    .bind(&b.deadline)
    .bind(&b.repeat_rule)
    .bind(&repeat_anchor)
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

/// Distinguish a field that was omitted (`None`, leave unchanged) from one sent as
/// JSON `null` (`Some(None)`, clear it). Plain `Option<Option<T>>` maps both to `None`.
fn nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Deserialize)]
pub struct UpdateTask {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub due_date: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub due_time: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub deadline: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub repeat_rule: Option<Option<String>>,
    pub priority: Option<i64>,
    pub completed: Option<bool>,
    #[serde(default)]
    pub completed_on: Option<String>,
    pub project_id: Option<i64>,
}

pub async fn update(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    Json(b): Json<UpdateTask>,
) -> ApiResult<Json<Task>> {
    let authorized_task = fetch_task(&st.db.pool, id).await?;
    require_member(&st.db.pool, user.id, authorized_task.project_id).await?;

    if let Some(new_pid) = b.project_id {
        if new_pid != authorized_task.project_id {
            require_member(&st.db.pool, user.id, new_pid).await?;
        }
    }

    // Serialize updates to this task before deriving any new state. In
    // particular, a concurrent reopen must not race a completion that creates
    // successor and leave both the source and successor active.
    let mut tx = st.db.pool.begin().await?;
    if st.db.kind == crate::db::DbKind::Sqlite {
        // SQLite has no SELECT ... FOR UPDATE. Acquire its single writer slot
        // before reading so a competing update waits, then observes our
        // committed state instead of failing a deferred-transaction upgrade.
        sqlx::query("UPDATE tasks SET updated_at = updated_at WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let lock_suffix = if st.db.kind == crate::db::DbKind::Postgres {
        " FOR UPDATE"
    } else {
        ""
    };
    let sql = format!("SELECT {TASK_COLS} FROM tasks t WHERE t.id = ?{lock_suffix}");
    let task = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::not_found)?;
    require_member(&st.db.pool, user.id, task.project_id).await?;
    let moved_from = b
        .project_id
        .filter(|new_pid| *new_pid != task.project_id)
        .map(|_| task.project_id);

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
    let mut new_repeat_rule = task.repeat_rule.clone();
    let mut new_repeat_anchor = task.repeat_anchor.clone();
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
    if let Some(repeat_rule) = &b.repeat_rule {
        if let Some(rule) = repeat_rule {
            validate_repeat_rule(rule)?;
        }
        new_repeat_rule = repeat_rule.clone();
    }
    if b.repeat_rule.as_ref().is_some_and(Option::is_some) && new_due_date.is_none() {
        return Err(ApiError::bad_request("repeating tasks require a due_date"));
    }
    if new_due_date.is_none() {
        new_repeat_rule = None;
        new_repeat_anchor = None;
    } else if new_repeat_rule.is_some() {
        if task.parent_id.is_some() {
            return Err(ApiError::bad_request(
                "subtasks cannot repeat independently",
            ));
        }
        if b.due_date.is_some() || b.repeat_rule.is_some() || new_repeat_anchor.is_none() {
            new_repeat_anchor = new_due_date.clone();
        }
    } else {
        new_repeat_anchor = None;
    }
    let completed_on = match b.completed_on.as_deref() {
        Some(value) if b.completed == Some(true) => {
            validate_date(value)?;
            Some(
                NaiveDate::parse_from_str(value, "%Y-%m-%d")
                    .map_err(|_| ApiError::bad_request("dates must be YYYY-MM-DD"))?,
            )
        }
        Some(_) => {
            return Err(ApiError::bad_request(
                "completed_on is only valid when completed is true",
            ));
        }
        None => None,
    };
    let completing = b.completed == Some(true) && task.completed_at.is_none();
    let new_completed_at = match b.completed {
        Some(true) => task.completed_at.clone().or_else(|| Some(now.clone())),
        Some(false) => None,
        None => task.completed_at.clone(),
    };
    if let Some(pid) = b.project_id {
        new_project_id = pid;
    }

    if b.completed == Some(false) {
        let successor: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
            "SELECT id FROM tasks WHERE repeat_source_id = ? LIMIT 1",
        ))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if successor.is_some() {
            return Err(ApiError::conflict(
                "cannot reopen a repeating occurrence after its next occurrence was created",
            ));
        }
    }

    sqlx::query(&*crate::db::sql(
        "UPDATE tasks SET updated_at = ?, name = ?, description = ?, due_date = ?, due_time = ?, deadline = ?, repeat_rule = ?, repeat_anchor = ?, priority = ?, completed_at = ?, project_id = ? WHERE id = ?",
    ))
    .bind(&now)
    .bind(new_name)
    .bind(new_description)
    .bind(new_due_date)
    .bind(new_due_time)
    .bind(new_deadline)
    .bind(new_repeat_rule)
    .bind(new_repeat_anchor)
    .bind(new_priority)
    .bind(new_completed_at)
    .bind(new_project_id)
    .bind(id)
    .execute(&mut *tx)
    .await?;

    // Completing a parent completes its remaining subtasks.
    let mut completed_subtasks: Vec<i64> = Vec::new();
    if completing {
        let rows: Vec<(i64,)> = sqlx::query_as(&*crate::db::sql(
            "SELECT id FROM tasks WHERE parent_id = ? AND completed_at IS NULL",
        ))
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        completed_subtasks = rows.into_iter().map(|r| r.0).collect();
        sqlx::query(&*crate::db::sql("UPDATE tasks SET completed_at = ?, updated_at = ? WHERE parent_id = ? AND completed_at IS NULL"))
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    // Subtasks follow their parent to a new project.
    if moved_from.is_some() {
        sqlx::query(&*crate::db::sql("UPDATE tasks SET project_id = (SELECT project_id FROM tasks WHERE id = ?) WHERE parent_id = ?"))
            .bind(id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    let sql = format!("SELECT {TASK_COLS} FROM tasks t WHERE t.id = ?");
    let task = sqlx::query_as::<_, Task>(&*crate::db::sql(&sql))
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let next_occurrences = if b.completed == Some(true) {
        ensure_next_occurrence(&mut tx, &task, completed_on).await?
    } else {
        Vec::new()
    };
    tx.commit().await?;

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
        for occurrence in &next_occurrences {
            publish_task(&st, occurrence).await;
        }
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
    for occurrence in &next_occurrences {
        crate::gcal::spawn_task_sync(&st, occurrence.id);
    }
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
    // Same discipline as the purge: lock, collect the subtree's files, delete, then clean up
    // after commit, so no upload can slip a file in between.
    let mut tx = st.db.pool.begin().await?;
    let postgres = st.db.kind == crate::db::DbKind::Postgres;
    if !postgres {
        sqlx::query("UPDATE tasks SET updated_at = updated_at WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let subtree = collect_subtree(&mut tx, postgres, &[id]).await?;
    // Re-check under the lock: the task may have moved projects since the check above.
    let locked: Option<(i64,)> =
        sqlx::query_as(&*crate::db::sql("SELECT project_id FROM tasks WHERE id = ?"))
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((root_project,)) = locked else {
        return Err(ApiError::not_found());
    };
    let allowed = member_projects(&mut tx, user.id).await?;
    if !allowed.contains(&root_project)
        || !roots_reaching_foreign_projects(&subtree.descendants, &allowed).is_empty()
    {
        return Err(ApiError::forbidden());
    }
    sqlx::query(&*crate::db::sql("DELETE FROM tasks WHERE id = ?"))
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let recipients = project_recipients(&st.db.pool, root_project).await;
    spawn_attachment_cleanup(&st, subtree.files.into_iter().map(|(name, _)| name).collect());
    st.hub
        .publish(recipients, "task.remove", json!({ "id": id }));
    // Cascade-deleted subtasks can live in other projects; tell those members too.
    let mut others: std::collections::BTreeMap<i64, Vec<i64>> = Default::default();
    for (sid, _, pid) in &subtree.descendants {
        others.entry(*pid).or_default().push(*sid);
    }
    for (pid, ids) in others {
        let recipients = project_recipients(&st.db.pool, pid).await;
        st.hub.publish(recipients.clone(), "tasks.remove", json!({ "ids": ids }));
        st.hub.publish(recipients, "tasks.refresh", json!({}));
    }
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

#[cfg(test)]
mod tests {
    use super::{next_repeating_due_date, roots_reaching_foreign_projects};

    #[test]
    fn roots_with_subtasks_in_foreign_projects_are_blocked() {
        let allowed = [1, 2].into_iter().collect();
        // (id, root, project): root 10 has a subtask in project 3, root 20 stays in 1/2.
        let descendants = [(11, 10, 1), (12, 10, 3), (21, 20, 2)];
        let blocked = roots_reaching_foreign_projects(&descendants, &allowed);
        assert_eq!(blocked, [10].into_iter().collect());
    }
    use chrono::NaiveDate;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn daily_repeat_skips_missed_occurrences() {
        assert_eq!(
            next_repeating_due_date(
                date("2026-07-01"),
                date("2026-07-01"),
                "daily",
                date("2026-07-13"),
            ),
            Some(date("2026-07-14"))
        );
    }

    #[test]
    fn weekly_repeat_keeps_the_original_weekday() {
        assert_eq!(
            next_repeating_due_date(
                date("2026-07-06"),
                date("2026-07-06"),
                "weekly",
                date("2026-07-13"),
            ),
            Some(date("2026-07-20"))
        );
    }

    #[test]
    fn monthly_repeat_preserves_anchor_day_after_short_month() {
        assert_eq!(
            next_repeating_due_date(
                date("2026-02-28"),
                date("2026-01-31"),
                "monthly",
                date("2026-02-28"),
            ),
            Some(date("2026-03-31"))
        );
    }

    #[test]
    fn yearly_repeat_recovers_leap_day() {
        assert_eq!(
            next_repeating_due_date(
                date("2027-02-28"),
                date("2024-02-29"),
                "yearly",
                date("2027-02-28"),
            ),
            Some(date("2028-02-29"))
        );
    }

    #[test]
    fn unknown_repeat_rule_is_rejected() {
        assert_eq!(
            next_repeating_due_date(
                date("2026-07-13"),
                date("2026-07-13"),
                "fortnightly",
                date("2026-07-13"),
            ),
            None
        );
    }
}

#[cfg(test)]
mod update_task_body_tests {
    use super::UpdateTask;

    #[test]
    fn explicit_null_clears_and_omitted_leaves_alone() {
        let b: UpdateTask = serde_json::from_str(r#"{"due_date":null,"due_time":null}"#).unwrap();
        assert_eq!(b.due_date, Some(None));
        assert_eq!(b.due_time, Some(None));
        assert_eq!(b.deadline, None);

        let b: UpdateTask = serde_json::from_str(r#"{"due_date":"2026-10-24"}"#).unwrap();
        assert_eq!(b.due_date, Some(Some("2026-10-24".to_string())));
        assert_eq!(b.due_time, None);
    }
}

#[cfg(test)]
mod completed_filter_tests {
    use super::{Bind, CompletedFilter, ListQuery};

    fn query(f: impl FnOnce(&mut ListQuery)) -> ListQuery {
        let mut q = ListQuery::default();
        f(&mut q);
        q
    }

    #[test]
    fn empty_filter_only_checks_completion_and_membership() {
        let filter = CompletedFilter::from_query(&ListQuery::default()).ok().unwrap();
        let (sql, binds) = filter.conditions();
        assert!(sql.contains("t.completed_at IS NOT NULL"));
        assert!(sql.contains("project_members WHERE user_id = ?"));
        assert!(!sql.contains("AND t.project_id = ?"));
        assert!(binds.is_empty());
    }

    #[test]
    fn bounds_are_normalised_and_bound_in_order() {
        let q = query(|q| {
            q.project_id = Some(7);
            q.completed_from = Some("2026-10-24T06:00:00-06:00".into());
            q.completed_to = Some("2026-10-25T05:59:59.999Z".into());
            q.due_from = Some("2026-10-25".into());
        });
        let filter = CompletedFilter::from_query(&q).ok().unwrap();
        let (sql, binds) = filter.conditions();
        assert!(sql.ends_with(
            "AND t.project_id = ? AND t.completed_at >= ? AND t.completed_at <= ? AND t.due_date >= ?"
        ));
        assert_eq!(
            binds,
            vec![
                Bind::Int(7),
                Bind::Text("2026-10-24T12:00:00.000Z".into()),
                Bind::Text("2026-10-25T05:59:59.999Z".into()),
                Bind::Text("2026-10-25".into())
            ]
        );
    }

    #[test]
    fn malformed_bounds_are_rejected() {
        let bad_date = query(|q| q.due_to = Some("tomorrow".into()));
        assert!(CompletedFilter::from_query(&bad_date).is_err());
        let bad_time = query(|q| q.completed_from = Some("2026-10-24".into()));
        assert!(CompletedFilter::from_query(&bad_time).is_err());
    }

    #[test]
    fn blank_bounds_are_ignored() {
        let q = query(|q| q.due_to = Some("  ".into()));
        let filter = CompletedFilter::from_query(&q).ok().unwrap();
        assert_eq!(filter.due_to, None);
    }
}

