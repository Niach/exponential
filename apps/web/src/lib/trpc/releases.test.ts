import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"
import { is, Param, SQL } from "drizzle-orm"

// EXP-56 releases router (Phase 1). Every mutation is member-gated via
// assertWorkspaceMember; setIssueRelease additionally enforces that the
// release and the issue live in the SAME workspace (FORBIDDEN otherwise), and
// addIssues filters the batch to same-workspace issues in non-trashed
// projects, refusing with BAD_REQUEST when nothing survives. The router runs
// against ctx.db, so a fake db is enough: `select()` shifts pre-seeded rows
// off a FIFO queue (thenable chain, mirrors workspace-members.test.ts),
// insert/update/delete record the drizzle table + payload they were called
// with, and `transaction()` hands the callback the same fake db (whose
// `execute` serves generateTxId's pg_current_xact_id probe).

const h = vi.hoisted(() => ({
  assertWorkspaceMember: vi.fn(
    async (..._args: unknown[]) => ({ role: `member` }) as unknown
  ),
  getIssueWorkspaceContext: vi.fn(async () => ({
    issueId: `issue-1`,
    projectId: `proj-1`,
    workspaceId: `ws-issue`,
  })),
}))

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/workspace-membership`, () => ({
  assertWorkspaceMember: h.assertWorkspaceMember,
  getIssueWorkspaceContext: h.getIssueWorkspaceContext,
}))

import { releasesRouter } from "@/lib/trpc/releases"
import { issueEvents, issues, releases } from "@/db/schema"

const WS = `11111111-1111-4111-8111-111111111111`
const RELEASE_ID = `22222222-2222-4222-8222-222222222222`
const ISSUE_ID = `33333333-3333-4333-8333-333333333333`
const ISSUE_ID_2 = `44444444-4444-4444-8444-444444444444`

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

const inserts: { table: unknown; values: Record<string, unknown> }[] = []
const updates: {
  table: unknown
  set: Record<string, unknown>
  where: unknown
}[] = []
const deletes: { table: unknown }[] = []

const select = vi.fn(() => selectChain())

const fakeDb = {
  select,
  insert: (table: unknown) => ({
    values: (values: Record<string, unknown>) => {
      inserts.push({ table, values })
      return {
        returning: async () => [{ id: RELEASE_ID, ...values }],
      }
    },
  }),
  update: (table: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: (whereArg: unknown) => {
        updates.push({ table, set: values, where: whereArg })
        return Promise.resolve()
      },
    }),
  }),
  delete: (table: unknown) => ({
    where: () => {
      deletes.push({ table })
      return Promise.resolve()
    },
  }),
  // generateTxId's `SELECT pg_current_xact_id()` probe + create's advisory
  // lock (the arg is captured so tests can inspect the lock's SQL params).
  execute: vi.fn(async (..._args: unknown[]) => ({ rows: [{ txid: `42` }] })),
  transaction: vi.fn(
    async (fn: (tx: typeof fakeDb) => Promise<unknown>): Promise<unknown> =>
      fn(fakeDb)
  ),
}

const caller = releasesRouter.createCaller({
  session: { user: { id: `actor` } },
  db: fakeDb,
  request: new Request(`http://localhost/`),
} as never)

// Digs the bound values out of a drizzle where-clause (eq/inArray produce SQL
// nodes whose Params carry the ids; inArray embeds its params as a nested
// array chunk) — lets the tests assert exactly which issue ids reached the
// bulk update.
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

async function rejectionOf(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => undefined,
    (e: unknown) => e
  )
}

beforeEach(() => {
  selectQueue.length = 0
  inserts.length = 0
  updates.length = 0
  deletes.length = 0
  select.mockClear()
  fakeDb.execute.mockClear()
  fakeDb.transaction.mockClear()
  h.assertWorkspaceMember.mockClear()
  h.assertWorkspaceMember.mockResolvedValue({ role: `member` })
  h.getIssueWorkspaceContext.mockClear()
  h.getIssueWorkspaceContext.mockResolvedValue({
    issueId: `issue-1`,
    projectId: `proj-1`,
    workspaceId: `ws-issue`,
  })
})

describe(`releases.create`, () => {
  it(`asserts membership on input.workspaceId and inserts an explicit name with createdBy = session user`, async () => {
    const result = await caller.create({ workspaceId: WS, name: `v1.0` })

    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, WS)
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(releases)
    expect(inserts[0]!.values).toEqual({
      workspaceId: WS,
      name: `v1.0`,
      createdBy: `actor`,
    })
    expect(result.txId).toBe(42)
    expect(result.release).toMatchObject({ id: RELEASE_ID, name: `v1.0` })
  })

  it(`an explicit name never reads existing releases (no auto-name select, no advisory lock)`, async () => {
    await caller.create({ workspaceId: WS, name: `Kraken` })

    expect(select).not.toHaveBeenCalled()
    // Only generateTxId's txid probe — no pg_advisory_xact_lock execute.
    expect(fakeDb.execute).toHaveBeenCalledTimes(1)
  })

  it(`auto-names from the max trailing "Release N" integer, gap-tolerant`, async () => {
    selectQueue.push([
      { name: `Release 3` },
      { name: `Kraken` },
      { name: `Release 12` },
    ])

    await caller.create({ workspaceId: WS })

    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.values).toEqual({
      workspaceId: WS,
      name: `Release 13`,
      createdBy: `actor`,
    })
    // txid probe + the per-workspace advisory lock, keyed on the workspaceId
    // (the sql template keeps raw interpolated values as chunks, not Params).
    expect(fakeDb.execute).toHaveBeenCalledTimes(2)
    const lockSql = fakeDb.execute.mock.calls[1]![0] as SQL
    expect(is(lockSql, SQL)).toBe(true)
    expect(lockSql.queryChunks).toContain(WS)
  })

  it(`auto-names "Release 1" in an empty workspace`, async () => {
    selectQueue.push([])

    await caller.create({ workspaceId: WS })

    expect(inserts[0]!.values.name).toBe(`Release 1`)
  })

  it(`propagates a membership refusal before the transaction`, async () => {
    h.assertWorkspaceMember.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN` })
    )

    const error = await rejectionOf(
      caller.create({ workspaceId: WS, name: `nope` })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
    expect(inserts).toHaveLength(0)
  })
})

describe(`releases.setIssueRelease`, () => {
  it(`throws FORBIDDEN when the release belongs to another workspace`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-other` }])

    const error = await rejectionOf(
      caller.setIssueRelease({ issueId: ISSUE_ID, releaseId: RELEASE_ID })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect((error as TRPCError).message).toBe(
      `Release and issue belong to different workspaces`
    )
    expect(fakeDb.transaction).not.toHaveBeenCalled()
    expect(updates).toHaveLength(0)
  })

  it(`writes issues.releaseId and returns the txId on a same-workspace pair`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-issue` }])

    const result = await caller.setIssueRelease({
      issueId: ISSUE_ID,
      releaseId: RELEASE_ID,
    })

    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, `ws-issue`)
    expect(result).toEqual({ txId: 42 })
    expect(updates).toHaveLength(1)
    expect(updates[0]!.table).toBe(issues)
    expect(updates[0]!.set).toEqual({ releaseId: RELEASE_ID })
    expect(collectParams(updates[0]!.where)).toEqual([ISSUE_ID])
  })

  it(`releaseId: null clears the link without loading a release`, async () => {
    // The in-tx previous-releaseId read (release_removed's source).
    selectQueue.push([{ releaseId: null }])

    const result = await caller.setIssueRelease({
      issueId: ISSUE_ID,
      releaseId: null,
    })

    // Exactly ONE select — the in-tx previous read on issues; clearing never
    // loads a release row (no NOT_FOUND path for null).
    expect(select).toHaveBeenCalledTimes(1)
    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, `ws-issue`)
    expect(result).toEqual({ txId: 42 })
    expect(updates[0]!.set).toEqual({ releaseId: null })
    // null → null is a no-op for the timeline.
    expect(inserts.filter((i) => i.table === issueEvents)).toHaveLength(0)
  })

  it(`records release_removed + release_added when moving between releases`, async () => {
    const OLD_RELEASE = `55555555-5555-4555-8555-555555555555`
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-issue` }]) // release lookup
    selectQueue.push([{ releaseId: OLD_RELEASE }]) // in-tx previous read

    await caller.setIssueRelease({ issueId: ISSUE_ID, releaseId: RELEASE_ID })

    const events = inserts.filter((i) => i.table === issueEvents)
    expect(events.map((e) => [e.values.type, e.values.payload])).toEqual([
      [`release_removed`, { releaseId: OLD_RELEASE }],
      [`release_added`, { releaseId: RELEASE_ID }],
    ])
  })

  it(`records no events when re-setting the same release`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-issue` }])
    selectQueue.push([{ releaseId: RELEASE_ID }]) // unchanged

    await caller.setIssueRelease({ issueId: ISSUE_ID, releaseId: RELEASE_ID })

    expect(inserts.filter((i) => i.table === issueEvents)).toHaveLength(0)
  })

  it(`throws NOT_FOUND for an unknown release`, async () => {
    // Empty select queue → getReleaseOrThrow finds nothing.
    const error = await rejectionOf(
      caller.setIssueRelease({ issueId: ISSUE_ID, releaseId: RELEASE_ID })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
    expect(updates).toHaveLength(0)
  })
})

describe(`releases.addIssues`, () => {
  it(`throws BAD_REQUEST when no issue in the batch is eligible`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: WS }])
    selectQueue.push([]) // eligible-issues join finds nothing

    const error = await rejectionOf(
      caller.addIssues({ releaseId: RELEASE_ID, issueIds: [ISSUE_ID] })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toBe(
      `No addable issues in this workspace`
    )
    expect(fakeDb.transaction).not.toHaveBeenCalled()
    expect(updates).toHaveLength(0)
  })

  it(`only eligible ids reach the bulk update`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: WS }])
    // The workspace + non-trashed-project join keeps just ISSUE_ID.
    selectQueue.push([{ id: ISSUE_ID }])

    const result = await caller.addIssues({
      releaseId: RELEASE_ID,
      issueIds: [ISSUE_ID, ISSUE_ID_2],
    })

    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, WS)
    expect(result).toEqual({ txId: 42, added: 1 })
    expect(updates).toHaveLength(1)
    expect(updates[0]!.table).toBe(issues)
    expect(updates[0]!.set).toEqual({ releaseId: RELEASE_ID })
    expect(collectParams(updates[0]!.where)).toEqual([ISSUE_ID])
  })
})

describe(`releases.delete / markShipped`, () => {
  it(`delete asserts membership against the loaded release's workspace`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-release` }])

    const result = await caller.delete({ id: RELEASE_ID })

    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, `ws-release`)
    expect(result).toEqual({ txId: 42 })
    expect(deletes).toHaveLength(1)
    expect(deletes[0]!.table).toBe(releases)
  })

  it(`delete refuses a non-member before touching the row`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-release` }])
    h.assertWorkspaceMember.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN` })
    )

    const error = await rejectionOf(caller.delete({ id: RELEASE_ID }))
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    expect(deletes).toHaveLength(0)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
  })

  it(`markShipped asserts membership against the release's workspace and stamps shippedAt`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-release` }])

    const result = await caller.markShipped({ id: RELEASE_ID, shipped: true })

    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, `ws-release`)
    expect(result).toEqual({ txId: 42 })
    expect(updates).toHaveLength(1)
    expect(updates[0]!.table).toBe(releases)
    expect(updates[0]!.set.shippedAt).toBeInstanceOf(Date)
  })

  it(`markShipped { shipped: false } clears shippedAt`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-release` }])

    await caller.markShipped({ id: RELEASE_ID, shipped: false })

    expect(updates[0]!.set).toEqual({ shippedAt: null })
  })

  it(`markShipped throws NOT_FOUND for an unknown release`, async () => {
    const error = await rejectionOf(
      caller.markShipped({ id: RELEASE_ID, shipped: true })
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
    expect(updates).toHaveLength(0)
  })
})

describe(`releases.update`, () => {
  it(`asserts membership against the loaded release's workspace and applies only provided fields`, async () => {
    selectQueue.push([{ id: RELEASE_ID, workspaceId: `ws-release` }])

    const result = await caller.update({ id: RELEASE_ID, name: `v1.1` })

    expect(h.assertWorkspaceMember).toHaveBeenCalledWith(`actor`, `ws-release`)
    expect(result).toEqual({ txId: 42 })
    expect(updates).toHaveLength(1)
    expect(updates[0]!.table).toBe(releases)
    expect(updates[0]!.set).toEqual({ name: `v1.1` })
  })
})
