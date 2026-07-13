CREATE TABLE IF NOT EXISTS users (
  id            BIGSERIAL PRIMARY KEY,
  email         TEXT NOT NULL UNIQUE,
  name          TEXT NOT NULL,
  password_hash TEXT NOT NULL,
  ics_token     TEXT NOT NULL UNIQUE,
  created_at    TEXT NOT NULL DEFAULT (to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
);

CREATE TABLE IF NOT EXISTS sessions (
  token      TEXT PRIMARY KEY,
  user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
);

CREATE TABLE IF NOT EXISTS projects (
  id         BIGSERIAL PRIMARY KEY,
  name       TEXT NOT NULL,
  color      TEXT NOT NULL DEFAULT 'slate',
  parent_id  BIGINT REFERENCES projects(id) ON DELETE CASCADE,
  owner_id   BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  is_inbox   BIGINT NOT NULL DEFAULT 0,
  sort_order BIGINT NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
);

CREATE TABLE IF NOT EXISTS project_members (
  project_id BIGINT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  role       TEXT NOT NULL DEFAULT 'member',
  PRIMARY KEY (project_id, user_id)
);

CREATE TABLE IF NOT EXISTS api_keys (
  id           BIGSERIAL PRIMARY KEY,
  user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  prefix       TEXT NOT NULL,
  token_hash   TEXT NOT NULL UNIQUE,
  last_used_at TEXT,
  created_at   TEXT NOT NULL DEFAULT (to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
);

CREATE TABLE IF NOT EXISTS tasks (
  id           BIGSERIAL PRIMARY KEY,
  project_id   BIGINT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  parent_id    BIGINT REFERENCES tasks(id) ON DELETE CASCADE,
  creator_id   BIGINT NOT NULL REFERENCES users(id),
  name         TEXT NOT NULL,
  description  TEXT NOT NULL DEFAULT '',
  due_date     TEXT,
  due_time     TEXT,
  deadline      TEXT,
  repeat_rule      TEXT,
  repeat_anchor    TEXT,
  repeat_source_id BIGINT REFERENCES tasks(id) ON DELETE SET NULL,
  priority         BIGINT NOT NULL DEFAULT 4,
  completed_at TEXT,
  sort_order   BIGINT NOT NULL DEFAULT 0,
  created_at   TEXT NOT NULL DEFAULT (to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')),
  updated_at   TEXT NOT NULL DEFAULT (to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
);

CREATE TABLE IF NOT EXISTS comments (
  id         BIGSERIAL PRIMARY KEY,
  task_id    BIGINT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  body       TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
);

CREATE TABLE IF NOT EXISTS attachments (
  id          BIGSERIAL PRIMARY KEY,
  task_id     BIGINT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
  user_id     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  filename    TEXT NOT NULL,
  stored_name TEXT NOT NULL,
  mime        TEXT NOT NULL,
  size        BIGINT NOT NULL,
  created_at  TEXT NOT NULL DEFAULT (to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
);

CREATE TABLE IF NOT EXISTS project_templates (
  id          BIGSERIAL PRIMARY KEY,
  owner_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name        TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  color       TEXT NOT NULL DEFAULT 'slate',
  tasks_json  TEXT NOT NULL,
  created_at  TEXT NOT NULL DEFAULT (to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
);

CREATE TABLE IF NOT EXISTS google_accounts (
  user_id            BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  access_token       TEXT NOT NULL,
  refresh_token      TEXT NOT NULL,
  token_expires_at   TEXT NOT NULL,
  calendar_id        TEXT NOT NULL,
  time_zone          TEXT NOT NULL DEFAULT 'UTC',
  channel_id         TEXT,
  resource_id        TEXT,
  channel_expires_at TEXT,
  sync_token         TEXT,
  created_at         TEXT NOT NULL DEFAULT (to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'))
);

CREATE TABLE IF NOT EXISTS gcal_events (
  user_id  BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  task_id  BIGINT NOT NULL,
  event_id TEXT NOT NULL,
  PRIMARY KEY (user_id, task_id)
);

CREATE INDEX IF NOT EXISTS idx_gcal_events_task ON gcal_events(task_id);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_projects_owner ON projects(owner_id);
CREATE INDEX IF NOT EXISTS idx_members_user ON project_members(user_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_user ON api_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_project_templates_owner ON project_templates(owner_id);
CREATE INDEX IF NOT EXISTS idx_tasks_project ON tasks(project_id);
CREATE INDEX IF NOT EXISTS idx_tasks_parent ON tasks(parent_id);
CREATE INDEX IF NOT EXISTS idx_comments_task ON comments(task_id);
CREATE INDEX IF NOT EXISTS idx_attachments_task ON attachments(task_id);
