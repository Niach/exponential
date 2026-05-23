import { afterEach, beforeEach, describe, expect, it } from "bun:test"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { Database } from "bun:sqlite"
import type { CompanionConfig } from "./config"
import {
  startDispatcher,
  type IssueEvent,
  type IssuePipeline,
} from "./dispatcher"
import type { StateHandle, IssueRow, IssueStatus } from "./state"
import pino from "pino"

const baseConfig: CompanionConfig = {
  exponential: {
    baseUrl: `http://localhost:5173`,
    workspaceId: `00000000-0000-0000-0000-000000000000`,
    workspaceSlug: `feedback`,
    agentId: `00000000-0000-0000-0000-000000000000`,
    botUserId: `00000000-0000-0000-0000-000000000000`,
  },
  driver: {
    default: `claude`,
    maxConcurrentIssues: 2,
    turnTimeoutMs: 60_000,
    issueBudgetMs: 60_000,
  },
  worktrees: {
    root: `/tmp/x`,
    minFreeBytes: 0,
    branchPrefix: `agent`,
  },
}

function inMemoryState(): StateHandle {
  const db = new Database(`:memory:`)
  db.exec(`
    CREATE TABLE issues (
      id TEXT PRIMARY KEY, identifier TEXT NOT NULL, title TEXT NOT NULL,
      project_id TEXT NOT NULL, status TEXT NOT NULL,
      worktree_path TEXT, branch TEXT, pr_url TEXT, driver TEXT,
      attempts INTEGER NOT NULL DEFAULT 0, last_error TEXT,
      updated_at INTEGER NOT NULL
    );
    CREATE TABLE shape_offsets (shape_name TEXT PRIMARY KEY, offset TEXT NOT NULL, handle TEXT NOT NULL);
  `)
  const rowToIssue = (r: Record<string, unknown>): IssueRow => ({
    id: r.id as string,
    identifier: r.identifier as string,
    title: r.title as string,
    projectId: r.project_id as string,
    status: r.status as IssueStatus,
    worktreePath: (r.worktree_path as string) ?? null,
    branch: (r.branch as string) ?? null,
    repoPath: (r.repo_path as string) ?? null,
    prUrl: (r.pr_url as string) ?? null,
    driver: (r.driver as string) ?? null,
    attempts: r.attempts as number,
    lastError: (r.last_error as string) ?? null,
    planRevision: (r.plan_revision as number | null) ?? 0,
    updatedAt: r.updated_at as number,
  })
  return {
    db,
    close: () => db.close(),
    upsertIssue: (row) => {
      db.run(
        `INSERT OR REPLACE INTO issues (id, identifier, title, project_id, status, attempts, updated_at) VALUES (?, ?, ?, ?, ?, COALESCE(?, 0), ?)`,
        [
          row.id,
          row.identifier,
          row.title,
          row.projectId,
          row.status,
          row.attempts ?? 0,
          Date.now(),
        ]
      )
    },
    getIssue: (id) => {
      const r = db.query(`SELECT * FROM issues WHERE id = ?`).get(id) as Record<
        string,
        unknown
      > | null
      return r ? rowToIssue(r) : null
    },
    listIssues: (filter) => {
      let sql = `SELECT * FROM issues`
      const params: string[] = []
      if (filter?.status?.length) {
        sql += ` WHERE status IN (${filter.status.map(() => `?`).join(`,`)})`
        params.push(...filter.status)
      }
      const rows = db.query(sql).all(...params) as Record<string, unknown>[]
      return rows.map(rowToIssue)
    },
    setIssueStatus: (id, status, error) => {
      db.run(
        `UPDATE issues SET status = ?, last_error = ?, updated_at = ? WHERE id = ?`,
        [status, error ?? null, Date.now(), id]
      )
    },
    patchIssue: () => {},
    bumpAttempts: () => 0,
    saveOffset: () => {},
    loadOffset: () => null,
    kvGet: () => null,
    kvSet: () => {},
  }
}

const SILENT = pino({ level: `silent` })
let tmpDir: string

beforeEach(() => {
  tmpDir = mkdtempSync(join(tmpdir(), `companion-test-`))
})
afterEach(() => {
  rmSync(tmpDir, { recursive: true, force: true })
})

function event(id: string, assigneeId: string | null = `bot`): IssueEvent {
  return {
    type: assigneeId ? `assigned` : `unassigned`,
    issueId: id,
    identifier: `EXP-${id}`,
    title: `Test issue ${id}`,
    projectId: `proj-${id}`,
    assigneeId,
  }
}

describe(`dispatcher`, () => {
  it(`runs the pipeline once per assigned issue and reaches done`, async () => {
    const state = inMemoryState()
    const seen: string[] = []
    const pipeline: IssuePipeline = async (issue, deps) => {
      seen.push(issue.id)
      deps.state.setIssueStatus(issue.id, `done`)
    }
    const d = startDispatcher({
      config: baseConfig,
      state,
      log: SILENT,
      pipeline,
    })
    d.enqueue(event(`a`))
    await new Promise((r) => setTimeout(r, 50))
    await d.stop()
    expect(seen).toEqual([`a`])
    expect(state.getIssue(`a`)?.status).toBe(`done`)
    state.close()
  })

  it(`ignores duplicate assigned events for an issue already in flight`, async () => {
    const state = inMemoryState()
    let resolveOuter: () => void = () => {}
    const gate = new Promise<void>((r) => (resolveOuter = r))
    let calls = 0
    const pipeline: IssuePipeline = async (issue, deps) => {
      calls++
      await gate
      deps.state.setIssueStatus(issue.id, `done`)
    }
    const d = startDispatcher({
      config: baseConfig,
      state,
      log: SILENT,
      pipeline,
    })
    d.enqueue(event(`a`))
    d.enqueue(event(`a`))
    d.enqueue(event(`a`))
    await new Promise((r) => setTimeout(r, 30))
    expect(calls).toBe(1)
    resolveOuter()
    await new Promise((r) => setTimeout(r, 30))
    await d.stop()
    expect(calls).toBe(1)
    state.close()
  })

  it(`respects maxConcurrentIssues`, async () => {
    const state = inMemoryState()
    let active = 0
    let maxSeen = 0
    let resolveOuter: () => void = () => {}
    const gate = new Promise<void>((r) => (resolveOuter = r))
    const pipeline: IssuePipeline = async (issue, deps) => {
      active++
      maxSeen = Math.max(maxSeen, active)
      await gate
      active--
      deps.state.setIssueStatus(issue.id, `done`)
    }
    const d = startDispatcher({
      config: {
        ...baseConfig,
        driver: { ...baseConfig.driver, maxConcurrentIssues: 2 },
      },
      state,
      log: SILENT,
      pipeline,
    })
    d.enqueue(event(`a`))
    d.enqueue(event(`b`))
    d.enqueue(event(`c`))
    d.enqueue(event(`d`))
    await new Promise((r) => setTimeout(r, 30))
    expect(maxSeen).toBe(2)
    resolveOuter()
    await new Promise((r) => setTimeout(r, 50))
    await d.stop()
    state.close()
  })

  it(`cancels an unassigned in-flight issue`, async () => {
    const state = inMemoryState()
    let resolveOuter: () => void = () => {}
    const gate = new Promise<void>((r) => (resolveOuter = r))
    const pipeline: IssuePipeline = async () => {
      await gate
    }
    const d = startDispatcher({
      config: baseConfig,
      state,
      log: SILENT,
      pipeline,
    })
    d.enqueue(event(`a`))
    await new Promise((r) => setTimeout(r, 20))
    d.enqueue(event(`a`, null))
    expect(state.getIssue(`a`)?.status).toBe(`cancelled`)
    resolveOuter()
    await d.stop()
    state.close()
  })
})
