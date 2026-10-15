import { describe, expect, it, vi } from "vitest"
import {
  addPlaceholderToRoster,
  claimPlaceholder,
  collectPlaceholderMemberIds,
  createPlaceholderMember,
  createUnsentPlaceholderInvite,
  deletePlaceholdersIfOrphaned,
  mergePlaceholderIntoUser,
  placeholderNameFromEmail,
  providerProfileFromClaims,
  resolvePlaceholderIdentity,
} from "@/lib/placeholder-members"
import { teamInvites, teamMembers, users } from "@/db/schema"

// A minimal transaction fake for the effectful helpers: `select()` shifts
// rows off a FIFO queue, `insert`/`update` record their writes, `update(...)
// .returning()` serves its own queue.
function fakeTx(selects: unknown[][] = [], updateReturning: unknown[][] = []) {
  const inserts: { table: unknown; values: Record<string, unknown> }[] = []
  const updates: {
    table: unknown
    set: Record<string, unknown>
    where: unknown
  }[] = []
  const chain = () => {
    const p = Promise.resolve(selects.shift() ?? []) as Promise<unknown[]> &
      Record<string, () => unknown>
    for (const m of [`from`, `innerJoin`, `where`, `limit`]) p[m] = () => p
    return p
  }
  const tx = {
    select: () => chain(),
    insert: (table: unknown) => ({
      values: (values: Record<string, unknown>) => {
        inserts.push({ table, values })
        return Promise.resolve()
      },
    }),
    update: (table: unknown) => ({
      set: (set: Record<string, unknown>) => ({
        where: (where: unknown) => {
          updates.push({ table, set, where })
          const p = Promise.resolve() as Promise<void> & {
            returning: () => Promise<unknown[]>
          }
          p.returning = () => Promise.resolve(updateReturning.shift() ?? [])
          return p
        },
      }),
    }),
    delete: vi.fn(),
    execute: vi.fn(async () => ({ rows: [] })),
  }
  return { tx: tx as never, inserts, updates, execute: tx.execute }
}

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

describe(`createPlaceholderMember`, () => {
  it(`creates the row verified so a Google/Apple/OIDC login can link onto it`, async () => {
    // Better Auth's implicit linking refuses an unverified local row
    // (requireLocalEmailVerified defaults on) → account_not_linked.
    const { tx, inserts } = fakeTx()
    await createPlaceholderMember(tx, {
      teamId: `team-1`,
      role: `member`,
      identity: { email: `bob@example.com`, name: `Bob` },
    })
    expect(inserts[0]!.table).toBe(users)
    expect(inserts[0]!.values).toMatchObject({
      email: `bob@example.com`,
      emailVerified: true,
    })
    expect(inserts[0]!.values.placeholderAt).toBeInstanceOf(Date)
  })
})

describe(`createUnsentPlaceholderInvite`, () => {
  it(`writes a link nobody was sent: sent_at null, already expired`, async () => {
    // EXP-1076: the import seats people it never invited. The row exists so
    // Members can badge "Not invited" and offer "Send invite"; every
    // expiry-based reader must already read it as dead.
    const now = new Date(`2026-01-01T00:00:00.000Z`)
    const { tx, inserts } = fakeTx()
    const { inviteId } = await createUnsentPlaceholderInvite(tx, {
      teamId: `team-1`,
      placeholderUserId: `ph-1`,
      invitedById: `owner-1`,
      email: `  Bob@Example.com `,
      role: `member`,
      now,
    })
    expect(inserts).toHaveLength(1)
    expect(inserts[0]!.table).toBe(teamInvites)
    expect(inserts[0]!.values).toMatchObject({
      id: inviteId,
      teamId: `team-1`,
      invitedById: `owner-1`,
      role: `member`,
      email: `bob@example.com`,
      placeholderUserId: `ph-1`,
      sentAt: null,
    })
    expect(inserts[0]!.values.expiresAt).toBe(now)
    // Still a real random token — the binding is a row, not a credential.
    expect(inserts[0]!.values.token).toMatch(/^[0-9a-f]{64}$/)
  })
})

describe(`claimPlaceholder`, () => {
  it(`clears the flag, stamps onboarding (where null) and accepts the pending invites`, async () => {
    const { tx, updates } = fakeTx([], [[{ id: `ph-1` }]])
    await claimPlaceholder(tx, `ph-1`, new Date())
    expect(updates).toHaveLength(2)
    expect(updates[0]!.table).toBe(users)
    expect(updates[0]!.set.placeholderAt).toBeNull()
    // The claimed row sits on a roster already: the create-or-join wizard
    // must never show. coalesce keeps an existing stamp.
    expect(flattenSqlChunks(updates[0]!.set.onboardingCompletedAt)).toContain(
      users.onboardingCompletedAt
    )
    expect(sqlText(updates[0]!.set.onboardingCompletedAt)).toContain(`coalesce`)
    expect(updates[1]!.table).toBe(teamInvites)
    expect(updates[1]!.set.acceptedAt).toBeInstanceOf(Date)
    // EXP-1141: only the invites of teams that SEAT the row — one bound to a
    // seatless placeholder is a join still to happen on the invite page, so
    // the mailbox sign-in must not consume it.
    expect(flattenSqlChunks(updates[1]!.where)).toContain(
      teamInvites.placeholderUserId
    )
    expect(sqlText(updates[1]!.where)).toContain(`exists`)
    expect(flattenSqlChunks(updates[1]!.where)).toContain(teamMembers.userId)
  })

  it(`is a no-op for a real account`, async () => {
    const { tx, updates } = fakeTx([], [[]])
    await claimPlaceholder(tx, `real-user`)
    expect(updates).toHaveLength(1)
    expect(updates[0]!.table).toBe(users)
  })
})

describe(`mergePlaceholderIntoUser`, () => {
  it(`refuses to merge a row that is not (or no longer) an unclaimed placeholder`, async () => {
    const { tx, updates, execute } = fakeTx([
      [{ email: `bob@example.com`, placeholderAt: null }],
    ])
    await expect(
      mergePlaceholderIntoUser(tx, {
        placeholderId: `u-1`,
        userId: `u-2`,
        teamId: `team-1`,
        userEmail: `other@example.com`,
        role: `member`,
      })
    ).resolves.toEqual({ merged: false, deletedPlaceholder: false })
    // No attribution rewrite, not even the preserve-timestamps guard.
    expect(updates).toHaveLength(0)
    expect(execute).not.toHaveBeenCalled()
  })

  it(`is a no-op when the placeholder IS the accepter`, async () => {
    const { tx, updates } = fakeTx()
    await expect(
      mergePlaceholderIntoUser(tx, {
        placeholderId: `u-1`,
        userId: `u-1`,
        teamId: `team-1`,
        userEmail: `bob@example.com`,
        role: `member`,
      })
    ).resolves.toEqual({ merged: false, deletedPlaceholder: false })
    expect(updates).toHaveLength(0)
  })
})

// EXP-630 placeholder members — the pure decisions: what an invite names the
// row it creates, and which provider profile replaces that name on claim.

describe(`resolvePlaceholderIdentity`, () => {
  it(`keeps the typed name and normalizes the address`, () => {
    expect(
      resolvePlaceholderIdentity({ email: `  Hannes.Robier@YouSpi.com `, name: ` Hannes Robier ` })
    ).toEqual({ email: `hannes.robier@youspi.com`, name: `Hannes Robier` })
  })

  it(`falls back to the mailbox local part like the sign-in-code default`, () => {
    expect(resolvePlaceholderIdentity({ email: `dennis@straehhuber.com` })).toEqual({
      email: `dennis@straehhuber.com`,
      name: `dennis`,
    })
    expect(resolvePlaceholderIdentity({ email: `dennis@straehhuber.com`, name: `   ` }).name).toBe(
      `dennis`
    )
    expect(placeholderNameFromEmail(`@nolocal`)).toBe(`@nolocal`)
  })
})

describe(`providerProfileFromClaims`, () => {
  it(`reads Google's name and picture`, () => {
    expect(
      providerProfileFromClaims({
        name: `Dennis Strähhuber`,
        picture: `https://lh3.googleusercontent.com/a/x`,
        email: `dennis@example.com`,
      })
    ).toEqual({ name: `Dennis Strähhuber`, image: `https://lh3.googleusercontent.com/a/x` })
  })

  it(`assembles an OIDC given + family name and ignores a non-https picture`, () => {
    expect(
      providerProfileFromClaims({ given_name: `Ada`, family_name: `Lovelace`, picture: `data:x` })
    ).toEqual({ name: `Ada Lovelace` })
  })

  it(`returns nothing for Apple's name-less token or junk`, () => {
    expect(providerProfileFromClaims({ email: `x@privaterelay.appleid.com`, sub: `1` })).toEqual({})
    expect(providerProfileFromClaims(null)).toEqual({})
    expect(providerProfileFromClaims(`str`)).toEqual({})
    expect(providerProfileFromClaims({ name: `   ` })).toEqual({})
  })
})

describe(`team-delete placeholder purge (EXP-1132)`, () => {
  it(`reads nothing for no teams`, async () => {
    const { tx } = fakeTx([[{ id: `ph-1` }]])
    expect(await collectPlaceholderMemberIds(tx, [])).toEqual([])
  })

  it(`returns each placeholder member once, across the teams`, async () => {
    const { tx } = fakeTx([[{ id: `ph-1` }, { id: `ph-2` }, { id: `ph-1` }]])
    expect(await collectPlaceholderMemberIds(tx, [`team-1`, `team-2`])).toEqual([
      `ph-1`,
      `ph-2`,
    ])
  })

  it(`drops only the rows the orphan check clears`, async () => {
    const { tx, execute } = fakeTx()
    execute
      .mockResolvedValueOnce({ rows: [{ orphaned: true }] } as never)
      .mockResolvedValueOnce({ rows: [{ orphaned: false }] } as never)
    const deleted: unknown[] = []
    ;(tx as { delete: unknown }).delete = (table: unknown) => ({
      where: () => {
        deleted.push(table)
        return Promise.resolve()
      },
    })

    await deletePlaceholdersIfOrphaned(tx, [`ph-gone`, `ph-referenced`])

    expect(execute).toHaveBeenCalledTimes(2)
    expect(deleted).toEqual([users])
  })

  it(`keeps a placeholder a provider login already linked an accounts row to`, async () => {
    // Someone signed in through the placeholder's mailbox but the claim hook
    // never flipped placeholder_at: the row IS a login now, so the orphan
    // predicate must refuse it alongside the attribution guards.
    const { tx, execute } = fakeTx()
    execute.mockResolvedValueOnce({ rows: [{ orphaned: false }] } as never)
    ;(tx as { delete: unknown }).delete = vi.fn()

    await deletePlaceholdersIfOrphaned(tx, [`ph-linked`])

    const calls = execute.mock.calls as unknown[][]
    const predicate = sqlText(calls[0]?.[0]).replace(/\s+/g, ` `)
    expect(predicate).toContain(`u.placeholder_at IS NOT NULL`)
    expect(predicate).toContain(
      `NOT EXISTS (SELECT 1 FROM accounts acc WHERE acc.user_id = u.id)`
    )
    for (const guard of [
      `team_members tm WHERE tm.user_id = u.id`,
      `issues i WHERE i.assignee_id = u.id OR i.creator_id = u.id`,
      `comments c WHERE c.author_id = u.id`,
      `issue_events e WHERE e.actor_user_id = u.id`,
      `attachments a WHERE a.uploader_id = u.id`,
    ]) {
      expect(predicate).toContain(`NOT EXISTS (SELECT 1 FROM ${guard})`)
    }
    expect((tx as { delete: ReturnType<typeof vi.fn> }).delete).not.toHaveBeenCalled()
  })
})

// FEED-108: THE roster-without-an-invitation resolver, shared by the Linear
// import's placeholder port and `teamMembers.addPlaceholder`. The fake tx's
// `execute` serves, in order: the per-address advisory lock, then the
// orphan predicate (`deletePlaceholderIfOrphaned`), then the outside-reference
// predicate (`placeholderReferencedOutside`).
describe(`addPlaceholderToRoster (FEED-108)`, () => {
  const NOW = new Date(`2026-10-15T12:00:00.000Z`)
  const seated = (id: string) => ({ id, placeholderAt: NOW })
  const account = (id: string) => ({ id, placeholderAt: null })

  function roster(selects: unknown[][], executes: unknown[] = []) {
    const made = fakeTx(selects)
    const queue = [...executes]
    made.execute.mockImplementation(async () => (queue.shift() ?? { rows: [] }) as never)
    const del = vi.fn(() => ({ where: async () => undefined }))
    ;(made.tx as { delete: unknown }).delete = del
    return { ...made, del }
  }

  const add = (tx: never, email = `Hannes@Example.com`, name?: string) =>
    addPlaceholderToRoster(tx, {
      teamId: `team-1`,
      email,
      name,
      invitedById: `owner-1`,
      now: NOW,
    })

  it(`takes the per-address lock first, then seats an unknown address as a fresh placeholder`, async () => {
    const { tx, inserts, execute } = roster([[]])
    const result = await add(tx, `Hannes@Example.com`, `Hannes`)
    expect(result.kind).toBe(`created`)
    const calls = execute.mock.calls as unknown[][]
    expect(sqlText(calls[0]?.[0])).toContain(`pg_advisory_xact_lock(hashtext(`)
    expect(JSON.stringify(calls[0]?.[0])).toContain(`placeholder_address:hannes@example.com`)
    // users + team_members (member role, never owner) + the unsent invite.
    expect(inserts.map((row) => row.table)).toEqual([users, teamMembers, teamInvites])
    expect(inserts[0]!.values).toMatchObject({
      name: `Hannes`,
      email: `hannes@example.com`,
      placeholderAt: NOW,
    })
    expect(inserts[1]!.values).toMatchObject({ teamId: `team-1`, role: `member` })
    expect(inserts[2]!.values).toMatchObject({
      teamId: `team-1`,
      invitedById: `owner-1`,
      email: `hannes@example.com`,
      sentAt: null,
      expiresAt: NOW,
    })
    if (result.kind === `created`) {
      expect(inserts[1]!.values.userId).toBe(result.userId)
      expect(inserts[2]!.values.placeholderUserId).toBe(result.userId)
    }
  })

  it(`an empty name falls back to the mailbox local part`, async () => {
    const { tx, inserts } = roster([[]])
    await add(tx, `hannes@example.com`)
    expect(inserts[0]!.values.name).toBe(`hannes`)
  })

  it(`an address already on this roster is a member, real account or placeholder alike`, async () => {
    for (const owner of [account(`u-real`), seated(`u-ph`)]) {
      const { tx, inserts, del } = roster([[owner], [{ id: `m-1` }]])
      expect(await add(tx)).toEqual({ kind: `member`, userId: owner.id })
      expect(inserts).toHaveLength(0)
      expect(del).not.toHaveBeenCalled()
    }
  })

  it(`a real account that is not a member here is outside: the person joins by invite`, async () => {
    const { tx, inserts } = roster([[account(`u-real`)], []])
    expect(await add(tx)).toEqual({ kind: `outside` })
    expect(inserts).toHaveLength(0)
  })

  it(`a placeholder seated on another team is outside too (a typed address proves nothing)`, async () => {
    // owner → not a member here → seated somewhere.
    const { tx, inserts, execute } = roster([[seated(`u-ph`)], [], [{ id: `m-other` }]])
    expect(await add(tx)).toEqual({ kind: `outside` })
    expect(inserts).toHaveLength(0)
    // Only the lock ran — no orphan probe, no outside probe.
    expect(execute).toHaveBeenCalledTimes(1)
  })

  it(`EXP-1132: a seatless ghost nothing references is dropped and re-created`, async () => {
    const { tx, inserts, del } = roster(
      [[seated(`u-ghost`)], [], []],
      [{ rows: [] }, { rows: [{ orphaned: true }] }]
    )
    const result = await add(tx)
    expect(result.kind).toBe(`created`)
    expect(del).toHaveBeenCalledTimes(1)
    expect(inserts.map((row) => row.table)).toEqual([users, teamMembers, teamInvites])
    if (result.kind === `created`) expect(result.userId).not.toBe(`u-ghost`)
  })

  it(`EXP-1141: a seatless placeholder referenced only by this team is re-seated with its id`, async () => {
    // orphaned: false (it has comments here) → referenced_outside: false →
    // no invite bound here yet.
    const { tx, inserts, del } = roster(
      [[seated(`u-removed`)], [], [], []],
      [{ rows: [] }, { rows: [{ orphaned: false }] }, { rows: [{ referenced_outside: false }] }]
    )
    expect(await add(tx)).toEqual({ kind: `rebound`, userId: `u-removed` })
    expect(del).not.toHaveBeenCalled()
    expect(inserts.map((row) => row.table)).toEqual([teamMembers, teamInvites])
    expect(inserts[0]!.values).toMatchObject({ teamId: `team-1`, userId: `u-removed`, role: `member` })
    expect(inserts[1]!.values).toMatchObject({ placeholderUserId: `u-removed`, sentAt: null })
  })

  it(`re-seating keeps a link the owner already sent (no second invite row)`, async () => {
    const { tx, inserts } = roster(
      [[seated(`u-removed`)], [], [], [{ id: `inv-live` }]],
      [{ rows: [] }, { rows: [{ orphaned: false }] }, { rows: [{ referenced_outside: false }] }]
    )
    expect(await add(tx)).toEqual({ kind: `rebound`, userId: `u-removed` })
    expect(inserts.map((row) => row.table)).toEqual([teamMembers])
  })

  it(`a seatless placeholder another team references or invited is outside, nothing written`, async () => {
    const { tx, inserts, execute } = roster(
      [[seated(`u-theirs`)], [], []],
      [{ rows: [] }, { rows: [{ orphaned: false }] }, { rows: [{ referenced_outside: true }] }]
    )
    expect(await add(tx)).toEqual({ kind: `outside` })
    expect(inserts).toHaveLength(0)
    const calls = execute.mock.calls as unknown[][]
    const predicate = sqlText(calls[2]?.[0]).replace(/\s+/g, ` `)
    // A linked login, another team's attributions, another team's pending invite.
    expect(predicate).toContain(`FROM accounts acc WHERE acc.user_id =`)
    expect(predicate).toContain(`i.team_id IS DISTINCT FROM`)
    expect(predicate).toContain(`c.team_id IS DISTINCT FROM`)
    expect(predicate).toContain(`e.team_id IS DISTINCT FROM`)
    expect(predicate).toContain(`a.team_id IS DISTINCT FROM`)
    expect(predicate).toContain(`ti.placeholder_user_id =`)
    expect(predicate).toContain(`ti.accepted_at IS NULL`)
  })
})
