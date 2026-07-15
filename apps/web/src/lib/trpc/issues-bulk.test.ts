import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"
import { is, Param, SQL } from "drizzle-orm"

// Bulk action bar server half: issues.bulkUpdate / issues.bulkDelete. The
// batch must resolve to exactly ONE workspace (join through projects, trashed
// projects excluded — stale ids silently skipped, empty survivor set is a
// hard BAD_REQUEST), membership + assignee validation run ONCE, and the whole
// batch commits in ONE transaction under ONE txId. Per-issue side effects go
// through the same applyStatusDerivations/finalizeIssueUpdateInTx core as the
// single update (completedAt stamping, duplicate-link clearing, recurrence
// clone), and the post-commit notification fan-out is skipped entirely past
// 25 ids. Fake-db harness: FIFO select queue,
// recording update/delete chains, transaction() handing back the same fake.

const h = vi.hoisted(() => ({
  assertWorkspaceMember: vi.fn(
    async (..._args: unknown[]) => ({ role: `member` }) as unknown
  ),
  assertAssigneeInWorkspace: vi.fn(async (..._args: unknown[]) => undefined),
  ensureSubscribed: vi.fn(),
  recordIssueEvent: vi.fn(),
  cloneIssueForRecurrence: vi.fn(async (..._args: unknown[]) => ({
    issue: { id: `clone-1` },
    attachmentCopies: [] as { sourceKey: string; destKey: string }[],
  })),
  copyRecurrenceAttachments: vi.fn(),
  collectIssueAttachmentStorageKeysInTx: vi.fn(
    async (..._args: unknown[]) => [] as string[]
  ),
  deleteStorageObjects: vi.fn(),
  fireAndForgetAssignmentNotify: vi.fn(),
  fireAndForgetStatusChangeNotify: vi.fn(),
  fireAndForgetReporterResolution: vi.fn(),
}))

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/workspace-membership`, () => ({
  resolveWorkspaceAccess: vi.fn(),
  assertAssigneeInWorkspace: h.assertAssigneeInWorkspace,
  assertIssueAccess: vi.fn(),
  assertWorkspaceMember: h.assertWorkspaceMember,
  getIssueWorkspaceContext: vi.fn(),
  getProjectWorkspaceId: vi.fn(),
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
  collectIssueAttachmentStorageKeysInTx: h.collectIssueAttachmentStorageKeysInTx,
  deleteStorageObjects: h.deleteStorageObjects,
}))
vi.mock(`@/lib/issue-recurrence`, () => ({
  cloneIssueForRecurrence: h.cloneIssueForRecurrence,
  copyRecurrenceAttachments: h.copyRecurrenceAttachments,
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetAssignmentNotify: h.fireAndForgetAssignmentNotify,
  fireAndForgetIssueMentionNotify: vi.fn(),
  fireAndForgetStatusChangeNotify: h.fireAndForgetStatusChangeNotify,
  fireAndForgetReporterResolution: h.fireAndForgetReporterResolution,
}))
vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: h.ensureSubscribed,
}))
vi.mock(`@/lib/integrations/activity`, () => ({
  recordIssueEvent: h.recordIssueEvent,
}))

import { issuesRouter } from "@/lib/trpc/issues"

const WS = `11111111-1111-4111-8111-111111111111`
const WS_OTHER = `99999999-9999-4999-8999-999999999999`

function uuid(n: number): string {
  return `00000000-0000-4000-8000-${String(n).padStart(12, `0`)}`
}
const ID_A = uuid(1)
const ID_B = uuid(2)

// FIFO select queue: each ctx.db.select() call resolves the next seeded rows.
const selectQueue: unknown[][] = []

function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
  const p = Promise.resolve(
    selectQueue.shift() ?? []
  ) as Promise<unknown[]> & Record<string, () => unknown>
  for (const m of [`from`, `where`, `innerJoin`, `limit`]) {
    p[m] = () => p
  }
  return p
}

// Digs the bound values out of a drizzle where-clause (eq/inArray produce SQL
// nodes whose Params carry the ids; inArray embeds its params as a nested
// array chunk).
function collectParams(node: unknown, out: unknown[] = []): unknown[] {
  if (is(node, Param)) {
    out.push(node.value)
  } else if (is(node, SQL)) {
    for (const chunk of node.queryChunks) collectParams(chunk, out)
  } else if (Array.isArray(node)) {
    for (const item of node) collectParams(item, out)
  }
  return out
}

const updates: { set: Record<string, unknown>; where: unknown }[] = []
const deletes: { where: unknown }[] = []
// Backs the update chain's .returning(): merges set into the seeded row so
// finalizeIssueUpdateInTx compares realistic persisted values.
const rowsById = new Map<string, Record<string, unknown>>()

const select = vi.fn(() => selectChain())

const fakeDb = {
  select,
  update: (_table: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: (whereArg: unknown) => ({
        returning: async () => {
          updates.push({ set: values, where: whereArg })
          const id = collectParams(whereArg)[0] as string
          const existing = rowsById.get(id)
          // No stored row = hard-deleted between eligibility select and
          // UPDATE — returning() yields nothing, like real Postgres.
          if (!existing) return []
          const merged = { ...existing, ...values, id }
          rowsById.set(id, merged)
          return [merged]
        },
      }),
    }),
  }),
  delete: (_table: unknown) => ({
    where: (whereArg: unknown) => ({
      returning: async () => {
        deletes.push({ where: whereArg })
        return collectParams(whereArg).map((id) => ({ id }))
      },
    }),
  }),
  // generateTxId's `SELECT pg_current_xact_id()` probe.
  execute: vi.fn(async () => ({ rows: [{ txid: `77` }] })),
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

function issueRow(
  id: string,
  overrides: Record<string, unknown> = {}
): Record<string, unknown> {
  return {
    id,
    status: `todo`,
    projectId: `proj-1`,
    title: `Issue`,
    priority: `none`,
    assigneeId: null,
    recurrenceInterval: null,
    recurrenceUnit: null,
    duplicateOfId: null,
    workspaceId: WS,
    ...overrides,
  }
}

// Seeds the eligibility select AND the update chain's persisted-row store.
function seedEligible(rows: Record<string, unknown>[]) {
  selectQueue.push(rows)
  for (const row of rows) rowsById.set(row.id as string, { ...row })
}

async function rejectionOf(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => undefined,
    (e: unknown) => e
  )
}

beforeEach(() => {
  selectQueue.length = 0
  updates.length = 0
  deletes.length = 0
  rowsById.clear()
  select.mockClear()
  fakeDb.execute.mockClear()
  fakeDb.transaction.mockClear()
  h.assertWorkspaceMember.mockClear()
  h.assertWorkspaceMember.mockResolvedValue({ role: `member` })
  h.assertAssigneeInWorkspace.mockClear()
  h.assertAssigneeInWorkspace.mockResolvedValue(undefined)
  h.ensureSubscribed.mockClear()
  h.recordIssueEvent.mockClear()
  h.cloneIssueForRecurrence.mockClear()
  h.cloneIssueForRecurrence.mockResolvedValue({
    issue: { id: `clone-1` },
    attachmentCopies: [],
  })
  h.copyRecurrenceAttachments.mockClear()
  h.collectIssueAttachmentStorageKeysInTx.mockClear()
  h.collectIssueAttachmentStorageKeysInTx.mockResolvedValue([])
  h.deleteStorageObjects.mockClear()
  h.fireAndForgetAssignmentNotify.mockClear()
  h.fireAndForgetStatusChangeNotify.mockClear()
  h.fireAndForgetReporterResolution.mockClear()
})

function eventsOfType(type: string) {
  return h.recordIssueEvent.mock.calls
    .map((call) => call[1] as Record<string, unknown>)
    .filter((args) => args.type === type)
}

describe(`issues.bulkUpdate`, () => {
  it(`rejects a batch spanning two workspaces before any write`, async () => {
    seedEligible([
      issueRow(ID_A),
      issueRow(ID_B, { workspaceId: WS_OTHER }),
    ])

    const error = await rejectionOf(
      caller.bulkUpdate({ ids: [ID_A, ID_B], status: `done` })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toBe(
      `Issues must belong to one workspace`
    )
    expect(h.assertWorkspaceMember).not.toHaveBeenCalled()
    expect(fakeDb.transaction).not.toHaveBeenCalled()
    expect(updates).toHaveLength(0)
  })

  it(`rejects when no id survives the eligibility join`, async () => {
    selectQueue.push([]) // all ids stale or in trashed projects

    const error = await rejectionOf(
      caller.bulkUpdate({ ids: [ID_A], status: `done` })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toBe(`No updatable issues`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  it(`rejects an empty patch via the input refine`, async () => {
    const error = await rejectionOf(caller.bulkUpdate({ ids: [ID_A] }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(select).not.toHaveBeenCalled()
  })

  it(`bulk done stamps completedAt, records per-issue events, and returns ONE txId`, async () => {
    seedEligible([issueRow(ID_A), issueRow(ID_B)])

    const result = await caller.bulkUpdate({ ids: [ID_A, ID_B], status: `done` })

    expect(h.assertWorkspaceMember).toHaveBeenCalledTimes(1)
    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, WS)
    expect(result).toEqual({ txId: 77, updated: 2 })
    // ONE transaction, ONE generateTxId probe for the whole batch.
    expect(fakeDb.transaction).toHaveBeenCalledTimes(1)
    expect(fakeDb.execute).toHaveBeenCalledTimes(1)

    expect(updates).toHaveLength(2)
    for (const update of updates) {
      expect(update.set.status).toBe(`done`)
      expect(update.set.completedAt).toBeInstanceOf(Date)
    }
    const statusEvents = eventsOfType(`status_changed`)
    expect(statusEvents.map((e) => [e.issueId, e.payload])).toEqual([
      [ID_A, { from: `todo`, to: `done` }],
      [ID_B, { from: `todo`, to: `done` }],
    ])
    // ≤25 ids → per-issue status notifications fire post-commit.
    expect(h.fireAndForgetStatusChangeNotify).toHaveBeenCalledTimes(2)
    expect(h.fireAndForgetReporterResolution).toHaveBeenCalledTimes(2)
  })

  it(`moving off 'duplicate' via bulk status clears the duplicate link`, async () => {
    seedEligible([
      issueRow(ID_A, { status: `duplicate`, duplicateOfId: uuid(9) }),
    ])

    await caller.bulkUpdate({ ids: [ID_A], status: `todo` })

    expect(updates).toHaveLength(1)
    expect(updates[0]!.set.duplicateOfId).toBeNull()
    expect(updates[0]!.set.completedAt).toBeNull()
  })

  it(`bulk done on a recurring issue clones the next occurrence and copies its attachments post-commit`, async () => {
    seedEligible([
      issueRow(ID_A, { recurrenceInterval: 1, recurrenceUnit: `weekly` }),
      issueRow(ID_B),
    ])
    h.cloneIssueForRecurrence.mockResolvedValue({
      issue: { id: `clone-1` },
      attachmentCopies: [{ sourceKey: `src`, destKey: `dst` }],
    })

    await caller.bulkUpdate({ ids: [ID_A, ID_B], status: `done` })

    // Only the recurring issue clones.
    expect(h.cloneIssueForRecurrence).toHaveBeenCalledTimes(1)
    expect(h.cloneIssueForRecurrence).toHaveBeenCalledWith(
      fakeDb,
      expect.objectContaining({
        sourceIssueId: ID_A,
        sourceWorkspaceId: WS,
        recurrenceInterval: 1,
        recurrenceUnit: `weekly`,
        creatorId: `actor`,
      })
    )
    expect(h.copyRecurrenceAttachments).toHaveBeenCalledWith([
      { sourceKey: `src`, destKey: `dst` },
    ])
  })

  it(`bulk assign validates the assignee once and only notifies actual changes`, async () => {
    const ID_C = uuid(3)
    seedEligible([
      issueRow(ID_A), // null → victim: changes
      issueRow(ID_B, { assigneeId: `other` }), // other → victim: changes
      issueRow(ID_C, { assigneeId: `victim` }), // already victim: no-op
    ])

    await caller.bulkUpdate({
      ids: [ID_A, ID_B, ID_C],
      assigneeId: `victim`,
    })

    expect(h.assertAssigneeInWorkspace).toHaveBeenCalledTimes(1)
    expect(h.assertAssigneeInWorkspace).toHaveBeenCalledWith(`victim`, WS)

    const assigneeEvents = eventsOfType(`assignee_changed`)
    expect(assigneeEvents.map((e) => [e.issueId, e.payload])).toEqual([
      [ID_A, { from: null, to: `victim` }],
      [ID_B, { from: `other`, to: `victim` }],
    ])
    // The new assignee is auto-subscribed per changed issue.
    expect(
      h.ensureSubscribed.mock.calls.filter(
        (call) =>
          (call[1] as Record<string, unknown>).source === `assignee` &&
          (call[1] as Record<string, unknown>).userId === `victim`
      )
    ).toHaveLength(2)
    expect(h.fireAndForgetAssignmentNotify).toHaveBeenCalledTimes(2)
    expect(h.fireAndForgetStatusChangeNotify).not.toHaveBeenCalled()
  })

  it(`assigneeId null (bulk unassign) skips the assignee guard`, async () => {
    seedEligible([issueRow(ID_A, { assigneeId: `other` })])

    await caller.bulkUpdate({ ids: [ID_A], assigneeId: null })

    expect(h.assertAssigneeInWorkspace).not.toHaveBeenCalled()
    expect(updates[0]!.set.assigneeId).toBeNull()
  })

  it(`skips ALL per-issue notifications past 25 ids (fan-out cap)`, async () => {
    const ids = Array.from({ length: 26 }, (_, i) => uuid(i + 1))
    seedEligible(ids.map((id) => issueRow(id)))

    const result = await caller.bulkUpdate({ ids, status: `done` })

    expect(result.updated).toBe(26)
    // Events still record — only the push/email fan-out is capped.
    expect(eventsOfType(`status_changed`)).toHaveLength(26)
    expect(h.fireAndForgetStatusChangeNotify).not.toHaveBeenCalled()
    expect(h.fireAndForgetReporterResolution).not.toHaveBeenCalled()
    expect(h.fireAndForgetAssignmentNotify).not.toHaveBeenCalled()
  })

  it(`silently skips stale ids while updating the survivors`, async () => {
    seedEligible([issueRow(ID_A)]) // ID_B fell out of the join

    const result = await caller.bulkUpdate({
      ids: [ID_A, ID_B],
      priority: `high`,
    })

    expect(result.updated).toBe(1)
    expect(updates).toHaveLength(1)
    expect(collectParams(updates[0]!.where)).toEqual([ID_A])
    expect(updates[0]!.set).toEqual({ priority: `high` })
  })

  it(`rejects status 'duplicate' — bulk marking has no canonical-issue picker`, async () => {
    const error = await rejectionOf(
      caller.bulkUpdate({ ids: [ID_A], status: `duplicate` })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(select).not.toHaveBeenCalled()
  })

  it(`skips a row hard-deleted between the eligibility select and its UPDATE`, async () => {
    seedEligible([issueRow(ID_A), issueRow(ID_B)])
    rowsById.delete(ID_B) // deleted in the window — UPDATE returns no row

    const result = await caller.bulkUpdate({ ids: [ID_A, ID_B], status: `done` })

    // The survivor commits, the vanished row is silently skipped — the batch
    // must not roll back with a 500.
    expect(result.updated).toBe(1)
    expect(eventsOfType(`status_changed`).map((e) => e.issueId)).toEqual([
      ID_A,
    ])
    expect(h.fireAndForgetStatusChangeNotify).toHaveBeenCalledTimes(1)
  })
})

describe(`issues.bulkDelete`, () => {
  it(`rejects a batch spanning two workspaces`, async () => {
    selectQueue.push([
      { id: ID_A, workspaceId: WS },
      { id: ID_B, workspaceId: WS_OTHER },
    ])

    const error = await rejectionOf(caller.bulkDelete({ ids: [ID_A, ID_B] }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toBe(
      `Issues must belong to one workspace`
    )
    expect(fakeDb.transaction).not.toHaveBeenCalled()
    expect(deletes).toHaveLength(0)
  })

  it(`rejects when nothing is eligible`, async () => {
    selectQueue.push([])

    const error = await rejectionOf(caller.bulkDelete({ ids: [ID_A] }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toBe(`No deletable issues`)
  })

  it(`deletes in one statement and reclaims every issue's attachment blobs post-commit`, async () => {
    selectQueue.push([
      { id: ID_A, workspaceId: WS },
      { id: ID_B, workspaceId: WS },
    ])
    h.collectIssueAttachmentStorageKeysInTx
      .mockResolvedValueOnce([`key-a`])
      .mockResolvedValueOnce([`key-b1`, `key-b2`])

    const result = await caller.bulkDelete({ ids: [ID_A, ID_B] })

    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, WS)
    expect(result).toEqual({ txId: 77, deleted: 2 })
    expect(fakeDb.transaction).toHaveBeenCalledTimes(1)
    expect(fakeDb.execute).toHaveBeenCalledTimes(1)
    // In-tx key collection ran per issue, delete is ONE inArray statement.
    expect(h.collectIssueAttachmentStorageKeysInTx).toHaveBeenCalledTimes(2)
    expect(deletes).toHaveLength(1)
    expect(collectParams(deletes[0]!.where)).toEqual([ID_A, ID_B])
    expect(h.deleteStorageObjects).toHaveBeenCalledWith([
      `key-a`,
      `key-b1`,
      `key-b2`,
    ])
  })
})
