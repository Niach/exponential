import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-736 — the relations router's four invariants: no self-link, the other
// issue must be a same-team VISIBLE issue, a pick is stored in the ONE
// canonical direction (so both halves of a directed type land on the same
// row), and the reverse of a directed row is refused. `duplicate` is not
// written here at all — it delegates to issues.update, which owns the
// status/duplicateOfId lockstep and produces the mirrored row.
//
// Fake-db harness mirrors issues-duplicate.test.ts: FIFO select queue,
// transaction() handing back the same fake.

const h = vi.hoisted(() => ({
  assertIssueAccess: vi.fn(async (..._args: unknown[]) => ({
    issueId: `issue-1`,
    boardId: `board-1`,
    teamId: `ws-1`,
  })),
  insertRelationInTx: vi.fn(async (..._args: unknown[]) => ({ id: `rel-1` })),
  deleteRelationInTx: vi.fn(async (..._args: unknown[]) => ({ id: `rel-1` })),
  issuesUpdate: vi.fn(async (..._args: unknown[]) => ({ txId: 99 })),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/team-membership`, () => ({
  assertIssueAccess: h.assertIssueAccess,
}))

// The duplicate arms delegate through the issues router; stubbing the module
// keeps this file off issues.ts's long module-scope import chain.
vi.mock(`@/lib/trpc/issues`, () => ({
  issuesRouter: { createCaller: () => ({ update: h.issuesUpdate }) },
}))

vi.mock(`@/lib/issue-relations`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/issue-relations")>()
  return {
    // canonicalizeRelation stays REAL — the direction it produces is exactly
    // what these tests assert.
    canonicalizeRelation: actual.canonicalizeRelation,
    insertRelationInTx: h.insertRelationInTx,
    deleteRelationInTx: h.deleteRelationInTx,
  }
})

import { relationsRouter } from "@/lib/trpc/relations"

const ISSUE_ID = `11111111-1111-4111-8111-111111111111`
const OTHER_ID = `22222222-2222-4222-8222-222222222222`
const RELATION_ID = `33333333-3333-4333-8333-333333333333`

const selectQueue: unknown[][] = []

function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
  const p = Promise.resolve(selectQueue.shift() ?? []) as Promise<unknown[]> &
    Record<string, () => unknown>
  for (const m of [`from`, `where`, `innerJoin`, `limit`, `orderBy`, `for`]) {
    p[m] = () => p
  }
  return p
}

const fakeDb = {
  select: vi.fn(() => selectChain()),
  execute: vi.fn(async () => ({ rows: [{ txid: `77` }] })),
  transaction: vi.fn(
    async (fn: (tx: typeof fakeDb) => Promise<unknown>): Promise<unknown> =>
      fn(fakeDb)
  ),
}

const caller = relationsRouter.createCaller({
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

beforeEach(() => {
  selectQueue.length = 0
  fakeDb.select.mockClear()
  fakeDb.transaction.mockClear()
  h.assertIssueAccess.mockClear()
  h.insertRelationInTx.mockClear()
  h.deleteRelationInTx.mockClear()
  h.issuesUpdate.mockClear()
  h.assertIssueAccess.mockResolvedValue({
    issueId: ISSUE_ID,
    boardId: `board-1`,
    teamId: `ws-1`,
  })
})

describe(`relations.create`, () => {
  it(`refuses a self-relation before touching the database`, async () => {
    const error = await rejectionOf(
      caller.create({
        issueId: ISSUE_ID,
        relatedIssueId: ISSUE_ID,
        type: `blocks`,
      })
    )

    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(h.assertIssueAccess).not.toHaveBeenCalled()
  })

  it(`refuses an issue from another team or a hidden board`, async () => {
    // Empty result = no visible issue with that id (boardVisible filtered it).
    selectQueue.push([])

    const error = await rejectionOf(
      caller.create({
        issueId: ISSUE_ID,
        relatedIssueId: OTHER_ID,
        type: `blocks`,
      })
    )

    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()

    selectQueue.push([{ teamId: `ws-other` }])
    const crossTeam = await rejectionOf(
      caller.create({
        issueId: ISSUE_ID,
        relatedIssueId: OTHER_ID,
        type: `blocks`,
      })
    )
    expect((crossTeam as TRPCError).code).toBe(`BAD_REQUEST`)
  })

  it(`writes the forward pick as given`, async () => {
    selectQueue.push([{ teamId: `ws-1` }]) // the other issue
    selectQueue.push([]) // no reverse row

    const result = await caller.create({
      issueId: ISSUE_ID,
      relatedIssueId: OTHER_ID,
      type: `parent`,
    })

    expect(h.insertRelationInTx).toHaveBeenCalledWith(
      fakeDb,
      expect.objectContaining({
        issueId: ISSUE_ID,
        relatedIssueId: OTHER_ID,
        type: `parent`,
        source: `user`,
        teamId: `ws-1`,
      })
    )
    expect(result.txId).toBe(77)
  })

  it(`swaps the pair for an inverse pick so both halves share one row`, async () => {
    selectQueue.push([{ teamId: `ws-1` }])
    selectQueue.push([])

    await caller.create({
      issueId: ISSUE_ID,
      relatedIssueId: OTHER_ID,
      type: `blocks`,
      inverse: true,
    })

    expect(h.insertRelationInTx).toHaveBeenCalledWith(
      fakeDb,
      expect.objectContaining({
        issueId: OTHER_ID,
        relatedIssueId: ISSUE_ID,
        type: `blocks`,
      })
    )
  })

  it(`refuses the reverse of an existing directed row`, async () => {
    selectQueue.push([{ teamId: `ws-1` }])
    selectQueue.push([{ id: RELATION_ID }]) // B already blocks A

    const error = await rejectionOf(
      caller.create({
        issueId: ISSUE_ID,
        relatedIssueId: OTHER_ID,
        type: `blocks`,
      })
    )

    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(h.insertRelationInTx).not.toHaveBeenCalled()
  })

  it(`delegates a duplicate pick to issues.update (dual-write)`, async () => {
    selectQueue.push([{ teamId: `ws-1` }])

    const result = await caller.create({
      issueId: ISSUE_ID,
      relatedIssueId: OTHER_ID,
      type: `duplicate`,
    })

    expect(h.issuesUpdate).toHaveBeenCalledWith({
      id: ISSUE_ID,
      duplicateOfId: OTHER_ID,
    })
    expect(h.insertRelationInTx).not.toHaveBeenCalled()
    expect(result.txId).toBe(99)
  })

  it(`marks the OTHER issue when the duplicate pick is inverted`, async () => {
    selectQueue.push([{ teamId: `ws-1` }])

    await caller.create({
      issueId: ISSUE_ID,
      relatedIssueId: OTHER_ID,
      type: `duplicate`,
      inverse: true,
    })

    expect(h.issuesUpdate).toHaveBeenCalledWith({
      id: OTHER_ID,
      duplicateOfId: ISSUE_ID,
    })
  })
})

describe(`relations.delete`, () => {
  it(`404s an unknown row`, async () => {
    selectQueue.push([])

    const error = await rejectionOf(caller.delete({ id: RELATION_ID }))

    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
    expect(h.assertIssueAccess).not.toHaveBeenCalled()
  })

  it(`gates on the source issue and deletes an ordinary row`, async () => {
    selectQueue.push([
      {
        id: RELATION_ID,
        issueId: ISSUE_ID,
        relatedIssueId: OTHER_ID,
        type: `related`,
      },
    ])

    const result = await caller.delete({ id: RELATION_ID })

    expect(h.assertIssueAccess).toHaveBeenCalledWith(`actor`, ISSUE_ID, `write`)
    expect(h.deleteRelationInTx).toHaveBeenCalledWith(
      fakeDb,
      { id: RELATION_ID },
      `actor`
    )
    expect(result).toEqual({ txId: 77, id: RELATION_ID })
  })

  it(`unmarks the duplicate instead of deleting its mirror row`, async () => {
    selectQueue.push([
      {
        id: RELATION_ID,
        issueId: ISSUE_ID,
        relatedIssueId: OTHER_ID,
        type: `duplicate`,
      },
    ])

    const result = await caller.delete({ id: RELATION_ID })

    expect(h.issuesUpdate).toHaveBeenCalledWith({
      id: ISSUE_ID,
      duplicateOfId: null,
    })
    expect(h.deleteRelationInTx).not.toHaveBeenCalled()
    expect(result.txId).toBe(99)
  })
})
