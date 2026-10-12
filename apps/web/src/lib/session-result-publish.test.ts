import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1284: a published picture lands in the run's THREAD's Guide, under the
// run lock and then the thread's, mirrored onto the run's own row, stamped
// with the session. Only a picture the session OWNS under the same (topic,
// label) is displaced with its row and bytes (review T1): another session's
// stays. The db, the storage and the thread lookup are mocked; the list
// algebra and the lock helpers run for real.

const state = vi.hoisted(() => ({
  /** The locked reads in order: the run row, then (with a thread) its row. */
  lockedReads: [] as unknown[][],
  locks: [] as string[],
  thread: null as { id: string; teamId: string } | null,
  updates: [] as Array<{ table: unknown; values: Record<string, unknown> }>,
  deletedStorageKey: `sessions/old.png` as string | null,
  inserted: true,
  /** The Guide owner read: none = the session alone (owns legacy). */
  owner: [] as Array<Record<string, unknown>>,
}))

vi.mock(`@/db/connection`, () => {
  const tx = {
    execute: async () => ({ rows: state.owner }),
    select: () => {
      const rows = state.lockedReads.shift() ?? []
      const q: Record<string, unknown> = {}
      for (const m of [`from`, `where`, `limit`]) q[m] = () => q
      q.for = (mode: string) => {
        state.locks.push(mode)
        return Promise.resolve(rows)
      }
      return q
    },
    insert: () => ({
      values: () => ({
        onConflictDoNothing: () => ({
          returning: async () => (state.inserted ? [{ id: `att-new` }] : []),
        }),
      }),
    }),
    delete: () => ({
      where: () => ({
        returning: async () =>
          state.deletedStorageKey ? [{ storageKey: state.deletedStorageKey }] : [],
      }),
    }),
    update: (table: unknown) => ({
      set: (values: Record<string, unknown>) => ({
        where: async () => {
          state.updates.push({ table, values })
        },
      }),
    }),
  }
  return {
    db: { transaction: async (fn: (t: unknown) => Promise<unknown>) => fn(tx) },
  }
})
vi.mock(`@/lib/storage`, () => ({ deleteObject: vi.fn(async () => {}) }))
vi.mock(`@/lib/storage/session-attachment-upload`, () => ({
  rollbackSessionImage: vi.fn(async () => {}),
  sessionAttachmentValues: vi.fn(() => ({})),
}))
vi.mock(`@/lib/threads`, () => ({
  getThreadOfSession: async () => state.thread,
}))

import { codingSessions, threads } from "@/db/schema"
import { deleteObject } from "@/lib/storage"
import { rollbackSessionImage } from "@/lib/storage/session-attachment-upload"
import { publishSessionResultPicture } from "./session-result-publish"

const prepared = {
  attachmentId: `att-new`,
  storageKey: `sessions/new.png`,
  width: 1600,
  height: 900,
} as never

const picture = (topic: string, label: string, attachmentId: string) => ({
  topic,
  label,
  attachmentId,
  width: 1600,
  height: 900,
})

const input = {
  sessionId: `s-2`,
  teamId: `t-1`,
  topic: `Checkout`,
  label: `web`,
  uploaderId: `u-1`,
}

beforeEach(() => {
  state.lockedReads = []
  state.locks = []
  state.thread = null
  state.updates = []
  state.deletedStorageKey = `sessions/old.png`
  state.inserted = true
  state.owner = []
  vi.mocked(deleteObject).mockClear()
  vi.mocked(rollbackSessionImage).mockClear()
})

describe(`publishSessionResultPicture (EXP-1284 thread Guide)`, () => {
  it(`writes the thread's Guide and the run's mirror, stamped; another session's picture of the same name stays`, async () => {
    state.thread = { id: `th-1`, teamId: `t-1` }
    state.owner = [{ chain_ids: [`s-2`], first_session_id: `s-1` }]
    // The run's own mirror is stale; the thread holds the first session's shot.
    const theirs = { ...picture(`Checkout`, `web`, `att-old`), sessionId: `s-1` }
    state.lockedReads = [[{ results: null }], [{ results: [theirs] }]]
    const published = await publishSessionResultPicture(input, prepared)

    expect(state.locks).toEqual([`update`, `update`])
    expect(published.replaced).toBe(false)
    // The answer = what THIS session published.
    expect(published.results).toEqual([
      expect.objectContaining({
        topic: `Checkout`,
        label: `web`,
        attachmentId: `att-new`,
        sessionId: `s-2`,
      }),
    ])
    expect(state.updates.map((u) => u.table)).toEqual([threads, codingSessions])
    expect(state.updates[0]!.values.results).toEqual(state.updates[1]!.values.results)
    expect(state.updates[0]!.values.results).toEqual([
      theirs,
      expect.objectContaining({ attachmentId: `att-new`, sessionId: `s-2` }),
    ])
    expect(deleteObject).not.toHaveBeenCalled()
  })

  it(`replaces the session's OWN picture (and the legacy one it inherits), reclaiming the bytes`, async () => {
    state.thread = { id: `th-1`, teamId: `t-1` }
    // s-2 resumed s-1, the thread's first session: the unstamped entry is its.
    state.owner = [{ chain_ids: [`s-2`, `s-1`], first_session_id: `s-1` }]
    state.lockedReads = [[{ results: null }], [{ results: [picture(`Checkout`, `web`, `att-old`)] }]]
    const published = await publishSessionResultPicture(input, prepared)
    expect(published.replaced).toBe(true)
    expect(published.results).toEqual([
      expect.objectContaining({ attachmentId: `att-new`, sessionId: `s-2` }),
    ])
    expect(deleteObject).toHaveBeenCalledWith(`sessions/old.png`)
  })

  it(`a thread whose Guide was never written starts from the run's own results`, async () => {
    state.thread = { id: `th-1`, teamId: `t-1` }
    state.deletedStorageKey = null
    state.lockedReads = [[{ results: [picture(`Nav`, `ios`, `att-1`)] }], [{ results: null }]]
    const published = await publishSessionResultPicture(input, prepared)
    expect(published.results.map((row) => row.attachmentId)).toEqual([`att-1`, `att-new`])
  })

  it(`a run with no thread keeps writing its own row`, async () => {
    state.deletedStorageKey = null
    state.lockedReads = [[{ results: [] }]]
    await publishSessionResultPicture(input, prepared)
    expect(state.locks).toEqual([`update`])
    expect(state.updates.map((u) => u.table)).toEqual([codingSessions])
  })

  it(`a vanished run rolls the object back`, async () => {
    state.lockedReads = [[]]
    await expect(publishSessionResultPicture(input, prepared)).rejects.toThrow(
      `Session not found`
    )
    expect(rollbackSessionImage).toHaveBeenCalledWith(`sessions/new.png`)
  })
})
