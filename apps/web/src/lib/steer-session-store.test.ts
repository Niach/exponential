import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  createSteerSessionStore,
  type SteerSessionStore,
} from "@/lib/steer-session-store"
import { FEED_CAP } from "@/lib/agent-feed"

// The store never reaches the real client in tests — every store gets fake
// deps — but the module imports it at load, so keep the import inert.
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

// EXP-621: the per-session steer connection store — the socket, feed and
// composer draft all outlive the view. These tests drive it with a fake
// socket + mint (the injected test seams) and fake timers.

class FakeSocket {
  readyState = 0 // CONNECTING
  sent: string[] = []
  closed = false
  onopen: (() => void) | null = null
  onmessage: ((event: { data: unknown }) => void) | null = null
  onclose: ((event: { code: number }) => void) | null = null

  send(data: string) {
    this.sent.push(data)
  }
  close() {
    this.closed = true
  }

  // Test drivers
  open() {
    this.readyState = WebSocket.OPEN
    this.onopen?.()
  }
  frame(frame: object) {
    this.onmessage?.({ data: JSON.stringify(frame) })
  }
  serverClose(code = 1006) {
    this.readyState = WebSocket.CLOSED
    this.onclose?.({ code })
  }
}

function makeStore(overrides?: {
  mint?: () => Promise<{ disabled: true } | { ticket: string; url: string }>
}) {
  const sockets: FakeSocket[] = []
  const disposed = { current: false }
  const store = createSteerSessionStore(
    `session-1`,
    {
      mintTicket:
        overrides?.mint ??
        (() => Promise.resolve({ ticket: `t`, url: `wss://relay/ws?ticket=t` })),
      createSocket: () => {
        const socket = new FakeSocket()
        sockets.push(socket)
        return socket as unknown as WebSocket
      },
    },
    () => {
      disposed.current = true
    }
  )
  return { store, sockets, disposed }
}

/** Connect and drive the newest socket to a joined, live state. */
async function goLive(store: SteerSessionStore, sockets: FakeSocket[]) {
  store.connect()
  await vi.advanceTimersByTimeAsync(0)
  const socket = sockets[sockets.length - 1]
  socket.open()
  socket.frame({ t: `activity_reset` })
  await vi.advanceTimersByTimeAsync(100)
  return socket
}

beforeEach(() => {
  vi.useFakeTimers()
  vi.stubGlobal(`URL`, {
    ...URL,
    createObjectURL: vi.fn(() => `blob:${Math.random()}`),
    revokeObjectURL: vi.fn(),
  })
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe(`connection lifecycle`, () => {
  it(`dials on connect, joins the activity channel, goes live on the reset frame`, async () => {
    const { store, sockets } = makeStore()
    store.connect()
    expect(store.getSnapshot().phase.kind).toBe(`connecting`)
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    expect(socket.sent[0]).toBe(
      JSON.stringify({ t: `join`, channel: `activity` })
    )
    socket.frame({ t: `activity_reset` })
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    store.dispose()
  })

  it(`connect is idempotent — a live store never re-dials`, async () => {
    const { store, sockets } = makeStore()
    await goLive(store, sockets)
    store.connect()
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(1)
    store.dispose()
  })

  it(`applies activity frames to the feed and caps it`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    for (let i = 0; i < FEED_CAP + 10; i++) {
      socket.frame({
        t: `activity`,
        event: { kind: `narration`, text: `line ${i}` },
      })
    }
    await vi.advanceTimersByTimeAsync(1_000)
    const { feed } = store.getSnapshot()
    expect(feed).toHaveLength(FEED_CAP)
    expect(feed[feed.length - 1]).toMatchObject({ text: `line ${FEED_CAP + 9}` })
    store.dispose()
  })

  it(`activity_reset clears the retained feed`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `hi` } })
    await vi.advanceTimersByTimeAsync(1_000)
    expect(store.getSnapshot().feed).toHaveLength(1)
    socket.frame({ t: `activity_reset` })
    await vi.advanceTimersByTimeAsync(1_000)
    expect(store.getSnapshot().feed).toHaveLength(0)
    store.dispose()
  })

  it(`bye ends the session`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({ t: `bye`, outcome: `ended` })
    socket.serverClose(4001)
    expect(store.getSnapshot().phase.kind).toBe(`ended`)
    store.dispose()
  })

  it(`snapshot identity is stable between mutations`, async () => {
    const { store, sockets } = makeStore()
    await goLive(store, sockets)
    expect(store.getSnapshot()).toBe(store.getSnapshot())
    expect(store.getDraftSnapshot()).toBe(store.getDraftSnapshot())
    store.dispose()
  })
})

describe(`slow-consumer eviction (4008)`, () => {
  it(`redials silently — phase stays live, feed retained, no Disconnected`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `keep` } })
    await vi.advanceTimersByTimeAsync(1_000)

    socket.serverClose(4008)
    // The eviction is not surfaced: the phase holds and the feed stays.
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    expect(store.getSnapshot().feed).toHaveLength(1)

    // A redial is scheduled (jittered 1.5-3s first step).
    await vi.advanceTimersByTimeAsync(3_100)
    expect(sockets).toHaveLength(2)
    sockets[1].open()
    sockets[1].frame({ t: `activity_reset` })
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    store.dispose()
  })

  it(`does not redial a session already marked ended`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    store.noteSessionStatus(`ended`)
    socket.serverClose(4008)
    expect(store.getSnapshot().phase.kind).toBe(`closed`)
    await vi.advanceTimersByTimeAsync(60_000)
    expect(sockets).toHaveLength(1)
    store.dispose()
  })

  it(`other unexpected close codes stay terminal (manual Reconnect)`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.serverClose(1006)
    expect(store.getSnapshot().phase.kind).toBe(`closed`)
    await vi.advanceTimersByTimeAsync(60_000)
    expect(sockets).toHaveLength(1)
    store.reconnect()
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(2)
    store.dispose()
  })
})

describe(`draft`, () => {
  const image = (name: string) =>
    new File([`x`], name, { type: `image/png` })

  it(`text and images survive a disconnect and redial`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    store.setDraftText(`half-typed thought`)
    store.addDraftImages([image(`a.png`)])
    socket.serverClose(4008)
    await vi.advanceTimersByTimeAsync(3_100)
    expect(store.getDraftSnapshot().text).toBe(`half-typed thought`)
    expect(store.getDraftSnapshot().images).toHaveLength(1)
    store.dispose()
  })

  it(`two successive adds both land (the stale-closure regression)`, () => {
    const { store } = makeStore()
    store.addDraftImages([image(`a.png`)])
    store.addDraftImages([image(`b.png`)])
    expect(store.getDraftSnapshot().images).toHaveLength(2)
    store.dispose()
  })

  it(`rejects oversized/non-image files and over-cap extras with counts`, () => {
    const { store } = makeStore()
    const notImage = new File([`x`], `a.txt`, { type: `text/plain` })
    expect(store.addDraftImages([notImage])).toEqual({
      rejected: 1,
      overflow: 0,
    })
    const many = [1, 2, 3, 4, 5].map((n) => image(`${n}.png`))
    const result = store.addDraftImages(many)
    expect(result.rejected).toBe(0)
    expect(result.overflow).toBeGreaterThan(0)
    store.dispose()
  })

  it(`clearDraftAfterSend revokes every blob URL and empties the draft`, () => {
    const { store } = makeStore()
    store.setDraftText(`hello`)
    store.addDraftImages([image(`a.png`), image(`b.png`)])
    store.clearDraftAfterSend()
    expect(store.getDraftSnapshot()).toEqual({ text: ``, images: [] })
    expect(URL.revokeObjectURL).toHaveBeenCalledTimes(2)
  })

  it(`dispose revokes pending blob URLs`, () => {
    const { store } = makeStore()
    store.addDraftImages([image(`a.png`)])
    store.dispose()
    expect(URL.revokeObjectURL).toHaveBeenCalledTimes(1)
  })
})

describe(`sending`, () => {
  it(`sendMessage chunks text, appends a local echo, and reports success`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    const before = socket.sent.length
    expect(store.sendMessage(`do the thing`)).toBe(true)
    expect(socket.sent.length).toBe(before + 2) // text + \r
    expect(store.getSnapshot().feed.at(-1)).toMatchObject({
      kind: `user_message`,
      text: `do the thing`,
    })
    store.dispose()
  })

  it(`sendMessage returns false with no open socket (draft is kept by the caller)`, () => {
    const { store } = makeStore()
    expect(store.sendMessage(`too early`)).toBe(false)
  })
})

describe(`registry lifecycle`, () => {
  it(`an unsubscribed ended store self-disposes after the grace delay`, async () => {
    const { store, sockets, disposed } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({ t: `bye`, outcome: `ended` })
    socket.serverClose(4001)
    expect(disposed.current).toBe(false)
    await vi.advanceTimersByTimeAsync(6_000)
    expect(disposed.current).toBe(true)
  })

  it(`a subscriber cancels the ended self-dispose until it unsubscribes`, async () => {
    const { store, sockets, disposed } = makeStore()
    const unsubscribe = store.subscribe(() => {})
    const socket = await goLive(store, sockets)
    socket.frame({ t: `bye`, outcome: `ended` })
    socket.serverClose(4001)
    await vi.advanceTimersByTimeAsync(60_000)
    expect(disposed.current).toBe(false)
    unsubscribe()
    await vi.advanceTimersByTimeAsync(6_000)
    expect(disposed.current).toBe(true)
  })

  it(`reap waits out the grace period and spares kept/subscribed stores`, async () => {
    const { store, disposed } = makeStore()
    store._scheduleReap()
    await vi.advanceTimersByTimeAsync(30_000)
    expect(disposed.current).toBe(false)
    // A keep within the window cancels the reap entirely.
    store._cancelReap()
    await vi.advanceTimersByTimeAsync(120_000)
    expect(disposed.current).toBe(false)
    // Un-kept again and left alone → disposed after the full grace.
    store._scheduleReap()
    await vi.advanceTimersByTimeAsync(61_000)
    expect(disposed.current).toBe(true)
  })
})
