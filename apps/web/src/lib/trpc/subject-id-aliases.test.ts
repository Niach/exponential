import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-707 transitional aliases. The rename of the subject param on
// boards.update (`id` -> `boardId`) and teams.update (`id` -> `teamId`) is
// NOT hard yet: desktop 0.14.28 is in the wild and still posts `id`, so both
// procedures accept EXACTLY ONE of the two keys and normalize to the new one.
// Removal trigger: desktop min >= 0.14.29. (issues.bulkDelete's `ids` alias
// is pinned next to its siblings in issues-bulk.test.ts.)
const h = vi.hoisted(() => {
  const selectQueue: unknown[][] = []
  const updates: Record<string, unknown>[] = []
  const updateWheres: unknown[] = []
  const updateReturns: unknown[][] = []

  function selectChain(): Promise<unknown[]> &
    Record<string, (...args: unknown[]) => unknown> {
    const p = Promise.resolve(selectQueue.shift() ?? []) as Promise<unknown[]> &
      Record<string, (...args: unknown[]) => unknown>
    for (const m of [`from`, `where`, `innerJoin`, `orderBy`, `limit`]) {
      p[m] = () => p
    }
    return p
  }

  function updateChain() {
    return {
      set: (values: Record<string, unknown>) => {
        updates.push(values)
        const chain = {
          where: (condition: unknown) => {
            updateWheres.push(condition)
            return chain
          },
          returning: () =>
            Promise.resolve(updateReturns.shift() ?? [{ id: `row` }]),
          then: (resolve: (value: unknown) => unknown) => resolve(undefined),
        }
        return chain
      },
    }
  }

  const execute = vi.fn(async () => ({ rows: [{ txid: `42` }] }))

  const fakeDb = {
    select: () => selectChain(),
    update: () => updateChain(),
    execute,
    transaction: async (fn: (tx: unknown) => Promise<unknown>) =>
      fn({ execute, update: () => updateChain(), select: () => selectChain() }),
  }

  return {
    selectQueue,
    updates,
    updateWheres,
    updateReturns,
    execute,
    fakeDb,
    assertBoardMember: vi.fn(async () => undefined),
    assertTeamOwner: vi.fn(async () => ({ role: `owner` })),
  }
})

vi.mock(`@/db/connection`, () => ({ db: h.fakeDb }))
vi.mock(`@/lib/team-membership`, () => ({
  assertBoardMember: h.assertBoardMember,
  assertTeamOwner: h.assertTeamOwner,
  assertTeamMember: vi.fn(),
  resolveTeamAccess: vi.fn(),
  getTeamMember: vi.fn(async () => null),
}))
vi.mock(`@/lib/billing`, () => ({
  assertCanCreateTeam: vi.fn(async () => undefined),
  assertCanUseHelpdesk: vi.fn(async () => undefined),
}))
vi.mock(`@/lib/billing/billing-handover`, () => ({
  assertTeamDeletableBilling: vi.fn(async () => undefined),
}))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: vi.fn(async () => undefined),
}))
vi.mock(`@/lib/email-enabled`, () => ({ emailEnabled: true }))

import { boardsRouter } from "@/lib/trpc/boards"
import { teamsRouter } from "@/lib/trpc/teams"

const BOARD = `22222222-2222-4222-8222-222222222222`
const TEAM = `11111111-1111-4111-8111-111111111111`

function boardsCaller() {
  return boardsRouter.createCaller({
    session: { user: { id: `user-a` } },
    db: h.fakeDb,
    request: new Request(`http://localhost/`),
  } as never)
}

function teamsCaller() {
  return teamsRouter.createCaller({
    session: { user: { id: `user-a` } },
    db: h.fakeDb,
  } as never)
}

async function rejectionOf(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => undefined,
    (e: unknown) => e
  )
}

beforeEach(() => {
  h.selectQueue.length = 0
  h.updates.length = 0
  h.updateWheres.length = 0
  h.updateReturns.length = 0
  h.execute.mockClear()
  h.assertBoardMember.mockClear()
  h.assertTeamOwner.mockClear()
})

describe(`boards.update subject-id alias`, () => {
  it(`accepts the canonical boardId`, async () => {
    h.updateReturns.push([{ id: BOARD, name: `New` }])

    const result = await boardsCaller().update({
      boardId: BOARD,
      name: `New`,
    })

    expect(h.assertBoardMember).toHaveBeenCalledWith(`user-a`, BOARD)
    expect(h.updates).toEqual([{ name: `New` }])
    expect(result.board).toEqual({ id: BOARD, name: `New` })
  })

  it(`accepts the deprecated id and normalizes it`, async () => {
    h.updateReturns.push([{ id: BOARD, name: `New` }])

    const result = await boardsCaller().update({ id: BOARD, name: `New` })

    expect(h.assertBoardMember).toHaveBeenCalledWith(`user-a`, BOARD)
    // The alias key never leaks into the SET clause.
    expect(h.updates).toEqual([{ name: `New` }])
    expect(result.board).toEqual({ id: BOARD, name: `New` })
  })

  it(`rejects both keys and neither key`, async () => {
    const both = await rejectionOf(
      boardsCaller().update({ boardId: BOARD, id: BOARD, name: `New` })
    )
    expect(both).toBeInstanceOf(TRPCError)
    expect((both as TRPCError).code).toBe(`BAD_REQUEST`)

    const neither = await rejectionOf(boardsCaller().update({ name: `New` }))
    expect(neither).toBeInstanceOf(TRPCError)
    expect((neither as TRPCError).code).toBe(`BAD_REQUEST`)

    expect(h.assertBoardMember).not.toHaveBeenCalled()
    expect(h.updates).toHaveLength(0)
  })

  it(`keeps the empty-patch no-op on the alias path`, async () => {
    h.selectQueue.push([{ id: BOARD, name: `Same` }])

    const result = await boardsCaller().update({ id: BOARD })

    expect(result.board).toEqual({ id: BOARD, name: `Same` })
    expect(h.updates).toHaveLength(0)
  })
})

describe(`teams.update subject-id alias`, () => {
  it(`accepts the canonical teamId`, async () => {
    h.updateReturns.push([{ id: TEAM, name: `Acme` }])

    const result = await teamsCaller().update({ teamId: TEAM, name: `Acme` })

    expect(h.assertTeamOwner).toHaveBeenCalledWith(`user-a`, TEAM)
    expect(h.updates[0]!.name).toBe(`Acme`)
    expect(result.team).toEqual({ id: TEAM, name: `Acme` })
  })

  it(`accepts the deprecated id and normalizes it`, async () => {
    h.updateReturns.push([{ id: TEAM, name: `Acme` }])

    const result = await teamsCaller().update({ id: TEAM, name: `Acme` })

    expect(h.assertTeamOwner).toHaveBeenCalledWith(`user-a`, TEAM)
    // The alias key never leaks into the SET clause.
    expect(Object.keys(h.updates[0]!).sort()).toEqual([`name`, `updatedAt`])
    expect(result.team).toEqual({ id: TEAM, name: `Acme` })
  })

  it(`rejects both keys and neither key`, async () => {
    const both = await rejectionOf(
      teamsCaller().update({ teamId: TEAM, id: TEAM, name: `Acme` })
    )
    expect(both).toBeInstanceOf(TRPCError)
    expect((both as TRPCError).code).toBe(`BAD_REQUEST`)

    const neither = await rejectionOf(teamsCaller().update({ name: `Acme` }))
    expect(neither).toBeInstanceOf(TRPCError)
    expect((neither as TRPCError).code).toBe(`BAD_REQUEST`)

    expect(h.assertTeamOwner).not.toHaveBeenCalled()
    expect(h.updates).toHaveLength(0)
  })
})
