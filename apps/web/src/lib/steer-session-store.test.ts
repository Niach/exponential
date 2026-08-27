import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  acquireSteerSession,
  createSteerSessionStore,
  disposeAllSteerSessions,
  type SteerSessionStore,
} from "@/lib/steer-session-store"
import { FEED_CAP } from "@/lib/agent-feed"
import { TRPCClientError } from "@trpc/client"

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
    // The eviction is not surfaced as a phase change: the phase holds and the
    // feed stays — but `connected` dips so send affordances dim honestly.
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    expect(store.getSnapshot().feed).toHaveLength(1)
    expect(store.getSnapshot().connected).toBe(false)

    // A redial is scheduled (jittered 1.5-3s first step).
    await vi.advanceTimersByTimeAsync(3_100)
    expect(sockets).toHaveLength(2)
    sockets[1].open()
    sockets[1].frame({ t: `activity_reset` })
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    expect(store.getSnapshot().connected).toBe(true)
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

// EXP-625: revival is driven by whether a dial is ALIVE, not by the phase
// alone. A mute socket and a hung mint both used to strand the viewer on
// "Connecting…" with nothing to click.
describe(`stuck dials`, () => {
  it(`closes a socket that opens but never answers the join`, async () => {
    const { store, sockets } = makeStore()
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    expect(store.getSnapshot().phase.kind).toBe(`connecting`)
    await vi.advanceTimersByTimeAsync(15_100)
    expect(socket.closed).toBe(true)
    // The store closes it; the browser then fires onclose as usual.
    socket.serverClose(1006)
    expect(store.getSnapshot().phase.kind).toBe(`closed`)
    store.dispose()
  })

  it(`gives up on a mint that never resolves`, async () => {
    const { store, sockets } = makeStore({
      mint: () => new Promise<never>(() => {}),
    })
    store.connect()
    await vi.advanceTimersByTimeAsync(19_000)
    expect(store.getSnapshot().phase.kind).toBe(`connecting`)
    await vi.advanceTimersByTimeAsync(1_500)
    const { phase } = store.getSnapshot()
    expect(phase.kind).toBe(`closed`)
    expect(phase.kind === `closed` && phase.detail).toContain(`in time`)
    expect(sockets).toHaveLength(0)
    store.dispose()
  })
})

describe(`kick (wakeup nudge)`, () => {
  it(`redials a closed store whose session is still running`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.serverClose(1006)
    expect(store.getSnapshot().phase.kind).toBe(`closed`)
    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(2)
    store.dispose()
  })

  it(`leaves a closed store alone once the session ended`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    store.noteSessionStatus(`ended`)
    socket.serverClose(1006)
    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(1_000)
    expect(sockets).toHaveLength(1)
    store.dispose()
  })

  it(`is a no-op on a live store`, async () => {
    const { store, sockets } = makeStore()
    await goLive(store, sockets)
    store.kick(`visible`)
    store.kick(`online`)
    await vi.advanceTimersByTimeAsync(1_000)
    expect(sockets).toHaveLength(1)
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    store.dispose()
  })

  it(`retries a starting store immediately instead of waiting out the backoff`, async () => {
    const { store, sockets } = makeStore()
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    sockets[0].open()
    sockets[0].frame({ t: `error`, code: `no_such_session` })
    sockets[0].serverClose(4001)
    expect(store.getSnapshot().phase.kind).toBe(`starting`)

    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(2)
    // The pending backoff redial was cancelled, not merely beaten.
    expect(store.getSnapshot().phase.kind).toBe(`starting`)
    await vi.advanceTimersByTimeAsync(5_000)
    expect(sockets).toHaveLength(2)
    store.dispose()
  })

  it(`closes the in-flight dial's socket instead of leaking a duplicate viewer`, async () => {
    const { store, sockets } = makeStore()
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    sockets[0].open()
    sockets[0].frame({ t: `error`, code: `no_such_session` })
    sockets[0].serverClose(4001)
    expect(store.getSnapshot().phase.kind).toBe(`starting`)

    // The backoff redial lands and its socket opens + joins, but the relay
    // has not answered the join yet — the dial is still in flight.
    await vi.advanceTimersByTimeAsync(5_000)
    expect(sockets).toHaveLength(2)
    sockets[1].open()
    expect(store.getSnapshot().connected).toBe(true)

    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(3)
    // The abandoned socket is CLOSED, not merely ignored: an open one stays
    // joined at the relay as a second viewer.
    expect(sockets[1].closed).toBe(true)
    expect(sockets[2].closed).toBe(false)

    // The successor is the only live socket — it joins and goes live.
    sockets[2].open()
    sockets[2].frame({ t: `activity_reset` })
    await vi.advanceTimersByTimeAsync(0)
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    expect(store.getSnapshot().connected).toBe(true)
    store.dispose()
  })
})

describe(`registry wakeups`, () => {
  it(`the visibilitychange listener kicks every retained store`, () => {
    const store = acquireSteerSession(`wake-1`)
    const kick = vi.spyOn(store, `kick`)
    Object.defineProperty(document, `visibilityState`, {
      configurable: true,
      get: () => `visible`,
    })
    document.dispatchEvent(new Event(`visibilitychange`))
    expect(kick).toHaveBeenCalledWith(`visible`)

    // The pair is removed once the registry empties.
    disposeAllSteerSessions()
    kick.mockClear()
    document.dispatchEvent(new Event(`visibilitychange`))
    expect(kick).not.toHaveBeenCalled()
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

// EXP-648: the relay ticks a `keepalive` to every joined viewer every 15s, so
// a live socket that has been mute for three of them is dead (an OS-killed
// connection under a suspended tab that never delivered a close frame) —
// NOT an agent parked on a question or plan approval. Mirrors the mobile
// viewers' 45s window.
describe(`live staleness (EXP-648)`, () => {
  it(`keepalive frames keep a live socket fresh — a kick stays a no-op`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    await vi.advanceTimersByTimeAsync(40_000)
    socket.frame({ t: `keepalive` })
    await vi.advanceTimersByTimeAsync(40_000)
    // 80s since the join answer, 40s since the beat: fresh.
    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(1)
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    expect(store.getSnapshot().connected).toBe(true)
    expect(store.getSnapshot().feed).toHaveLength(0)
    store.dispose()
  })

  it(`redials silently once a live socket has been mute past the stale window`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `working` } })
    await vi.advanceTimersByTimeAsync(1_000)
    expect(store.getSnapshot().feed).toHaveLength(1)

    await vi.advanceTimersByTimeAsync(45_100)
    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(2)
    // The dead socket is CLOSED (never a duplicate viewer at the relay), the
    // phase holds so nothing flashes "Disconnected", the feed is retained.
    expect(sockets[0].closed).toBe(true)
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    expect(store.getSnapshot().feed).toHaveLength(1)

    // The dead socket's late close is inert (generation gate).
    sockets[0].serverClose(1006)
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    expect(sockets).toHaveLength(2)

    sockets[1].open()
    sockets[1].frame({ t: `activity_reset` })
    await vi.advanceTimersByTimeAsync(0)
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    expect(store.getSnapshot().connected).toBe(true)
    store.dispose()
  })

  it(`a second kick during the silent redial does not double-dial`, async () => {
    const { store, sockets } = makeStore()
    await goLive(store, sockets)
    await vi.advanceTimersByTimeAsync(45_100)
    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(2)

    // visible + online fire back to back: the young redial is left alone.
    store.kick(`online`)
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(2)
    // Opened + joined but not answered yet: still a young dial.
    sockets[1].open()
    store.kick(`device-online`)
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(2)

    // A redial the relay never answers is itself retried after the window.
    await vi.advanceTimersByTimeAsync(45_100)
    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(3)
    expect(sockets[1].closed).toBe(true)
    store.dispose()
  })
})

// EXP-648: a "no" a retry cannot turn into a "yes" must not cost one mint per
// visibilitychange/online event per retained store.
describe(`terminal closes (EXP-648)`, () => {
  function trpcError(code: string, message: string) {
    return new TRPCClientError(message, {
      result: {
        error: { message, code: -32003, data: { code, httpStatus: 403 } },
      },
    })
  }

  it(`a disabled instance is left alone by kicks but not by Reconnect`, async () => {
    const mint = vi.fn(() => Promise.resolve({ disabled: true as const }))
    const { store, sockets } = makeStore({ mint })
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    expect(store.getSnapshot().phase.kind).toBe(`closed`)
    store.kick(`visible`)
    store.kick(`online`)
    await vi.advanceTimersByTimeAsync(1_000)
    expect(mint).toHaveBeenCalledTimes(1)
    expect(sockets).toHaveLength(0)
    // The user asking is different from the tab merely waking up.
    store.reconnect()
    await vi.advanceTimersByTimeAsync(0)
    expect(mint).toHaveBeenCalledTimes(2)
    store.dispose()
  })

  it.each([`FORBIDDEN`, `NOT_FOUND`])(
    `a mint refused with %s is terminal`,
    async (code) => {
      const mint = vi.fn(() => Promise.reject(trpcError(code, `Nope`)))
      const { store } = makeStore({ mint })
      store.connect()
      await vi.advanceTimersByTimeAsync(0)
      expect(store.getSnapshot().phase).toMatchObject({
        kind: `closed`,
        detail: `Nope`,
      })
      store.kick(`visible`)
      store.kick(`online`)
      await vi.advanceTimersByTimeAsync(1_000)
      expect(mint).toHaveBeenCalledTimes(1)
      store.reconnect()
      await vi.advanceTimersByTimeAsync(0)
      expect(mint).toHaveBeenCalledTimes(2)
      store.dispose()
    }
  )

  it(`a mint that failed transiently stays retryable`, async () => {
    const mint = vi.fn(() => Promise.reject(new Error(`fetch failed`)))
    const { store } = makeStore({ mint })
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    expect(store.getSnapshot().phase.kind).toBe(`closed`)
    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(0)
    expect(mint).toHaveBeenCalledTimes(2)
    store.dispose()
  })

  it(`a 4003 close is terminal; other codes stay retryable`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.serverClose(4003)
    expect(store.getSnapshot().phase.kind).toBe(`closed`)
    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(1_000)
    expect(sockets).toHaveLength(1)
    store.reconnect()
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(2)
    store.dispose()
  })
})
