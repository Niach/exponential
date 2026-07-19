import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-188 contract: teams.create is open to EVERY authed user (the old
// instance-admin gate is gone — only the invisible free-tier owned-team cap
// remains, mocked here), teams.getDefault is the NON-CREATING default-team
// resolver (oldest non-feedback membership or null), and teams.delete no
// longer refuses the user's last team (nothing self-heals a replacement
// anymore — a team-less user routes back into onboarding). The router runs
// against ctx.db, so a fake db object is enough — `select()` shifts
// pre-seeded rows off a FIFO queue, `insert()` records values and shifts
// `.returning()` rows off its own queue, `delete()` records the drizzle
// table object, `execute()` fakes generateTxId's
// `SELECT pg_current_xact_id()` probe, and `transaction()` hands the
// callback the same fake db.
const selectQueue: unknown[][] = []

function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
  const p = Promise.resolve(
    selectQueue.shift() ?? []
  ) as Promise<unknown[]> & Record<string, () => unknown>
  for (const m of [`from`, `where`, `innerJoin`, `orderBy`, `limit`]) {
    p[m] = () => p
  }
  return p
}

const deletes: { table: unknown }[] = []
const inserts: { table: unknown; values: Record<string, unknown> }[] = []
const insertReturningQueue: unknown[][] = []

type FakeDb = {
  select: () => ReturnType<typeof selectChain>
  insert: (table: unknown) => {
    values: (
      values: Record<string, unknown>
    ) => Promise<void> & { returning: () => Promise<unknown[]> }
  }
  delete: (table: unknown) => { where: () => Promise<void> }
  execute: ReturnType<typeof vi.fn>
  transaction: <T>(fn: (tx: FakeDb) => Promise<T>) => Promise<T>
}

const fakeDb: FakeDb = {
  select: () => selectChain(),
  insert: (table: unknown) => ({
    values: (values: Record<string, unknown>) => {
      inserts.push({ table, values })
      const p = Promise.resolve() as Promise<void> & {
        returning: () => Promise<unknown[]>
      }
      p.returning = () => Promise.resolve(insertReturningQueue.shift() ?? [])
      return p
    },
  }),
  delete: (table: unknown) => ({
    where: () => {
      deletes.push({ table })
      return Promise.resolve()
    },
  }),
  execute: vi.fn(async () => ({ rows: [{ txid: `42` }] })),
  transaction: async (fn) => fn(fakeDb),
}

// `@/lib/trpc` (imported by the router) pulls in the real connection module;
// keep Postgres out of the test.
vi.mock(`@/db/connection`, () => ({ db: {} }))

vi.mock(`@/lib/team-membership`, () => ({
  assertTeamOwner: vi.fn(async () => ({ role: `owner` })),
  getTeamMember: vi.fn(async () => null),
}))

const assertCanCreateTeam = vi.fn(async () => {})
vi.mock(`@/lib/billing`, () => ({
  assertCanCreateTeam: (...args: unknown[]) =>
    assertCanCreateTeam(...(args as [])),
  assertCanUseHelpdesk: vi.fn(async () => {}),
}))

const FEEDBACK_WS = `99999999-9999-4999-8999-999999999999`
vi.mock(`@/lib/bootstrap-cloud`, () => ({
  getFeedbackTeamId: vi.fn(async () => FEEDBACK_WS),
}))

const cancelCreemSubscriptionsBestEffort = vi.fn(async () => {})
vi.mock(`@/lib/billing/creem-subscriptions`, () => ({
  findActiveSubscriptionsForTeams: vi.fn(async () => []),
  cancelCreemSubscriptionsBestEffort: (...args: unknown[]) =>
    cancelCreemSubscriptionsBestEffort(...(args as [])),
}))

const deleteStorageObjects = vi.fn(async () => {})
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: (...args: unknown[]) =>
    deleteStorageObjects(...(args as [])),
}))

import { teamsRouter } from "@/lib/trpc/teams"
import { teams, teamMembers } from "@/db/schema"

const WS = `11111111-1111-4111-8111-111111111111`

function caller() {
  return teamsRouter.createCaller({
    session: { user: { id: `user-a`, name: `User A` } },
    db: fakeDb,
  } as never)
}

beforeEach(() => {
  selectQueue.length = 0
  deletes.length = 0
  inserts.length = 0
  insertReturningQueue.length = 0
  fakeDb.execute.mockClear()
  assertCanCreateTeam.mockClear()
  cancelCreemSubscriptionsBestEffort.mockClear()
  deleteStorageObjects.mockClear()
})

describe(`teams.create — open to every user (EXP-188)`, () => {
  it(`creates a team for a regular (non-admin) user who becomes owner`, async () => {
    // uniqueSlug probe: slug is free.
    selectQueue.push([])
    const teamRow = { id: WS, name: `Ship It`, slug: `ship-it` }
    insertReturningQueue.push([teamRow])

    const result = await caller().create({ name: `Ship It` })

    expect(result).toEqual({ team: teamRow, txId: 42 })
    // The only gate is the free-tier owned-team cap — no admin check.
    expect(assertCanCreateTeam).toHaveBeenCalledWith(`user-a`)
    expect(inserts).toHaveLength(2)
    expect(inserts[0]!.table).toBe(teams)
    expect(inserts[1]!.table).toBe(teamMembers)
    expect(inserts[1]!.values).toMatchObject({
      teamId: WS,
      userId: `user-a`,
      role: `owner`,
    })
  })

  it(`propagates the free-tier owned-team cap`, async () => {
    assertCanCreateTeam.mockRejectedValueOnce(
      Object.assign(new Error(`cap`), { code: `FORBIDDEN` })
    )
    await expect(caller().create({ name: `One Too Many` })).rejects.toThrow(
      `cap`
    )
    expect(inserts).toHaveLength(0)
  })
})

describe(`teams.getDefault — non-creating resolver (EXP-188)`, () => {
  it(`returns null when the user has no non-feedback membership`, async () => {
    // findNonFeedbackMembership: no rows — and crucially, NO insert happens
    // (the old ensureDefault would have self-healed a personal team here).
    selectQueue.push([])

    const result = await caller().getDefault()

    expect(result).toEqual({ team: null })
    expect(inserts).toHaveLength(0)
  })

  it(`returns the oldest non-feedback membership's team`, async () => {
    selectQueue.push([{ teamId: WS }])
    const teamRow = { id: WS, name: `Ship It`, slug: `ship-it` }
    selectQueue.push([teamRow])

    const result = await caller().getDefault()

    expect(result).toEqual({ team: teamRow })
  })
})

describe(`teams.delete (EXP-188: no last-team guard)`, () => {
  it(`deletes the user's only team, reclaiming storage`, async () => {
    // Only in-tx select left: attachments storage-key collection — there is
    // no membership pre-check anymore, so a solo owner's single team goes.
    selectQueue.push([{ storageKey: `attachments/a.png` }])

    const result = await caller().delete({ teamId: WS })

    expect(result).toEqual({ ok: true, txId: 42 })
    expect(deletes).toHaveLength(1)
    expect(deletes[0]!.table).toBe(teams)
    expect(deleteStorageObjects).toHaveBeenCalledWith([`attachments/a.png`])
  })

  it(`still refuses to delete the bootstrap feedback team`, async () => {
    await expect(
      caller().delete({ teamId: FEEDBACK_WS })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(deletes).toHaveLength(0)
    expect(cancelCreemSubscriptionsBestEffort).not.toHaveBeenCalled()
    expect(deleteStorageObjects).not.toHaveBeenCalled()
  })
})
