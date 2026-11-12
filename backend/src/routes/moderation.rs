//! Reporting objectionable content and blocking abusive users, as required
//! for apps with user-generated content (App Store Review Guideline 1.2).

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{AnyPool, FromRow};

use crate::auth::AuthUser;
use crate::error::{ApiError, ApiResult};
use crate::events::project_recipients;
use crate::routes::projects::{project_json, require_member};
use crate::AppState;

pub const REPORT_REASONS: &[&str] = &[
    "spam",
    "harassment",
    "hate",
    "sexual",
    "violence",
    "illegal",
    "other",
];

const MAX_REPORTS_PER_DAY: i64 = 20;

/// True when either user has blocked the other.
pub async fn blocked_between(db: &AnyPool, a: i64, b: i64) -> ApiResult<bool> {
    let row: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT 1 FROM user_blocks WHERE (blocker_id = ? AND blocked_id = ?) \
         OR (blocker_id = ? AND blocked_id = ?)",
    ))
    .bind(a)
    .bind(b)
    .bind(b)
    .bind(a)
    .fetch_optional(db)
    .await?;
    Ok(row.is_some())
}

/// Ids of users who have blocked `author`; they should not receive the
/// author's comments.
pub async fn blockers_of(db: &AnyPool, author: i64) -> Vec<i64> {
    sqlx::query_as::<_, (i64,)>(&*crate::db::sql(
        "SELECT blocker_id FROM user_blocks WHERE blocked_id = ?",
    ))
    .bind(author)
    .fetch_all(db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| r.0)
    .collect()
}

/// True when the project's owner and `user` have blocked one another, so
/// nobody may add `user` to that project.
pub async fn blocked_from_project(db: &AnyPool, project_id: i64, user: i64) -> ApiResult<bool> {
    let row: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT 1 FROM projects p JOIN user_blocks b ON \
         (b.blocker_id = p.owner_id AND b.blocked_id = ?) OR (b.blocker_id = ? AND b.blocked_id = p.owner_id) \
         WHERE p.id = ?",
    ))
    .bind(user)
    .bind(user)
    .bind(project_id)
    .fetch_optional(db)
    .await?;
    Ok(row.is_some())
}

/// True when `other` authored something the user can currently see (a task,
/// comment or attachment in one of the user's projects), even if `other` has
/// since left every shared project.
async fn authored_visible_content(db: &AnyPool, user: i64, other: i64) -> ApiResult<bool> {
    let row: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT 1 FROM tasks t JOIN project_members m ON m.project_id = t.project_id AND m.user_id = ? \
         WHERE t.creator_id = ? \
            OR EXISTS (SELECT 1 FROM comments c WHERE c.task_id = t.id AND c.user_id = ?) \
            OR EXISTS (SELECT 1 FROM attachments a WHERE a.task_id = t.id AND a.user_id = ?) \
         LIMIT 1",
    ))
    .bind(user)
    .bind(other)
    .bind(other)
    .bind(other)
    .fetch_optional(db)
    .await?;
    Ok(row.is_some())
}

async fn knows(db: &AnyPool, user: i64, other: i64) -> ApiResult<bool> {
    Ok(shares_project(db, user, other).await? || authored_visible_content(db, user, other).await?)
}

async fn shares_project(db: &AnyPool, a: i64, b: i64) -> ApiResult<bool> {
    let row: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT 1 FROM project_members m1 JOIN project_members m2 \
         ON m2.project_id = m1.project_id WHERE m1.user_id = ? AND m2.user_id = ?",
    ))
    .bind(a)
    .bind(b)
    .fetch_optional(db)
    .await?;
    Ok(row.is_some())
}

#[derive(Deserialize)]
pub struct ReportBody {
    pub kind: String,
    pub target_id: i64,
    pub reason: String,
    #[serde(default)]
    pub details: String,
}

pub async fn report(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(b): Json<ReportBody>,
) -> ApiResult<Json<Value>> {
    let reason = b.reason.trim().to_lowercase();
    if !REPORT_REASONS.contains(&reason.as_str()) {
        return Err(ApiError::bad_request("choose a reason for the report"));
    }
    let details: String = b.details.trim().chars().take(2000).collect();
    let db = &st.db.pool;

    // Resolve the author, a text snapshot and the project the reporter must
    // belong to. Missing content and content the reporter can't see both
    // answer 404 so reports can't probe for ids.
    // Caps report spam and the storage that retained attachment copies use.
    let since = (chrono::Utc::now() - chrono::Duration::hours(24))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    let (recent,): (i64,) = sqlx::query_as(&*crate::db::sql(
        "SELECT COUNT(*) FROM content_reports WHERE reporter_id = ? AND created_at > ?",
    ))
    .bind(user.id)
    .bind(&since)
    .fetch_one(db)
    .await?;
    if recent >= MAX_REPORTS_PER_DAY {
        return Err(ApiError::bad_request(
            "you've sent a lot of reports today — email support@toodue.com for anything urgent",
        ));
    }

    let mut retain: Option<String> = None;
    let (author, snapshot) = match b.kind.as_str() {
        "comment" => {
            let row: Option<(i64, String, i64)> = sqlx::query_as(&*crate::db::sql(
                "SELECT c.user_id, c.body, t.project_id FROM comments c \
                 JOIN tasks t ON t.id = c.task_id WHERE c.id = ?",
            ))
            .bind(b.target_id)
            .fetch_optional(db)
            .await?;
            let (author, body, project) = row.ok_or_else(ApiError::not_found)?;
            require_member(db, user.id, project)
                .await
                .map_err(|_| ApiError::not_found())?;
            (author, body)
        }
        "task" => {
            let row: Option<(i64, String, String, i64)> = sqlx::query_as(&*crate::db::sql(
                "SELECT creator_id, name, description, project_id FROM tasks WHERE id = ?",
            ))
            .bind(b.target_id)
            .fetch_optional(db)
            .await?;
            let (author, name, desc, project) = row.ok_or_else(ApiError::not_found)?;
            require_member(db, user.id, project)
                .await
                .map_err(|_| ApiError::not_found())?;
            (author, format!("{name}\n\n{desc}").trim().to_string())
        }
        "attachment" => {
            let row: Option<(i64, String, String, i64)> = sqlx::query_as(&*crate::db::sql(
                "SELECT a.user_id, a.filename, a.stored_name, t.project_id FROM attachments a \
                 JOIN tasks t ON t.id = a.task_id WHERE a.id = ?",
            ))
            .bind(b.target_id)
            .fetch_optional(db)
            .await?;
            let (author, filename, stored, project) = row.ok_or_else(ApiError::not_found)?;
            require_member(db, user.id, project)
                .await
                .map_err(|_| ApiError::not_found())?;
            retain = Some(stored.clone());
            (author, format!("{filename} (copy kept at reports/attachment-{}-{stored})", b.target_id))
        }
        "user" => {
            if b.target_id == user.id || !knows(db, user.id, b.target_id).await? {
                return Err(ApiError::not_found());
            }
            (b.target_id, String::new())
        }
        _ => return Err(ApiError::bad_request("unknown report type")),
    };
    // Any member can edit a task, so people may report text on tasks they
    // created; other content is only ever authored by one person.
    if author == user.id && b.kind != "task" {
        return Err(ApiError::bad_request("you can't report your own content"));
    }

    // Keep one copy of a reported attachment so moderators can still review it
    // if the uploader deletes it. Repeat reports reuse the copy.
    if let Some(stored) = &retain {
        let dir = st.data_dir.join("reports");
        let dest = dir.join(format!("attachment-{}-{stored}", b.target_id));
        if !tokio::fs::try_exists(&dest).await.unwrap_or(false) {
            // Copy to a unique temp file and rename, so a partial copy is never
            // mistaken for a finished one.
            tokio::fs::create_dir_all(&dir).await?;
            let tmp = dir.join(format!(".tmp-{}", crate::auth::random_token()));
            let res = match tokio::fs::copy(st.data_dir.join("attachments").join(stored), &tmp).await {
                Ok(_) => tokio::fs::rename(&tmp, &dest).await,
                Err(e) => Err(e),
            };
            if let Err(e) = res {
                let _ = tokio::fs::remove_file(&tmp).await;
                tracing::error!("could not keep reported attachment {stored}: {e}");
                return Err(ApiError::internal(
                    "couldn't save the report, please try again",
                ));
            }
        }
    }

    let author_label: Option<(String, String)> = sqlx::query_as(&*crate::db::sql(
        "SELECT name, email FROM users WHERE id = ?",
    ))
    .bind(author)
    .fetch_optional(db)
    .await?;
    let snapshot = match author_label {
        Some((name, email)) if snapshot.is_empty() => format!("{name} <{email}>"),
        Some((name, email)) if b.kind == "task" => {
            format!("Task created by {name} <{email}> (may have been edited by others):\n{snapshot}")
        }
        Some((name, email)) => format!("By {name} <{email}>:\n{snapshot}"),
        None => snapshot,
    };

    let (id,): (i64,) = sqlx::query_as(&*crate::db::sql(
        "INSERT INTO content_reports \
         (reporter_id, reporter_email, reported_user_id, kind, target_id, reason, details, snapshot) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    ))
    .bind(user.id)
    .bind(&user.email)
    .bind(author)
    .bind(&b.kind)
    .bind(b.target_id)
    .bind(&reason)
    .bind(&details)
    .bind(&snapshot)
    .fetch_one(db)
    .await?;

    tracing::warn!(
        report_id = id,
        kind = %b.kind,
        target_id = b.target_id,
        reported_user_id = author,
        reporter_id = user.id,
        reason = %reason,
        "content report filed"
    );
    notify_moderators(&st, id, &b.kind, &reason, &snapshot, &details);
    Ok(Json(json!({ "ok": true, "id": id })))
}

/// Posts new reports to `REPORTS_WEBHOOK_URL` (Slack-compatible `text`
/// payload) so moderators hear about them right away.
fn notify_moderators(
    st: &AppState,
    id: i64,
    kind: &str,
    reason: &str,
    snapshot: &str,
    details: &str,
) {
    let Ok(url) = std::env::var("REPORTS_WEBHOOK_URL") else {
        return;
    };
    if url.trim().is_empty() {
        return;
    }
    let excerpt: String = snapshot.chars().take(500).collect();
    let text = format!(
        "TooDue content report #{id}: {kind} reported for {reason}\n{excerpt}\nReporter notes: {details}"
    );
    let http = st.http.clone();
    tokio::spawn(async move {
        match http.post(url.trim()).json(&json!({ "text": text })).send().await {
            Ok(res) if !res.status().is_success() => {
                tracing::error!("report webhook for report {id} returned {}", res.status())
            }
            Ok(_) => {}
            Err(e) => tracing::error!("report webhook for report {id} failed: {e}"),
        }
    });
}

#[derive(Serialize, FromRow)]
pub struct BlockedUser {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub created_at: String,
}

pub async fn list_blocks(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
) -> ApiResult<Json<Vec<BlockedUser>>> {
    Ok(Json(
        sqlx::query_as::<_, BlockedUser>(&*crate::db::sql(
            "SELECT u.id, u.name, u.email, b.created_at FROM user_blocks b \
             JOIN users u ON u.id = b.blocked_id WHERE b.blocker_id = ? ORDER BY u.name",
        ))
        .bind(user.id)
        .fetch_all(&st.db.pool)
        .await?,
    ))
}

#[derive(Deserialize)]
pub struct BlockBody {
    pub user_id: i64,
}

/// Records the block and severs the projects the two users share directly:
/// the blocked user is removed from the blocker's projects, and the blocker
/// leaves the blocked user's projects. Returns the affected project ids.
pub(crate) async fn apply_block(
    db: &AnyPool,
    blocker: i64,
    blocked: i64,
) -> Result<Vec<(i64, i64)>, sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query(&*crate::db::sql(
        "INSERT INTO user_blocks (blocker_id, blocked_id) \
         SELECT ?, ? WHERE NOT EXISTS \
         (SELECT 1 FROM user_blocks WHERE blocker_id = ? AND blocked_id = ?)",
    ))
    .bind(blocker)
    .bind(blocked)
    .bind(blocker)
    .bind(blocked)
    .execute(&mut *tx)
    .await?;
    // (project_id, removed user id)
    let removed: Vec<(i64, i64)> = sqlx::query_as(&*crate::db::sql(
        "SELECT m.project_id, m.user_id FROM project_members m \
         JOIN projects p ON p.id = m.project_id \
         WHERE (p.owner_id = ? AND m.user_id = ?) OR (p.owner_id = ? AND m.user_id = ?)",
    ))
    .bind(blocker)
    .bind(blocked)
    .bind(blocked)
    .bind(blocker)
    .fetch_all(&mut *tx)
    .await?;
    for (project_id, member) in &removed {
        sqlx::query(&*crate::db::sql(
            "DELETE FROM project_members WHERE project_id = ? AND user_id = ?",
        ))
        .bind(project_id)
        .bind(member)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(removed)
}

pub async fn block(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Json(b): Json<BlockBody>,
) -> ApiResult<Json<Value>> {
    if b.user_id == user.id {
        return Err(ApiError::bad_request("you can't block yourself"));
    }
    let already: Option<(i64,)> = sqlx::query_as(&*crate::db::sql(
        "SELECT 1 FROM user_blocks WHERE blocker_id = ? AND blocked_id = ?",
    ))
    .bind(user.id)
    .bind(b.user_id)
    .fetch_optional(&st.db.pool)
    .await?;
    // Only people you collaborate with can be blocked, so ids can't be probed.
    if already.is_none() && !knows(&st.db.pool, user.id, b.user_id).await? {
        return Err(ApiError::not_found());
    }

    let removed = apply_block(&st.db.pool, user.id, b.user_id).await?;
    tracing::info!(blocker = user.id, blocked = b.user_id, "user blocked");
    for (project_id, member) in &removed {
        st.hub
            .publish(vec![*member], "project.remove", json!({ "id": project_id }));
        if let Ok(v) = project_json(&st.db.pool, *project_id).await {
            let recipients = project_recipients(&st.db.pool, *project_id).await;
            st.hub.publish(recipients, "project.upsert", v);
        }
        crate::gcal::spawn_project_sync(&st, *project_id);
    }
    Ok(Json(json!({
        "ok": true,
        "removed_projects": removed.iter().map(|r| r.0).collect::<Vec<_>>(),
    })))
}

pub async fn unblock(
    State(st): State<AppState>,
    AuthUser(user): AuthUser,
    Path(blocked): Path<i64>,
) -> ApiResult<Json<Value>> {
    sqlx::query(&*crate::db::sql(
        "DELETE FROM user_blocks WHERE blocker_id = ? AND blocked_id = ?",
    ))
    .bind(user.id)
    .bind(blocked)
    .execute(&st.db.pool)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use sqlx::any::{AnyConnectOptions, AnyPoolOptions};

    async fn pool() -> (sqlx::AnyPool, std::path::PathBuf) {
        sqlx::any::install_default_drivers();
        let path = std::env::temp_dir().join(format!(
            "toodue-moderation-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let opts: AnyConnectOptions = format!("sqlite://{}?mode=rwc", path.display())
            .parse()
            .unwrap();
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .after_connect(|c, _| {
                Box::pin(async move {
                    sqlx::query("PRAGMA foreign_keys = ON").execute(c).await?;
                    Ok(())
                })
            })
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../schema.sql"))
            .execute(&pool)
            .await
            .unwrap();
        (pool, path)
    }

    #[tokio::test]
    async fn block_severs_direct_shares_only_and_is_mutual_for_sharing() {
        let (pool, path) = pool().await;
        sqlx::raw_sql(
            "INSERT INTO users (id, email, name, password_hash, ics_token) VALUES \
               (1, 'a@x.com', 'A', 'h', 't1'), (2, 'b@x.com', 'B', 'h', 't2'), \
               (3, 'c@x.com', 'C', 'h', 't3');
             INSERT INTO projects (id, name, owner_id) VALUES (10, 'A', 1), (20, 'B', 2), (30, 'C', 3);
             INSERT INTO project_members (project_id, user_id) VALUES \
               (10, 1), (10, 2), (20, 2), (20, 1), (30, 3), (30, 1), (30, 2);",
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut removed = super::apply_block(&pool, 1, 2).await.unwrap();
        removed.sort();
        assert_eq!(removed, [(10, 2), (20, 1)]);
        // Blocking twice is a no-op.
        assert!(super::apply_block(&pool, 1, 2).await.unwrap().is_empty());

        let members: Vec<(i64, i64)> =
            sqlx::query_as("SELECT project_id, user_id FROM project_members ORDER BY 1, 2")
                .fetch_all(&pool)
                .await
                .unwrap();
        // A third party's project keeps both users.
        assert_eq!(members, [(10, 1), (20, 2), (30, 1), (30, 2), (30, 3)]);

        assert!(super::blocked_between(&pool, 1, 2).await.unwrap());
        assert!(super::blocked_between(&pool, 2, 1).await.unwrap());
        assert!(!super::blocked_between(&pool, 1, 3).await.unwrap());
        assert_eq!(super::blockers_of(&pool, 2).await, [1]);
        assert!(super::blockers_of(&pool, 1).await.is_empty());

        // A's project stays closed to B even when someone else shares it.
        assert!(super::blocked_from_project(&pool, 10, 2).await.unwrap());
        assert!(super::blocked_from_project(&pool, 20, 1).await.unwrap());
        assert!(!super::blocked_from_project(&pool, 30, 2).await.unwrap());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn authors_who_left_can_still_be_blocked() {
        let (pool, path) = pool().await;
        sqlx::raw_sql(
            "INSERT INTO users (id, email, name, password_hash, ics_token) VALUES \
               (1, 'a@x.com', 'A', 'h', 't1'), (2, 'b@x.com', 'B', 'h', 't2'), \
               (3, 'c@x.com', 'C', 'h', 't3');
             INSERT INTO projects (id, name, owner_id) VALUES (10, 'A', 1);
             INSERT INTO project_members (project_id, user_id) VALUES (10, 1);
             INSERT INTO tasks (id, project_id, creator_id, name) VALUES (100, 10, 1, 't');
             INSERT INTO comments (task_id, user_id, body) VALUES (100, 2, 'left behind');",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(super::knows(&pool, 1, 2).await.unwrap());
        assert!(!super::knows(&pool, 1, 3).await.unwrap());
        let _ = std::fs::remove_file(path);
    }
}
