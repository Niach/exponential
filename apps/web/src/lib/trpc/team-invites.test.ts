import { beforeEach, describe, expect, it, vi } from "vitest"

// Three contracts live here:
//
// 1. REV-4: teamInvites.list is member-visible (and relayed verbatim by the
//    MCP exponential_invites_list tool), so it must never return the invite
//    bearer `token` — accept() is not recipient-bound, and a leaked
//    owner-role token lets any member escalate to owner. The token's only
//    surface is the `create` mutation response, to the owner who minted it.
//
// 2. EXP-188 invite-by-email: `create` persists the optional recipient email
//    and best-effort-delivers the invite link (a transport failure must never
//    roll back the invite), and `accept` stamps users.onboardingCompletedAt
//    in-tx — guarded by an IS NULL predicate so an existing timestamp is
//    never overwritten — on BOTH the fresh-join and alreadyMember paths.
//
// 3. REV2-71: `accept` runs the seat gate in-tx on the fresh-join path only,
//    after the invite-validity checks, the alreadyMember no-op and the
//    single-use claim.
//
// The router runs against ctx.db, so a fake db is enough: `select()` shifts
// rows off a FIFO queue, `insert()`/`update()` record their target table +
// values and serve `.returning()` from their own queues, `execute()` fakes
// generateTxId, and `transaction()` hands back the same fake db.
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

// `inTx` records whether the write ran through the transaction handle —
// conversion events MUST NOT (see the post-commit note in teamInvites.accept).
let inTransaction = false
const inserts: {
  table: unknown
  values: Record<string, unknown>
  inTx: boolean
}[] = []
const insertReturningQueue: unknown[][] = []
const updates: {
  table: unknown
  set: Record<string, unknown>
  where: unknown
}[] = []
const updateReturningQueue: unknown[][] = []
const deletes: { table: unknown; where: unknown }[] = []

type FakeDb = {
  select: () => ReturnType<typeof selectChain>
  insert: (table: unknown) => {
    values: (
      values: Record<string, unknown>
    ) => Promise<void> & { returning: () => Promise<unknown[]> }
  }
  update: (table: unknown) => {
    set: (set: Record<string, unknown>) => {
      where: (
        where: unknown
      ) => Promise<void> & { returning: () => Promise<unknown[]> }
    }
  }
  delete: (table: unknown) => { where: (where: unknown) => Promise<void> }
  execute: ReturnType<typeof vi.fn>
  transaction: <T>(fn: (tx: FakeDb) => Promise<T>) => Promise<T>
}

const fakeDb: FakeDb = {
  select: () => selectChain(),
  delete: (table: unknown) => ({
    where: (where: unknown) => {
      deletes.push({ table, where })
      return Promise.resolve()
    },
  }),
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
    set: (set: Record<string, unknown>) => ({
      where: (where: unknown) => {
        updates.push({ table, set, where })
        const p = Promise.resolve() as Promise<void> & {
          returning: () => Promise<unknown[]>
        }
        p.returning = () =>
          Promise.resolve(updateReturningQueue.shift() ?? [])
        return p
      },
    }),
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
// keep Postgres out of the test. The router ALSO reads the shared db directly
// for the per-address invite-email cap — serve that count from a queue
// (default 0 = under the cap).
const inviteEmailCountQueue: number[] = []

function connectionSelectChain(): Promise<unknown[]> &
  Record<string, () => unknown> {
  const p = Promise.resolve([
    { value: inviteEmailCountQueue.shift() ?? 0 },
  ]) as Promise<unknown[]> & Record<string, () => unknown>
  for (const m of [`from`, `where`, `limit`]) {
    p[m] = () => p
  }
  return p
}

vi.mock(`@/db/connection`, () => ({
  db: { select: () => connectionSelectChain() },
}))

vi.mock(`@/lib/admin`, () => ({
  isUserAdmin: vi.fn(async () => false),
}))

vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: vi.fn(async () => ({ role: `owner` })),
}))

const assertCanInviteMember = vi.fn(async (_teamId: string) => {})
vi.mock(`@/lib/billing`, () => ({
  assertCanInviteMember: (...args: unknown[]) =>
    assertCanInviteMember(...(args as [string])),
}))

const sendTeamInviteEmail = vi.fn(async () => ({ delivered: true }))
vi.mock(`@/lib/email`, () => ({
  sendTeamInviteEmail: (...args: unknown[]) =>
    sendTeamInviteEmail(...(args as [])),
  deliveryStatus: (result: { delivered: boolean; suppressed?: boolean }) =>
    result.delivered ? `sent` : result.suppressed ? `suppressed` : `failed`,
}))

vi.mock(`@/lib/notification-email-policy`, () => ({
  appBaseUrl: () => `http://localhost:3000`,
}))

// EXP-630: a placeholder is only created when the invitee could ever sign in
// AS it (mail transport or a social/OIDC provider). The instance posture is
// read through buildAuthConfig; default here = a mail transport exists.
const authConfig = {
  passwordEnabled: true,
  signupEnabled: true,
  passwordResetEnabled: true,
  oidcProviders: [] as { id: string; name: string }[],
  googleLoginEnabled: false,
  appleLoginEnabled: false,
  githubEnabled: false,
  deviceFlowEnabled: true,
  emailOtpEnabled: true,
  passkeyEnabled: false,
}
vi.mock(`@/lib/auth/config`, () => ({
  buildAuthConfig: () => ({ ...authConfig }),
}))

// EXP-630: the placeholder-member library runs real SQL over the tx; here
// the router's ORCHESTRATION is the contract (when a placeholder is created,
// claimed or merged), so the three effectful helpers are stubbed and the
// pure ones stay real.
const createPlaceholderMember = vi.fn(async () => ({ userId: `ph-1` }))
const claimPlaceholder = vi.fn(async () => {})
const mergePlaceholderIntoUser = vi.fn(async () => ({
  merged: true,
  deletedPlaceholder: true,
}))
vi.mock(`@/lib/placeholder-members`, async (importOriginal) => ({
  // eslint-disable-next-line quotes
  ...(await importOriginal<typeof import("@/lib/placeholder-members")>()),
  createPlaceholderMember: (...args: unknown[]) =>
    createPlaceholderMember(...(args as [])),
  claimPlaceholder: (...args: unknown[]) => claimPlaceholder(...(args as [])),
  mergePlaceholderIntoUser: (...args: unknown[]) =>
    mergePlaceholderIntoUser(...(args as [])),
}))

import {
  inviteListSelection,
  placeholderClaimable,
  teamInvitesRouter,
} from "@/lib/trpc/team-invites"
import {
  conversionEvents,
  emailDeliveries,
  teamInvites,
  teamMembers,
  users,
} from "@/db/schema"

const WS = `11111111-1111-4111-8111-111111111111`
const INVITE_ID = `33333333-3333-4333-8333-333333333333`

function caller() {
  return teamInvitesRouter.createCaller({
    session: {
      user: { id: `user-a`, name: `User A`, email: `a@example.com` },
    },
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
  inserts.length = 0
  insertReturningQueue.length = 0
  updates.length = 0
  updateReturningQueue.length = 0
  deletes.length = 0
  inviteEmailCountQueue.length = 0
  authConfig.passwordResetEnabled = true
  authConfig.emailOtpEnabled = true
  authConfig.googleLoginEnabled = false
  authConfig.appleLoginEnabled = false
  authConfig.oidcProviders = []
  fakeDb.execute.mockClear()
  sendTeamInviteEmail.mockClear()
  sendTeamInviteEmail.mockResolvedValue({ delivered: true })
  assertCanInviteMember.mockClear()
  assertCanInviteMember.mockResolvedValue(undefined)
  createPlaceholderMember.mockClear()
  claimPlaceholder.mockClear()
  mergePlaceholderIntoUser.mockClear()
  mergePlaceholderIntoUser.mockResolvedValue({
    merged: true,
    deletedPlaceholder: true,
  })
})

describe(`teamInvites.list selection contract`, () => {
  it(`excludes the invite bearer token`, () => {
    expect(Object.keys(inviteListSelection)).not.toContain(`token`)
  })

  it(`selects exactly the member-visible invite fields`, () => {
    expect(Object.keys(inviteListSelection).sort()).toEqual([
      `acceptedAt`,
      `createdAt`,
      `email`,
      `expiresAt`,
      `id`,
      `invitedById`,
      `placeholderUserId`,
      `role`,
      `teamId`,
      `updatedAt`,
    ])
  })
})

describe(`teamInvites.create — invite by email (EXP-188)`, () => {
  it(`persists the email and delivers the invite link`, async () => {
    insertReturningQueue.push([
      { id: INVITE_ID, teamId: WS, email: `new@example.com` },
    ])
    // No account owns the address yet; then the team-name lookup for the
    // email subject/body.
    selectQueue.push([])
    selectQueue.push([{ name: `Acme` }])

    const result = await caller().create({
      teamId: WS,
      email: `New@Example.com`,
      name: `New Person`,
    })

    // EXP-630: the address is free, so the person joins at once as a
    // placeholder member (seat gate first) and the invite is bound to it.
    expect(assertCanInviteMember).toHaveBeenCalledWith(WS)
    expect(createPlaceholderMember).toHaveBeenCalledWith(
      fakeDb,
      expect.objectContaining({
        teamId: WS,
        role: `member`,
        identity: { email: `new@example.com`, name: `New Person` },
      })
    )
    expect(result.memberUserId).toBe(`ph-1`)
    // Three inserts: the invite row, the email_deliveries ledger row (every
    // invite-email attempt is ledgered so bounces trace per-message), and
    // the invite_sent conversion event (EXP-362).
    expect(inserts).toHaveLength(3)
    expect(inserts[0]!.table).toBe(teamInvites)
    expect(inserts[0]!.inTx).toBe(true)
    expect(inserts[0]!.values).toMatchObject({
      email: `new@example.com`,
      placeholderUserId: `ph-1`,
    })
    expect(sendTeamInviteEmail).toHaveBeenCalledWith({
      to: `new@example.com`,
      teamName: `Acme`,
      inviterName: `User A`,
      inviteUrl: `http://localhost:3000/invite/${result.token}`,
    })
    expect(result.emailDelivered).toBe(true)
    expect(inserts[1]!.table).toBe(emailDeliveries)
    expect(inserts[1]!.values).toMatchObject({
      kind: `team_invite`,
      status: `sent`,
      toEmail: `new@example.com`,
    })
    expect(inserts[2]!.table).toBe(conversionEvents)
    expect(inserts[2]!.values).toMatchObject({ name: `invite_sent` })
  })

  it(`skips the email past the per-address weekly cap but still creates the invite`, async () => {
    insertReturningQueue.push([
      { id: INVITE_ID, teamId: WS, email: `new@example.com` },
    ])
    // 3 invite emails already sent to this address in the last 7 days.
    inviteEmailCountQueue.push(3)
    selectQueue.push([])

    const result = await caller().create({
      teamId: WS,
      email: `new@example.com`,
    })

    expect(sendTeamInviteEmail).not.toHaveBeenCalled()
    expect(result.emailDelivered).toBe(false)
    expect(result.invite).toMatchObject({ id: INVITE_ID })
    // invite + suppressed ledger row + invite_sent conversion event.
    expect(inserts).toHaveLength(3)
    expect(inserts[1]!.table).toBe(emailDeliveries)
    expect(inserts[1]!.values).toMatchObject({
      kind: `team_invite`,
      status: `suppressed`,
    })
    expect(inserts[2]!.table).toBe(conversionEvents)
  })

  it(`returns emailDelivered null and sends nothing without an email`, async () => {
    insertReturningQueue.push([{ id: INVITE_ID, teamId: WS, email: null }])

    const result = await caller().create({ teamId: WS })

    expect(result.emailDelivered).toBeNull()
    expect(result.token).toMatch(/^[0-9a-f]{64}$/)
    expect(sendTeamInviteEmail).not.toHaveBeenCalled()
  })

  it(`never rolls back the invite when email delivery fails`, async () => {
    insertReturningQueue.push([
      { id: INVITE_ID, teamId: WS, email: `new@example.com` },
    ])
    selectQueue.push([])
    selectQueue.push([{ name: `Acme` }])
    sendTeamInviteEmail.mockRejectedValueOnce(new Error(`SES down`))

    const result = await caller().create({
      teamId: WS,
      email: `new@example.com`,
    })

    expect(result.invite).toMatchObject({ id: INVITE_ID })
    expect(result.emailDelivered).toBe(false)
  })

  it(`creates no placeholder for a link invite`, async () => {
    insertReturningQueue.push([{ id: INVITE_ID, teamId: WS, email: null }])
    const result = await caller().create({ teamId: WS })
    expect(createPlaceholderMember).not.toHaveBeenCalled()
    expect(result.memberUserId).toBeNull()
    expect(inserts[0]!.values.placeholderUserId).toBeNull()
  })
})

describe(`teamInvites.create — placeholder members (EXP-630)`, () => {
  it(`invites an existing account the old way: no placeholder, they join by accepting`, async () => {
    insertReturningQueue.push([
      { id: INVITE_ID, teamId: WS, email: `real@example.com` },
    ])
    // The address belongs to a claimed account that is not a member.
    selectQueue.push([{ id: `user-real`, placeholderAt: null }])
    selectQueue.push([])
    selectQueue.push([{ name: `Acme` }])

    const result = await caller().create({ teamId: WS, email: `real@example.com` })

    expect(createPlaceholderMember).not.toHaveBeenCalled()
    expect(assertCanInviteMember).toHaveBeenCalledWith(WS)
    expect(result.memberUserId).toBeNull()
    expect(inserts[0]!.values.placeholderUserId).toBeNull()
  })

  it(`refuses an address that is already a (joined) member`, async () => {
    selectQueue.push([{ id: `user-real`, placeholderAt: null }])
    selectQueue.push([{ id: `member-row` }])

    await expect(
      caller().create({ teamId: WS, email: `real@example.com` })
    ).rejects.toThrow(/already a member/)
    expect(inserts).toHaveLength(0)
  })

  it(`re-links an unclaimed placeholder already on the roster instead of a second one`, async () => {
    insertReturningQueue.push([{ id: INVITE_ID, teamId: WS }])
    selectQueue.push([{ id: `ph-old`, placeholderAt: new Date() }])
    selectQueue.push([{ id: `member-row` }])
    selectQueue.push([{ name: `Acme` }])

    const result = await caller().create({ teamId: WS, email: `old@example.com` })

    expect(createPlaceholderMember).not.toHaveBeenCalled()
    // No seat is taken — the placeholder holds one already.
    expect(assertCanInviteMember).not.toHaveBeenCalled()
    expect(result.memberUserId).toBe(`ph-old`)
    // The superseded links are DELETED (one live row per placeholder — an
    // expired row would sit in older clients' pending lists forever), the
    // fresh row replaces them.
    expect(deletes).toHaveLength(1)
    expect(deletes[0]!.table).toBe(teamInvites)
    expect(flattenSqlChunks(deletes[0]!.where)).toContain(teamInvites.teamId)
    expect(flattenSqlChunks(deletes[0]!.where)).toContain(
      teamInvites.placeholderUserId
    )
    expect(updates).toHaveLength(0)
    expect(inserts[0]!.values.placeholderUserId).toBe(`ph-old`)
  })

  it(`treats another team's unclaimed placeholder like any existing account: never seated here`, async () => {
    insertReturningQueue.push([{ id: INVITE_ID, teamId: WS }])
    selectQueue.push([{ id: `ph-elsewhere`, placeholderAt: new Date() }])
    selectQueue.push([])
    selectQueue.push([{ name: `Acme` }])

    const result = await caller().create({ teamId: WS, email: `x@example.com`, role: `owner` })

    expect(assertCanInviteMember).toHaveBeenCalledWith(WS)
    expect(createPlaceholderMember).not.toHaveBeenCalled()
    // A typed address proves nothing: no membership row, an unbound invite.
    expect(inserts.map((row) => row.table)).not.toContain(teamMembers)
    expect(inserts[0]!.table).toBe(teamInvites)
    expect(inserts[0]!.values.placeholderUserId).toBeNull()
    expect(result.memberUserId).toBeNull()
  })

  it(`re-invites a placeholder at a corrected address and name`, async () => {
    insertReturningQueue.push([{ id: INVITE_ID, teamId: WS }])
    // The target placeholder (joined with its membership), the
    // no-membership-elsewhere check, then the free-address check.
    selectQueue.push([{ id: `ph-1`, email: `wrong@example.com`, placeholderAt: new Date() }])
    selectQueue.push([])
    selectQueue.push([])
    selectQueue.push([{ name: `Acme` }])

    const result = await caller().create({
      teamId: WS,
      placeholderUserId: `ph-1`,
      email: `Right@example.com`,
      name: `Right Name`,
    })

    expect(result.memberUserId).toBe(`ph-1`)
    expect(updates[0]!.table).toBe(users)
    expect(updates[0]!.set).toMatchObject({ email: `right@example.com`, name: `Right Name` })
    expect(deletes).toHaveLength(1)
    expect(deletes[0]!.table).toBe(teamInvites)
    expect(inserts[0]!.values).toMatchObject({
      email: `right@example.com`,
      placeholderUserId: `ph-1`,
    })
    expect(sendTeamInviteEmail).toHaveBeenCalledWith(
      expect.objectContaining({ to: `right@example.com` })
    )
  })

  it(`resends at the SAME address without the membership-elsewhere check`, async () => {
    insertReturningQueue.push([{ id: INVITE_ID, teamId: WS }])
    selectQueue.push([{ id: `ph-1`, email: `same@example.com`, placeholderAt: new Date() }])
    selectQueue.push([{ name: `Acme` }])

    const result = await caller().create({
      teamId: WS,
      placeholderUserId: `ph-1`,
      email: `same@example.com`,
    })

    expect(result.memberUserId).toBe(`ph-1`)
    expect(updates[0]!.set).toMatchObject({ email: `same@example.com` })
    expect(deletes).toHaveLength(1)
  })

  // The cross-team hijack chain: B's owner types A's placeholder address →
  // (fixed above) it is NOT seated in B → the re-address call finds no member
  // → NOT_FOUND. And for legacy rows where the placeholder IS on both
  // rosters, the address change is refused outright.
  it(`refuses to re-address a placeholder that is on another team's roster too`, async () => {
    // Step 1: inviting the foreign placeholder's address binds nothing.
    insertReturningQueue.push([{ id: INVITE_ID, teamId: WS }])
    selectQueue.push([{ id: `ph-a`, placeholderAt: new Date() }])
    selectQueue.push([])
    selectQueue.push([{ name: `Acme` }])
    const invited = await caller().create({ teamId: WS, email: `bob@example.com` })
    expect(invited.memberUserId).toBeNull()

    // Step 2: the re-address is refused — the join finds no member of WS.
    selectQueue.push([])
    await expect(
      caller().create({ teamId: WS, placeholderUserId: `ph-a`, email: `attacker@evil.com` })
    ).rejects.toMatchObject({ code: `NOT_FOUND` })

    // Legacy: a placeholder seated in WS AND elsewhere — the email rewrite
    // would hand the other team's seat to whoever owns the new address.
    selectQueue.push([{ id: `ph-a`, email: `bob@example.com`, placeholderAt: new Date() }])
    selectQueue.push([{ id: `member-of-team-a` }])
    await expect(
      caller().create({ teamId: WS, placeholderUserId: `ph-a`, email: `attacker@evil.com` })
    ).rejects.toMatchObject({ code: `FORBIDDEN` })
    expect(updates.filter((row) => row.table === users)).toHaveLength(0)
    expect(deletes).toHaveLength(0)
  })

  it(`falls back to a plain invite when nobody could ever sign in as the placeholder`, async () => {
    // Password-only self-host: no mail transport, no social/OIDC provider.
    authConfig.passwordResetEnabled = false
    authConfig.emailOtpEnabled = false
    expect(placeholderClaimable()).toBe(false)

    insertReturningQueue.push([{ id: INVITE_ID, teamId: WS, email: `new@example.com` }])
    selectQueue.push([])
    selectQueue.push([{ name: `Acme` }])

    const result = await caller().create({ teamId: WS, email: `new@example.com` })

    expect(assertCanInviteMember).toHaveBeenCalledWith(WS)
    expect(createPlaceholderMember).not.toHaveBeenCalled()
    expect(result.memberUserId).toBeNull()
    expect(inserts[0]!.values.placeholderUserId).toBeNull()

    // Any one claim path is enough.
    authConfig.oidcProviders = [{ id: `corp`, name: `Corp` }]
    expect(placeholderClaimable()).toBe(true)
    authConfig.oidcProviders = []
    authConfig.googleLoginEnabled = true
    expect(placeholderClaimable()).toBe(true)
  })

  it(`refuses to re-invite a member who has joined, or onto a taken address`, async () => {
    selectQueue.push([{ id: `u-1`, email: `a@example.com`, placeholderAt: null }])
    await expect(
      caller().create({ teamId: WS, placeholderUserId: `u-1`, email: `a@example.com` })
    ).rejects.toThrow(/already joined/)

    selectQueue.push([{ id: `ph-1`, email: `a@example.com`, placeholderAt: new Date() }])
    // No membership elsewhere, then the new address is taken.
    selectQueue.push([])
    selectQueue.push([{ id: `someone-else` }])
    await expect(
      caller().create({ teamId: WS, placeholderUserId: `ph-1`, email: `b@example.com` })
    ).rejects.toThrow(/belongs to another account/)
  })
})

describe(`teamInvites.accept — placeholder invites (EXP-630)`, () => {
  const placeholderInvite = {
    id: INVITE_ID,
    teamId: WS,
    role: `member`,
    acceptedAt: null,
    expiresAt: new Date(Date.now() + 60_000),
    placeholderUserId: `user-a`,
  }

  it(`reports the placeholder itself, signed in through its email, as a member`, async () => {
    selectQueue.push([placeholderInvite])
    // The placeholder IS a member already.
    selectQueue.push([{ teamId: WS, userId: `user-a` }])
    selectQueue.push([{ id: WS, name: `Acme` }])

    const result = await caller().accept({ token: `tok` })

    expect(result).toMatchObject({ alreadyMember: true, txId: 42 })
    expect(claimPlaceholder).toHaveBeenCalledWith(fakeDb, `user-a`, expect.any(Date))
    expect(mergePlaceholderIntoUser).not.toHaveBeenCalled()
    // No membership insert — the roster row exists. The invite row is the
    // claim helper's to stamp (it was still pending here → funnel event).
    expect(inserts.map((row) => row.table)).toEqual([conversionEvents])
    expect(updates).toHaveLength(1)
    expectOnboardingStamp(updates[0]!)
    expect(assertCanInviteMember).not.toHaveBeenCalled()
  })

  it(`does not report a used or expired invite to the placeholder it belongs to (the session hook consumed it)`, async () => {
    selectQueue.push([
      {
        ...placeholderInvite,
        acceptedAt: new Date(Date.now() - 60_000),
        expiresAt: new Date(Date.now() - 1_000),
      },
    ])
    selectQueue.push([{ teamId: WS, userId: `user-a` }])
    selectQueue.push([{ id: WS, name: `Acme` }])

    const result = await caller().accept({ token: `tok` })

    expect(result).toMatchObject({ alreadyMember: true, txId: 42 })
    expect(claimPlaceholder).toHaveBeenCalled()
    // Already consumed by the hook → no second invite_accepted event.
    expect(inserts).toHaveLength(0)
  })

  it(`joins afresh when the placeholder's own seat was removed meanwhile`, async () => {
    selectQueue.push([placeholderInvite])
    selectQueue.push([])
    selectQueue.push([{ id: WS, name: `Acme` }])
    updateReturningQueue.push([{ id: INVITE_ID }])

    const result = await caller().accept({ token: `tok` })

    expect(result).toMatchObject({ alreadyMember: false, txId: 42 })
    expect(claimPlaceholder).toHaveBeenCalled()
    expect(assertCanInviteMember).toHaveBeenCalledWith(WS)
    expect(inserts[0]!.table).toBe(teamMembers)
    expect(inserts[0]!.values).toMatchObject({ teamId: WS, userId: `user-a` })
  })

  it(`never merges a colleague's placeholder into an existing member`, async () => {
    selectQueue.push([{ ...placeholderInvite, placeholderUserId: `ph-1` }])
    // The accepter is a member already — the link was forwarded to them.
    selectQueue.push([{ teamId: WS, userId: `user-a` }])
    selectQueue.push([{ id: WS, name: `Acme` }])

    const result = await caller().accept({ token: `tok` })

    expect(result).toMatchObject({ alreadyMember: true })
    expect(mergePlaceholderIntoUser).not.toHaveBeenCalled()
    expect(claimPlaceholder).not.toHaveBeenCalled()
    // The token is not burned: no teamInvites update, no funnel event.
    expect(updates.map((row) => row.table)).toEqual([users])
    expect(inserts).toHaveLength(0)
  })

  it(`falls through to an ordinary join when the row is no longer a placeholder inside the tx`, async () => {
    mergePlaceholderIntoUser.mockResolvedValueOnce({
      merged: false,
      deletedPlaceholder: false,
    })
    selectQueue.push([{ ...placeholderInvite, placeholderUserId: `ph-1` }])
    selectQueue.push([])
    selectQueue.push([{ id: WS, name: `Acme` }])
    selectQueue.push([{ id: `member-row` }])
    updateReturningQueue.push([{ id: INVITE_ID }])

    const result = await caller().accept({ token: `tok` })

    expect(result).toMatchObject({ alreadyMember: false, txId: 42 })
    expect(assertCanInviteMember).toHaveBeenCalledWith(WS)
    expect(inserts[0]!.table).toBe(teamMembers)
    expect(inserts[0]!.values).toMatchObject({ teamId: WS, userId: `user-a`, role: `member` })
  })

  it(`merges the placeholder into a different accepting account, handing over its seat`, async () => {
    selectQueue.push([{ ...placeholderInvite, placeholderUserId: `ph-1`, role: `owner` }])
    selectQueue.push([])
    selectQueue.push([{ id: WS, name: `Acme` }])
    // The placeholder still holds its seat → no seat gate.
    selectQueue.push([{ id: `member-row` }])
    updateReturningQueue.push([{ id: INVITE_ID }])

    const result = await caller().accept({ token: `tok` })

    expect(result).toMatchObject({ alreadyMember: false, txId: 42 })
    expect(claimPlaceholder).not.toHaveBeenCalled()
    expect(mergePlaceholderIntoUser).toHaveBeenCalledWith(fakeDb, {
      placeholderId: `ph-1`,
      userId: `user-a`,
      teamId: WS,
      userEmail: `a@example.com`,
      role: `owner`,
    })
    expect(assertCanInviteMember).not.toHaveBeenCalled()
    expect(inserts.map((row) => row.table)).toEqual([conversionEvents])
  })

  it(`takes a fresh seat only when the placeholder was removed meanwhile`, async () => {
    selectQueue.push([{ ...placeholderInvite, placeholderUserId: `ph-1` }])
    selectQueue.push([])
    selectQueue.push([{ id: WS, name: `Acme` }])
    selectQueue.push([])
    updateReturningQueue.push([{ id: INVITE_ID }])

    await caller().accept({ token: `tok` })

    expect(assertCanInviteMember).toHaveBeenCalledWith(WS)
    expect(mergePlaceholderIntoUser).toHaveBeenCalled()
  })

  it(`still refuses a used placeholder invite to anyone but the placeholder`, async () => {
    selectQueue.push([{ ...placeholderInvite, placeholderUserId: `ph-1` }])
    selectQueue.push([])
    selectQueue.push([{ id: WS, name: `Acme` }])
    updateReturningQueue.push([])

    await expect(caller().accept({ token: `tok` })).rejects.toThrow(/already been used/)
    expect(claimPlaceholder).not.toHaveBeenCalled()
    expect(mergePlaceholderIntoUser).not.toHaveBeenCalled()

    selectQueue.push([
      { ...placeholderInvite, placeholderUserId: `ph-1`, acceptedAt: new Date() },
    ])
    selectQueue.push([])
    selectQueue.push([{ id: WS, name: `Acme` }])
    await expect(caller().accept({ token: `tok` })).rejects.toThrow(/already been used/)
  })
})

describe(`teamInvites.revoke — placeholder invites (EXP-630)`, () => {
  it(`expires a placeholder's invite instead of deleting the marker row`, async () => {
    selectQueue.push([
      { id: INVITE_ID, teamId: WS, placeholderUserId: `ph-1`, acceptedAt: null },
    ])
    await caller().revoke({ id: INVITE_ID })
    expect(updates).toHaveLength(1)
    expect(updates[0]!.table).toBe(teamInvites)
    expect(updates[0]!.set.expiresAt).toBeInstanceOf(Date)
  })
})

// Drizzle SQL objects nest their pieces in `queryChunks`; flatten so the
// assertions can look for the onboardingCompletedAt column + its IS NULL
// predicate inside the recorded where clause.
function flattenSqlChunks(node: unknown): unknown[] {
  const chunks = (node as { queryChunks?: unknown[] }).queryChunks
  if (!Array.isArray(chunks)) return [node]
  return chunks.flatMap(flattenSqlChunks)
}

function sqlText(node: unknown): string {
  return flattenSqlChunks(node)
    .map((chunk) => {
      const value = (chunk as { value?: unknown }).value
      return Array.isArray(value) ? value.join(``) : ``
    })
    .join(` `)
}

function expectOnboardingStamp(update: {
  table: unknown
  set: Record<string, unknown>
  where: unknown
}) {
  expect(update.table).toBe(users)
  expect(update.set.onboardingCompletedAt).toBeInstanceOf(Date)
  // The IS NULL predicate is what makes the stamp first-time-only — an
  // already-onboarded user's timestamp must never be overwritten.
  expect(flattenSqlChunks(update.where)).toContain(
    users.onboardingCompletedAt
  )
  expect(sqlText(update.where)).toContain(`is null`)
}

describe(`teamInvites.accept — onboarding stamp (EXP-188)`, () => {
  const validInvite = {
    id: INVITE_ID,
    teamId: WS,
    role: `member`,
    acceptedAt: null,
    expiresAt: new Date(Date.now() + 60_000),
  }

  it(`stamps onboardingCompletedAt (where null) when joining`, async () => {
    // In-tx: invite by token, existing-member check (none), team row.
    selectQueue.push([validInvite])
    selectQueue.push([])
    selectQueue.push([{ id: WS, name: `Acme` }])
    // Consuming the single-use invite succeeds.
    updateReturningQueue.push([{ id: INVITE_ID }])

    const result = await caller().accept({ token: `tok` })

    expect(result).toMatchObject({ alreadyMember: false, txId: 42 })
    expect(updates).toHaveLength(2)
    expectOnboardingStamp(updates[0]!)
    expect(updates[1]!.table).toBe(teamInvites)
    // Membership + the invite_accepted conversion event (EXP-362).
    expect(inserts).toHaveLength(2)
    expect(inserts[0]!.table).toBe(teamMembers)
    expect(inserts[1]!.table).toBe(conversionEvents)
    expect(inserts[1]!.values).toMatchObject({ name: `invite_accepted` })
    // Recorded AFTER the transaction commits: recordConversionEvent swallows
    // errors, and a swallowed failure inside the tx would poison it — the
    // COMMIT would silently become a ROLLBACK while accept reported success.
    expect(inserts[0]!.inTx).toBe(true)
    expect(inserts[1]!.inTx).toBe(false)
  })

  it(`stamps onboardingCompletedAt on the alreadyMember path too`, async () => {
    selectQueue.push([validInvite])
    // Existing membership — the single-use invite must not be burned.
    selectQueue.push([{ teamId: WS, userId: `user-a` }])
    selectQueue.push([{ id: WS, name: `Acme` }])

    const result = await caller().accept({ token: `tok` })

    expect(result).toMatchObject({ alreadyMember: true })
    expect(updates).toHaveLength(1)
    expectOnboardingStamp(updates[0]!)
    expect(inserts).toHaveLength(0)
  })
})

// REV2-71: the seat gate is a FRESH-JOIN gate. An over-seat team must never
// lock existing members out (a re-clicked invite link, a second device, the
// native join-team flows), and a used/expired invite must surface its own
// error rather than a plan-limit one.
describe(`teamInvites.accept — seat gate ordering (REV2-71)`, () => {
  const validInvite = {
    id: INVITE_ID,
    teamId: WS,
    role: `member`,
    acceptedAt: null,
    expiresAt: new Date(Date.now() + 60_000),
  }

  it(`returns the alreadyMember no-op in a team that is at its seat cap`, async () => {
    assertCanInviteMember.mockRejectedValue(new Error(`up to 3 seats.`))
    selectQueue.push([validInvite])
    selectQueue.push([{ teamId: WS, userId: `user-a` }])
    selectQueue.push([{ id: WS, name: `Acme` }])

    const result = await caller().accept({ token: `tok` })

    expect(result).toMatchObject({ alreadyMember: true })
    expect(assertCanInviteMember).not.toHaveBeenCalled()
  })

  it(`reports a used invite as used, not as a plan limit`, async () => {
    assertCanInviteMember.mockRejectedValue(new Error(`up to 3 seats.`))
    selectQueue.push([{ ...validInvite, acceptedAt: new Date() }])

    await expect(caller().accept({ token: `tok` })).rejects.toThrow(
      /already been used/
    )
    expect(assertCanInviteMember).not.toHaveBeenCalled()
  })

  it(`still blocks a fresh join past the seat cap`, async () => {
    assertCanInviteMember.mockRejectedValue(new Error(`up to 3 seats.`))
    selectQueue.push([validInvite])
    selectQueue.push([])
    selectQueue.push([{ id: WS, name: `Acme` }])
    updateReturningQueue.push([{ id: INVITE_ID }])

    await expect(caller().accept({ token: `tok` })).rejects.toThrow(
      /up to 3 seats/
    )
    expect(assertCanInviteMember).toHaveBeenCalledWith(WS)
    // The membership insert never runs (and the claim update rolls back with
    // the transaction on a real db).
    expect(inserts).toHaveLength(0)
  })
})

// EXP-557: instance admins get NO bypass — invite management is owner-only
// and the router no longer consults users.is_admin at all.
describe(`instance-admin bypass removal (EXP-557)`, () => {
  it(`create refuses a non-owner even when the caller is an instance admin`, async () => {
    const { isUserAdmin } = await import(`@/lib/admin`)
    vi.mocked(isUserAdmin).mockResolvedValue(true)
    const { assertTeamMember } = await import(`@/lib/team-membership`)
    const { TRPCError } = await import(`@trpc/server`)
    vi.mocked(assertTeamMember).mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN`, message: `not an owner` })
    )
    await expect(caller().create({ teamId: WS })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    // The old owner-OR-admin helper consulted this first; nothing does now.
    expect(isUserAdmin).not.toHaveBeenCalled()
    vi.mocked(isUserAdmin).mockResolvedValue(false)
  })
})
