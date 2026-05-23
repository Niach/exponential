import { Database } from "bun:sqlite"
import { mkdirSync } from "node:fs"
import { join } from "node:path"
import { STATE_DIR } from "./config"

export type IssueStatus =
  | `queued`
  | `claimed`
  | `planning`
  | `awaiting_approval`
  | `coding`
  | `testing`
  | `pushed`
  | `in_review`
  | `done`
  | `failed`
  | `stalled`
  | `cancelled`
  | `needs_human`

export interface IssueRow {
  id: string
  identifier: string
  title: string
  projectId: string
  status: IssueStatus
  worktreePath: string | null
  branch: string | null
  /** Path to the source git clone (parent of the worktree). */
  repoPath: string | null
  prUrl: string | null
  driver: string | null
  attempts: number
  lastError: string | null
  /**
   * Last `agentPlanRevision` we observed on the server when we entered the
   * pipeline. Used to detect "the server has the plan I just submitted" and
   * to avoid re-running plan mode when nothing changed across daemon
   * restarts.
   */
  planRevision: number
  updatedAt: number
}

export interface ShapeOffsetRow {
  shapeName: string
  offset: string
  handle: string
}

export interface StateHandle {
  db: Database
  close(): void
  upsertIssue(row: Partial<IssueRow> & Pick<IssueRow, `id` | `identifier` | `title` | `projectId` | `status`>): void
  getIssue(id: string): IssueRow | null
  listIssues(filter?: { status?: IssueStatus[] }): IssueRow[]
  setIssueStatus(id: string, status: IssueStatus, error?: string | null): void
  patchIssue(id: string, patch: Partial<IssueRow>): void
  bumpAttempts(id: string): number
  saveOffset(row: ShapeOffsetRow): void
  loadOffset(shapeName: string): ShapeOffsetRow | null
}

const SCHEMA = `
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
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS issues_status_idx ON issues(status);

CREATE TABLE IF NOT EXISTS shape_offsets (
  shape_name TEXT PRIMARY KEY,
  offset TEXT NOT NULL,
  handle TEXT NOT NULL
);
`

// Idempotent ALTERs for state.db files created before this column was added.
// Bun's sqlite throws on duplicate column; we swallow and move on.
const IDEMPOTENT_MIGRATIONS = [
  `ALTER TABLE issues ADD COLUMN repo_path TEXT`,
  `ALTER TABLE issues ADD COLUMN plan_revision INTEGER NOT NULL DEFAULT 0`,
]

function rowToIssue(row: Record<string, unknown>): IssueRow {
  return {
    id: row.id as string,
    identifier: row.identifier as string,
    title: row.title as string,
    projectId: row.project_id as string,
    status: row.status as IssueStatus,
    worktreePath: (row.worktree_path as string | null) ?? null,
    branch: (row.branch as string | null) ?? null,
    repoPath: (row.repo_path as string | null) ?? null,
    prUrl: (row.pr_url as string | null) ?? null,
    driver: (row.driver as string | null) ?? null,
    attempts: row.attempts as number,
    lastError: (row.last_error as string | null) ?? null,
    planRevision: (row.plan_revision as number | null) ?? 0,
    updatedAt: row.updated_at as number,
  }
}

export function openState(): StateHandle {
  mkdirSync(STATE_DIR, { recursive: true })
  const db = new Database(join(STATE_DIR, `state.db`), { create: true })
  db.exec(`PRAGMA journal_mode = WAL;`)
  db.exec(SCHEMA)
  for (const stmt of IDEMPOTENT_MIGRATIONS) {
    try {
      db.exec(stmt)
    } catch {
      // duplicate-column or already-applied — fine
    }
  }

  return {
    db,
    close: () => db.close(),
    upsertIssue: (row) => {
      const now = Date.now()
      db.run(
        `INSERT INTO issues (id, identifier, title, project_id, status, worktree_path, branch, pr_url, driver, attempts, last_error, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, COALESCE(?, 0), ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           identifier = excluded.identifier,
           title = excluded.title,
           project_id = excluded.project_id,
           status = excluded.status,
           updated_at = excluded.updated_at`,
        [
          row.id,
          row.identifier,
          row.title,
          row.projectId,
          row.status,
          row.worktreePath ?? null,
          row.branch ?? null,
          row.prUrl ?? null,
          row.driver ?? null,
          row.attempts ?? 0,
          row.lastError ?? null,
          now,
        ]
      )
    },
    getIssue: (id) => {
      const row = db
        .query(`SELECT * FROM issues WHERE id = ?`)
        .get(id) as Record<string, unknown> | null
      return row ? rowToIssue(row) : null
    },
    listIssues: (filter) => {
      let sql = `SELECT * FROM issues`
      const params: string[] = []
      if (filter?.status?.length) {
        const placeholders = filter.status.map(() => `?`).join(`,`)
        sql += ` WHERE status IN (${placeholders})`
        params.push(...filter.status)
      }
      sql += ` ORDER BY updated_at DESC`
      const rows = db.query(sql).all(...params) as Record<string, unknown>[]
      return rows.map(rowToIssue)
    },
    setIssueStatus: (id, status, error) => {
      db.run(
        `UPDATE issues SET status = ?, last_error = ?, updated_at = ? WHERE id = ?`,
        [status, error ?? null, Date.now(), id]
      )
    },
    patchIssue: (id, patch) => {
      type Bind = string | number | null
      const fields: string[] = []
      const params: Bind[] = []
      const map: Record<string, string> = {
        status: `status`,
        worktreePath: `worktree_path`,
        branch: `branch`,
        repoPath: `repo_path`,
        prUrl: `pr_url`,
        driver: `driver`,
        attempts: `attempts`,
        lastError: `last_error`,
        planRevision: `plan_revision`,
      }
      for (const [k, dbKey] of Object.entries(map)) {
        if (k in patch) {
          fields.push(`${dbKey} = ?`)
          params.push(((patch as Record<string, unknown>)[k] as Bind) ?? null)
        }
      }
      if (fields.length === 0) return
      fields.push(`updated_at = ?`)
      params.push(Date.now(), id)
      db.run(`UPDATE issues SET ${fields.join(`, `)} WHERE id = ?`, params)
    },
    bumpAttempts: (id) => {
      const row = db
        .query(`UPDATE issues SET attempts = attempts + 1, updated_at = ? WHERE id = ? RETURNING attempts`)
        .get(Date.now(), id) as { attempts: number } | null
      return row?.attempts ?? 0
    },
    saveOffset: (row) => {
      db.run(
        `INSERT INTO shape_offsets (shape_name, offset, handle) VALUES (?, ?, ?)
         ON CONFLICT(shape_name) DO UPDATE SET offset = excluded.offset, handle = excluded.handle`,
        [row.shapeName, row.offset, row.handle]
      )
    },
    loadOffset: (shapeName) => {
      const row = db
        .query(`SELECT shape_name, offset, handle FROM shape_offsets WHERE shape_name = ?`)
        .get(shapeName) as
        | { shape_name: string; offset: string; handle: string }
        | null
      return row
        ? { shapeName: row.shape_name, offset: row.offset, handle: row.handle }
        : null
    },
  }
}
