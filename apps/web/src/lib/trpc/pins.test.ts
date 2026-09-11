import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-778: the ONE pins mutation. Locks the toggle semantics — an existing
// row is deleted (no target lookup at all, so an unpin still works once the
// target is gone), a missing row is inserted only after the target is proven
// to live in the team, and a new pin appends past the caller's tail.

const h = vi.hoisted(() => ({
  assertTeamMember: vi.fn(async (..._args: unknown[]) => undefined),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: h.assertTeamMember,
}))

import { pinsRouter } from "@/lib/trpc/pins"

const TEAM_ID = `11111111-1111-4111-8111-111111111111`
const TARGET_ID = `22222222-2222-4222-8222-222222222222`

const state = {
  // Rows the delete returns — a hit means "was pinned".
  deleted: [] as { id: string }[],
  // Whether the target lookup finds the row in the team.
  targetFound: true,
  // The caller's current max sort_order.
  tail: 0,
  inserted: [] as Record<string, unknown>[],
  lookups: 0,
  // How many inserts carried ON CONFLICT DO NOTHING (the double-tap guard).
  conflictGuarded: 0,
}

function chain(rows: () => unknown[]) {
  const c: Record<string, unknown> = {}
  for (const name of [`from`, `innerJoin`, `where`, `limit`]) {
    c[name] = () => c
  }
  c.then = (resolve: (rows: unknown[]) => void) => resolve(rows())
  return c
}

const fakeTx = {
  execute: async () => ({ rows: [{ txid: `42` }] }),
  delete: () => ({
    where: () => ({ returning: async () => state.deleted }),
  }),
  select: (shape: Record<string, unknown>) => {
    // The tail query selects `{ max }`; every other select is a target lookup.
    if (`max` in shape) return chain(() => [{ max: state.tail }])
    state.lookups += 1
    return chain(() => (state.targetFound ? [{ id: TARGET_ID }] : []))
  },
  insert: () => ({
    values: (row: Record<string, unknown>) => {
      state.inserted.push(row)
      const c: Record<string, unknown> = {}
      c.onConflictDoNothing = () => {
        state.conflictGuarded += 1
        return c
      }
      c.then = (resolve: (value: unknown) => void) => resolve(undefined)
      return c
    },
  }),
}

const caller = pinsRouter.createCaller({
  session: { user: { id: `actor` } },
  db: {
    transaction: async (fn: (tx: unknown) => Promise<unknown>) => fn(fakeTx),
  },
  request: new Request(`http://localhost/`),
} as never)

describe(`pins.toggle`, () => {
  beforeEach(() => {
    state.deleted = []
    state.targetFound = true
    state.tail = 0
    state.inserted = []
    state.lookups = 0
    state.conflictGuarded = 0
    h.assertTeamMember.mockClear()
  })

  it(`unpins an existing row without looking the target up`, async () => {
    state.deleted = [{ id: `pin-1` }]
    const result = await caller.toggle({
      teamId: TEAM_ID,
      kind: `issue`,
      targetId: TARGET_ID,
    })
    expect(result).toEqual({ txId: 42, pinned: false })
    expect(h.assertTeamMember).toHaveBeenCalledWith(`actor`, TEAM_ID)
    expect(state.lookups).toBe(0)
    expect(state.inserted).toEqual([])
  })

  it.each([`issue`, `session`, `action`] as const)(
    `pins a %s of the team, appending past the caller's tail`,
    async (kind) => {
      state.tail = 3
      const result = await caller.toggle({
        teamId: TEAM_ID,
        kind,
        targetId: TARGET_ID,
      })
      expect(result).toEqual({ txId: 42, pinned: true })
      expect(state.lookups).toBe(1)
      expect(state.inserted).toEqual([
        {
          userId: `actor`,
          teamId: TEAM_ID,
          kind,
          issueId: kind === `issue` ? TARGET_ID : null,
          sessionId: kind === `session` ? TARGET_ID : null,
          actionId: kind === `action` ? TARGET_ID : null,
          sortOrder: 4,
        },
      ])
    }
  )

  it(`guards the insert against a racing pin of the same target`, async () => {
    // Two toggles of one unpinned target (a double tap) both pass the
    // delete and both insert; the loser hits the partial unique index. The
    // fake db cannot race itself, so lock the guard: the insert carries
    // ON CONFLICT DO NOTHING and the answer stays `pinned: true`.
    const result = await caller.toggle({
      teamId: TEAM_ID,
      kind: `issue`,
      targetId: TARGET_ID,
    })
    expect(result).toEqual({ txId: 42, pinned: true })
    expect(state.conflictGuarded).toBe(1)
  })

  it(`refuses a target that is not in the team`, async () => {
    state.targetFound = false
    await expect(
      caller.toggle({ teamId: TEAM_ID, kind: `action`, targetId: TARGET_ID })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect(state.inserted).toEqual([])
  })

  it(`is member-only`, async () => {
    h.assertTeamMember.mockRejectedValueOnce(new Error(`FORBIDDEN`))
    await expect(
      caller.toggle({ teamId: TEAM_ID, kind: `issue`, targetId: TARGET_ID })
    ).rejects.toThrow(`FORBIDDEN`)
    expect(state.inserted).toEqual([])
  })
})
