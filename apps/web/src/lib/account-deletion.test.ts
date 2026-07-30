import { beforeEach, describe, expect, it, vi } from "vitest"

// guardAndCleanupTeamsForUserDeletion runs inside the caller's transaction, so
// a structural fake `tx` is enough: `select()` shifts pre-seeded row batches
// off a FIFO queue, `delete()`/`update()` record what they were handed.
const mocks = vi.hoisted(() => ({
  teamSubscriptions: [] as Array<{
    id: string
    creemSubscriptionId: string | null
  }>,
}))

vi.mock(`@/lib/billing/creem-subscriptions`, () => ({
  findActiveSubscriptionsForTeams: async () => mocks.teamSubscriptions,
}))

import {
  attachments,
  comments,
  emailBounces,
  githubInstallationRepoGrants,
  issues,
  teams,
  teamInvites,
  verifications,
} from "@/db/schema"
import {
  classifyTeamsForUserDeletion,
  guardAndCleanupTeamsForUserDeletion,
  type MembershipRow,
} from "./account-deletion"

const selectQueue: unknown[][] = []
const deletes: unknown[] = []
const updates: { table: unknown; values: Record<string, unknown> }[] = []

// LAZY on purpose: the helper builds a `myTeamIds` sub-select it never
// awaits, so a chain that shifted its rows at call time would eat a batch.
function selectChain(): Record<string, unknown> {
  const chain: Record<string, unknown> = {}
  for (const m of [`from`, `where`, `limit`, `orderBy`]) {
    chain[m] = () => chain
  }
  chain.then = (
    resolve: (rows: unknown[]) => unknown,
    reject: (err: unknown) => unknown
  ) => Promise.resolve(selectQueue.shift() ?? []).then(resolve, reject)
  return chain
}

const returningQueue: unknown[][] = []

const fakeTx = {
  select: () => selectChain(),
  delete: (table: unknown) => ({
    where: () => {
      deletes.push(table)
      const p = Promise.resolve() as Promise<void> & {
        returning: () => Promise<unknown[]>
      }
      p.returning = () => Promise.resolve(returningQueue.shift() ?? [])
      return p
    },
  }),
  update: (table: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: () => {
        updates.push({ table, values })
        return Promise.resolve()
      },
    }),
  }),
} as never

// The orphan guard behind users.deleteAccount AND admin.deleteUser: deleting
// a user must never silently strand a multi-member team without an
// owner, and solo teams are deleted along with the account.

const USER = `user-1`

function m(
  teamId: string,
  userId: string,
  role: `owner` | `member`
): MembershipRow {
  return { teamId, userId, role }
}

describe(`classifyTeamsForUserDeletion`, () => {
  it(`flags a team as stranded when the user is the sole owner with other members`, () => {
    const rows = [m(`ws-a`, USER, `owner`), m(`ws-a`, `user-2`, `member`)]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [`ws-a`],
      solo: [],
    })
  })

  it(`flags a team as solo when the user is the entire membership`, () => {
    const rows = [m(`ws-personal`, USER, `owner`)]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [`ws-personal`],
    })
  })

  it(`treats a solo membership as solo even when the role is member (defensive)`, () => {
    const rows = [m(`ws-x`, USER, `member`)]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [`ws-x`],
    })
  })

  it(`allows deletion when another owner exists`, () => {
    const rows = [
      m(`ws-a`, USER, `owner`),
      m(`ws-a`, `user-2`, `owner`),
      m(`ws-a`, `user-3`, `member`),
    ]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

  it(`allows deletion when the user is a plain member of someone else's team`, () => {
    // Even when that team has a sole owner — the OWNER is not the one
    // being deleted, so nothing is stranded.
    const rows = [m(`ws-a`, `user-2`, `owner`), m(`ws-a`, USER, `member`)]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

  it(`ignores teams the user is not a member of`, () => {
    const rows = [m(`ws-other`, `user-2`, `owner`)]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

  it(`classifies a mixed multi-team membership correctly`, () => {
    const rows = [
      // Personal team → solo.
      m(`ws-personal`, USER, `owner`),
      // Sole owner of a team with members → stranded.
      m(`ws-team`, USER, `owner`),
      m(`ws-team`, `user-2`, `member`),
      // Co-owned team → fine.
      m(`ws-coowned`, USER, `owner`),
      m(`ws-coowned`, `user-3`, `owner`),
      // Plain membership elsewhere → fine.
      m(`ws-guest`, `user-4`, `owner`),
      m(`ws-guest`, USER, `member`),
    ]
    expect(classifyTeamsForUserDeletion(rows, USER)).toEqual({
      stranded: [`ws-team`],
      solo: [`ws-personal`],
    })
  })

  it(`returns nothing for an empty membership list`, () => {
    expect(classifyTeamsForUserDeletion([], USER)).toEqual({
      stranded: [],
      solo: [],
    })
  })

})

// ── guardAndCleanupTeamsForUserDeletion ─────────────────────────────────────
// The select queue must be seeded in the order the helper queries:
//   1. memberships          2. team names (ONLY when stranded)
//   3. the user's email     4. attachment keys (ONLY when a solo team dies)
//   5. issues with the address   6. comments with the address
const SOLO = `ws-solo`
const SHARED = `ws-shared`
const EMAIL = `ada@example.com`

function seedBaseline({
  memberships,
  email = EMAIL,
}: {
  memberships: MembershipRow[]
  email?: string | null
}) {
  selectQueue.push(memberships)
  selectQueue.push(email === null ? [] : [{ email }])
}

beforeEach(() => {
  selectQueue.length = 0
  deletes.length = 0
  updates.length = 0
  returningQueue.length = 0
  mocks.teamSubscriptions = []
})

describe(`guardAndCleanupTeamsForUserDeletion — billing (REV2-55)`, () => {
  it(`only ever hands back subscriptions bound to the solo teams it deletes`, async () => {
    // The user co-owns SHARED (survives, and its plan survives with it) and
    // owns SOLO alone (dies, so its plan must be cancelled).
    mocks.teamSubscriptions = [{ id: `row-solo`, creemSubscriptionId: `sub_1` }]
    seedBaseline({
      memberships: [
        m(SOLO, USER, `owner`),
        m(SHARED, USER, `owner`),
        m(SHARED, `user-2`, `owner`),
      ],
    })
    selectQueue.push([{ storageKey: `attachments/a.png` }])
    selectQueue.push([])
    selectQueue.push([])
    returningQueue.push([{ id: SOLO }])

    const result = await guardAndCleanupTeamsForUserDeletion(
      fakeTx,
      USER,
      `self`
    )

    expect(result.deletedTeamIds).toEqual([SOLO])
    expect(result.doomedTeamSubscriptions).toEqual([
      { id: `row-solo`, creemSubscriptionId: `sub_1` },
    ])
    expect(result.storageKeys).toEqual([`attachments/a.png`])
  })

  it(`cancels nothing when the user has no solo team to destroy`, async () => {
    // The buyer-scoped capture is gone: a subscription this user purchased for
    // SHARED keeps running, because it belongs to SHARED.
    mocks.teamSubscriptions = [{ id: `row-x`, creemSubscriptionId: `sub_x` }]
    seedBaseline({
      memberships: [m(SHARED, USER, `owner`), m(SHARED, `user-2`, `owner`)],
    })
    selectQueue.push([])
    selectQueue.push([])

    const result = await guardAndCleanupTeamsForUserDeletion(
      fakeTx,
      USER,
      `self`
    )

    expect(result.deletedTeamIds).toEqual([])
    expect(result.doomedTeamSubscriptions).toEqual([])
    expect(deletes).not.toContain(teams)
  })

  it(`fails closed on a stranded team without deleting anything`, async () => {
    selectQueue.push([m(SHARED, USER, `owner`), m(SHARED, `user-2`, `member`)])
    selectQueue.push([{ name: `Shared` }])

    await expect(
      guardAndCleanupTeamsForUserDeletion(fakeTx, USER, `self`)
    ).rejects.toThrow(/only owner/)
    expect(deletes).toHaveLength(0)
    expect(updates).toHaveLength(0)
  })
})

describe(`guardAndCleanupTeamsForUserDeletion — storage (REV2-36)`, () => {
  it(`reclaims only the dying teams' blobs, never the ones the user uploaded elsewhere`, async () => {
    seedBaseline({ memberships: [m(SOLO, USER, `owner`)] })
    // The single attachments query is team-scoped; an uploader-scoped arm
    // would have needed its own batch here (uploader_id is `set null` now, so
    // those rows and blobs survive with the surviving issue that embeds them).
    selectQueue.push([
      { storageKey: `a.png` },
      { storageKey: `a.png` },
      { storageKey: `b.png` },
    ])
    selectQueue.push([])
    selectQueue.push([])
    returningQueue.push([{ id: SOLO }])

    const result = await guardAndCleanupTeamsForUserDeletion(
      fakeTx,
      USER,
      `self`
    )

    expect(result.storageKeys).toEqual([`a.png`, `b.png`])
  })

  it(`collects no storage keys at all when no team is deleted`, async () => {
    seedBaseline({
      memberships: [m(SHARED, USER, `owner`), m(SHARED, `user-2`, `owner`)],
    })
    selectQueue.push([])
    selectQueue.push([])

    const result = await guardAndCleanupTeamsForUserDeletion(
      fakeTx,
      USER,
      `admin`
    )

    expect(result.storageKeys).toEqual([])
  })
})

describe(`guardAndCleanupTeamsForUserDeletion — mentions (REV2-37)`, () => {
  it(`rewrites the departing address to @former-member in issues and comments`, async () => {
    seedBaseline({
      memberships: [m(SHARED, USER, `owner`), m(SHARED, `user-2`, `owner`)],
    })
    selectQueue.push([
      { id: `issue-1`, description: `cc @${EMAIL} please review` },
    ])
    selectQueue.push([{ id: `comment-1`, body: `thanks @Ada@Example.com!` }])

    await guardAndCleanupTeamsForUserDeletion(fakeTx, USER, `self`)

    expect(updates).toHaveLength(2)
    expect(updates[0]!.table).toBe(issues)
    expect(updates[0]!.values.description).toBe(
      `cc @former-member please review`
    )
    // Tokens are matched case-insensitively — the same person.
    expect(updates[1]!.table).toBe(comments)
    expect(updates[1]!.values.body).toBe(`thanks @former-member!`)
  })

  it(`leaves other people's mentions (and plain emails) untouched`, async () => {
    seedBaseline({
      memberships: [m(SHARED, USER, `owner`), m(SHARED, `user-2`, `owner`)],
    })
    // An ILIKE prefilter can match a row whose only occurrence is a bare
    // address (no `@` prefix) or a longer address that merely ends with ours.
    selectQueue.push([
      { id: `issue-1`, description: `mail ${EMAIL} directly` },
      { id: `issue-2`, description: `ping @other@example.com` },
      { id: `issue-3`, description: null },
    ])
    selectQueue.push([])

    await guardAndCleanupTeamsForUserDeletion(fakeTx, USER, `self`)

    expect(updates).toHaveLength(0)
  })

  it(`skips the scrub entirely when the users row is already gone`, async () => {
    seedBaseline({
      memberships: [m(SHARED, USER, `owner`), m(SHARED, `user-2`, `owner`)],
      email: null,
    })

    await guardAndCleanupTeamsForUserDeletion(fakeTx, USER, `admin`)

    expect(updates).toHaveLength(0)
    expect(deletes).toEqual([githubInstallationRepoGrants])
  })
})

describe(`guardAndCleanupTeamsForUserDeletion — email residue (REV2-75)`, () => {
  it(`deletes bounce, verification and invite rows carrying the address`, async () => {
    seedBaseline({
      memberships: [m(SHARED, USER, `owner`), m(SHARED, `user-2`, `owner`)],
    })
    selectQueue.push([])
    selectQueue.push([])

    await guardAndCleanupTeamsForUserDeletion(fakeTx, USER, `self`)

    expect(deletes).toEqual([
      githubInstallationRepoGrants,
      emailBounces,
      verifications,
      teamInvites,
    ])
  })

  it(`runs the residue cleanup on the admin path too`, async () => {
    seedBaseline({ memberships: [m(SOLO, USER, `owner`)] })
    selectQueue.push([{ storageKey: `a.png` }])
    selectQueue.push([])
    selectQueue.push([])
    returningQueue.push([{ id: SOLO }])

    await guardAndCleanupTeamsForUserDeletion(fakeTx, USER, `admin`)

    expect(deletes).toEqual([
      githubInstallationRepoGrants,
      teams,
      emailBounces,
      verifications,
      teamInvites,
    ])
    expect(deletes).not.toContain(attachments)
  })
})
