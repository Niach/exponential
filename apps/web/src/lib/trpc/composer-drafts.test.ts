import { beforeEach, describe, expect, it, vi } from "vitest"

// #FEED-219 (composer-draft.json): the composer-drafts router. What it locks:
//   * the write is an UPSERT on (caller, key), the caller's id always from
//     the session, never the input;
//   * a key must parse (`parseComposerDraftKey`), an empty or whitespace-only
//     text is refused (the client deletes instead), and the caller must be
//     able to read the team the row files under;
//   * `delete` is owner-scoped by predicate, no team lookup.

const h = vi.hoisted(() => ({
  resolveTeamAccess: vi.fn(async (..._args: unknown[]) => undefined),
  inserted: [] as Record<string, unknown>[],
  conflicts: [] as Record<string, unknown>[],
  deletes: 0,
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({ resolveTeamAccess: h.resolveTeamAccess }))

import { composerDraftsRouter } from "@/lib/trpc/composer-drafts"

const TEAM = `22222222-2222-4222-8222-222222222222`
const KEY = `comment:33333333-3333-4333-8333-333333333333`

const fakeTx = {
  execute: async () => ({ rows: [{ txid: `42` }] }),
  insert: () => ({
    values: (row: Record<string, unknown>) => {
      h.inserted.push(row)
      return {
        onConflictDoUpdate: (config: Record<string, unknown>) => {
          h.conflicts.push(config)
          return { returning: async () => [{ id: `row`, ...row }] }
        },
      }
    },
  }),
  delete: () => ({
    where: () => ({
      returning: async () => {
        h.deletes += 1
        return [{ id: `row` }]
      },
    }),
  }),
}

const caller = composerDraftsRouter.createCaller({
  session: { user: { id: `actor` } },
  db: { transaction: async (fn: (tx: unknown) => Promise<unknown>) => fn(fakeTx) },
  request: new Request(`http://localhost/api/trpc/composerDrafts.upsert`),
} as never)

describe(`composerDrafts`, () => {
  beforeEach(() => {
    h.resolveTeamAccess.mockClear()
    h.inserted = []
    h.conflicts = []
    h.deletes = 0
  })

  it(`upserts the caller's row for the key after a team read check`, async () => {
    const result = await caller.upsert({ teamId: TEAM, key: KEY, text: `half a thought` })
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(`actor`, TEAM, `read`)
    expect(h.inserted).toEqual([{ userId: `actor`, teamId: TEAM, key: KEY, text: `half a thought` }])
    expect(h.conflicts).toHaveLength(1)
    expect(result.txId).toBe(42)
  })

  it(`refuses an unknown key`, async () => {
    await expect(caller.upsert({ teamId: TEAM, key: `thread:x`, text: `hi` })).rejects.toThrow()
    expect(h.inserted).toEqual([])
  })

  it(`refuses an empty text (a delete)`, async () => {
    await expect(caller.upsert({ teamId: TEAM, key: KEY, text: ` \n ` })).rejects.toThrow(
      /delete/
    )
    expect(h.inserted).toEqual([])
  })

  it(`deletes by predicate without a team lookup`, async () => {
    const result = await caller.delete({ key: KEY })
    expect(result).toEqual({ txId: 42, deleted: true })
    expect(h.deletes).toBe(1)
    expect(h.resolveTeamAccess).not.toHaveBeenCalled()
  })
})
