import { beforeEach, describe, expect, it, vi } from "vitest"

// Two contracts live here:
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

const inserts: { table: unknown; values: Record<string, unknown> }[] = []
const insertReturningQueue: unknown[][] = []
const updates: {
  table: unknown
  set: Record<string, unknown>
  where: unknown
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
    set: (set: Record<string, unknown>) => {
      where: (
        where: unknown
      ) => Promise<void> & { returning: () => Promise<unknown[]> }
    }
  }
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
  transaction: async (fn) => fn(fakeDb),
}

// `@/lib/trpc` (imported by the router) pulls in the real connection module;
// keep Postgres out of the test.
vi.mock(`@/db/connection`, () => ({ db: {} }))

vi.mock(`@/lib/admin`, () => ({
  isUserAdmin: vi.fn(async () => false),
}))

vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: vi.fn(async () => ({ role: `owner` })),
}))

vi.mock(`@/lib/billing`, () => ({
  assertCanInviteMember: vi.fn(async () => {}),
}))

const sendTeamInviteEmail = vi.fn(async () => ({ delivered: true }))
vi.mock(`@/lib/email`, () => ({
  sendTeamInviteEmail: (...args: unknown[]) =>
    sendTeamInviteEmail(...(args as [])),
}))

vi.mock(`@/lib/notification-email-policy`, () => ({
  appBaseUrl: () => `http://localhost:3000`,
}))

import { inviteListSelection, teamInvitesRouter } from "@/lib/trpc/team-invites"
import { teamInvites, teamMembers, users } from "@/db/schema"

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
  selectQueue.length = 0
  inserts.length = 0
  insertReturningQueue.length = 0
  updates.length = 0
  updateReturningQueue.length = 0
  fakeDb.execute.mockClear()
  sendTeamInviteEmail.mockClear()
  sendTeamInviteEmail.mockResolvedValue({ delivered: true })
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
    // Team-name lookup for the email subject/body.
    selectQueue.push([{ name: `Acme` }])

    const result = await caller().create({
      teamId: WS,
      email: `new@example.com`,
    })

    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(teamInvites)
    expect(inserts[0]!.values.email).toBe(`new@example.com`)
    expect(sendTeamInviteEmail).toHaveBeenCalledWith({
      to: `new@example.com`,
      teamName: `Acme`,
      inviterName: `User A`,
      inviteUrl: `http://localhost:3000/invite/${result.token}`,
    })
    expect(result.emailDelivered).toBe(true)
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
    selectQueue.push([{ name: `Acme` }])
    sendTeamInviteEmail.mockRejectedValueOnce(new Error(`SES down`))

    const result = await caller().create({
      teamId: WS,
      email: `new@example.com`,
    })

    expect(result.invite).toMatchObject({ id: INVITE_ID })
    expect(result.emailDelivered).toBe(false)
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
    // Pre-tx seat-gate teamId probe.
    selectQueue.push([{ teamId: WS }])
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
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(teamMembers)
  })

  it(`stamps onboardingCompletedAt on the alreadyMember path too`, async () => {
    selectQueue.push([{ teamId: WS }])
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
