import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-188 contract: teams.create is open to EVERY authed user (the old
// instance-admin gate is gone — only the invisible free-tier owned-team cap
// remains, mocked here), teams.getDefault is the NON-CREATING default-team
// resolver (oldest membership or null), and teams.delete no
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
// `inTx` records whether the write ran through the transaction handle —
// conversion events MUST NOT (see the post-commit note in teams.create).
let inTransaction = false
const inserts: {
  table: unknown
  values: Record<string, unknown>
  inTx: boolean
}[] = []
const insertReturningQueue: unknown[][] = []

const updates: {
  table: unknown
  values: Record<string, unknown>
  returning?: unknown
}[] = []
const updateReturningQueue: unknown[][] = []

type FakeDb = {
  select: () => ReturnType<typeof selectChain>
  insert: (table: unknown) => {
    values: (
      values: Record<string, unknown>
    ) => Promise<void> & { returning: () => Promise<unknown[]> }
  }
  update: (table: unknown) => {
    set: (values: Record<string, unknown>) => {
      where: () => Promise<void> & { returning: () => Promise<unknown[]> }
    }
  }
  delete: (table: unknown) => { where: () => Promise<void> }
  execute: ReturnType<typeof vi.fn>
  transaction: <T>(fn: (tx: FakeDb) => Promise<T>) => Promise<T>
}

const fakeDb: FakeDb = {
  select: () => selectChain(),
  insert: (table: unknown) => ({
    values: (values: Record<string, unknown>) => {
      inserts.push({ table, values, inTx: inTransaction })
      const p = Promise.resolve() as Promise<void> & {
        returning: () => Promise<unknown[]>
        onConflictDoNothing: () => Promise<void>
      }
      p.returning = () => Promise.resolve(insertReturningQueue.shift() ?? [])
      p.onConflictDoNothing = () => Promise.resolve()
      return p
    },
  }),
  update: (table: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: () => {
        updates.push({ table, values })
        const p = Promise.resolve() as Promise<void> & {
          returning: (projection?: unknown) => Promise<unknown[]>
        }
        p.returning = (projection?: unknown) => {
          updates[updates.length - 1]!.returning = projection
          return Promise.resolve(updateReturningQueue.shift() ?? [])
        }
        return p
      },
    }),
  }),
  delete: (table: unknown) => ({
    where: () => {
      deletes.push({ table })
      return Promise.resolve()
    },
  }),
  execute: vi.fn(async () => ({ rows: [{ txid: `42` }] })),
  transaction: async (fn) => {
    inTransaction = true
    try {
      return await fn(fakeDb)
    } finally {
      inTransaction = false
    }
  },
}

// `@/lib/trpc` (imported by the router) pulls in the real connection module;
// keep Postgres out of the test.
vi.mock(`@/db/connection`, () => ({ db: {} }))

const assertTeamMember = vi.fn(async () => ({ role: `member` }))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: (...args: unknown[]) =>
    assertTeamMember(...(args as [])),
  assertTeamOwner: vi.fn(async () => ({ role: `owner` })),
  getTeamMember: vi.fn(async () => null),
}))

const assertCanCreateTeam = vi.fn(async () => {})
const assertCanUseHelpdesk = vi.fn(async () => {})
const getInviteCapacity = vi.fn(async () => ({
  remaining: 2 as number | null,
}))
vi.mock(`@/lib/billing`, () => ({
  assertCanCreateTeam: (...args: unknown[]) =>
    assertCanCreateTeam(...(args as [])),
  assertCanUseHelpdesk: (...args: unknown[]) =>
    assertCanUseHelpdesk(...(args as [])),
  getInviteCapacity: (...args: unknown[]) =>
    getInviteCapacity(...(args as [])),
}))

// REV2-10: enabling the helpdesk without a mail transport accepts tickets
// into a black hole (the emailed magic link is the reporter's ONLY
// credential), so the toggle refuses. Mutable so both postures are testable.
const transport = { enabled: true }
vi.mock(`@/lib/email-enabled`, () => ({
  get emailEnabled() {
    return transport.enabled
  },
}))

// REV2-55: deleting a paying team is GATED on its subscription being
// cancelled first (the router no longer cancels anything itself).
const assertTeamDeletableBilling = vi.fn(async () => {})
vi.mock(`@/lib/billing/billing-handover`, () => ({
  assertTeamDeletableBilling: (...args: unknown[]) =>
    assertTeamDeletableBilling(...(args as [])),
}))

const deleteStorageObjects = vi.fn(async () => {})
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: (...args: unknown[]) =>
    deleteStorageObjects(...(args as [])),
}))

import { teamsRouter } from "@/lib/trpc/teams"
import { conversionEvents, teams, teamMembers } from "@/db/schema"

const WS = `11111111-1111-4111-8111-111111111111`

function caller() {
  return teamsRouter.createCaller({
    session: { user: { id: `user-a`, name: `User A` } },
    db: fakeDb,
  } as never)
}

beforeEach(() => {
  // Conversion tracking is cloud-only — the funnel assertions below would
  // silently pass on a self-hosted-shaped env otherwise (CI sets no
  // CLOUD_INSTANCE), so pin it here instead of inheriting the shell.
  vi.stubEnv(`CLOUD_INSTANCE`, `true`)
  inTransaction = false
  selectQueue.length = 0
  deletes.length = 0
  inserts.length = 0
  insertReturningQueue.length = 0
  updates.length = 0
  updateReturningQueue.length = 0
  transport.enabled = true
  fakeDb.execute.mockClear()
  assertCanCreateTeam.mockClear()
  assertCanUseHelpdesk.mockClear()
  assertCanUseHelpdesk.mockResolvedValue(undefined)
  assertTeamDeletableBilling.mockClear()
  assertTeamDeletableBilling.mockResolvedValue(undefined)
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
    // teams + teamMembers + the team_created conversion event (EXP-362).
    expect(inserts).toHaveLength(3)
    expect(inserts[0]!.table).toBe(teams)
    expect(inserts[1]!.table).toBe(teamMembers)
    expect(inserts[1]!.values).toMatchObject({
      teamId: WS,
      userId: `user-a`,
      role: `owner`,
    })
    expect(inserts[2]!.table).toBe(conversionEvents)
    expect(inserts[2]!.values).toMatchObject({ name: `team_created` })
    // Recorded AFTER the transaction commits: recordConversionEvent swallows
    // errors, and a swallowed failure inside the tx would poison it — the
    // COMMIT would silently become a ROLLBACK while create reported success.
    expect(inserts[0]!.inTx).toBe(true)
    expect(inserts[1]!.inTx).toBe(true)
    expect(inserts[2]!.inTx).toBe(false)
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

describe(`teams.inviteCapacity — member-readable seat headroom (EXP-725)`, () => {
  it(`asserts membership (any role) and relays the billing answer verbatim`, async () => {
    getInviteCapacity.mockResolvedValueOnce({ remaining: null })
    const result = await caller().inviteCapacity({ teamId: WS })
    expect(assertTeamMember).toHaveBeenCalledWith(`user-a`, WS)
    expect(getInviteCapacity).toHaveBeenCalledWith(WS)
    expect(result).toEqual({ remaining: null })
  })

  it(`a non-member is refused before billing is consulted`, async () => {
    assertTeamMember.mockRejectedValueOnce(new Error(`not a member`))
    getInviteCapacity.mockClear()
    await expect(caller().inviteCapacity({ teamId: WS })).rejects.toThrow(
      `not a member`
    )
    expect(getInviteCapacity).not.toHaveBeenCalled()
  })
})

describe(`teams.getDefault — non-creating resolver (EXP-188)`, () => {
  it(`returns null when the user has no membership`, async () => {
    // findOldestMembership: no rows — and crucially, NO insert happens
    // (the old ensureDefault would have self-healed a personal team here).
    selectQueue.push([])

    const result = await caller().getDefault()

    expect(result).toEqual({ team: null })
    expect(inserts).toHaveLength(0)
  })

  it(`returns the oldest membership's team`, async () => {
    selectQueue.push([{ teamId: WS }])
    const teamRow = { id: WS, name: `Ship It`, slug: `ship-it` }
    selectQueue.push([teamRow])

    const result = await caller().getDefault()

    expect(result).toEqual({ team: teamRow })
  })
})

// REV2-10: the helpdesk's whole reporter channel is email — the magic link is
// the only credential. Enabling it on an instance with no transport accepts
// tickets nobody can ever answer.
describe(`teams.update helpdesk transport gate (REV2-10)`, () => {
  it(`refuses to enable the helpdesk with no mail transport`, async () => {
    transport.enabled = false
    await expect(
      caller().update({ teamId: WS, helpdeskEnabled: true })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    expect(updates).toHaveLength(0)
    // The plan gate never even runs — the setup problem comes first.
    expect(assertCanUseHelpdesk).not.toHaveBeenCalled()
  })

  it(`enables it when a transport is configured (plan gate still applies)`, async () => {
    updateReturningQueue.push([{ id: WS, helpdeskEnabled: true }])
    const result = await caller().update({ teamId: WS, helpdeskEnabled: true })
    expect(assertCanUseHelpdesk).toHaveBeenCalledWith(WS)
    expect(result.team).toMatchObject({ helpdeskEnabled: true })
  })

  it(`always allows DISABLING it, transport or not`, async () => {
    transport.enabled = false
    updateReturningQueue.push([{ id: WS, helpdeskEnabled: false }])
    await caller().update({ teamId: WS, helpdeskEnabled: false })
    expect(updates).toHaveLength(1)
    expect(updates[0]!.table).toBe(teams)
  })

  // REV2-67: `.returning()` used to hand back the whole row, comp_tier and
  // all — the column the teams shape deliberately keeps off the wire.
  it(`returns only the synced contract columns`, async () => {
    updateReturningQueue.push([{ id: WS, name: `Ship It` }])
    await caller().update({ teamId: WS, name: `Ship It` })
    expect(updates).toHaveLength(1)
    const projection = updates[0]!.returning as Record<string, unknown>
    expect(Object.keys(projection).sort()).toEqual([
      `createdAt`,
      `endSessionsOnMerge`,
      `helpdeskEnabled`,
      `iconUrl`,
      `id`,
      `name`,
      `prMergedAutomation`,
      `prMergedStatusId`,
      `prOpenedAutomation`,
      `prOpenedStatusId`,
      `slug`,
      `updatedAt`,
    ])
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

  // REV2-55: a team with a live subscription must be un-subscribed first —
  // deleting it would strand a paying ghost in Creem (team_id goes set null).
  it(`refuses a team whose subscription is still active, deleting nothing`, async () => {
    assertTeamDeletableBilling.mockRejectedValueOnce(
      Object.assign(new Error(`cancel the subscription first`), {
        code: `PRECONDITION_FAILED`,
      })
    )
    await expect(caller().delete({ teamId: WS })).rejects.toThrow(
      `cancel the subscription first`
    )
    expect(deletes).toHaveLength(0)
    expect(deleteStorageObjects).not.toHaveBeenCalled()
  })

  it(`runs the billing gate before deleting a normal team`, async () => {
    selectQueue.push([{ storageKey: `attachments/a.png` }])
    await caller().delete({ teamId: WS })
    expect(assertTeamDeletableBilling).toHaveBeenCalledWith(WS)
  })
})
