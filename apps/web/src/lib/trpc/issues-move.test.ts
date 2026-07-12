import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"
import { is, Param, SQL, StringChunk } from "drizzle-orm"

// issues.move (EXP-57): same-workspace project move with Linear-style
// renumbering. The target's next number is allocated inside the transaction
// exactly like the INSERT-only generate_issue_number trigger (max read +
// issue_number_counters upsert), the issue row gets project_id + number +
// identifier in ONE update, every trigger-denormalized child project_id is
// re-pointed (their populate triggers are INSERT-only too), and a
// project_moved event records the hop. PR/branch linkage is untouched.
// Mirrors issues-bulk.test.ts's fake-db harness.

const h = vi.hoisted(() => ({
  assertIssueAccess: vi.fn(async (..._args: unknown[]) => ({}) as unknown),
  getProjectWorkspaceId: vi.fn(async (..._args: unknown[]) => ({}) as unknown),
  recordIssueEvent: vi.fn(),
}))

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/workspace-membership`, () => ({
  resolveWorkspaceAccess: vi.fn(),
  assertAssigneeInWorkspace: vi.fn(),
  assertIssueAccess: h.assertIssueAccess,
  assertWorkspaceMember: vi.fn(),
  getIssueWorkspaceContext: vi.fn(),
  getProjectWorkspaceId: h.getProjectWorkspaceId,
  getSoleHumanMemberId: vi.fn(),
}))

// Side-effect-free stubs for issues.ts's remaining module-scope imports.
vi.mock(`@/lib/integrations/github-pr`, () => ({
  fetchPullFiles: vi.fn(),
  mergePullRequest: vi.fn(),
  resolveRepoToken: vi.fn(),
  GitHubMergeError: class extends Error {},
}))
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => false,
  resolveRepoInstallationToken: vi.fn(),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrMergeState: vi.fn(),
}))
vi.mock(`@/lib/storage/issue-attachments`, () => ({
  canonicalizeMarkdownImageUrls: vi.fn(),
  extractAttachmentIdsFromDescription: vi.fn(),
  hasMarkdownImages: () => false,
}))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  collectAndDeleteRemovedAttachmentsInTx: vi.fn(),
  collectAndDeleteUnreferencedAttachmentsInTx: vi.fn(),
  collectIssueAttachmentStorageKeysInTx: vi.fn(async () => []),
  deleteStorageObjects: vi.fn(),
}))
vi.mock(`@/lib/issue-recurrence`, () => ({
  cloneIssueForRecurrence: vi.fn(),
  copyRecurrenceAttachments: vi.fn(),
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetAssignmentNotify: vi.fn(),
  fireAndForgetIssueMentionNotify: vi.fn(),
  fireAndForgetStatusChangeNotify: vi.fn(),
  fireAndForgetReporterResolution: vi.fn(),
}))
vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: vi.fn(),
}))
vi.mock(`@/lib/integrations/activity`, () => ({
  recordIssueEvent: h.recordIssueEvent,
}))

import { issuesRouter } from "@/lib/trpc/issues"
import {
  attachments,
  codingSessions,
  comments,
  issueEvents,
  issues,
  issueLabels,
  issueSubscribers,
} from "@/db/schema"

const WS = `11111111-1111-4111-8111-111111111111`
const WS_OTHER = `99999999-9999-4999-8999-999999999999`
const ISSUE_ID = `00000000-0000-4000-8000-000000000001`
const PROJ_FROM = `00000000-0000-4000-8000-00000000000a`
const PROJ_TO = `00000000-0000-4000-8000-00000000000b`

// FIFO select queue: each tx.select() call resolves the next seeded rows.
const selectQueue: unknown[][] = []

function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
  const p = Promise.resolve(
    selectQueue.shift() ?? []
  ) as Promise<unknown[]> & Record<string, () => unknown>
  for (const m of [`from`, `where`, `innerJoin`, `limit`, `for`]) {
    p[m] = () => p
  }
  return p
}

// Digs the bound values out of drizzle SQL nodes. eq() wraps values in Param;
// raw sql`` templates keep interpolated values as bare chunks (they become
// params only at query build time), so plain strings/numbers count too.
function collectParams(node: unknown, out: unknown[] = []): unknown[] {
  if (is(node, Param)) {
    out.push(node.value)
  } else if (is(node, StringChunk)) {
    // Literal SQL text — never a bound value.
  } else if (is(node, SQL)) {
    for (const chunk of node.queryChunks) collectParams(chunk, out)
  } else if (Array.isArray(node)) {
    for (const item of node) collectParams(item, out)
  } else if (typeof node === `string` || typeof node === `number`) {
    out.push(node)
  }
  return out
}

const updates: {
  table: unknown
  set: Record<string, unknown>
  where: unknown
}[] = []
// The issues UPDATE's .returning() row (merged set over this base).
let issueRowBase: Record<string, unknown> | null = null

// FIFO execute queue for the raw-SQL calls (txid probe, max read, counter
// upsert). Each entry is the `rows` array for one call; the SQL nodes are
// recorded for param assertions.
const executeQueue: Record<string, unknown>[][] = []
const executeCalls: unknown[] = []

const fakeDb = {
  select: vi.fn(() => selectChain()),
  update: (table: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: (whereArg: unknown) => {
        updates.push({ table, set: values, where: whereArg })
        // Child re-points are awaited bare; the issues update chains
        // .returning(). Hand back a thenable that supports both.
        const result = Promise.resolve([]) as unknown as Promise<unknown[]> & {
          returning: () => Promise<unknown[]>
        }
        result.returning = async () => {
          if (!issueRowBase) return []
          return [{ ...issueRowBase, ...values }]
        }
        return result
      },
    }),
  }),
  execute: vi.fn(async (query: unknown) => {
    executeCalls.push(query)
    return { rows: executeQueue.shift() ?? [] }
  }),
  transaction: vi.fn(
    async (fn: (tx: typeof fakeDb) => Promise<unknown>): Promise<unknown> =>
      fn(fakeDb)
  ),
}

const caller = issuesRouter.createCaller({
  session: { user: { id: `actor` } },
  db: fakeDb,
  request: new Request(`http://localhost/`),
} as never)

async function rejectionOf(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => undefined,
    (e: unknown) => e
  )
}

function seedHappyPath() {
  h.assertIssueAccess.mockResolvedValue({
    issueId: ISSUE_ID,
    projectId: PROJ_FROM,
    workspaceId: WS,
  })
  h.getProjectWorkspaceId.mockResolvedValue({
    id: PROJ_TO,
    workspaceId: WS,
  })
  // txid probe → target max read → counter upsert.
  executeQueue.push([{ txid: `77` }], [{ current_max: 16 }], [{ counter: 17 }])
  // current issue read → target project read.
  selectQueue.push([{ identifier: `EXP-42`, projectId: PROJ_FROM }])
  selectQueue.push([{ prefix: `ABC`, slug: `abc` }])
  issueRowBase = {
    id: ISSUE_ID,
    projectId: PROJ_FROM,
    number: 42,
    identifier: `EXP-42`,
    title: `Issue`,
    prUrl: `https://github.com/o/r/pull/9`,
    branch: `exp/EXP-42`,
  }
}

beforeEach(() => {
  selectQueue.length = 0
  executeQueue.length = 0
  executeCalls.length = 0
  updates.length = 0
  issueRowBase = null
  fakeDb.select.mockClear()
  fakeDb.execute.mockClear()
  fakeDb.transaction.mockClear()
  h.assertIssueAccess.mockReset()
  h.getProjectWorkspaceId.mockReset()
  h.recordIssueEvent.mockClear()
})

describe(`issues.move`, () => {
  it(`rejects moving to the issue's current project before any write`, async () => {
    h.assertIssueAccess.mockResolvedValue({
      issueId: ISSUE_ID,
      projectId: PROJ_TO,
      workspaceId: WS,
    })

    const error = await rejectionOf(
      caller.move({ id: ISSUE_ID, projectId: PROJ_TO })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toBe(
      `Issue is already in this project`
    )
    expect(h.assertIssueAccess).toHaveBeenCalledWith(
      `actor`,
      ISSUE_ID,
      `write`
    )
    expect(h.getProjectWorkspaceId).not.toHaveBeenCalled()
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  it(`rejects a cross-workspace target before any write`, async () => {
    h.assertIssueAccess.mockResolvedValue({
      issueId: ISSUE_ID,
      projectId: PROJ_FROM,
      workspaceId: WS,
    })
    h.getProjectWorkspaceId.mockResolvedValue({
      id: PROJ_TO,
      workspaceId: WS_OTHER,
    })

    const error = await rejectionOf(
      caller.move({ id: ISSUE_ID, projectId: PROJ_TO })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toBe(
      `Issues can only move within their workspace`
    )
    expect(fakeDb.transaction).not.toHaveBeenCalled()
    expect(updates).toHaveLength(0)
  })

  it(`renumbers in the target, re-points every denormalized child, and records project_moved`, async () => {
    seedHappyPath()

    const result = await caller.move({ id: ISSUE_ID, projectId: PROJ_TO })

    // ONE transaction; txid probe + max read + counter upsert on execute.
    expect(fakeDb.transaction).toHaveBeenCalledTimes(1)
    expect(fakeDb.execute).toHaveBeenCalledTimes(3)
    // The max read scopes to the target project; the counter upsert binds the
    // target project and the read max (trigger-parity allocation).
    expect(collectParams(executeCalls[1])).toEqual([PROJ_TO])
    expect(collectParams(executeCalls[2])).toEqual([PROJ_TO, 16, 16])

    // The issue row gets project + number + identifier in one update.
    const issueUpdate = updates.find((u) => u.table === issues)
    expect(issueUpdate).toBeDefined()
    expect(issueUpdate!.set).toEqual({
      projectId: PROJ_TO,
      number: 17,
      identifier: `ABC-17`,
    })
    expect(collectParams(issueUpdate!.where)).toEqual([ISSUE_ID])

    // Every trigger-denormalized child table is re-pointed to the target.
    for (const table of [
      comments,
      attachments,
      issueEvents,
      issueSubscribers,
      issueLabels,
      codingSessions,
    ]) {
      const childUpdate = updates.find((u) => u.table === table)
      expect(childUpdate).toBeDefined()
      expect(childUpdate!.set).toEqual({ projectId: PROJ_TO })
      expect(collectParams(childUpdate!.where)).toEqual([ISSUE_ID])
    }
    expect(updates).toHaveLength(7)

    // Timeline event with the full hop payload.
    expect(h.recordIssueEvent).toHaveBeenCalledTimes(1)
    expect(h.recordIssueEvent).toHaveBeenCalledWith(
      fakeDb,
      expect.objectContaining({
        issueId: ISSUE_ID,
        workspaceId: WS,
        actorUserId: `actor`,
        type: `project_moved`,
        payload: {
          fromProjectId: PROJ_FROM,
          toProjectId: PROJ_TO,
          fromIdentifier: `EXP-42`,
          toIdentifier: `ABC-17`,
        },
      })
    )

    // Returns the fresh identity for client navigation; PR linkage survives.
    expect(result.txId).toBe(77)
    expect(result.projectSlug).toBe(`abc`)
    expect(result.issue.identifier).toBe(`ABC-17`)
    expect(result.issue.number).toBe(17)
    expect(result.issue.projectId).toBe(PROJ_TO)
    expect(result.issue).toMatchObject({
      prUrl: `https://github.com/o/r/pull/9`,
      branch: `exp/EXP-42`,
    })
  })

  it(`404s when the issue vanished between the access check and the tx read`, async () => {
    seedHappyPath()
    // Overwrite the current-issue read with an empty result.
    selectQueue.length = 0
    selectQueue.push([])

    const error = await rejectionOf(
      caller.move({ id: ISSUE_ID, projectId: PROJ_TO })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
    expect(updates).toHaveLength(0)
    expect(h.recordIssueEvent).not.toHaveBeenCalled()
  })
})
