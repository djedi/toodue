use std::borrow::Cow;
use std::path::Path;

use sqlx::any::{AnyConnectOptions, AnyPoolOptions};
use sqlx::AnyPool;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DbKind {
    Sqlite,
    Postgres,
}

#[derive(Clone)]
pub struct Database {
    pub pool: AnyPool,
    pub kind: DbKind,
}

impl Database {
    pub fn sql(&self, sqlite: &'static str, postgres: &'static str) -> &'static str {
        match self.kind {
            DbKind::Sqlite => sqlite,
            DbKind::Postgres => postgres,
        }
    }
}

use std::sync::OnceLock;

static DB_KIND: OnceLock<DbKind> = OnceLock::new();

pub fn sql(s: &str) -> Cow<'_, str> {
    let kind = DB_KIND.get().copied().unwrap_or(DbKind::Sqlite);
    if kind != DbKind::Postgres {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut param = 1usize;
    let mut chars = s.chars().peekable();
    let mut in_single = false;
    while let Some(c) = chars.next() {
        if c == '\'' {
            out.push(c);
            if in_single && chars.peek() == Some(&'\'') {
                out.push(chars.next().unwrap());
            } else {
                in_single = !in_single;
            }
        } else if c == '?' && !in_single {
            out.push('$');
            out.push_str(&param.to_string());
            param += 1;
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

pub async fn connect(data_dir: &Path) -> Result<Database, sqlx::Error> {
    sqlx::any::install_default_drivers();
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        let path = data_dir.join("toodue.db");
        format!("sqlite://{}?mode=rwc", path.display())
    });
    let opts: AnyConnectOptions = url.parse()?;
    let kind = if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        DbKind::Postgres
    } else if url.starts_with("sqlite:") {
        DbKind::Sqlite
    } else {
        return Err(sqlx::Error::Configuration(
            "TooDue supports sqlite and postgres only".into(),
        ));
    };
    let pool = AnyPoolOptions::new()
        .max_connections(8)
        .after_connect(move |connection, _meta| {
            Box::pin(async move {
                if kind == DbKind::Sqlite {
                    sqlx::query("PRAGMA foreign_keys = ON")
                        .execute(connection)
                        .await?;
                }
                Ok(())
            })
        })
        .connect_with(opts)
        .await?;
    let _ = DB_KIND.set(kind);
    if kind == DbKind::Sqlite {
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await?;
    }
    let schema = match kind {
        DbKind::Sqlite => include_str!("schema.sql"),
        DbKind::Postgres => include_str!("schema.postgres.sql"),
    };
    sqlx::raw_sql(schema).execute(&pool).await?;
    ensure_task_repeat_columns(&pool, kind).await?;
    Ok(Database { pool, kind })
}

async fn ensure_task_repeat_columns(pool: &AnyPool, kind: DbKind) -> Result<(), sqlx::Error> {
    match kind {
        DbKind::Postgres => {
            sqlx::query("ALTER TABLE tasks ADD COLUMN IF NOT EXISTS repeat_rule TEXT")
                .execute(pool)
                .await?;
            sqlx::query("ALTER TABLE tasks ADD COLUMN IF NOT EXISTS repeat_anchor TEXT")
                .execute(pool)
                .await?;
            sqlx::query(
                "ALTER TABLE tasks ADD COLUMN IF NOT EXISTS repeat_source_id BIGINT REFERENCES tasks(id) ON DELETE SET NULL",
            )
            .execute(pool)
            .await?;
        }
        DbKind::Sqlite => {
            for (column, definition) in [
                ("repeat_rule", "TEXT"),
                ("repeat_anchor", "TEXT"),
                (
                    "repeat_source_id",
                    "INTEGER REFERENCES tasks(id) ON DELETE SET NULL",
                ),
            ] {
                let exists: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM pragma_table_info('tasks') WHERE name = ?",
                )
                .bind(column)
                .fetch_one(pool)
                .await?;
                if exists == 0 {
                    let statement = format!("ALTER TABLE tasks ADD COLUMN {column} {definition}");
                    sqlx::query(&statement).execute(pool).await?;
                }
            }
        }
    }
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_repeat_source ON tasks(repeat_source_id)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_task_repeat_columns, DbKind};
    use sqlx::any::{AnyConnectOptions, AnyPoolOptions};

    #[tokio::test]
    async fn upgrades_legacy_sqlite_task_table() {
        sqlx::any::install_default_drivers();
        let path = std::env::temp_dir().join(format!(
            "toodue-repeat-migration-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let options: AnyConnectOptions = url.parse().unwrap();
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE tasks (id INTEGER PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();

        ensure_task_repeat_columns(&pool, DbKind::Sqlite)
            .await
            .unwrap();

        let columns: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM pragma_table_info('tasks') WHERE name LIKE 'repeat_%' ORDER BY cid",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            columns.into_iter().map(|row| row.0).collect::<Vec<_>>(),
            ["repeat_rule", "repeat_anchor", "repeat_source_id"]
        );
        let index_exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_tasks_repeat_source'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(index_exists, 1);

        pool.close().await;
        std::fs::remove_file(path).unwrap();
    }
}
