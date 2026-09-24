import { describe, expect, it, vi } from "vitest"
import {
  claimPlaceholder,
  createPlaceholderMember,
  mergePlaceholderIntoUser,
  placeholderNameFromEmail,
  providerProfileFromClaims,
  resolvePlaceholderIdentity,
} from "@/lib/placeholder-members"
import { teamInvites, users } from "@/db/schema"

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
    for (const m of [`from`, `where`, `limit`]) p[m] = () => p
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
