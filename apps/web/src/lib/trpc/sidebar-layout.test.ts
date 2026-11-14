import { beforeEach, describe, expect, it, vi } from "vitest"

// #EXP-1832 (sidebar-folders.json): the sidebarLayout router. What it locks:
//   * the caller's id always comes from the session, never the input, and
//     the caller must be able to read the team;
//   * the writes keep the layout's shape: an item's parent is a folder of
//     the same section (existing or written in the same call), a folder id
//     never takes over another user's row, only folders are deleted;
//   * a folder name is `sidebarFolderName` (trimmed, empty = New folder).

const TEAM = `22222222-2222-4222-8222-222222222222`
const FOLDER = `33333333-3333-4333-8333-333333333333`
const OTHER = `44444444-4444-4444-8444-444444444444`
const BOARD = `55555555-5555-4555-8555-555555555555`

const h = vi.hoisted(() => ({
  resolveTeamAccess: vi.fn(async (..._args: unknown[]) => undefined),
  existing: [] as { id: string; kind: string; teamId: string; section: string }[],
  taken: [] as { id: string }[],
  inserted: [] as Record<string, unknown>[],
  executed: 0,
  deleted: 0,
  updated: [] as Record<string, unknown>[],
  updateRows: [{ id: `x` }] as { id: string }[],
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({ resolveTeamAccess: h.resolveTeamAccess }))

import { sidebarLayoutRouter } from "@/lib/trpc/sidebar-layout"

const fakeTx = {
  execute: async (query: unknown) => {
    // generateTxId's own query answers a txid; the item upserts count.
    const text = JSON.stringify(query)
    if (text.includes(`txid`)) return { rows: [{ txid: `42` }] }
    h.executed += 1
    return { rows: [] }
  },
  select: () => ({
    from: () => ({
      where: (..._args: unknown[]) => {
        const rows = h.existing
        return Object.assign(Promise.resolve(rows), { limit: async () => h.taken })
      },
    }),
  }),
  insert: () => ({
    values: (row: Record<string, unknown>) => {
      h.inserted.push(row)
      return { onConflictDoUpdate: async () => undefined }
    },
  }),
  delete: () => ({
    where: async () => {
      h.deleted += 1
    },
  }),
  update: () => ({
    set: (set: Record<string, unknown>) => {
      h.updated.push(set)
      return { where: () => ({ returning: async () => h.updateRows }) }
    },
  }),
}

const caller = sidebarLayoutRouter.createCaller({
  session: { user: { id: `actor` } },
  db: { transaction: async (fn: (tx: unknown) => Promise<unknown>) => fn(fakeTx) },
  request: new Request(`http://localhost/api/trpc/sidebarLayout.save`),
} as never)

describe(`sidebarLayout`, () => {
  beforeEach(() => {
    h.resolveTeamAccess.mockClear()
    h.existing = []
    h.taken = []
    h.inserted = []
    h.executed = 0
    h.deleted = 0
    h.updated = []
    h.updateRows = [{ id: `x` }]
  })

  it(`save: a new folder and its items, the caller's team checked`, async () => {
    const result = await caller.save({
      teamId: TEAM,
      section: `board`,
      upserts: [
        { id: FOLDER, kind: `folder`, targetId: null, parentId: null, name: `  Ops  `, sortOrder: 1 },
        { id: null, kind: `item`, targetId: BOARD, parentId: FOLDER, name: null, sortOrder: 1 },
      ],
      deletes: [],
    })
    expect(result.txId).toBeDefined()
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(`actor`, TEAM, `read`)
    expect(h.inserted).toEqual([
      expect.objectContaining({ id: FOLDER, userId: `actor`, teamId: TEAM, section: `board`, kind: `folder`, name: `Ops` }),
    ])
    expect(h.executed).toBe(1)
  })

  it(`save: an item's parent must be a folder of this section`, async () => {
    h.existing = [{ id: OTHER, kind: `folder`, teamId: TEAM, section: `agent` }]
    await expect(
      caller.save({
        teamId: TEAM,
        section: `board`,
        upserts: [{ id: null, kind: `item`, targetId: BOARD, parentId: OTHER, name: null, sortOrder: 1 }],
        deletes: [],
      })
    ).rejects.toThrow(/parent must be a folder/)
  })

  it(`save: a folder id never takes over another user's row`, async () => {
    h.taken = [{ id: FOLDER }]
    await expect(
      caller.save({
        teamId: TEAM,
        section: `board`,
        upserts: [{ id: FOLDER, kind: `folder`, targetId: null, parentId: null, name: `x`, sortOrder: 1 }],
        deletes: [],
      })
    ).rejects.toThrow(/taken/)
  })

  it(`save: only this section's folders are deleted`, async () => {
    h.existing = [{ id: FOLDER, kind: `item`, teamId: TEAM, section: `board` }]
    await expect(caller.save({ teamId: TEAM, section: `board`, upserts: [], deletes: [FOLDER] })).rejects.toThrow(/folders/)
    h.existing = [{ id: FOLDER, kind: `folder`, teamId: TEAM, section: `board` }]
    await caller.save({ teamId: TEAM, section: `board`, upserts: [], deletes: [FOLDER] })
    expect(h.deleted).toBe(1)
  })

  it(`rename: trimmed, empty = the default name; no row = NOT_FOUND`, async () => {
    expect((await caller.rename({ id: FOLDER, name: `   ` })).name).toBe(`New folder`)
    expect(h.updated[0]).toEqual(expect.objectContaining({ name: `New folder` }))
    h.updateRows = []
    await expect(caller.rename({ id: FOLDER, name: `Ops` })).rejects.toThrow(/No such folder/)
  })
})
