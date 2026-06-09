//! Local agent state store — a rusqlite port of `apps/companion/src/state.ts`.
//! Tracks each assigned issue's pipeline status, the Electric shape offsets, and
//! a small kv (the pollControl cursor). WAL journal mode like the companion.

use rusqlite::{params, params_from_iter, types::Value, Connection};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS issues (
  id TEXT PRIMARY KEY,
  identifier TEXT NOT NULL,
  title TEXT NOT NULL,
  project_id TEXT NOT NULL,
  status TEXT NOT NULL,
  worktree_path TEXT,
  branch TEXT,
  repo_path TEXT,
  pr_url TEXT,
  driver TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  last_error TEXT,
  plan_revision INTEGER NOT NULL DEFAULT 0,
  claude_session_id TEXT,
  interactive_owned INTEGER NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS issues_status_idx ON issues(status);

CREATE TABLE IF NOT EXISTS shape_offsets (
  shape_name TEXT PRIMARY KEY,
  offset TEXT NOT NULL,
  handle TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS daemon_kv (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IssueRow {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub project_id: String,
    pub status: String,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
    pub repo_path: Option<String>,
    pub pr_url: Option<String>,
    pub driver: Option<String>,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub plan_revision: i64,
    // The claude session id of an interactive plan run, so approve-and-continue
    // can `--continue` it. `interactive_owned` = a desktop interactive session
    // owns this issue → the background dispatcher must NOT auto-run the code stage.
    pub claude_session_id: Option<String>,
    pub interactive_owned: i64,
    pub updated_at: i64,
}

/// The identity fields required to upsert an issue (the rest default).
pub struct IssueSeed<'a> {
    pub id: &'a str,
    pub identifier: &'a str,
    pub title: &'a str,
    pub project_id: &'a str,
    pub status: &'a str,
}

/// A sparse update — only `Some` fields are written (mirrors `patchIssue`'s
/// dynamic SET; current call sites never set columns back to NULL).
#[derive(Debug, Clone, Default)]
pub struct IssuePatch {
    pub status: Option<String>,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
    pub repo_path: Option<String>,
    pub pr_url: Option<String>,
    pub driver: Option<String>,
    pub attempts: Option<i64>,
    pub last_error: Option<String>,
    pub plan_revision: Option<i64>,
    pub claude_session_id: Option<String>,
    pub interactive_owned: Option<i64>,
}

pub struct ShapeOffset {
    pub shape_name: String,
    pub offset: String,
    pub handle: String,
}

pub struct State {
    conn: Connection,
}

impl State {
    /// Open (creating) the state db at `path` (`:memory:` for tests), apply the
    /// schema, and enable WAL.
    pub fn open(path: &str) -> rusqlite::Result<State> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.execute_batch(SCHEMA)?;
        Ok(State { conn })
    }

    pub fn upsert_issue(&self, seed: &IssueSeed) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO issues (id, identifier, title, project_id, status, attempts, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)
             ON CONFLICT(id) DO UPDATE SET
               identifier = excluded.identifier,
               title = excluded.title,
               project_id = excluded.project_id,
               status = excluded.status,
               updated_at = excluded.updated_at",
            params![seed.id, seed.identifier, seed.title, seed.project_id, seed.status, now_ms()],
        )?;
        Ok(())
    }

    pub fn get_issue(&self, id: &str) -> rusqlite::Result<Option<IssueRow>> {
        let mut stmt = self.conn.prepare("SELECT * FROM issues WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], row_to_issue)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// Issues, newest-first, optionally filtered to a set of statuses.
    pub fn list_issues(&self, statuses: &[&str]) -> rusqlite::Result<Vec<IssueRow>> {
        if statuses.is_empty() {
            let mut stmt = self.conn.prepare("SELECT * FROM issues ORDER BY updated_at DESC")?;
            let rows = stmt.query_map([], row_to_issue)?;
            return rows.collect();
        }
        let placeholders = vec!["?"; statuses.len()].join(",");
        let sql = format!("SELECT * FROM issues WHERE status IN ({placeholders}) ORDER BY updated_at DESC");
        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<Value> = statuses.iter().map(|s| Value::Text(s.to_string())).collect();
        let rows = stmt.query_map(params_from_iter(params), row_to_issue)?;
        rows.collect()
    }

    pub fn set_issue_status(&self, id: &str, status: &str, error: Option<&str>) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE issues SET status = ?1, last_error = ?2, updated_at = ?3 WHERE id = ?4",
            params![status, error, now_ms(), id],
        )?;
        Ok(())
    }

    pub fn patch_issue(&self, id: &str, patch: &IssuePatch) -> rusqlite::Result<()> {
        let mut sets: Vec<&str> = Vec::new();
        let mut vals: Vec<Value> = Vec::new();
        macro_rules! field {
            ($opt:expr, $col:literal, $conv:expr) => {
                if let Some(v) = &$opt {
                    sets.push(concat!($col, " = ?"));
                    vals.push($conv(v.clone()));
                }
            };
        }
        field!(patch.status, "status", Value::Text);
        field!(patch.worktree_path, "worktree_path", Value::Text);
        field!(patch.branch, "branch", Value::Text);
        field!(patch.repo_path, "repo_path", Value::Text);
        field!(patch.pr_url, "pr_url", Value::Text);
        field!(patch.driver, "driver", Value::Text);
        field!(patch.attempts, "attempts", Value::Integer);
        field!(patch.last_error, "last_error", Value::Text);
        field!(patch.plan_revision, "plan_revision", Value::Integer);
        field!(patch.claude_session_id, "claude_session_id", Value::Text);
        field!(patch.interactive_owned, "interactive_owned", Value::Integer);
        if sets.is_empty() {
            return Ok(());
        }
        sets.push("updated_at = ?");
        vals.push(Value::Integer(now_ms()));
        vals.push(Value::Text(id.to_string()));
        let sql = format!("UPDATE issues SET {} WHERE id = ?", sets.join(", "));
        self.conn.execute(&sql, params_from_iter(vals))?;
        Ok(())
    }

    /// Clear per-run fields on (re)assignment (mirrors the dispatcher's
    /// `patchIssue` reset, which sets these back to NULL / 0).
    pub fn reset_for_assignment(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE issues SET last_error = NULL, plan_revision = 0, pr_url = NULL,
               worktree_path = NULL, branch = NULL, claude_session_id = NULL,
               interactive_owned = 0, updated_at = ?1 WHERE id = ?2",
            params![now_ms(), id],
        )?;
        Ok(())
    }

    /// Startup sweep: no interactive session can survive an app restart (the
    /// host terminal dies with the process), so any `interactive_owned` flag
    /// found at boot is stale by construction. Clearing them here means a crash
    /// mid-session can never permanently block an issue's background pipeline.
    pub fn clear_interactive_owned_all(&self) -> rusqlite::Result<usize> {
        self.conn.execute(
            "UPDATE issues SET interactive_owned = 0, updated_at = ?1 WHERE interactive_owned != 0",
            params![now_ms()],
        )
    }

    pub fn bump_attempts(&self, id: &str) -> rusqlite::Result<i64> {
        self.conn.execute(
            "UPDATE issues SET attempts = attempts + 1, updated_at = ?1 WHERE id = ?2",
            params![now_ms(), id],
        )?;
        self.conn
            .query_row("SELECT attempts FROM issues WHERE id = ?1", params![id], |r| r.get(0))
            .or(Ok(0))
    }

    pub fn save_offset(&self, o: &ShapeOffset) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO shape_offsets (shape_name, offset, handle) VALUES (?1, ?2, ?3)
             ON CONFLICT(shape_name) DO UPDATE SET offset = excluded.offset, handle = excluded.handle",
            params![o.shape_name, o.offset, o.handle],
        )?;
        Ok(())
    }

    pub fn load_offset(&self, shape_name: &str) -> rusqlite::Result<Option<ShapeOffset>> {
        let r = self
            .conn
            .query_row(
                "SELECT shape_name, offset, handle FROM shape_offsets WHERE shape_name = ?1",
                params![shape_name],
                |r| {
                    Ok(ShapeOffset {
                        shape_name: r.get(0)?,
                        offset: r.get(1)?,
                        handle: r.get(2)?,
                    })
                },
            )
            .ok();
        Ok(r)
    }

    pub fn kv_get(&self, key: &str) -> rusqlite::Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM daemon_kv WHERE key = ?1", params![key], |r| r.get(0))
            .ok())
    }

    pub fn kv_set(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO daemon_kv (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

fn row_to_issue(r: &rusqlite::Row) -> rusqlite::Result<IssueRow> {
    Ok(IssueRow {
        id: r.get("id")?,
        identifier: r.get("identifier")?,
        title: r.get("title")?,
        project_id: r.get("project_id")?,
        status: r.get("status")?,
        worktree_path: r.get("worktree_path")?,
        branch: r.get("branch")?,
        repo_path: r.get("repo_path")?,
        pr_url: r.get("pr_url")?,
        driver: r.get("driver")?,
        attempts: r.get("attempts")?,
        last_error: r.get("last_error")?,
        plan_revision: r.get("plan_revision")?,
        claude_session_id: r.get("claude_session_id")?,
        interactive_owned: r.get("interactive_owned")?,
        updated_at: r.get("updated_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed<'a>(id: &'a str) -> IssueSeed<'a> {
        IssueSeed { id, identifier: "EXP-1", title: "t", project_id: "p", status: "queued" }
    }

    #[test]
    fn upsert_get_and_status() {
        let s = State::open(":memory:").unwrap();
        s.upsert_issue(&seed("a")).unwrap();
        let got = s.get_issue("a").unwrap().unwrap();
        assert_eq!(got.status, "queued");
        assert_eq!(got.plan_revision, 0);

        s.set_issue_status("a", "failed", Some("boom")).unwrap();
        let got = s.get_issue("a").unwrap().unwrap();
        assert_eq!(got.status, "failed");
        assert_eq!(got.last_error.as_deref(), Some("boom"));

        // upsert preserves status-independent fields and updates identity.
        s.upsert_issue(&IssueSeed { id: "a", identifier: "EXP-1", title: "renamed", project_id: "p", status: "queued" }).unwrap();
        assert_eq!(s.get_issue("a").unwrap().unwrap().title, "renamed");
    }

    #[test]
    fn patch_and_attempts() {
        let s = State::open(":memory:").unwrap();
        s.upsert_issue(&seed("a")).unwrap();
        s.patch_issue("a", &IssuePatch { branch: Some("agent/x".into()), plan_revision: Some(3), ..Default::default() }).unwrap();
        let got = s.get_issue("a").unwrap().unwrap();
        assert_eq!(got.branch.as_deref(), Some("agent/x"));
        assert_eq!(got.plan_revision, 3);

        assert_eq!(s.bump_attempts("a").unwrap(), 1);
        assert_eq!(s.bump_attempts("a").unwrap(), 2);
    }

    #[test]
    fn list_filter() {
        let s = State::open(":memory:").unwrap();
        s.upsert_issue(&IssueSeed { id: "a", identifier: "EXP-1", title: "t", project_id: "p", status: "coding" }).unwrap();
        s.upsert_issue(&IssueSeed { id: "b", identifier: "EXP-2", title: "t", project_id: "p", status: "done" }).unwrap();
        assert_eq!(s.list_issues(&[]).unwrap().len(), 2);
        let coding = s.list_issues(&["coding"]).unwrap();
        assert_eq!(coding.len(), 1);
        assert_eq!(coding[0].id, "a");
    }

    #[test]
    fn offsets_and_kv() {
        let s = State::open(":memory:").unwrap();
        s.save_offset(&ShapeOffset { shape_name: "ai".into(), offset: "0_1".into(), handle: "h".into() }).unwrap();
        s.save_offset(&ShapeOffset { shape_name: "ai".into(), offset: "0_2".into(), handle: "h".into() }).unwrap();
        assert_eq!(s.load_offset("ai").unwrap().unwrap().offset, "0_2");
        assert!(s.load_offset("missing").unwrap().is_none());

        assert!(s.kv_get("cursor").unwrap().is_none());
        s.kv_set("cursor", "2026-06-01").unwrap();
        assert_eq!(s.kv_get("cursor").unwrap().as_deref(), Some("2026-06-01"));
    }
}
