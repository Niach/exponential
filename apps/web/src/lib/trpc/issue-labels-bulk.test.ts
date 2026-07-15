import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"
import { is, Param, SQL } from "drizzle-orm"

// Bulk label writes (one label → many issues) for the multi-select action
// bar. bulkAdd loads the label, gates on membership in ITS workspace, keeps
// only same-workspace issues in non-trashed projects, and records label_added
// ONLY for rows the onConflictDoNothing insert actually created (returning())
// — a half-labelled selection must not double-log the already-labelled
// issues. bulkRemove deletes by (labelId, issueIds) and records label_removed
// per actually-deleted row. Fake-db harness mirrors issues-bulk.test.ts.

const h = vi.hoisted(() => ({
  assertWorkspaceMember: vi.fn(
    async (..._args: unknown[]) => ({ role: `member` }) as unknown
  ),
  recordIssueEvent: vi.fn(),
}))

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/workspace-membership`, () => ({
  assertIssueLabelWorkspaceMatch: vi.fn(),
  assertWorkspaceMember: h.assertWorkspaceMember,
}))

vi.mock(`@/lib/integrations/activity`, () => ({
  recordIssueEvent: h.recordIssueEvent,
}))

import { issueLabelsRouter } from "@/lib/trpc/issue-labels"

const WS = `11111111-1111-4111-8111-111111111111`
const LABEL_ID = `22222222-2222-4222-8222-222222222222`
const ISSUE_1 = `33333333-3333-4333-8333-333333333333`
const ISSUE_2 = `44444444-4444-4444-8444-444444444444`
const ISSUE_3 = `55555555-5555-4555-8555-555555555555`

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

const state = {
  // What the bulkAdd insert's returning() yields (the actually-new rows).
  insertReturning: [] as { issueId: string }[],
  // What the bulkRemove delete's returning() yields (actually-deleted rows).
  deleteReturning: [] as { issueId: string }[],
}

const inserts: { values: unknown }[] = []
const deletes: { where: unknown }[] = []

const select = vi.fn(() => selectChain())

const fakeDb = {
  select,
  insert: (_table: unknown) => ({
    values: (values: unknown) => {
      inserts.push({ values })
      return {
        onConflictDoNothing: () => ({
          returning: async () => state.insertReturning,
        }),
      }
    },
  }),
  delete: (_table: unknown) => ({
    where: (whereArg: unknown) => ({
      returning: async () => {
        deletes.push({ where: whereArg })
        return state.deleteReturning
      },
    }),
  }),
  // generateTxId's `SELECT pg_current_xact_id()` probe.
  execute: vi.fn(async () => ({ rows: [{ txid: `42` }] })),
  transaction: vi.fn(
    async (fn: (tx: typeof fakeDb) => Promise<unknown>): Promise<unknown> =>
      fn(fakeDb)
  ),
}

const caller = issueLabelsRouter.createCaller({
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

function eventsOfType(type: string) {
  return h.recordIssueEvent.mock.calls
    .map((call) => call[1] as Record<string, unknown>)
    .filter((args) => args.type === type)
}

beforeEach(() => {
  selectQueue.length = 0
  inserts.length = 0
  deletes.length = 0
  state.insertReturning = []
  state.deleteReturning = []
  select.mockClear()
  fakeDb.execute.mockClear()
  fakeDb.transaction.mockClear()
  h.assertWorkspaceMember.mockClear()
  h.assertWorkspaceMember.mockResolvedValue({ role: `member` })
  h.recordIssueEvent.mockClear()
})

describe(`issueLabels.bulkAdd`, () => {
  it(`inserts only eligible issues and records label_added only for actually-new rows`, async () => {
    selectQueue.push([{ workspaceId: WS }]) // label lookup
    // The workspace + non-trashed-project join drops ISSUE_3.
    selectQueue.push([
      { id: ISSUE_1, projectId: `proj-1` },
      { id: ISSUE_2, projectId: `proj-2` },
    ])
    // ISSUE_2 already carried the label — onConflictDoNothing skips it.
    state.insertReturning = [{ issueId: ISSUE_1 }]

    const result = await caller.bulkAdd({
      labelId: LABEL_ID,
      issueIds: [ISSUE_1, ISSUE_2, ISSUE_3],
    })

    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, WS)
    expect(result).toEqual({ txId: 42 })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toEqual([
      {
        issueId: ISSUE_1,
        labelId: LABEL_ID,
        workspaceId: WS,
        projectId: `proj-1`,
      },
      {
        issueId: ISSUE_2,
        labelId: LABEL_ID,
        workspaceId: WS,
        projectId: `proj-2`,
      },
    ])
    const added = eventsOfType(`label_added`)
    expect(added.map((e) => [e.issueId, e.payload])).toEqual([
      [ISSUE_1, { labelId: LABEL_ID }],
    ])
  })

  it(`throws BAD_REQUEST when no issue survives the eligibility join`, async () => {
    selectQueue.push([{ workspaceId: WS }]) // label lookup
    selectQueue.push([]) // nothing eligible (read in-tx, before the txId probe)

    const error = await rejectionOf(
      caller.bulkAdd({ labelId: LABEL_ID, issueIds: [ISSUE_1] })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(fakeDb.execute).not.toHaveBeenCalled()
    expect(inserts).toHaveLength(0)
  })

  it(`throws NOT_FOUND for an unknown label before membership or writes`, async () => {
    // Empty select queue → label lookup finds nothing.
    const error = await rejectionOf(
      caller.bulkAdd({ labelId: LABEL_ID, issueIds: [ISSUE_1] })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
    expect(h.assertWorkspaceMember).not.toHaveBeenCalled()
    expect(inserts).toHaveLength(0)
  })
})

describe(`issueLabels.bulkRemove`, () => {
  it(`deletes by (labelId, issueIds) and records label_removed per actually-deleted row`, async () => {
    selectQueue.push([{ workspaceId: WS }]) // label lookup
    state.deleteReturning = [{ issueId: ISSUE_1 }, { issueId: ISSUE_2 }]

    const result = await caller.bulkRemove({
      labelId: LABEL_ID,
      issueIds: [ISSUE_1, ISSUE_2, ISSUE_3],
    })

    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, WS)
    expect(result).toEqual({ txId: 42 })
    expect(deletes).toHaveLength(1)
    expect(collectParams(deletes[0]!.where)).toEqual([
      LABEL_ID,
      ISSUE_1,
      ISSUE_2,
      ISSUE_3,
    ])
    const removed = eventsOfType(`label_removed`)
    expect(removed.map((e) => [e.issueId, e.payload])).toEqual([
      [ISSUE_1, { labelId: LABEL_ID }],
      [ISSUE_2, { labelId: LABEL_ID }],
    ])
  })

  it(`records no events when nothing was linked`, async () => {
    selectQueue.push([{ workspaceId: WS }])
    state.deleteReturning = []

    const result = await caller.bulkRemove({
      labelId: LABEL_ID,
      issueIds: [ISSUE_1],
    })

    expect(result).toEqual({ txId: 42 })
    expect(h.recordIssueEvent).not.toHaveBeenCalled()
  })
})
