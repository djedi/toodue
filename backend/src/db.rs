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
        .connect_with(opts)
        .await?;
    let _ = DB_KIND.set(kind);
    if kind == DbKind::Sqlite {
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await?;
    }
    let schema = match kind {
        DbKind::Sqlite => include_str!("schema.sql"),
        DbKind::Postgres => include_str!("schema.postgres.sql"),
    };
    sqlx::raw_sql(schema).execute(&pool).await?;
    Ok(Database { pool, kind })
}
