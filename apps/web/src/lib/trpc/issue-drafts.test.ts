import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-878: the issue-drafts router. What it locks:
//   * the write is an idempotent UPSERT on a CLIENT-minted id, guarded by a
//     `user_id = me` setWhere so a guessed id can never overwrite somebody
//     else's row (empty `returning` ⇒ refusal, not a silent no-op);
//   * the board must belong to the claimed team and be visible, and a
//     `statusId` must belong to the team;
//   * a description may only embed images THIS draft owns — uploads are
//     eager, so a blob: URL or a foreign attachment is a client bug, not
//     something to store;
//   * `delete` collects the storage keys INSIDE the transaction (the cascade
//     drops the rows) and reclaims the blobs only AFTER it commits.

const h = vi.hoisted(() => ({
  getBoardTeamId: vi.fn(),
  resolveTeamAccess: vi.fn(async (..._args: unknown[]) => undefined),
  deleteStorageObjects: vi.fn(async (..._args: unknown[]) => undefined),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, () => ({
  getBoardTeamId: h.getBoardTeamId,
  resolveTeamAccess: h.resolveTeamAccess,
}))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  deleteStorageObjects: h.deleteStorageObjects,
}))

import { attachments, issueDrafts, issueStatuses } from "@/db/schema"
import { issueDraftsRouter } from "@/lib/trpc/issue-drafts"

const DRAFT = `11111111-1111-4111-8111-111111111111`
const TEAM = `22222222-2222-4222-8222-222222222222`
const BOARD = `33333333-3333-4333-8333-333333333333`
const STATUS = `44444444-4444-4444-8444-444444444444`
const ATT = `55555555-5555-4555-8555-555555555555`
const ORIGIN = `http://localhost:5173/api/trpc/issueDrafts.upsert`

const state = {
  /** Attachment rows the ownership probe finds for this draft. */
  ownedAttachments: [] as { id: string }[],
  /** Attachment rows the delete path collects (storage keys). */
  draftAttachments: [] as {
    storageKey: string
    posterStorageKey?: string | null
  }[],
  statusRows: [] as { teamId: string }[],
  draftRows: [] as { id: string }[],
  /** What the upsert's `returning()` yields — empty = somebody else's row. */
  upserted: [{ id: DRAFT }] as { id: string }[],
  /** What the delete's `returning()` yields. */
  deleted: [{ id: DRAFT }] as { id: string }[],
  inserted: [] as Record<string, unknown>[],
  conflictSets: [] as Record<string, unknown>[],
  /** Ordered log, so "blobs only after commit" is observable. */
  events: [] as string[],
}

function thenable(rows: unknown[]) {
  return {
    then: (resolve: (value: unknown[]) => unknown, reject?: unknown) =>
      Promise.resolve(rows).then(resolve, reject as never),
    limit: async (count: number) => rows.slice(0, count),
  }
}

function select(fields: Record<string, unknown>) {
  return {
    from: (table: unknown) => ({
      where: () => {
        if (table === issueStatuses) return thenable(state.statusRows)
        if (table === issueDrafts) return thenable(state.draftRows)
        if (table === attachments) {
          // The ownership probe selects `{ id }`; the delete path selects the
          // storage keys.
          return `storageKey` in fields
            ? thenable(state.draftAttachments)
            : thenable(state.ownedAttachments)
        }
        return thenable([])
      },
    }),
  }
}

const fakeTx = {
  execute: async () => ({ rows: [{ txid: `42` }] }),
  select,
  insert: () => ({
    values: (row: Record<string, unknown>) => {
      state.inserted.push(row)
      return {
        onConflictDoUpdate: (config: Record<string, unknown>) => {
          state.conflictSets.push(config.set as Record<string, unknown>)
          return { returning: async () => state.upserted }
        },
      }
    },
  }),
  delete: () => ({
    where: () => ({
      returning: async () => {
        state.events.push(`delete-rows`)
        return state.deleted
      },
    }),
  }),
}

const caller = issueDraftsRouter.createCaller({
  session: { user: { id: `actor` } },
  db: {
    select,
    transaction: async (fn: (tx: unknown) => Promise<unknown>) => {
      const result = await fn(fakeTx)
      state.events.push(`commit`)
      return result
    },
  },
  request: new Request(ORIGIN),
} as never)

function input(overrides: Record<string, unknown> = {}) {
  return {
    id: DRAFT,
    teamId: TEAM,
    boardId: BOARD,
    title: `Parked`,
    description: ``,
    labelIds: [],
    ...overrides,
  }
}

beforeEach(() => {
  state.ownedAttachments = []
  state.draftAttachments = []
  state.statusRows = []
  state.draftRows = []
  state.upserted = [{ id: DRAFT }]
  state.deleted = [{ id: DRAFT }]
  state.inserted = []
  state.conflictSets = []
  state.events = []
  h.getBoardTeamId.mockReset()
  h.getBoardTeamId.mockResolvedValue({ id: BOARD, teamId: TEAM })
  h.resolveTeamAccess.mockReset()
  h.resolveTeamAccess.mockResolvedValue(undefined)
  h.deleteStorageObjects.mockReset()
  h.deleteStorageObjects.mockImplementation(async () => {
    state.events.push(`reclaim`)
  })
})

describe(`issueDrafts.upsert`, () => {
  it(`writes the row and gates on create_issue in the board's team`, async () => {
    const result = await caller.upsert(input())

    expect(h.getBoardTeamId).toHaveBeenCalledWith(BOARD)
    expect(h.resolveTeamAccess).toHaveBeenCalledWith(
      `actor`,
      TEAM,
      `create_issue`
    )
    expect(result.txId).toBe(42)
    expect(state.inserted[0]).toMatchObject({
      id: DRAFT,
      userId: `actor`,
      teamId: TEAM,
      boardId: BOARD,
      title: `Parked`,
      description: ``,
      priority: `none`,
      labelIds: [],
    })
    // `user_id` is never re-set by the conflict branch — ownership is fixed
    // at insert and enforced by the setWhere.
    expect(state.conflictSets[0]).not.toHaveProperty(`userId`)
  })

  it(`refuses a board that belongs to another team`, async () => {
    h.getBoardTeamId.mockResolvedValue({ id: BOARD, teamId: `other-team` })
    await expect(caller.upsert(input())).rejects.toThrow(
      /board must belong to this team/i
    )
  })

  it(`refuses a status row from another team`, async () => {
    state.statusRows = [{ teamId: `other-team` }]
    await expect(caller.upsert(input({ statusId: STATUS }))).rejects.toThrow(
      /status must belong to this team/i
    )
  })

  // Somebody else's id: the setWhere makes the UPDATE touch nothing, so
  // `returning` comes back empty and the router refuses rather than pretend.
  it(`refuses an id that belongs to another user`, async () => {
    state.upserted = []
    await expect(caller.upsert(input())).rejects.toThrow(
      /belongs to someone else/i
    )
  })

  it(`refuses a blob: image — uploads are eager, never deferred`, async () => {
    await expect(
      caller.upsert(input({ description: `![shot](blob:abc-123)` }))
    ).rejects.toThrow(/only reference images uploaded to this draft/i)
    expect(state.inserted).toEqual([])
  })

  it(`refuses an attachment this draft does not own`, async () => {
    state.ownedAttachments = []
    await expect(
      caller.upsert(
        input({ description: `![shot](/api/attachments/${ATT})` })
      )
    ).rejects.toThrow(/only reference images uploaded to this draft/i)
  })

  it(`accepts and canonicalizes an image the draft owns`, async () => {
    state.ownedAttachments = [{ id: ATT }]
    await caller.upsert(
      input({
        description: `![shot](http://localhost:5173/api/attachments/${ATT}?w=480&x=1)`,
      })
    )
    expect(state.inserted[0].description).toBe(
      `![shot](/api/attachments/${ATT}?w=480)`
    )
  })
})

describe(`issueDrafts.delete`, () => {
  it(`collects the keys in the tx and reclaims the blobs after commit`, async () => {
    state.draftAttachments = [
      { storageKey: `drafts/${DRAFT}/a-shot.png`, posterStorageKey: null },
      {
        storageKey: `drafts/${DRAFT}/b-clip.mp4`,
        posterStorageKey: `drafts/${DRAFT}/b-clip.mp4.poster`,
      },
    ]

    const result = await caller.delete({ id: DRAFT })

    expect(result).toEqual({ txId: 42, deleted: true })
    expect(h.deleteStorageObjects).toHaveBeenCalledWith([
      `drafts/${DRAFT}/a-shot.png`,
      `drafts/${DRAFT}/b-clip.mp4`,
      `drafts/${DRAFT}/b-clip.mp4.poster`,
    ])
    // Blobs go only once the row is really gone.
    expect(state.events).toEqual([`delete-rows`, `commit`, `reclaim`])
  })

  it(`reclaims nothing when the row was not the caller's`, async () => {
    state.deleted = []
    state.draftAttachments = [{ storageKey: `drafts/${DRAFT}/a-shot.png` }]

    const result = await caller.delete({ id: DRAFT })

    expect(result).toEqual({ txId: 42, deleted: false })
    expect(h.deleteStorageObjects).toHaveBeenCalledWith([])
  })
})

describe(`issueDrafts.listAttachments`, () => {
  it(`404s a draft that is not the caller's`, async () => {
    state.draftRows = []
    await expect(caller.listAttachments({ id: DRAFT })).rejects.toThrow(
      /Draft not found/i
    )
  })

  it(`returns the draft's rows once ownership is proven`, async () => {
    state.draftRows = [{ id: DRAFT }]
    state.ownedAttachments = [{ id: ATT }]
    await expect(caller.listAttachments({ id: DRAFT })).resolves.toEqual([
      { id: ATT },
    ])
  })
})
