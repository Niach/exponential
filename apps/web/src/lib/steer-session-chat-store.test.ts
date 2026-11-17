import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { createSteerSessionStore } from "@/lib/steer-session-store"

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

// #EDE-115 (chat-store.json OPEN, S4): a web chat opens from the chat store
// before the live feed attaches; the join's replay replaces the seed; every
// event the feed carries is stored; a feed that is not open pages older
// transcript from the store.

class FakeSocket {
  readyState = 0
  sent: string[] = []
  onopen: (() => void) | null = null
  onmessage: ((event: { data: unknown }) => void) | null = null
  onclose: ((event: { code: number }) => void) | null = null
  send(data: string) {
    this.sent.push(data)
  }
  close() {}
  open() {
    this.readyState = WebSocket.OPEN
    this.onopen?.()
  }
  frame(frame: object) {
    this.onmessage?.({ data: JSON.stringify(frame) })
  }
}

const narration = (text: string) => ({ kind: `narration`, text })

function makeStore(tail: { events: { seq: number; event: unknown }[]; truncated: boolean } | null) {
  const sockets: FakeSocket[] = []
  const recorded: { seq: number; event: unknown }[] = []
  const pages: [number, number][] = []
  let releaseFeed: () => void = () => {}
  const feedHeld = new Promise<void>((resolve) => (releaseFeed = resolve))
  const store = createSteerSessionStore(`session-1`, {
    openFeed: async () => {
      await feedHeld
      const socket = new FakeSocket()
      sockets.push(socket)
      return socket as unknown as WebSocket
    },
    stored: {
      record: (_, events) => recorded.push(...events),
      openTail: async () => tail,
      page: async (_, before, limit) => {
        pages.push([before, limit])
        return { events: [narration(`older ${before - 1}`)], seqs: [before - 1] }
      },
    },
  })
  return { store, sockets, recorded, pages, releaseFeed }
}

const texts = (store: ReturnType<typeof createSteerSessionStore>) =>
  store.getSnapshot().feed.map((item) => (item as { text?: string }).text)

beforeEach(() => vi.useFakeTimers())
afterEach(() => vi.useRealTimers())

describe(`opening from the chat store`, () => {
  it(`paints the stored tail before the feed attaches, then the replay replaces it`, async () => {
    const { store, sockets, releaseFeed } = makeStore({
      events: [
        { seq: 10, event: { kind: `user_message`, text: `Fix the café` } },
        { seq: 11, event: narration(`Looking.`) },
        { seq: 12, event: { kind: `turn`, state: `started` } },
      ],
      truncated: true,
    })
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(0)
    expect(texts(store)).toEqual([`Fix the café`, `Looking.`])
    expect(store.getSnapshot().phase.kind).toBe(`connecting`)
    // A stored `turn` may be a finished run's: never a working caption.
    expect(store.getSnapshot().turnState).toBe(`ended`)
    expect(store.getSnapshot().canLoadEarlier).toBe(true)
    expect(store.getSnapshot().replayed).toBe(false)

    releaseFeed()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]!
    socket.open()
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, seq: 11, event: narration(`Looking.`) })
    socket.frame({ t: `activity`, seq: 12, event: narration(`Fixed it.`) })
    socket.frame({ t: `activity_synced`, firstSeq: 11, lastSeq: 12 })
    await vi.advanceTimersByTimeAsync(100)
    // Below the replay's first seq the seed stays (a prefix the replay does not restate).
    expect(texts(store)).toEqual([`Fix the café`, `Looking.`, `Fixed it.`])
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    expect(store.getSnapshot().replayed).toBe(true)
    store.dispose()
  })

  it(`stores every event the feed carries`, async () => {
    const { store, sockets, recorded, releaseFeed } = makeStore(null)
    store.connect()
    releaseFeed()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]!
    socket.open()
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, seq: 5, event: narration(`a`) })
    socket.frame({ t: `activity_synced`, firstSeq: 5, lastSeq: 5, truncated: true })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.loadEarlier()).toBe(true)
    const ask = JSON.parse(socket.sent.at(-1)!) as { requestId: string }
    socket.frame({ t: `history_chunk`, requestId: ask.requestId, events: [narration(`b`)], seqs: [4], done: true })
    await vi.advanceTimersByTimeAsync(100)
    expect(recorded).toEqual([
      { seq: 5, event: narration(`a`) },
      { seq: 4, event: narration(`b`) },
    ])
    expect(texts(store)).toEqual([`b`, `a`])
    store.dispose()
  })

  it(`pages older transcript from the store while no feed is open`, async () => {
    const { store, pages } = makeStore({ events: [{ seq: 50, event: narration(`newest`) }], truncated: true })
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    expect(texts(store)).toEqual([`newest`])
    expect(store.loadEarlier()).toBe(true)
    await vi.advanceTimersByTimeAsync(100)
    expect(pages[0]![0]).toBe(50)
    expect(texts(store)).toEqual([`older 49`, `newest`])
    store.dispose()
  })
})
