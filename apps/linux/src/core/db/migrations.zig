//! Local SQLite schema — a faithful port of the iOS `DatabaseManager.runMigrations`
//! (apps/ios/.../Data/DB/DatabaseManager.swift), mirroring the Postgres tables
//! Electric syncs to clients. Column names/nullability match packages/db-schema.
//!
//! SQLite affinities are looser than Postgres: uuid/timestamp/date/enum columns
//! are TEXT (ISO-8601 for timestamps), booleans are INTEGER, jsonb
//! (issues.description, comments.body) is TEXT, fractional sort orders are REAL.
//! The iOS v2 attachment width/height migration is folded into the table here.

const std = @import("std");
const sqlite = @import("sqlite.zig");

pub fn run(conn: *sqlite.Conn) !void {
    try conn.exec(schema_sql);
    // Self-heal existing caches: `CREATE TABLE IF NOT EXISTS issues` is a no-op
    // when the table already exists, so columns added in later releases would be
    // missing on an upgraded install (and a synced issue row carrying them would
    // fail to UPSERT). ALTER-ADD any new column that isn't present yet. New
    // tables don't need this — IF NOT EXISTS creates them in full.
    for (new_issue_columns) |col| {
        if (!issueHasColumn(conn, col.name)) try conn.exec(col.ddl);
    }
    // users.is_agent (added server-side in the agent redesign). Without it, the
    // users shape — which now carries is_agent — fails to UPSERT and the local
    // users table stays empty (members render as raw ids).
    if (!tableHasColumn(conn, "users", "is_agent")) {
        try conn.exec("ALTER TABLE users ADD COLUMN is_agent INTEGER");
    }
}

const IssueColumn = struct { name: []const u8, ddl: [:0]const u8 };

// Columns added to `issues` after the original schema. Keep in sync with the
// CREATE TABLE above and packages/db-schema/src/schema.ts.
const new_issue_columns = [_]IssueColumn{
    .{ .name = "pr_url", .ddl = "ALTER TABLE issues ADD COLUMN pr_url TEXT" },
    .{ .name = "pr_number", .ddl = "ALTER TABLE issues ADD COLUMN pr_number INTEGER" },
    .{ .name = "pr_state", .ddl = "ALTER TABLE issues ADD COLUMN pr_state TEXT" },
    .{ .name = "branch", .ddl = "ALTER TABLE issues ADD COLUMN branch TEXT" },
    .{ .name = "pr_merged_at", .ddl = "ALTER TABLE issues ADD COLUMN pr_merged_at TEXT" },
    .{ .name = "agent_session_id", .ddl = "ALTER TABLE issues ADD COLUMN agent_session_id TEXT" },
    .{ .name = "agent_run_mode", .ddl = "ALTER TABLE issues ADD COLUMN agent_run_mode TEXT" },
    .{ .name = "agent_interactive_claimed_at", .ddl = "ALTER TABLE issues ADD COLUMN agent_interactive_claimed_at TEXT" },
};

fn issueHasColumn(conn: *sqlite.Conn, name: []const u8) bool {
    return tableHasColumn(conn, "issues", name);
}

fn tableHasColumn(conn: *sqlite.Conn, table: []const u8, name: []const u8) bool {
    var buf: [128]u8 = undefined;
    const sql = std.fmt.bufPrintZ(&buf, "PRAGMA table_info({s})", .{table}) catch return true;
    var stmt = conn.prepare(sql) catch return true;
    defer stmt.finalize();
    // PRAGMA table_info columns: 0=cid, 1=name, 2=type, ...
    while (stmt.step() catch false) {
        if (std.mem.eql(u8, stmt.columnText(1), name)) return true;
    }
    return false;
}

test "run() is a no-op-safe full schema on a fresh DB" {
    var conn = try sqlite.Conn.open(":memory:");
    defer conn.close();
    try run(&conn);
    // Fresh CREATE TABLE includes every new column, so no ALTER should fire and
    // all must be present.
    for (new_issue_columns) |col| {
        try std.testing.expect(issueHasColumn(&conn, col.name));
    }
}

test "run() backfills new issue columns on an upgraded cache" {
    var conn = try sqlite.Conn.open(":memory:");
    defer conn.close();
    // Simulate a pre-existing cache: an old `issues` table missing the new cols
    // (includes project_id so schema_sql's idx_issues_project index still applies).
    try conn.exec(
        \\CREATE TABLE issues (
        \\  id TEXT PRIMARY KEY,
        \\  project_id TEXT NOT NULL,
        \\  title TEXT NOT NULL,
        \\  created_at TEXT NOT NULL,
        \\  updated_at TEXT NOT NULL
        \\);
    );
    try std.testing.expect(!issueHasColumn(&conn, "pr_url"));
    try run(&conn);
    for (new_issue_columns) |col| {
        try std.testing.expect(issueHasColumn(&conn, col.name));
    }
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
    \\  pr_url TEXT,
    \\  pr_number INTEGER,
    \\  pr_state TEXT,
    \\  branch TEXT,
    \\  pr_merged_at TEXT,
    \\  agent_session_id TEXT,
    \\  agent_run_mode TEXT,
    \\  agent_interactive_claimed_at TEXT,
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
    \\  is_agent INTEGER,
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
    \\
    \\CREATE TABLE IF NOT EXISTS notifications (
    \\  id TEXT PRIMARY KEY,
    \\  user_id TEXT NOT NULL,
    \\  issue_id TEXT,
    \\  type TEXT NOT NULL,
    \\  title TEXT NOT NULL,
    \\  body TEXT,
    \\  read_at TEXT,
    \\  pushed_at TEXT,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_notifications_user_unread ON notifications(user_id, read_at);
    \\
    \\CREATE TABLE IF NOT EXISTS issue_subscribers (
    \\  id TEXT PRIMARY KEY,
    \\  issue_id TEXT NOT NULL,
    \\  user_id TEXT NOT NULL,
    \\  workspace_id TEXT NOT NULL,
    \\  source TEXT NOT NULL,
    \\  unsubscribed INTEGER NOT NULL DEFAULT 0,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_issue_subscribers_user ON issue_subscribers(user_id);
    \\CREATE INDEX IF NOT EXISTS idx_issue_subscribers_workspace ON issue_subscribers(workspace_id);
    \\
    \\CREATE TABLE IF NOT EXISTS issue_events (
    \\  id TEXT PRIMARY KEY,
    \\  issue_id TEXT NOT NULL,
    \\  workspace_id TEXT NOT NULL,
    \\  actor_user_id TEXT,
    \\  type TEXT NOT NULL,
    \\  payload TEXT,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_issue_events_issue ON issue_events(issue_id);
    \\CREATE INDEX IF NOT EXISTS idx_issue_events_workspace ON issue_events(workspace_id);
    \\
    \\CREATE TABLE IF NOT EXISTS agent_runs (
    \\  issue_id TEXT PRIMARY KEY,
    \\  workspace_id TEXT NOT NULL,
    \\  plan_text TEXT,
    \\  question TEXT,
    \\  question_asked_at TEXT,
    \\  plan_revision INTEGER NOT NULL DEFAULT 0,
    \\  approved_at TEXT,
    \\  approved_by TEXT,
    \\  last_comment_seen_at TEXT,
    \\  session_id TEXT,
    \\  run_mode TEXT,
    \\  interactive_claimed_at TEXT,
    \\  interactive_claimed_expires_at TEXT,
    \\  last_error TEXT,
    \\  created_at TEXT NOT NULL,
    \\  updated_at TEXT NOT NULL
    \\);
    \\CREATE INDEX IF NOT EXISTS idx_agent_runs_workspace ON agent_runs(workspace_id);
;
