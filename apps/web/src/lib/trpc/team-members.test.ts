import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// teamMembers.remove is the single membership-end path (kick AND
// self-leave). REV-8: removal must also delete the ex-member's
// issue_subscribers rows in that team inside the same transaction, so
// notification fan-out and the team-scoped issue-subscribers shape stop
// referencing them. The router runs against ctx.db, so a fake db object is
// enough — `select()` shifts pre-seeded rows off a FIFO queue, `delete()`
// records the drizzle table object it was called with, and `transaction()`
// just hands the callback the same fake db.
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

const deletes: { table: unknown }[] = []

type FakeDb = {
  select: () => ReturnType<typeof selectChain>
  delete: (table: unknown) => { where: () => Promise<void> }
  transaction: (fn: (tx: FakeDb) => Promise<void>) => Promise<void>
}

const fakeDb: FakeDb = {
  select: () => selectChain(),
  delete: (table: unknown) => ({
    where: () => {
      deletes.push({ table })
      return Promise.resolve()
    },
  }),
  transaction: async (fn) => fn(fakeDb),
}

// `@/lib/trpc` (imported by the router) pulls in the real connection module;
// keep Postgres out of the test.
vi.mock(`@/db/connection`, () => ({ db: {} }))

vi.mock(`@/lib/admin`, () => ({
  isUserAdmin: vi.fn(async () => false),
  assertAdmin: vi.fn(async () => {}),
}))

const assertTeamMember = vi.fn(
  async (..._args: unknown[]) => ({ role: `owner` })
)
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: (...args: unknown[]) => assertTeamMember(...args),
}))

// REV2-7: remove must clear the shape-scope membership caches post-commit.
vi.mock(`@/lib/auth/membership-cache`, () => ({
  invalidateMembershipCaches: vi.fn(),
}))

import { teamMembersRouter } from "@/lib/trpc/team-members"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { issueSubscribers, teamMembers } from "@/db/schema"

const MEMBER_ID = `22222222-2222-4222-8222-222222222222`
const WS = `11111111-1111-4111-8111-111111111111`

function callerFor(userId: string) {
  return teamMembersRouter.createCaller({
    session: { user: { id: userId } },
    db: fakeDb,
  } as never)
}

beforeEach(() => {
  selectQueue.length = 0
  deletes.length = 0
  assertTeamMember.mockClear()
  vi.mocked(invalidateMembershipCaches).mockClear()
})

describe(`teamMembers.remove — offboarding cleanup (REV-8)`, () => {
  it(`kick: deletes the membership row AND the ex-member's issue_subscribers rows`, async () => {
    selectQueue.push([
      { id: MEMBER_ID, userId: `user-b`, teamId: WS, role: `member` },
    ])

    const result = await callerFor(`user-a`).remove({ memberId: MEMBER_ID })

    expect(result).toEqual({ ok: true })
    expect(deletes).toHaveLength(2)
    expect(deletes[0]!.table).toBe(teamMembers)
    expect(deletes[1]!.table).toBe(issueSubscribers)
    // REV2-7: membership caches cleared post-commit.
    expect(invalidateMembershipCaches).toHaveBeenCalledTimes(1)
  })

  it(`self-leave: same cleanup, without requiring owner rights`, async () => {
    selectQueue.push([
      { id: MEMBER_ID, userId: `user-b`, teamId: WS, role: `member` },
    ])

    await callerFor(`user-b`).remove({ memberId: MEMBER_ID })

    expect(assertTeamMember).not.toHaveBeenCalled()
    expect(deletes).toHaveLength(2)
    expect(deletes[0]!.table).toBe(teamMembers)
    expect(deletes[1]!.table).toBe(issueSubscribers)
  })

  it(`still refuses to remove the last owner (guard survives the transaction refactor)`, async () => {
    selectQueue.push([
      { id: MEMBER_ID, userId: `user-b`, teamId: WS, role: `owner` },
    ])
    // The owners-of-team query finds a single owner — the target.
    selectQueue.push([
      { id: MEMBER_ID, userId: `user-b`, teamId: WS, role: `owner` },
    ])

    await expect(
      callerFor(`user-a`).remove({ memberId: MEMBER_ID })
    ).rejects.toThrow(TRPCError)
    expect(deletes).toHaveLength(0)
    // REV2-7: no membership change → no cache invalidation.
    expect(invalidateMembershipCaches).not.toHaveBeenCalled()
  })
})
