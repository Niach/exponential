//! Local SQLite schema — a faithful port of the iOS `DatabaseManager.runMigrations`
//! (apps/ios/.../Data/DB/DatabaseManager.swift), mirroring the Postgres tables
//! Electric syncs to clients. Column names/nullability match packages/db-schema.
//!
//! SQLite affinities are looser than Postgres: uuid/timestamp/date/enum columns
//! are TEXT (ISO-8601 for timestamps), booleans are INTEGER, jsonb
//! (issues.description, comments.body) is TEXT, fractional sort orders are REAL.
//! The iOS v2 attachment width/height migration is folded into the table here.

const sqlite = @import("sqlite.zig");

pub fn run(conn: *sqlite.Conn) !void {
    try conn.exec(schema_sql);
}

const schema_sql =
    \\CREATE TABLE IF NOT EXISTS electric_offsets (
    \\  shape TEXT PRIMARY KEY,
    \\  handle TEXT NOT NULL,
    \\  "offset" TEXT NOT NULL
    \\);
    \\
    \\CREATE TABLE IF NOT EXISTS workspaces (
    \\  id TEXT PRIMARY KEY,
    \\  name TEXT NOT NULL,
    \\  slug TEXT NOT NULL,
    \\  icon_url TEXT,
    \\  is_public INTEGER NOT NULL DEFAULT 0,
    \\  public_write_policy TEXT NOT NULL DEFAULT 'members',
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\
    \\CREATE TABLE IF NOT EXISTS projects (
    \\  id TEXT PRIMARY KEY,
    \\  workspace_id TEXT NOT NULL,
    \\  name TEXT NOT NULL,
    \\  slug TEXT NOT NULL,
    \\  prefix TEXT NOT NULL,
    \\  color TEXT NOT NULL DEFAULT '#6366f1',
    \\  sort_order REAL NOT NULL DEFAULT 0,
    \\  archived_at TEXT,
    \\  github_repo TEXT,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_projects_workspace ON projects(workspace_id);
    \\
    \\CREATE TABLE IF NOT EXISTS issues (
    \\  id TEXT PRIMARY KEY,
    \\  project_id TEXT NOT NULL,
    \\  number INTEGER NOT NULL DEFAULT 0,
    \\  identifier TEXT NOT NULL DEFAULT '',
    \\  title TEXT NOT NULL,
    \\  description TEXT,
    \\  status TEXT NOT NULL DEFAULT 'backlog',
    \\  priority TEXT NOT NULL DEFAULT 'none',
    \\  assignee_id TEXT,
    \\  creator_id TEXT NOT NULL,
    \\  due_date TEXT,
    \\  due_time TEXT,
    \\  end_time TEXT,
    \\  sort_order REAL NOT NULL DEFAULT 0,
    \\  completed_at TEXT,
    \\  archived_at TEXT,
    \\  recurrence_interval INTEGER,
    \\  recurrence_unit TEXT,
    \\  google_calendar_event_id TEXT,
    \\  google_calendar_last_synced_at TEXT,
    \\  google_calendar_last_sync_error TEXT,
    \\  agent_plan_state TEXT,
    \\  agent_plan_revision INTEGER NOT NULL DEFAULT 0,
    \\  agent_plan_approved_at TEXT,
    \\  agent_plan_approved_by TEXT,
    \\  agent_last_comment_seen_at TEXT,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_issues_project ON issues(project_id);
    \\
    \\CREATE TABLE IF NOT EXISTS labels (
    \\  id TEXT PRIMARY KEY,
    \\  workspace_id TEXT NOT NULL,
    \\  name TEXT NOT NULL,
    \\  color TEXT NOT NULL DEFAULT '#6366f1',
    \\  sort_order REAL NOT NULL DEFAULT 0,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_labels_workspace ON labels(workspace_id);
    \\
    \\CREATE TABLE IF NOT EXISTS issue_labels (
    \\  issue_id TEXT NOT NULL,
    \\  label_id TEXT NOT NULL,
    \\  workspace_id TEXT NOT NULL,
    \\  PRIMARY KEY (issue_id, label_id)
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_issue_labels_label ON issue_labels(label_id);
    \\CREATE INDEX IF NOT EXISTS idx_issue_labels_workspace ON issue_labels(workspace_id);
    \\
    \\CREATE TABLE IF NOT EXISTS users (
    \\  id TEXT PRIMARY KEY,
    \\  name TEXT,
    \\  email TEXT NOT NULL,
    \\  image TEXT,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\
    \\CREATE TABLE IF NOT EXISTS workspace_members (
    \\  id TEXT PRIMARY KEY,
    \\  workspace_id TEXT NOT NULL,
    \\  user_id TEXT NOT NULL,
    \\  role TEXT NOT NULL,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_ws_members_workspace ON workspace_members(workspace_id);
    \\CREATE INDEX IF NOT EXISTS idx_ws_members_user ON workspace_members(user_id);
    \\
    \\CREATE TABLE IF NOT EXISTS workspace_invites (
    \\  id TEXT PRIMARY KEY,
    \\  workspace_id TEXT NOT NULL,
    \\  role TEXT NOT NULL,
    \\  token TEXT NOT NULL,
    \\  expires_at TEXT NOT NULL,
    \\  accepted_at TEXT,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_ws_invites_workspace ON workspace_invites(workspace_id);
    \\CREATE INDEX IF NOT EXISTS idx_ws_invites_token ON workspace_invites(token);
    \\
    \\CREATE TABLE IF NOT EXISTS comments (
    \\  id TEXT PRIMARY KEY,
    \\  issue_id TEXT NOT NULL,
    \\  workspace_id TEXT NOT NULL,
    \\  author_id TEXT NOT NULL,
    \\  body TEXT,
    \\  kind TEXT NOT NULL DEFAULT 'regular',
    \\  edited_at TEXT,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_comments_issue ON comments(issue_id);
    \\CREATE INDEX IF NOT EXISTS idx_comments_workspace ON comments(workspace_id);
    \\
    \\CREATE TABLE IF NOT EXISTS attachments (
    \\  id TEXT PRIMARY KEY,
    \\  workspace_id TEXT NOT NULL,
    \\  issue_id TEXT NOT NULL,
    \\  comment_id TEXT,
    \\  uploader_id TEXT NOT NULL,
    \\  filename TEXT NOT NULL,
    \\  content_type TEXT NOT NULL,
    \\  size_bytes INTEGER NOT NULL,
    \\  storage_key TEXT NOT NULL,
    \\  url TEXT NOT NULL,
    \\  width INTEGER,
    \\  height INTEGER,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_attachments_workspace ON attachments(workspace_id);
    \\CREATE INDEX IF NOT EXISTS idx_attachments_issue ON attachments(issue_id);
;
