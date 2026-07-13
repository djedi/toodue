#!/usr/bin/env python3
"""Generate PostgreSQL SQL that migrates a TooDue SQLite database.

Usage:
  python3 scripts/migrate-sqlite-to-postgres.py /path/to/toodue.db | \
    docker exec -i toodue-db psql -U toodue -d toodue -v ON_ERROR_STOP=1

The script writes SQL to stdout and never prints secrets. It preserves primary
keys so existing sessions, shared-project memberships, API keys, Google Calendar
mappings, comments, and attachments continue to line up after migration.
"""

from __future__ import annotations

import sqlite3
import sys
from pathlib import Path

TABLES: list[tuple[str, list[str]]] = [
    ("users", ["id", "email", "name", "password_hash", "ics_token", "created_at"]),
    ("sessions", ["token", "user_id", "expires_at", "created_at"]),
    ("projects", ["id", "name", "color", "parent_id", "owner_id", "is_inbox", "sort_order", "created_at"]),
    ("project_members", ["project_id", "user_id", "role"]),
    ("api_keys", ["id", "user_id", "name", "prefix", "token_hash", "last_used_at", "created_at"]),
    ("tasks", ["id", "project_id", "parent_id", "creator_id", "name", "description", "due_date", "due_time", "deadline", "repeat_rule", "repeat_anchor", "repeat_source_id", "priority", "completed_at", "sort_order", "created_at", "updated_at"]),
    ("comments", ["id", "task_id", "user_id", "body", "created_at"]),
    ("attachments", ["id", "task_id", "user_id", "filename", "stored_name", "mime", "size", "created_at"]),
    ("google_accounts", ["user_id", "access_token", "refresh_token", "token_expires_at", "calendar_id", "time_zone", "channel_id", "resource_id", "channel_expires_at", "sync_token", "created_at"]),
    ("gcal_events", ["user_id", "task_id", "event_id"]),
]

SEQUENCE_TABLES = ["users", "projects", "api_keys", "tasks", "comments", "attachments"]


def q_ident(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


def q_value(value: object) -> str:
    if value is None:
        return "NULL"
    if isinstance(value, bytes):
        return "decode('" + value.hex() + "', 'hex')"
    text = str(value)
    return "'" + text.replace("'", "''") + "'"


def table_exists(conn: sqlite3.Connection, table: str) -> bool:
    row = conn.execute(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?", (table,)
    ).fetchone()
    return row is not None


def table_columns(conn: sqlite3.Connection, table: str) -> set[str]:
    return {str(row[1]) for row in conn.execute(f"PRAGMA table_info({q_ident(table)})")}


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: migrate-sqlite-to-postgres.py /path/to/toodue.db", file=sys.stderr)
        return 2
    db_path = Path(sys.argv[1])
    if not db_path.exists():
        print(f"SQLite database not found: {db_path}", file=sys.stderr)
        return 2

    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row

    print("BEGIN;")
    # The schema's foreign keys are not DEFERRABLE, and projects/tasks are
    # self-referencing, so row order alone can violate parent_id constraints.
    # Disabling FK triggers for the session is safe here: the source data
    # already satisfied the same constraints in SQLite. Requires a superuser
    # connection (the bootstrap user of the official postgres image is one).
    print("SET session_replication_role = replica;")
    print(
        "TRUNCATE TABLE gcal_events, google_accounts, attachments, comments, tasks, "
        "api_keys, project_members, projects, sessions, users RESTART IDENTITY CASCADE;"
    )

    for table, columns in TABLES:
        if not table_exists(conn, table):
            continue
        available = table_columns(conn, table)
        col_sql = ", ".join(q_ident(c) for c in columns)
        select_columns = [
            q_ident(c) if c in available else f"NULL AS {q_ident(c)}" for c in columns
        ]
        select_sql = f"SELECT {', '.join(select_columns)} FROM {q_ident(table)}"
        for row in conn.execute(select_sql):
            values = ", ".join(q_value(row[c]) for c in columns)
            print(f"INSERT INTO {q_ident(table)} ({col_sql}) VALUES ({values});")

    for table in SEQUENCE_TABLES:
        print(
            "SELECT setval(pg_get_serial_sequence('"
            + table
            + "', 'id'), COALESCE((SELECT MAX(id) FROM "
            + q_ident(table)
            + "), 1), (SELECT COUNT(*) > 0 FROM "
            + q_ident(table)
            + "));"
        )

    print("COMMIT;")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
