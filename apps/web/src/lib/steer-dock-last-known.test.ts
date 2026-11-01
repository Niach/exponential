import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// #EXP-1538 (dock.json LOCAL FIRST): the stream's latest-wins Dock slots
// (THE Queue's task_list + background_tasks, the relay queue, compaction,
// the rate limit) show the LAST-KNOWN value per session while the stream
// connects, even after the registry reaped the store; an ended session
// shows none of them; sign-out forgets them.

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

class FakeSocket {
  readyState = 0
  sent: string[] = []
  onopen: (() => void) | null = null
  onmessage: ((event: { data: unknown }) => void) | null = null
  onclose: ((event: { code: number }) => void) | null = null
  private seq = 0
  send(data: string) {
    this.sent.push(data)
  }
  close() {}
  open() {
    this.readyState = 1
    this.onopen?.()
  }
  frame(frame: { t: string; [key: string]: unknown }) {
    const out =
      frame.t === `activity`
        ? { ...frame, seq: this.seq++ }
        : frame.t === `activity_synced`
          ? { firstSeq: 0, lastSeq: Math.max(0, this.seq - 1), ...frame }
          : frame
    this.onmessage?.({ data: JSON.stringify(out) })
  }
}

const sockets: FakeSocket[] = []
vi.mock(`@/lib/enet/steer-feed`, () => ({
  openSteerFeed: async () => {
    const socket = new FakeSocket()
    sockets.push(socket)
    return socket
  },
}))
vi.mock(`@/lib/enet/conn-stats`, () => ({
  connStatsBuffer: () => ({ record: () => {} }),
  SteerViewStats: class {
    start() {}
    end() {}
  },
}))

const { acquireSteerSession, disposeAllSteerSessions, retainSteerSessions, REPLAY_QUIET_MS, LAST_KNOWN_DOCK_CAP } =
  await import(
  `@/lib/steer-session-store`
)

const TASKS = [{ content: `Read the issue`, status: `in_progress` }]

/** Open a store, go live with a replay carrying THE Queue + a queued
 *  message + a rate-limit window, then let the reaper dispose it. */
async function viewThenReap(sessionId: string) {
  const store = acquireSteerSession(sessionId)
  const unsubscribe = store.subscribe(() => {})
  store.connect()
  await vi.advanceTimersByTimeAsync(0)
  const socket = sockets.at(-1)!
  socket.open()
  socket.frame({ t: `activity_reset` })
  socket.frame({ t: `activity`, event: { kind: `task_list`, entries: TASKS } })
  socket.frame({ t: `activity`, event: { kind: `queue`, messages: [{ id: `q1`, text: `next` }] } })
  socket.frame({ t: `activity`, event: { kind: `rate_limit`, status: `limited`, resetsAt: Date.now() + 60_000 } })
  socket.frame({ t: `activity_synced` })
  await vi.advanceTimersByTimeAsync(REPLAY_QUIET_MS + 50)
  expect(store.getSnapshot().taskList).toEqual(TASKS)
  unsubscribe()
  retainSteerSessions(new Set())
  await vi.advanceTimersByTimeAsync(61_000)
  expect(acquireSteerSession(sessionId)).not.toBe(store)
  return store
}

beforeEach(() => {
  vi.useFakeTimers()
  sockets.length = 0
})

afterEach(() => {
  disposeAllSteerSessions()
  vi.useRealTimers()
})

describe(`last-known Dock slots (#EXP-1538)`, () => {
  it(`a re-acquired store draws THE Queue in its first snapshot, before the stream connects`, async () => {
    await viewThenReap(`s-1`)
    const store = acquireSteerSession(`s-1`)
    const first = store.getSnapshot()
    expect(first.phase.kind).toBe(`idle`)
    expect(first.taskList).toEqual(TASKS)
    expect(first.queue.map((entry) => entry.text)).toEqual([`next`])
    expect(first.rateLimit).not.toBeNull()
    // Connecting keeps them.
    store.connect()
    expect(store.getSnapshot().phase.kind).toBe(`connecting`)
    expect(store.getSnapshot().taskList).toEqual(TASKS)
    // …until the replay restates them in one commit.
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets.at(-1)!
    socket.open()
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `task_list`, entries: [] } })
    expect(store.getSnapshot().taskList).toEqual(TASKS)
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(REPLAY_QUIET_MS + 50)
    expect(store.getSnapshot().taskList).toEqual([])
    expect(store.getSnapshot().queue).toEqual([])
    expect(store.getSnapshot().rateLimit).toBeNull()
  })

  it(`a session that ended keeps no last-known slots`, async () => {
    const store = acquireSteerSession(`s-2`)
    const unsubscribe = store.subscribe(() => {})
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets.at(-1)!
    socket.open()
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `task_list`, entries: TASKS } })
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(REPLAY_QUIET_MS + 50)
    store.noteSessionStatus(`ended`)
    unsubscribe()
    retainSteerSessions(new Set())
    await vi.advanceTimersByTimeAsync(61_000)
    expect(acquireSteerSession(`s-2`).getSnapshot().taskList).toEqual([])
  })

  it(`a seeded queue that the stream never restated is not handed back to the draft`, async () => {
    await viewThenReap(`s-3`)
    const store = acquireSteerSession(`s-3`)
    store.noteSessionStatus(`ended`)
    sockets.length = 0
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets.at(-1)!
    socket.open()
    socket.frame({ t: `bye` })
    socket.onclose?.({ code: 1000 })
    await vi.advanceTimersByTimeAsync(0)
    expect(store.getDraftSnapshot().text).toBe(``)
  })

  it(`a store that showed nothing keeps no entry, and the memory is capped`, async () => {
    const live = async (sessionId: string, entries: unknown[]) => {
      const store = acquireSteerSession(sessionId)
      store.connect()
      await vi.advanceTimersByTimeAsync(0)
      const socket = sockets.at(-1)!
      socket.open()
      socket.frame({ t: `activity_reset` })
      socket.frame({ t: `activity`, event: { kind: `task_list`, entries } })
      socket.frame({ t: `activity_synced` })
    }
    await live(`empty`, [])
    for (let i = 0; i <= LAST_KNOWN_DOCK_CAP; i++) await live(`cap-${i}`, TASKS)
    await vi.advanceTimersByTimeAsync(REPLAY_QUIET_MS + 50)
    retainSteerSessions(new Set())
    await vi.advanceTimersByTimeAsync(61_000)
    const hasTasks = (sessionId: string) => acquireSteerSession(sessionId).getSnapshot().taskList.length > 0
    expect(hasTasks(`empty`)).toBe(false)
    expect(hasTasks(`cap-0`)).toBe(false)
    expect(hasTasks(`cap-1`)).toBe(true)
    expect(hasTasks(`cap-${LAST_KNOWN_DOCK_CAP}`)).toBe(true)
  })

  it(`sign-out forgets every last-known slot`, async () => {
    await viewThenReap(`s-4`)
    disposeAllSteerSessions()
    expect(acquireSteerSession(`s-4`).getSnapshot().taskList).toEqual([])
  })
})
