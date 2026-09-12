import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  acquireSteerSession,
  createSteerSessionStore,
  disposeAllSteerSessions,
  REPLAY_MAX_MS,
  REPLAY_QUIET_MS,
  type SteerSessionStore,
} from "@/lib/steer-session-store"
import { COMPACTION_TIMEOUT_MS, FEED_BYTE_CAP } from "@/lib/agent-feed"
import { MAX_STEER_IMAGES } from "@/lib/steer-image-message"
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

/** Connect and drive the newest socket to a joined, live state — the relay
 *  answers a join with `activity_reset` + (an empty) replay + the
 *  `activity_synced` marker (EXP-656). */
async function goLive(store: SteerSessionStore, sockets: FakeSocket[]) {
  store.connect()
  await vi.advanceTimersByTimeAsync(0)
  const socket = sockets[sockets.length - 1]
  socket.open()
  socket.frame({ t: `activity_reset` })
  socket.frame({ t: `activity_synced` })
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

  // EXP-783: a run far past the OLD 2000-event cap keeps every event — the
  // renderer windows, the store does not truncate.
  it(`applies activity frames and keeps the whole run`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    for (let i = 0; i < 5_000; i++) {
      socket.frame({
        t: `activity`,
        event: { kind: `narration`, text: `line ${i}` },
      })
    }
    await vi.advanceTimersByTimeAsync(1_000)
    const { feed } = store.getSnapshot()
    expect(feed).toHaveLength(5_000)
    expect(feed[0]).toMatchObject({ text: `line 0` })
    expect(feed[feed.length - 1]).toMatchObject({ text: `line 4999` })
    store.dispose()
  })

  // …bounded by BYTES instead: the oldest go, the newest always survives.
  it(`evicts the oldest rows once the byte budget is passed`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    const chunk = `x`.repeat(64 * 1024)
    for (let i = 0; i < 400; i++) {
      socket.frame({
        t: `activity`,
        event: { kind: `narration`, text: `${i}${chunk}` },
      })
    }
    await vi.advanceTimersByTimeAsync(1_000)
    const { feed } = store.getSnapshot()
    // 400 × 64 KiB is 25 MiB, past the 16 MiB budget.
    expect(feed.length).toBeLessThan(400)
    expect(feed.length).toBeGreaterThan(0)
    const bytes = feed.reduce(
      (sum, item) => sum + ((item as { text?: string }).text?.length ?? 0),
      0
    )
    expect(bytes).toBeLessThanOrEqual(FEED_BYTE_CAP)
    expect(feed[feed.length - 1]).toMatchObject({ text: `399${chunk}` })
    store.dispose()
  })

  // EXP-783: a replay that names its span keeps the pages the reader had
  // already scrolled back to load.
  it(`a replay that names its span keeps the prefix below it`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    for (let seq = 0; seq < 6; seq++) {
      socket.frame({
        t: `activity`,
        event: { kind: `narration`, text: `line ${seq}` },
        seq,
      })
    }
    await vi.advanceTimersByTimeAsync(1_000)
    const firstIds = store.getSnapshot().feed.slice(0, 3).map((item) => item.id)

    // The relay's log only reaches back to seq 3.
    socket.frame({ t: `activity_reset` })
    for (let seq = 3; seq < 8; seq++) {
      socket.frame({
        t: `activity`,
        event: { kind: `narration`, text: `line ${seq}` },
        seq,
      })
    }
    socket.frame({ t: `activity_synced`, firstSeq: 3, lastSeq: 7, truncated: true })
    await vi.advanceTimersByTimeAsync(1_000)

    const { feed, canLoadEarlier } = store.getSnapshot()
    expect(feed.map((item) => (item as { text: string }).text)).toEqual([
      `line 0`,
      `line 1`,
      `line 2`,
      `line 3`,
      `line 4`,
      `line 5`,
      `line 6`,
      `line 7`,
    ])
    // The retained rows kept their React keys, so nothing the reader had open
    // moved, and the ids stay strictly increasing across the seam.
    expect(feed.slice(0, 3).map((item) => item.id)).toEqual(firstIds)
    expect(feed.every((item, ix) => ix === 0 || feed[ix - 1].id < item.id)).toBe(true)
    // The relay said its log is a tail, so there is older transcript to ask
    // the device for.
    expect(canLoadEarlier).toBe(true)
    store.dispose()
  })

  // EXP-783: an older page lands in FRONT, and a re-asked page cannot double
  // the transcript.
  it(`loadEarlier prepends a history page exactly once`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    for (let seq = 5; seq < 8; seq++) {
      socket.frame({
        t: `activity`,
        event: { kind: `narration`, text: `line ${seq}` },
        seq,
      })
    }
    socket.frame({ t: `activity_synced`, firstSeq: 5, lastSeq: 7, truncated: true })
    await vi.advanceTimersByTimeAsync(1_000)

    expect(store.loadEarlier()).toBe(true)
    const ask = socket.sent
      .map((raw) => JSON.parse(raw))
      .find((frame) => frame.t === `history_page`)
    expect(ask).toMatchObject({ beforeSeq: 5, limit: 200 })

    const page = {
      t: `history_chunk` as const,
      requestId: ask.requestId,
      events: [2, 3, 4].map((seq) => ({
        kind: `narration` as const,
        text: `line ${seq}`,
      })),
      seqs: [2, 3, 4],
      done: true,
    }
    socket.frame(page)
    await vi.advanceTimersByTimeAsync(1_000)
    expect(
      store.getSnapshot().feed.map((item) => (item as { text: string }).text)
    ).toEqual([`line 2`, `line 3`, `line 4`, `line 5`, `line 6`, `line 7`])
    const feed = store.getSnapshot().feed
    expect(feed.every((item, ix) => ix === 0 || feed[ix - 1].id < item.id)).toBe(true)

    // The request is retired — a duplicate chunk for it changes nothing.
    socket.frame(page)
    await vi.advanceTimersByTimeAsync(1_000)
    expect(store.getSnapshot().feed).toHaveLength(6)
    store.dispose()
  })

  it(`an empty replay (activity_reset + activity_synced) clears the retained feed`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `hi` } })
    await vi.advanceTimersByTimeAsync(1_000)
    expect(store.getSnapshot().feed).toHaveLength(1)
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity_synced` })
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

  // EXP-772: the ACP coalescer flushes one assistant message as several
  // narration events sharing a `messageId`.
  it(`merges narration fragments that share a messageId`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    const fragment = (text: string, messageId?: string) => ({
      t: `activity`,
      event: { kind: `narration`, text, messageId },
    })
    socket.frame(fragment(`Looking`, `m1`))
    socket.frame(fragment(` at the code.`, `m1`))
    socket.frame(fragment(`Next message.`, `m2`))
    // No id at all (an older publisher): its own row, always.
    socket.frame(fragment(`Legacy line.`))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed.map((item) => (item as { text: string }).text)).toEqual([
      `Looking at the code.`,
      `Next message.`,
      `Legacy line.`,
    ])
    // A tool row in between ends the bubble — the fragment opens a new one.
    socket.frame({ t: `activity`, event: { kind: `tool`, name: `Read` } })
    socket.frame(fragment(` more`, `m2`))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed).toHaveLength(5)
    store.dispose()
  })

  // EXP-846: the look-back. A subagent spawned inside one assistant message
  // interleaves ITS rows between that message's flushes — the wire shape is
  // fragment, subagent tool call, its update, fragment. The bubble is one row.
  it(`merges narration fragments across a subagent's rows`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({
      t: `activity`,
      event: { kind: `narration`, text: `Reading`, messageId: `m1` },
    })
    socket.frame({
      t: `activity`,
      event: { kind: `tool`, name: `Grep`, id: `call-1`, subagentId: `sub-1` },
    })
    socket.frame({
      t: `activity`,
      event: { kind: `tool_update`, id: `call-1`, status: `completed` },
    })
    socket.frame({
      t: `activity`,
      event: { kind: `narration`, text: ` the code.`, messageId: `m1` },
    })
    await vi.advanceTimersByTimeAsync(100)
    const feed = store.getSnapshot().feed
    expect(feed).toHaveLength(2)
    expect(feed[0]).toMatchObject({
      kind: `narration`,
      text: `Reading the code.`,
      messageId: `m1`,
    })
    expect(feed[1]).toMatchObject({
      kind: `tool`,
      name: `Grep`,
      subagentId: `sub-1`,
      settled: true,
    })
    store.dispose()
  })

  // EXP-773: an ended run's transcript lives on the DEVICE. The relay parks
  // the viewer, asks the machine to republish its journal, and the feed
  // stays visible after the `bye`.
  it(`history_pending names the device and the history bye ends the run`, async () => {
    const { store, sockets } = makeStore()
    store.noteSessionStatus(`ended`)
    store.noteDeviceLabel(`buildbox`)
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `history_pending` })
    expect(store.getSnapshot().phase).toEqual({
      kind: `history_pending`,
      detail: `Fetching the transcript from buildbox…`,
    })
    // The republish streams like any replay — and never claims to be live.
    // EXP-796: the marker ends the loading phase on its own; the room
    // lingers open (no bye) so the reader can page.
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `did it` } })
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().phase).toEqual({ kind: `ended` })
    expect(store.getSnapshot().connected).toBe(true)
    expect(store.getSnapshot().feed).toHaveLength(1)
    socket.frame({ t: `bye`, outcome: `history` })
    socket.serverClose(4001)
    expect(store.getSnapshot().phase).toEqual({ kind: `ended`, detail: undefined })
    // The transcript stays on screen, read-only.
    expect(store.getSnapshot().feed).toHaveLength(1)
    store.dispose()
  })

  // EXP-796: the relay keeps a served history room open for minutes so
  // "Load earlier" pages keep flowing down the device's control socket; the
  // late `bye {history}` + 4001 is the linger expiring — terminal, never a
  // drop to redial from.
  it(`a served history room lingers for pages until its late history bye`, async () => {
    const { store, sockets } = makeStore()
    // A watched store: an UNWATCHED ended store frees its socket after the
    // ended grace, which is the last viewer leaving the room.
    const unsubscribe = store.subscribe(() => {})
    store.noteSessionStatus(`ended`)
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `history_pending` })
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `tail` }, seq: 40 })
    socket.frame({ t: `activity_synced`, firstSeq: 40, lastSeq: 40, truncated: true })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().phase.kind).toBe(`ended`)
    expect(store.getSnapshot().canLoadEarlier).toBe(true)
    // Minutes of keepalives: still ended, still pageable, no redial, and a
    // wakeup leaves the open socket alone.
    for (let i = 0; i < 8; i++) {
      await vi.advanceTimersByTimeAsync(15_000)
      socket.frame({ t: `keepalive` })
    }
    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(100)
    expect(sockets).toHaveLength(1)
    expect(store.getSnapshot().phase.kind).toBe(`ended`)
    expect(store.getSnapshot().connected).toBe(true)
    socket.sent.length = 0
    expect(store.loadEarlier()).toBe(true)
    expect(JSON.parse(socket.sent[0])).toMatchObject({ t: `history_page`, beforeSeq: 40 })
    socket.frame({
      t: `history_chunk`,
      requestId: `p1`,
      events: [{ kind: `narration`, text: `earlier` }],
      seqs: [39],
      done: true,
    })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed.map((i) => (i as { text: string }).text)).toEqual([
      `earlier`,
      `tail`,
    ])
    // The linger expires.
    socket.frame({ t: `bye`, outcome: `history` })
    socket.serverClose(4001)
    await vi.advanceTimersByTimeAsync(60_000)
    const snapshot = store.getSnapshot()
    expect(snapshot.phase).toEqual({ kind: `ended`, detail: undefined })
    expect(snapshot.feed).toHaveLength(2)
    expect(snapshot.canLoadEarlier).toBe(false)
    expect(store.loadEarlier()).toBe(false)
    expect(sockets).toHaveLength(1)
    unsubscribe()
    store.dispose()
  })

  it(`an unwatched served history store frees its socket after the ended grace`, async () => {
    const { store, sockets } = makeStore()
    store.noteSessionStatus(`ended`)
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `history_pending` })
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().phase.kind).toBe(`ended`)
    expect(socket.closed).toBe(false)
    await vi.advanceTimersByTimeAsync(120_000)
    expect(socket.closed).toBe(true)
  })

  it(`a device going offline mid-page ends the lingering room, terminally`, async () => {
    const { store, sockets } = makeStore()
    store.noteSessionStatus(`ended`)
    store.noteDeviceLabel(`buildbox`)
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `history_pending` })
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `tail` }, seq: 40 })
    socket.frame({ t: `activity_synced`, firstSeq: 40, lastSeq: 40, truncated: true })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.loadEarlier()).toBe(true)
    socket.frame({ t: `error`, code: `device_offline` })
    socket.frame({ t: `bye`, outcome: `device_offline` })
    socket.serverClose(4001)
    expect(store.getSnapshot().phase).toEqual({
      kind: `ended`,
      detail: `buildbox is offline. The transcript lives on that machine.`,
    })
    expect(store.getSnapshot().feed).toHaveLength(1)
    expect(store.getSnapshot().canLoadEarlier).toBe(false)
    store.kick(`test`)
    await vi.advanceTimersByTimeAsync(60_000)
    expect(sockets).toHaveLength(1)
    store.dispose()
  })

  // A run the synced row still calls alive: the served history is not an
  // ending, the publisher's takeover of the lingering room lands on the SAME
  // socket as an ordinary replay, and only then is the viewer live.
  it(`a served history on a live run waits in starting for the publisher's takeover`, async () => {
    const { store, sockets } = makeStore()
    store.noteSessionStatus(`running`)
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `history_pending` })
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `journal` } })
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().phase.kind).toBe(`starting`)
    expect(store.getSnapshot().connected).toBe(true)
    // No redial while the socket talks — a wakeup included.
    await vi.advanceTimersByTimeAsync(15_000)
    socket.frame({ t: `keepalive` })
    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(10_000)
    expect(sockets).toHaveLength(1)
    // The publisher arrives: the room replays from scratch and goes live.
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `live now` } })
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().phase.kind).toBe(`live`)
    expect(store.getSnapshot().feed.map((i) => (i as { text: string }).text)).toEqual([
      `live now`,
    ])
    store.dispose()
  })

  it(`a device_offline history error is terminal and never redials`, async () => {
    const { store, sockets } = makeStore()
    store.noteSessionStatus(`ended`)
    store.noteDeviceLabel(`buildbox`)
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `error`, code: `device_offline` })
    socket.serverClose(4001)
    expect(store.getSnapshot().phase).toEqual({
      kind: `closed`,
      detail: `buildbox is offline. The transcript lives on that machine.`,
      terminal: true,
    })
    // Terminal: neither the backoff nor a wakeup opens a second socket.
    store.kick(`test`)
    await vi.advanceTimersByTimeAsync(60_000)
    expect(sockets).toHaveLength(1)
    store.dispose()
  })

  // EXP-773: the relay parks EVERY join it has no live room for, so a run
  // whose publisher has not hello'd yet gets a device answer instead of
  // `no_such_session`. None of those answers is an ending while the synced
  // row says the run is alive.
  it(`a history bye on a live run goes back to starting and redials`, async () => {
    const { store, sockets } = makeStore()
    store.noteSessionStatus(`running`)
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `history_pending` })
    socket.frame({
      t: `activity`,
      event: { kind: `narration`, text: `partial` },
    })
    socket.frame({ t: `bye`, outcome: `history` })
    socket.serverClose(4001)
    expect(store.getSnapshot().phase.kind).toBe(`starting`)
    await vi.advanceTimersByTimeAsync(5_000)
    expect(sockets.length).toBeGreaterThan(1)
    store.dispose()
  })

  it(`a history_unavailable on a live run redials too`, async () => {
    const { store, sockets } = makeStore()
    store.noteSessionStatus(`running`)
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `history_pending` })
    socket.frame({ t: `error`, code: `history_unavailable` })
    socket.frame({ t: `bye`, outcome: `history_unavailable` })
    socket.serverClose(4001)
    expect(store.getSnapshot().phase.kind).toBe(`starting`)
    await vi.advanceTimersByTimeAsync(5_000)
    expect(sockets.length).toBeGreaterThan(1)
    store.dispose()
  })

  // The relay's timeout sends the error frame and THEN closes the room with
  // the same code as the bye outcome — the human caption must survive it.
  it(`the history timeout keeps its caption instead of the raw outcome`, async () => {
    const { store, sockets } = makeStore()
    store.noteSessionStatus(`ended`)
    store.noteDeviceLabel(`buildbox`)
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `history_pending` })
    socket.frame({ t: `error`, code: `history_unavailable` })
    socket.frame({ t: `bye`, outcome: `history_unavailable` })
    socket.serverClose(4001)
    expect(store.getSnapshot().phase).toEqual({
      kind: `ended`,
      detail: `No transcript on buildbox.`,
    })
    store.dispose()
  })

  it(`history_unavailable says so, without a device name when none is known`, async () => {
    const { store, sockets } = makeStore()
    store.noteSessionStatus(`ended`)
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `error`, code: `history_unavailable` })
    socket.serverClose(4001)
    expect(store.getSnapshot().phase).toEqual({
      kind: `closed`,
      detail: `No transcript on the device.`,
      terminal: true,
    })
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

  // EXP-781: the one ENDED case that must retry. A history replay is the
  // device pushing a whole journal in a burst, which is exactly what overruns
  // the relay's per-viewer buffer — and the staged feed is dropped on close,
  // so landing in `closed` (where `kick()` refuses to recover) strands the
  // reader on an empty transcript with no way back.
  it(`an ended run redials when the eviction hit a history replay`, async () => {
    const { store, sockets } = makeStore()
    store.noteSessionStatus(`ended`)
    store.noteDeviceLabel(`buildbox`)
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `history_pending` })
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `half` } })
    socket.serverClose(4008)

    // Not terminal, and the redial is on the shared jittered backoff.
    expect(store.getSnapshot().phase.kind).not.toBe(`closed`)
    await vi.advanceTimersByTimeAsync(3_100)
    expect(sockets).toHaveLength(2)

    // The retry replays from scratch, so the half-delivered staging being
    // discarded costs nothing.
    sockets[1].open()
    sockets[1].frame({ t: `history_pending` })
    sockets[1].frame({ t: `activity_reset` })
    sockets[1].frame({
      t: `activity`,
      event: { kind: `narration`, text: `whole` },
    })
    sockets[1].frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(
      store.getSnapshot().feed.map((item) => (item as { text: string }).text)
    ).toEqual([`whole`])
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
      added: 0,
    })
    const many = [1, 2, 3, 4, 5].map((n) => image(`${n}.png`))
    const result = store.addDraftImages(many)
    expect(result.rejected).toBe(0)
    expect(result.overflow).toBeGreaterThan(0)
    // `added` is what the composer numbers its `[Image #N]` markers from
    // (EXP-698) — the over-cap extras never joined the strip.
    expect(result.added).toBe(MAX_STEER_IMAGES)
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

// EXP-672: answers go out ONLY as the semantic `answer` frame, naming the
// card's wire id — the legacy raw-keystroke path (and its multi-select
// toggle) is gone.
describe(`answering questions`, () => {
  const card = (questionId: string) =>
    ({
      id: 1,
      kind: `question` as const,
      text: `Which approach?`,
      options: [{ label: `Refactor`, key: `1` }],
      multiSelect: false,
      planMode: false,
      questionId,
    })

  it(`sends one answer frame and locks the card`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.sent.length = 0
    store.answerQuestion(card(`tu_1`), [`1`], [`Refactor`])
    expect(socket.sent.map((s) => JSON.parse(s))).toEqual([
      { t: `answer`, questionId: `tu_1`, keys: [`1`] },
    ])
    expect(store.getSnapshot().answerStates[`tu_1`]).toMatchObject({
      status: `sending`,
      labels: [`Refactor`],
    })
    store.dispose()
  })

  it(`a locked card never fires twice`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    store.answerQuestion(card(`tu_1`), [`1`], [`Refactor`])
    socket.sent.length = 0
    store.answerQuestion(card(`tu_1`), [`1`], [`Refactor`])
    expect(socket.sent).toEqual([])
    store.dispose()
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

  // EXP-639: an ended run has no publisher left, so the silent redial can
  // only draw `no_such_session` and park the viewer in `starting` until the
  // row syncs. Same rule the `closed` case already applies.
  it(`never redials a run the synced row already calls ended`, async () => {
    const { store, sockets } = makeStore()
    await goLive(store, sockets)
    store.noteSessionStatus(`ended`)
    await vi.advanceTimersByTimeAsync(45_100)
    store.kick(`visible`)
    await vi.advanceTimersByTimeAsync(0)
    expect(sockets).toHaveLength(1)
    expect(store.getSnapshot().phase.kind).toBe(`live`)
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

// EXP-724: the compaction strip. `started` opens it, `ended` closes it AND
// writes the persistent marker, and nothing may leave it running forever —
// the agent visibly resuming, a feed reset, the session ending or the
// backstop all take it down.
describe(`compaction`, () => {
  const compaction = (
    phase: `started` | `ended`,
    over: Record<string, unknown> = {}
  ) => ({ t: `activity`, event: { kind: `compaction`, phase, ...over } })

  it(`started opens the strip and ended closes it with a marker row`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(compaction(`started`, { trigger: `manual` }))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().compacting).toMatchObject({ trigger: `manual` })
    expect(store.getSnapshot().feed).toEqual([])
    socket.frame(compaction(`ended`))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().compacting).toBeNull()
    expect(store.getSnapshot().feed).toEqual([{ id: 0, kind: `compaction` }])
    store.dispose()
  })

  it(`a bare ended still writes the marker (codex auto-compaction)`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(compaction(`ended`))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().compacting).toBeNull()
    expect(store.getSnapshot().feed).toEqual([{ id: 0, kind: `compaction` }])
    store.dispose()
  })

  it(`the agent resuming clears a strip whose ended marker never came`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(compaction(`started`))
    await vi.advanceTimersByTimeAsync(100)
    socket.frame({
      t: `activity`,
      event: { kind: `narration`, text: `Back at it` },
    })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().compacting).toBeNull()
    // No marker row — the fold never reported an end.
    expect(store.getSnapshot().feed).toEqual([
      { id: 0, kind: `narration`, text: `Back at it` },
    ])
    store.dispose()
  })

  it(`a human turn or a diff leaves the strip standing`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(compaction(`started`))
    socket.frame({ t: `activity`, event: { kind: `user_message`, text: `hi` } })
    socket.frame({ t: `activity`, event: { kind: `diff`, diff: `--- a` } })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().compacting).not.toBeNull()
    store.dispose()
  })

  it(`the backstop expires the strip after COMPACTION_TIMEOUT_MS`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(compaction(`started`))
    await vi.advanceTimersByTimeAsync(100)
    await vi.advanceTimersByTimeAsync(COMPACTION_TIMEOUT_MS - 1_000)
    expect(store.getSnapshot().compacting).not.toBeNull()
    await vi.advanceTimersByTimeAsync(2_000)
    expect(store.getSnapshot().compacting).toBeNull()
    // An expiry is not a completion: no marker row.
    expect(store.getSnapshot().feed).toEqual([])
    store.dispose()
  })

  it(`a replayed started from long ago expires at once`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(
      compaction(`started`, { at: Date.now() - COMPACTION_TIMEOUT_MS * 2 })
    )
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().compacting).toBeNull()
    store.dispose()
  })

  it(`a committed replay that carries no started drops the strip`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(compaction(`started`))
    await vi.advanceTimersByTimeAsync(100)
    socket.frame({ t: `activity_reset` })
    // Staged, not applied (EXP-751): the strip holds until the replay lands.
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().compacting).not.toBeNull()
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().compacting).toBeNull()
    store.dispose()
  })

  it(`a session that ends mid-fold drops the strip`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(compaction(`started`))
    await vi.advanceTimersByTimeAsync(100)
    socket.frame({ t: `bye`, outcome: `ended` })
    socket.serverClose(1000)
    expect(store.getSnapshot().phase.kind).toBe(`ended`)
    expect(store.getSnapshot().compacting).toBeNull()
    store.dispose()
  })
})

// EXP-785/786: `tool_update` folds INTO its tool row by call id — settle
// state and the per-call diff — and is never a row of its own.
describe(`tool_update folds into the tool row (EXP-785/786)`, () => {
  const toolEvent = (id: string, over: Record<string, unknown> = {}) => ({
    t: `activity`,
    event: {
      kind: `tool`,
      name: `Edit`,
      detail: `src/a.ts`,
      id,
      toolKind: `edit`,
      ...over,
    },
  })
  const updateEvent = (id: string, over: Record<string, unknown> = {}) => ({
    t: `activity`,
    event: { kind: `tool_update`, id, ...over },
  })

  it(`a tool row carries its callId and toolKind`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(toolEvent(`tc-1`))
    socket.frame(toolEvent(`tc-2`, { toolKind: `teleport`, id: `  ` }))
    await vi.advanceTimersByTimeAsync(100)
    const feed = store.getSnapshot().feed
    expect(feed[0]).toMatchObject({
      kind: `tool`,
      name: `Edit`,
      callId: `tc-1`,
      toolKind: `edit`,
    })
    // An unknown kind and a blank id are dropped, not kept as junk.
    expect(feed[1]).toMatchObject({ kind: `tool`, name: `Edit` })
    expect((feed[1] as { callId?: string }).callId).toBeUndefined()
    expect((feed[1] as { toolKind?: string }).toolKind).toBeUndefined()
    store.dispose()
  })

  it(`a settle and a diff fold into the row; a failed settle wins`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(toolEvent(`tc-1`))
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `between` } })
    socket.frame(toolEvent(`tc-2`, { toolKind: `execute`, name: `Bash` }))
    socket.frame(
      updateEvent(`tc-1`, {
        status: `completed`,
        diff: `--- a/src/a.ts\n+++ b/src/a.ts\n@@ -1 +1 @@\n-a\n+b\n`,
      })
    )
    await vi.advanceTimersByTimeAsync(100)
    let feed = store.getSnapshot().feed
    expect(feed).toHaveLength(3)
    expect(feed[0]).toMatchObject({
      kind: `tool`,
      callId: `tc-1`,
      settled: true,
      failed: false,
      diff: `--- a/src/a.ts\n+++ b/src/a.ts\n@@ -1 +1 @@\n-a\n+b\n`,
    })
    expect((feed[2] as { settled?: boolean }).settled).toBeUndefined()

    socket.frame(updateEvent(`tc-1`, { status: `failed` }))
    // A status-less update carrying only a diff never settles.
    socket.frame(updateEvent(`tc-2`, { diff: `+x\n` }))
    await vi.advanceTimersByTimeAsync(100)
    feed = store.getSnapshot().feed
    expect(feed[0]).toMatchObject({ settled: true, failed: true })
    expect((feed[0] as { diff?: string }).diff).toContain(`+b`)
    expect(feed[2]).toMatchObject({ diff: `+x\n` })
    expect((feed[2] as { settled?: boolean }).settled).toBeUndefined()
    store.dispose()
  })

  it(`an update for an unknown or evicted id is dropped`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(toolEvent(`tc-1`))
    socket.frame({ t: `activity`, event: { kind: `tool`, name: `Grep` } })
    socket.frame(updateEvent(`tc-nope`, { status: `failed`, diff: `+never\n` }))
    socket.frame(updateEvent(``, { status: `completed` }))
    await vi.advanceTimersByTimeAsync(100)
    const feed = store.getSnapshot().feed
    expect(feed).toHaveLength(2)
    expect((feed[0] as { settled?: boolean }).settled).toBeUndefined()
    expect((feed[1] as { settled?: boolean }).settled).toBeUndefined()
    store.dispose()
  })

  it(`folds into the NEWEST row with that id`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(toolEvent(`tc-1`))
    socket.frame(toolEvent(`tc-1`))
    socket.frame(updateEvent(`tc-1`, { status: `completed` }))
    await vi.advanceTimersByTimeAsync(100)
    const feed = store.getSnapshot().feed
    expect((feed[0] as { settled?: boolean }).settled).toBeUndefined()
    expect(feed[1]).toMatchObject({ settled: true, failed: false })
    store.dispose()
  })

  it(`a replayed update still folds after the staged commit`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({ t: `activity_reset` })
    socket.frame(toolEvent(`tc-1`))
    socket.frame(updateEvent(`tc-1`, { status: `failed` }))
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed).toHaveLength(1)
    expect(store.getSnapshot().feed[0]).toMatchObject({ callId: `tc-1`, failed: true })
    store.dispose()
  })
})

// EXP-784: `rate_limit` is the fourth latest-wins slot; an empty/`ok` status
// clears it.
describe(`rate_limit slot (EXP-784)`, () => {
  const rateLimitEvent = (over: Record<string, unknown> = {}) => ({
    t: `activity`,
    event: {
      kind: `rate_limit`,
      status: `allowed_warning`,
      resetsAt: 1_700_000_000_000,
      message: `80% of the 5-hour window`,
      ...over,
    },
  })

  it(`lands in the snapshot as a slot, never a feed row, and clears on ok or empty`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(rateLimitEvent())
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed).toEqual([])
    expect(store.getSnapshot().rateLimit).toEqual({
      status: `allowed_warning`,
      resetsAt: 1_700_000_000_000,
      message: `80% of the 5-hour window`,
    })
    socket.frame(rateLimitEvent({ status: `rejected`, resetsAt: undefined, message: `  ` }))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().rateLimit).toEqual({ status: `rejected` })
    socket.frame(rateLimitEvent({ status: `ok` }))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().rateLimit).toBeNull()
    socket.frame(rateLimitEvent({ status: `rejected` }))
    socket.frame(rateLimitEvent({ status: `` }))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().rateLimit).toBeNull()
    store.dispose()
  })

  it(`a committed replay repaints the slot from the staged events, or drops it`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(rateLimitEvent({ status: `rejected` }))
    await vi.advanceTimersByTimeAsync(100)
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `replayed` } })
    socket.frame(rateLimitEvent())
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().rateLimit?.status).toBe(`allowed_warning`)
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `again` } })
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().rateLimit).toBeNull()
    store.dispose()
  })
})

// EXP-848: `turn` is the FIFTH latest-wins slot — idle (`ended`) by default so
// nothing pulses before the first edge, and replayed so a late joiner learns it.
describe(`turn slot (EXP-848)`, () => {
  const turnEvent = (state: string) => ({
    t: `activity`,
    event: { kind: `turn`, state, at: 1_700_000_000_000 },
  })

  it(`defaults to ended, folds both edges as a slot, and never appends a row`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    expect(store.getSnapshot().turnState).toBe(`ended`)
    socket.frame(turnEvent(`started`))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().turnState).toBe(`started`)
    expect(store.getSnapshot().feed).toEqual([])
    socket.frame(turnEvent(`ended`))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().turnState).toBe(`ended`)
    expect(store.getSnapshot().feed).toEqual([])
    store.dispose()
  })

  it(`keeps the current state on an unreadable payload`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(turnEvent(`started`))
    socket.frame(turnEvent(`sideways`))
    socket.frame({ t: `activity`, event: { kind: `turn` } })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().turnState).toBe(`started`)
    store.dispose()
  })

  it(`an older history page never repaints the slot`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `live` }, seq: 10 })
    socket.frame(turnEvent(`ended`))
    socket.frame({ t: `activity_synced`, truncated: true, firstSeq: 10 })
    await vi.advanceTimersByTimeAsync(100)
    store.loadEarlier()
    const request = JSON.parse(
      socket.sent[socket.sent.length - 1]!
    ) as { requestId: string }
    socket.frame({
      t: `history_chunk`,
      requestId: request.requestId,
      done: true,
      events: [
        { kind: `narration`, text: `older` },
        { kind: `turn`, state: `started` },
      ],
      seqs: [1, 2],
    })
    await vi.advanceTimersByTimeAsync(100)
    // The page DID land (otherwise the assertion below would pass vacuously).
    expect(
      store.getSnapshot().feed.map((item) => (item as { text?: string }).text)
    ).toEqual([`older`, `live`])
    expect(store.getSnapshot().turnState).toBe(`ended`)
    store.dispose()
  })

  it(`a replay repaints the slot, and a replay without one is idle`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(turnEvent(`started`))
    await vi.advanceTimersByTimeAsync(100)
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `replayed` } })
    socket.frame(turnEvent(`started`))
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().turnState).toBe(`started`)
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `again` } })
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().turnState).toBe(`ended`)
    store.dispose()
  })
})

// EXP-847/846: the two optional fields the reducer stores on its rows — the
// subagent's title and an Exponential MCP tool's result preview.
describe(`subagent title + tool preview (EXP-847/846)`, () => {
  it(`keeps a subagent's title beside its agentType`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({
      t: `activity`,
      event: {
        kind: `subagent`,
        id: `sub-1`,
        agentType: `general-purpose`,
        title: `  Audit the launcher  `,
        status: `started`,
      },
    })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed[0]).toMatchObject({
      kind: `subagent`,
      agentType: `general-purpose`,
      title: `Audit the launcher`,
    })
    store.dispose()
  })

  it(`folds a tool_update preview onto the tool row, clamped`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({
      t: `activity`,
      event: { kind: `tool`, name: `exponential_issues_create`, id: `call-1` },
    })
    socket.frame({
      t: `activity`,
      event: {
        kind: `tool_update`,
        id: `call-1`,
        status: `completed`,
        preview: {
          identifier: `EXP-848`,
          title: `x`.repeat(250),
          count: 3.4,
          url: `   `,
        },
      },
    })
    await vi.advanceTimersByTimeAsync(100)
    const row = store.getSnapshot().feed[0] as { preview?: Record<string, unknown> }
    expect(row.preview).toEqual({
      identifier: `EXP-848`,
      title: `x`.repeat(200),
      count: 3,
    })
    store.dispose()
  })

  it(`ignores a preview that carried nothing usable`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({
      t: `activity`,
      event: { kind: `tool`, name: `exponential_boards_list`, id: `call-2` },
    })
    socket.frame({
      t: `activity`,
      event: { kind: `tool_update`, id: `call-2`, preview: { count: -1 } },
    })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed[0]).not.toHaveProperty(`preview`)
    store.dispose()
  })
})

// EXP-746: `config_state` and `usage` are latest-wins SLOTS on the snapshot,
// and the two outbound frames that change them are fire-and-forget.
describe(`live config + usage (EXP-746)`, () => {
  const configEvent = (over: Record<string, unknown> = {}) => ({
    t: `activity`,
    event: {
      kind: `config_state`,
      modes: [
        { id: `plan`, label: `Plan` },
        { id: `bypassPermissions`, label: `Build` },
      ],
      currentMode: `plan`,
      commands: [{ name: `review`, description: `Review the diff` }],
      ...over,
    },
  })
  const usageEvent = (over: Record<string, unknown> = {}) => ({
    t: `activity`,
    event: {
      kind: `usage`,
      contextUsed: 124_000,
      contextSize: 200_000,
      costUsd: 1.24,
      ...over,
    },
  })

  it(`config_state lands in the snapshot as a slot, never a feed row`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(configEvent())
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed).toEqual([])
    expect(store.getSnapshot().config).toEqual({
      modes: [
        { id: `plan`, label: `Plan` },
        { id: `bypassPermissions`, label: `Build` },
      ],
      commands: [{ name: `review`, description: `Review the diff` }],
      currentMode: `plan`,
    })
    store.dispose()
  })

  it(`a newer config_state replaces the whole snapshot`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(configEvent())
    socket.frame(
      configEvent({
        modes: [],
        currentMode: undefined,
        commands: undefined,
      })
    )
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().config).toEqual({
      modes: [],
      commands: [],
    })
    store.dispose()
  })

  it(`a malformed config_state leaves the previous snapshot standing`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(configEvent())
    await vi.advanceTimersByTimeAsync(100)
    const before = store.getSnapshot().config
    socket.frame({ t: `activity`, event: { kind: `config_state` } })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().config).toBe(before)
    store.dispose()
  })

  it(`usage lands in the snapshot and a zero context size clears it`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(usageEvent())
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().usage).toEqual({
      contextUsed: 124_000,
      contextSize: 200_000,
      costUsd: 1.24,
    })
    expect(store.getSnapshot().feed).toEqual([])
    // "Unknown", not "empty" — a stale meter beside a live run reads as
    // current, so the slot goes rather than standing.
    socket.frame(usageEvent({ contextUsed: 0, contextSize: 0 }))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().usage).toBeNull()
    store.dispose()
  })

  it(`a committed replay that carries neither drops the config and usage slots`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(configEvent())
    socket.frame(usageEvent())
    await vi.advanceTimersByTimeAsync(100)
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().config).toBeNull()
    expect(store.getSnapshot().usage).toBeNull()
    store.dispose()
  })

  // EXP-772: the mode is the ONLY thing the composer switches now — the
  // `set_config` sender went with the option chips.
  it(`setMode sends one set_mode frame and nothing else`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    const before = socket.sent.length
    expect(store.setMode(`plan`)).toBe(true)
    expect(socket.sent.slice(before)).toEqual([
      JSON.stringify({ t: `set_mode`, id: `plan` }),
    ])
    // Fire-and-forget: no optimistic slot write, the re-emission repaints.
    expect(store.getSnapshot().config).toBeNull()
    store.dispose()
  })

  // EXP-790: the composer's Stop glyph — one bare `interrupt` frame, no
  // local state (the publisher cancels the turn and the feed shows it).
  it(`interrupt sends one interrupt frame and nothing else`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    const before = socket.sent.length
    expect(store.interrupt()).toBe(true)
    expect(socket.sent.slice(before)).toEqual([JSON.stringify({ t: `interrupt` })])
    expect(store.getSnapshot().feed).toEqual([])
    store.dispose()
  })

  it(`interrupt on a closed socket returns false and sends nothing`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.serverClose(1006)
    const before = socket.sent.length
    expect(store.interrupt()).toBe(false)
    expect(socket.sent).toHaveLength(before)
    store.dispose()
  })

  it(`setMode on a closed socket returns false and sends nothing`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.serverClose(1006)
    const before = socket.sent.length
    expect(store.setMode(`plan`)).toBe(false)
    expect(socket.sent).toHaveLength(before)
    store.dispose()
  })

  it(`a future activity kind is still ignored`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame({ t: `activity`, event: { kind: `telemetry`, value: 7 } })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed).toEqual([])
    expect(store.getSnapshot().config).toBeNull()
    expect(store.getSnapshot().usage).toBeNull()
    store.dispose()
  })
})

// EXP-656 (natives) → EXP-751 (web): the relay answers EVERY viewer join with
// `activity_reset` + a full replay of the room log, and a publisher reconnect
// fans out the same pair. Applying that literally emptied the feed and then
// painted the rows as they streamed, so a reconnect visibly rebuilt the feed
// and blanked the chips and the usage line until their replayed snapshots
// landed. The replay is staged and swapped in as ONE commit instead. Test
// names mirror Android `SteerConnectionTest` / iOS `SteerReplayStagingTests`.
describe(`replay staging (EXP-751)`, () => {
  const narration = (text: string) => ({
    t: `activity`,
    event: { kind: `narration`, text },
  })
  const configFrame = {
    t: `activity`,
    event: {
      kind: `config_state`,
      modes: [
        { id: `plan`, label: `Plan` },
        { id: `bypassPermissions`, label: `Build` },
      ],
      currentMode: `plan`,
    },
  }
  const usageFrame = {
    t: `activity`,
    event: { kind: `usage`, contextUsed: 10_000, contextSize: 200_000 },
  }
  const questionFrame = {
    t: `activity`,
    event: {
      kind: `question`,
      id: `q1`,
      text: `Approve the plan?`,
      options: [{ label: `Yes`, key: `1` }],
    },
  }
  const texts = (store: SteerSessionStore) =>
    store.getSnapshot().feed.map((item) =>
      item.kind === `narration` || item.kind === `user_message` ? item.text : ``
    )

  /** A live connection with an established feed — the state a reader parked
   *  mid-plan is in when the relay decides to replay at them. */
  async function liveWithFeed() {
    const made = makeStore()
    const socket = await goLive(made.store, made.sockets)
    socket.frame(narration(`original one`))
    await vi.advanceTimersByTimeAsync(100)
    expect(made.store.getSnapshot().feed).toHaveLength(1)
    return { ...made, socket }
  }

  it(`a reset and replay commits once and never shows an empty feed`, async () => {
    const { store, socket } = await liveWithFeed()
    const sizes: number[] = []
    const unsubscribe = store.subscribe(() => {
      sizes.push(store.getSnapshot().feed.length)
    })

    socket.frame({ t: `activity_reset` })
    socket.frame(narration(`replayed one`))
    socket.frame(narration(`replayed two`))
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(REPLAY_QUIET_MS * 3)
    unsubscribe()

    // One notify for the whole burst, and never an empty feed in between
    // (which is what repainted the rows one by one).
    expect(sizes).toEqual([2])
    expect(texts(store)).toEqual([`replayed one`, `replayed two`])
    // The replayed prefix keeps its ids, so the React keys still line up.
    expect(store.getSnapshot().feed.map((item) => item.id)).toEqual([0, 1])
    store.dispose()
  })

  it(`the visible feed holds while a replay is staging`, async () => {
    const { store, socket } = await liveWithFeed()
    socket.frame({ t: `activity_reset` })
    socket.frame(narration(`replayed one`))
    await vi.advanceTimersByTimeAsync(100)
    // Still the OLD feed: nothing painted until the marker.
    expect(texts(store)).toEqual([`original one`])
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(texts(store)).toEqual([`replayed one`])
    store.dispose()
  })

  it(`a replay with no end marker commits on the quiet timeout`, async () => {
    const { store, socket } = await liveWithFeed()
    // An old relay: reset + replay, no activity_synced.
    socket.frame({ t: `activity_reset` })
    socket.frame(narration(`replayed one`))
    await vi.advanceTimersByTimeAsync(100)
    expect(texts(store)).toEqual([`original one`])
    await vi.advanceTimersByTimeAsync(REPLAY_QUIET_MS)
    expect(texts(store)).toEqual([`replayed one`])
    store.dispose()
  })

  it(`events arriving during staging land in the committed feed`, async () => {
    const { store, socket } = await liveWithFeed()
    socket.frame({ t: `activity_reset` })
    socket.frame(narration(`replayed one`))
    // A genuinely new event racing the tail of the replay is
    // indistinguishable on the wire — it must not be dropped.
    socket.frame(narration(`live during replay`))
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(texts(store)).toEqual([`replayed one`, `live during replay`])
    store.dispose()
  })

  it(`a keepalive ends a staged replay`, async () => {
    const { store, socket } = await liveWithFeed()
    const before = store.getSnapshot()
    socket.frame({ t: `activity_reset` })
    socket.frame(narration(`replayed one`))
    await vi.advanceTimersByTimeAsync(100)
    // The relay's own 15s beat proves the burst is over — long before the
    // quiet window would have.
    socket.frame({ t: `keepalive` })
    await vi.advanceTimersByTimeAsync(100)
    expect(texts(store)).toEqual([`replayed one`])
    // Outside a replay a keepalive stays inert: no commit, same snapshot.
    const committed = store.getSnapshot()
    expect(committed).not.toBe(before)
    socket.frame({ t: `keepalive` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot()).toBe(committed)
    store.dispose()
  })

  it(`a never-quiet replay commits at the hard cap`, async () => {
    const { store, socket } = await liveWithFeed()
    socket.frame({ t: `activity_reset` })
    // Quiet can never fire: something arrives every few ms.
    let n = 0
    const pump = setInterval(() => socket.frame(narration(`replayed ${n++}`)), 5)
    await vi.advanceTimersByTimeAsync(REPLAY_MAX_MS - 100)
    expect(texts(store)).toEqual([`original one`])
    await vi.advanceTimersByTimeAsync(200)
    clearInterval(pump)
    const committed = store.getSnapshot().feed.length
    expect(committed).toBeGreaterThan(1)
    expect(texts(store)[0]).toBe(`replayed 0`)
    // …and the stream keeps appending normally afterwards.
    socket.frame(narration(`after the cap`))
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed.length).toBeGreaterThan(committed)
    expect(texts(store).at(-1)).toBe(`after the cap`)
    store.dispose()
  })

  it(`a socket close during staging keeps the visible feed`, async () => {
    const { store, socket } = await liveWithFeed()
    socket.frame({ t: `activity_reset` })
    socket.frame(narration(`half delivered`))
    await vi.advanceTimersByTimeAsync(100)
    socket.serverClose(1006)
    await vi.advanceTimersByTimeAsync(REPLAY_MAX_MS + 100)
    // A half-delivered replay is worth less than the last complete picture
    // — the reader keeps what they were reading.
    expect(texts(store)).toEqual([`original one`])
    expect(store.getSnapshot().phase.kind).toBe(`closed`)
    store.dispose()
  })

  it(`a second reset restarts staging`, async () => {
    const { store, socket } = await liveWithFeed()
    socket.frame({ t: `activity_reset` })
    socket.frame(narration(`abandoned`))
    // The publisher republished mid-replay: the first buffer is dead.
    socket.frame({ t: `activity_reset` })
    socket.frame(narration(`restarted`))
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(texts(store)).toEqual([`restarted`])
    store.dispose()
  })

  it(`the marker commits only while staging`, async () => {
    const { store, socket } = await liveWithFeed()
    const before = store.getSnapshot()
    // An `activity_synced` outside a replay (a relay we joined before the
    // window opened) has nothing to commit — never a feed change.
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed).toBe(before.feed)
    store.dispose()
  })

  it(`an answer sent during staging keeps its lock`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(questionFrame)
    await vi.advanceTimersByTimeAsync(100)
    const card = store.getSnapshot().feed[0]
    expect(card.kind).toBe(`question`)

    // The replay window is ≤400ms in production — a plan-approval click
    // lands inside it more often than one would like, and its card must
    // not come back unlocked (a double-click would re-answer the ask).
    socket.frame({ t: `activity_reset` })
    await vi.advanceTimersByTimeAsync(100)
    if (card.kind === `question`) store.answerQuestion(card, [`1`], [`Yes`])
    expect(store.getSnapshot().answerStates[`q1`]).toMatchObject({
      status: `sending`,
    })
    socket.frame(questionFrame)
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().feed).toHaveLength(1)
    expect(store.getSnapshot().answerStates[`q1`]).toMatchObject({
      status: `sending`,
      labels: [`Yes`],
    })
    store.dispose()
  })

  it(`a lock whose card the replay did not bring back is released`, async () => {
    const { store, sockets } = makeStore()
    const socket = await goLive(store, sockets)
    socket.frame(questionFrame)
    await vi.advanceTimersByTimeAsync(100)
    const card = store.getSnapshot().feed[0]
    if (card.kind === `question`) store.answerQuestion(card, [`1`], [`Yes`])
    socket.frame({ t: `activity_reset` })
    socket.frame(narration(`no card here`))
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().answerStates).toEqual({})
    // …and its ack deadline with it: nothing flips to the retry state later.
    await vi.advanceTimersByTimeAsync(10_000)
    expect(store.getSnapshot().answerStates).toEqual({})
    store.dispose()
  })

  it(`a message sent during staging survives the commit`, async () => {
    const { store, socket } = await liveWithFeed()
    socket.frame({ t: `activity_reset` })
    await vi.advanceTimersByTimeAsync(100)
    // The replay predates this message, so only the local record of it can
    // put it back.
    expect(store.sendMessage(`steered mid-replay`)).toBe(true)
    socket.frame(narration(`replayed one`))
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(texts(store)).toEqual([`replayed one`, `steered mid-replay`])
    // Its transcript-derived twin, arriving live later, still dedupes.
    socket.frame({
      t: `activity`,
      event: { kind: `user_message`, text: `steered mid-replay` },
    })
    await vi.advanceTimersByTimeAsync(100)
    expect(texts(store)).toEqual([`replayed one`, `steered mid-replay`])
    store.dispose()
  })

  it(`a message the replay carries back is not shown twice`, async () => {
    const { store, socket } = await liveWithFeed()
    socket.frame({ t: `activity_reset` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.sendMessage(`steered mid-replay`)).toBe(true)
    socket.frame(narration(`replayed one`))
    // The desktop published the message before the burst ended.
    socket.frame({
      t: `activity`,
      event: { kind: `user_message`, text: `steered mid-replay` },
    })
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(texts(store)).toEqual([`replayed one`, `steered mid-replay`])
    store.dispose()
  })

  it(`a staged replay re-derives config, usage and the diff on commit`, async () => {
    const { store, socket } = await liveWithFeed()
    socket.frame(configFrame)
    socket.frame(usageFrame)
    socket.frame({ t: `activity`, event: { kind: `diff`, diff: `--- a` } })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().config).not.toBeNull()

    // Every viewer join triggers a replay: the commit folds from a FRESH
    // state, so the slots must come back off the replayed snapshots in the
    // SAME commit as the rows — never blank in between.
    const seen: Array<[number, string | undefined, number | undefined, string | null]> = []
    const unsubscribe = store.subscribe(() => {
      const snap = store.getSnapshot()
      seen.push([
        snap.feed.length,
        snap.config?.currentMode,
        snap.usage?.contextSize,
        snap.latestDiff,
      ])
    })
    socket.frame({ t: `activity_reset` })
    socket.frame(narration(`replayed one`))
    socket.frame({
      t: `activity`,
      event: { kind: `usage`, contextUsed: 20_000, contextSize: 100_000 },
    })
    socket.frame(configFrame)
    socket.frame({ t: `activity`, event: { kind: `diff`, diff: `--- b` } })
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    unsubscribe()
    expect(seen).toEqual([[1, `plan`, 100_000, `--- b`]])

    // A replay that carries none of them leaves the slots empty — they are
    // state derived from the log, not a sticky client cache.
    socket.frame({ t: `activity_reset` })
    socket.frame(narration(`replayed two`))
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().config).toBeNull()
    expect(store.getSnapshot().usage).toBeNull()
    expect(store.getSnapshot().latestDiff).toBeNull()
    store.dispose()
  })

  it(`a replayed compaction start re-arms the strip and its backstop`, async () => {
    const { store, socket } = await liveWithFeed()
    const started = (at: number) => ({
      t: `activity`,
      event: { kind: `compaction`, phase: `started`, at },
    })
    socket.frame({ t: `activity_reset` })
    socket.frame(started(Date.now()))
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    // The fold re-derived the strip from the replay…
    expect(store.getSnapshot().compacting).not.toBeNull()
    // …and its backstop is measured from the replayed start: one from long
    // ago expires on the next tick instead of running a fresh 3 minutes.
    socket.frame({ t: `activity_reset` })
    socket.frame(started(Date.now() - COMPACTION_TIMEOUT_MS * 2))
    socket.frame({ t: `activity_synced` })
    await vi.advanceTimersByTimeAsync(100)
    expect(store.getSnapshot().compacting).toBeNull()
    store.dispose()
  })
})

// EXP-796: "Load earlier" only while there is a socket to ask over — the relay
// keeps a history room lingering after the device's replay so pages keep
// flowing, and once that room really closes the button goes.
describe(`canLoadEarlier needs an open socket (EXP-796)`, () => {
  it(`is true on a truncated live replay and false once the socket closed`, async () => {
    const { store, sockets } = makeStore()
    store.connect()
    await vi.advanceTimersByTimeAsync(0)
    const socket = sockets[0]
    socket.open()
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, event: { kind: `narration`, text: `tail` }, seq: 9 })
    socket.frame({ t: `activity_synced`, firstSeq: 9, lastSeq: 9, truncated: true })
    await vi.advanceTimersByTimeAsync(1_000)
    expect(store.getSnapshot().canLoadEarlier).toBe(true)

    // The relay's `bye {outcome:'history'}` then the close: the feed stays,
    // the phase is ended, and there is no socket left to page over.
    store.noteSessionStatus(`ended`)
    socket.frame({ t: `bye`, outcome: `history` })
    socket.serverClose(1000)
    await vi.advanceTimersByTimeAsync(0)
    const snapshot = store.getSnapshot()
    expect(snapshot.phase.kind).toBe(`ended`)
    expect(snapshot.feed).toHaveLength(1)
    expect(snapshot.connected).toBe(false)
    expect(snapshot.canLoadEarlier).toBe(false)
    expect(store.loadEarlier()).toBe(false)
    store.dispose()
  })
})
