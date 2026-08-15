import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-500: archive is the NON-PURGING sibling of the 48h trash. Both hide a
// board and all of its issues from every client (server-side, via the Electric
// where clauses) and from every server read surface; only the trash ever
// deletes anything. These tests pin the procedure contracts — owner gating,
// idempotence, and the trash/archive interaction — against a fake db, so they
// never touch Postgres.
const h = vi.hoisted(() => {
  const selectQueue: unknown[][] = []
  const updates: Record<string, unknown>[] = []
  // Rows the UPDATE ... RETURNING inside the transaction reports back.
  const updateReturns: { id: string }[][] = []

  function selectChain(): Promise<unknown[]> &
    Record<string, (...args: unknown[]) => unknown> {
    const p = Promise.resolve(selectQueue.shift() ?? []) as Promise<unknown[]> &
      Record<string, (...args: unknown[]) => unknown>
    for (const m of [`from`, `where`, `orderBy`, `limit`]) {
      p[m] = () => p
    }
    return p
  }

  function updateChain() {
    return {
      set: (values: Record<string, unknown>) => {
        updates.push(values)
        const chain = {
          where: () => chain,
          returning: () =>
            Promise.resolve(updateReturns.shift() ?? [{ id: `b` }]),
          then: (resolve: (value: unknown) => unknown) => resolve(undefined),
        }
        return chain
      },
    }
  }

  const fakeDb = {
    select: () => selectChain(),
    update: () => updateChain(),
    transaction: async (fn: (tx: unknown) => Promise<unknown>) =>
      fn({
        execute: () => Promise.resolve({ rows: [{ txid: `42` }] }),
        update: () => updateChain(),
      }),
  }

  return {
    selectQueue,
    updates,
    updateReturns,
    fakeDb,
    assertTeamOwner: vi.fn(),
  }
})

vi.mock(`@/db/connection`, () => ({ db: h.fakeDb }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamOwner: h.assertTeamOwner,
  assertBoardMember: vi.fn(),
  resolveTeamAccess: vi.fn(),
}))

import { boardsRouter } from "@/lib/trpc/boards"

const TEAM = `11111111-1111-4111-8111-111111111111`
const BOARD = `22222222-2222-4222-8222-222222222222`

function caller() {
  return boardsRouter.createCaller({
    session: { user: { id: `owner-1` } },
    db: h.fakeDb,
    request: new Request(`http://localhost/`),
  } as never)
}

beforeEach(() => {
  h.selectQueue.length = 0
  h.updates.length = 0
  h.updateReturns.length = 0
  h.assertTeamOwner.mockReset()
  h.assertTeamOwner.mockResolvedValue(undefined)
})

describe(`boards.archive`, () => {
  it(`stamps archivedAt and returns a txId so clients can await sync`, async () => {
    h.selectQueue.push([{ teamId: TEAM, deletedAt: null, archivedAt: null }])

    const result = await caller().archive({ boardId: BOARD })

    expect(h.assertTeamOwner).toHaveBeenCalledWith(`owner-1`, TEAM)
    expect(result.ok).toBe(true)
    expect(result).toHaveProperty(`txId`)
    expect(h.updates).toHaveLength(1)
    expect(h.updates[0].archivedAt).toBeInstanceOf(Date)
  })

  it(`is an idempotent no-op for an already-archived board`, async () => {
    h.selectQueue.push([
      { teamId: TEAM, deletedAt: null, archivedAt: new Date() },
    ])

    const result = await caller().archive({ boardId: BOARD })

    // Nothing changed, so no write and no sync barrier is needed.
    expect(result).toEqual({ ok: true })
    expect(h.updates).toHaveLength(0)
  })

  it(`refuses a trashed board — its purge countdown would become a lie`, async () => {
    h.selectQueue.push([
      { teamId: TEAM, deletedAt: new Date(), archivedAt: null },
    ])

    await expect(caller().archive({ boardId: BOARD })).rejects.toThrow(
      /in the trash/
    )
    expect(h.updates).toHaveLength(0)
  })

  it(`404s an unknown board before it ever checks ownership`, async () => {
    h.selectQueue.push([])

    await expect(caller().archive({ boardId: BOARD })).rejects.toThrow(
      TRPCError
    )
    expect(h.assertTeamOwner).not.toHaveBeenCalled()
  })

  it(`is owner-only`, async () => {
    h.selectQueue.push([{ teamId: TEAM, deletedAt: null, archivedAt: null }])
    h.assertTeamOwner.mockRejectedValue(
      new TRPCError({ code: `FORBIDDEN`, message: `nope` })
    )

    await expect(caller().archive({ boardId: BOARD })).rejects.toThrow(/nope/)
    expect(h.updates).toHaveLength(0)
  })
})

describe(`boards.unarchive`, () => {
  it(`clears archivedAt`, async () => {
    h.selectQueue.push([{ teamId: TEAM }])
    h.updateReturns.push([{ id: BOARD }])

    const result = await caller().unarchive({ boardId: BOARD })

    expect(h.assertTeamOwner).toHaveBeenCalledWith(`owner-1`, TEAM)
    expect(result.ok).toBe(true)
    expect(h.updates[0]).toEqual({ archivedAt: null })
  })

  it(`404s when the board was not archived (0 rows updated)`, async () => {
    h.selectQueue.push([{ teamId: TEAM }])
    h.updateReturns.push([])

    await expect(caller().unarchive({ boardId: BOARD })).rejects.toThrow(
      /not archived/
    )
  })

  it(`is owner-only`, async () => {
    h.selectQueue.push([{ teamId: TEAM }])
    h.assertTeamOwner.mockRejectedValue(
      new TRPCError({ code: `FORBIDDEN`, message: `nope` })
    )

    await expect(caller().unarchive({ boardId: BOARD })).rejects.toThrow(/nope/)
    expect(h.updates).toHaveLength(0)
  })
})

describe(`boards.listArchived`, () => {
  it(`is owner-gated and returns the archived rows`, async () => {
    h.selectQueue.push([
      {
        id: BOARD,
        name: `Old work`,
        slug: `old-work`,
        prefix: `OLD`,
        color: `#6366f1`,
        icon: null,
        repositoryId: null,
        archivedAt: new Date(`2026-08-01T00:00:00Z`),
      },
    ])

    const rows = await caller().listArchived({ teamId: TEAM })

    expect(h.assertTeamOwner).toHaveBeenCalledWith(`owner-1`, TEAM)
    expect(rows).toHaveLength(1)
    expect(rows[0].name).toBe(`Old work`)
    // No purgeAt: unlike the trash, nothing ever deletes an archived board.
    expect(rows[0]).not.toHaveProperty(`purgeAt`)
  })
})
