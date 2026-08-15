import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// teamMembers.remove is the single membership-end path (kick AND
// self-leave). REV-8: removal must also delete the ex-member's
// issue_subscribers rows in that team inside the same transaction, so
// notification fan-out and the team-scoped issue-subscribers shape stop
// referencing them. REV2-28: it must clear their assignments in the same
// transaction too. REV-23: the last-owner guard must run INSIDE the write
// transaction, after the per-team advisory lock — outside it, two
// concurrent removes/demotes of the last two owners both passed the count
// and left the team ownerless. The router runs against ctx.db, so a fake
// db object is enough — `select()` shifts pre-seeded rows off a FIFO
// queue, `delete()` / `update()` record the drizzle table object they were
// called with, `execute()` records raw SQL (the advisory lock), and
// `transaction()` hands the callback the same fake db while flagging
// `inTx` so ops can assert where they ran.
const selectQueue: unknown[][] = []
// Whether each select() (in call order) ran inside the transaction.
const selectInTx: boolean[] = []

let inTx = false

function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
  selectInTx.push(inTx)
  const p = Promise.resolve(
    selectQueue.shift() ?? []
  ) as Promise<unknown[]> & Record<string, () => unknown>
  for (const m of [`from`, `where`, `innerJoin`, `limit`]) {
    p[m] = () => p
  }
  return p
}

const deletes: { table: unknown }[] = []
const updates: { table: unknown; values: Record<string, unknown> }[] = []
// Raw-SQL calls (the REV-23 advisory lock), with tx placement.
const executes: { query: unknown; inTx: boolean }[] = []
// EXP-481: rows handed back by update(...).returning() — FIFO like
// selectQueue (empty = no rows, the common case).
const updateReturningQueue: unknown[][] = []

type ThenableChain = Promise<unknown[]> & Record<string, () => unknown>

type UpdateChain = {
  set: (values: Record<string, unknown>) => { where: () => ThenableChain }
}

type FakeDb = {
  select: () => ReturnType<typeof selectChain>
  delete: (table: unknown) => { where: () => Promise<void> }
  update: (table: unknown) => UpdateChain
  execute: (query: unknown) => Promise<void>
  transaction: (fn: (tx: FakeDb) => Promise<unknown>) => Promise<unknown>
}

const fakeDb: FakeDb = {
  select: () => selectChain(),
  delete: (table: unknown) => ({
    where: () => {
      deletes.push({ table })
      return Promise.resolve()
    },
  }),
  update: (table: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: () => {
        updates.push({ table, values })
        const p = Promise.resolve([] as unknown[]) as ThenableChain
        p.returning = () => Promise.resolve(updateReturningQueue.shift() ?? [])
        return p
      },
    }),
  }),
  execute: (query: unknown) => {
    executes.push({ query, inTx })
    return Promise.resolve()
  },
  transaction: async (fn) => {
    inTx = true
    try {
      return await fn(fakeDb)
    } finally {
      inTx = false
    }
  },
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

// EXP-481: membership end clears device shares; a cleared share ends the
// foreign-hosted sessions it was the consent for.
vi.mock(`@/lib/coding-session-kill`, () => ({
  endForeignHostedSessions: vi.fn(async () => [] as string[]),
}))

import { teamMembersRouter } from "@/lib/trpc/team-members"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { endForeignHostedSessions } from "@/lib/coding-session-kill"
import { devices, issues, issueSubscribers, teamMembers } from "@/db/schema"

const MEMBER_ID = `22222222-2222-4222-8222-222222222222`
const WS = `11111111-1111-4111-8111-111111111111`

function callerFor(userId: string) {
  return teamMembersRouter.createCaller({
    session: { user: { id: userId } },
    db: fakeDb,
  } as never)
}

// The advisory lock rides sql`` — its key string sits in the query chunks.
function lockKeyOf(query: unknown): string {
  return JSON.stringify(query)
}

beforeEach(() => {
  selectQueue.length = 0
  selectInTx.length = 0
  deletes.length = 0
  updates.length = 0
  executes.length = 0
  updateReturningQueue.length = 0
  inTx = false
  assertTeamMember.mockClear()
  vi.mocked(invalidateMembershipCaches).mockClear()
  vi.mocked(endForeignHostedSessions).mockClear()
})

const targetRow = (role: `owner` | `member`) => ({
  id: MEMBER_ID,
  userId: `user-b`,
  teamId: WS,
  role,
})

describe(`teamMembers.remove — offboarding cleanup (REV-8)`, () => {
  it(`kick: deletes the membership row AND the ex-member's issue_subscribers rows`, async () => {
    // Pre-lock target read, then the in-tx re-read under the lock.
    selectQueue.push([targetRow(`member`)], [targetRow(`member`)])

    const result = await callerFor(`user-a`).remove({ memberId: MEMBER_ID })

    expect(result).toEqual({ ok: true })
    expect(deletes).toHaveLength(2)
    expect(deletes[0]!.table).toBe(teamMembers)
    expect(deletes[1]!.table).toBe(issueSubscribers)
    // REV2-28: their assignments in that team are cleared too.
    // EXP-481: and their device shares with this team — the synced devices
    // shape scopes on shared_team_id single-table and cannot re-check
    // membership the way devices.list's join does.
    expect(updates).toEqual([
      { table: issues, values: { assigneeId: null } },
      {
        table: devices,
        values: { sharedTeamId: null, updatedAt: expect.any(Date) },
      },
    ])
    // No share was actually cleared (returning was empty) — no kill fan-out.
    expect(endForeignHostedSessions).not.toHaveBeenCalled()
    // REV2-7: membership caches cleared post-commit.
    expect(invalidateMembershipCaches).toHaveBeenCalledTimes(1)
  })

  it(`ends the ex-member's foreign-hosted sessions when a share was cleared (EXP-481)`, async () => {
    selectQueue.push([targetRow(`member`)], [targetRow(`member`)])
    // The devices share-clear update reports one cleared row.
    updateReturningQueue.push([{ id: `device-row-1` }])

    await callerFor(`user-a`).remove({ memberId: MEMBER_ID })

    expect(endForeignHostedSessions).toHaveBeenCalledWith(`user-b`, WS)
  })

  it(`self-leave: same cleanup, without requiring owner rights`, async () => {
    selectQueue.push([targetRow(`member`)], [targetRow(`member`)])

    await callerFor(`user-b`).remove({ memberId: MEMBER_ID })

    expect(assertTeamMember).not.toHaveBeenCalled()
    expect(deletes).toHaveLength(2)
    expect(deletes[0]!.table).toBe(teamMembers)
    expect(deletes[1]!.table).toBe(issueSubscribers)
    expect(updates).toEqual([
      { table: issues, values: { assigneeId: null } },
      {
        table: devices,
        values: { sharedTeamId: null, updatedAt: expect.any(Date) },
      },
    ])
  })

  it(`still refuses to remove the last owner (guard survives the transaction refactor)`, async () => {
    // Pre-lock target read, in-tx re-read, then the owners-of-team count —
    // which finds a single owner: the target.
    selectQueue.push(
      [targetRow(`owner`)],
      [targetRow(`owner`)],
      [targetRow(`owner`)]
    )

    await expect(
      callerFor(`user-a`).remove({ memberId: MEMBER_ID })
    ).rejects.toThrow(TRPCError)
    expect(deletes).toHaveLength(0)
    // REV2-28: the rejected removal leaves their assignments alone.
    expect(updates).toHaveLength(0)
    // REV2-7: no membership change → no cache invalidation.
    expect(invalidateMembershipCaches).not.toHaveBeenCalled()
  })

  it(`REV-23: takes the per-team advisory lock and runs the guard inside the transaction`, async () => {
    selectQueue.push(
      [targetRow(`owner`)],
      [targetRow(`owner`)],
      [targetRow(`owner`), { id: `other`, teamId: WS, role: `owner` }]
    )

    await callerFor(`user-a`).remove({ memberId: MEMBER_ID })

    // The advisory lock is the first statement of the write transaction.
    expect(executes).toHaveLength(1)
    expect(executes[0]!.inTx).toBe(true)
    expect(lockKeyOf(executes[0]!.query)).toContain(`pg_advisory_xact_lock`)
    expect(lockKeyOf(executes[0]!.query)).toContain(`team_members:${WS}`)
    // Target re-read + owner count both ran under the lock (in-tx), not on
    // the pre-lock snapshot; only the initial target fetch runs outside.
    expect(selectInTx).toEqual([false, true, true])
    expect(deletes[0]!.table).toBe(teamMembers)
  })

  it(`REV-23: a concurrent removal observed under the lock fails the guard, not the count`, async () => {
    // Outer read still saw the row; under the lock it is gone (the racing
    // remove committed first) — abort instead of deleting blind.
    selectQueue.push([targetRow(`member`)], [])

    await expect(
      callerFor(`user-a`).remove({ memberId: MEMBER_ID })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect(deletes).toHaveLength(0)
  })
})

describe(`teamMembers.updateRole — last-owner demote guard (REV-23)`, () => {
  it(`refuses to demote the last owner, counting inside the locked transaction`, async () => {
    selectQueue.push(
      [targetRow(`owner`)],
      [targetRow(`owner`)],
      [targetRow(`owner`)]
    )

    await expect(
      callerFor(`user-a`).updateRole({ memberId: MEMBER_ID, role: `member` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(updates).toHaveLength(0)
    expect(executes).toHaveLength(1)
    expect(executes[0]!.inTx).toBe(true)
    expect(lockKeyOf(executes[0]!.query)).toContain(`team_members:${WS}`)
    expect(selectInTx).toEqual([false, true, true])
  })

  it(`demotes when another owner remains — and trusts the in-tx role, not the stale snapshot`, async () => {
    // Outer read raced a promotion and still says member; the in-tx re-read
    // sees the real owner role, so the guard DOES count owners.
    selectQueue.push(
      [targetRow(`member`)],
      [targetRow(`owner`)],
      [targetRow(`owner`), { id: `other`, teamId: WS, role: `owner` }]
    )
    updateReturningQueue.push([{ ...targetRow(`member`) }])

    const result = await callerFor(`user-a`).updateRole({
      memberId: MEMBER_ID,
      role: `member`,
    })

    expect(result.member).toMatchObject({ id: MEMBER_ID, role: `member` })
    expect(updates).toEqual([{ table: teamMembers, values: { role: `member` } }])
  })

  it(`promotions skip the owner count but still serialize on the team lock`, async () => {
    selectQueue.push([targetRow(`member`)], [targetRow(`member`)])
    updateReturningQueue.push([{ ...targetRow(`owner`) }])

    await callerFor(`user-a`).updateRole({ memberId: MEMBER_ID, role: `owner` })

    expect(executes).toHaveLength(1)
    expect(executes[0]!.inTx).toBe(true)
    // Two selects only: no owner count on a promotion.
    expect(selectInTx).toEqual([false, true])
  })
})
