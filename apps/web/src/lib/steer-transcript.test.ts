import { afterEach, describe, expect, it, vi } from "vitest"
import { verifySteerTicket } from "@exp/steer-ticket"
import {
  readRunTranscript,
  TranscriptReadError,
  type TranscriptSocket,
} from "@/lib/steer-transcript"

// EXP-1216: the server-side viewer. A fake socket stands in for Bun's global
// WebSocket; each test scripts what the "relay" answers once the join lands.

const CONFIG = { url: `https://relay.test`, secret: `relay-secret` }
const RUN = `66666666-6666-4666-8666-666666666666`

class FakeSocket implements TranscriptSocket {
  static last: FakeSocket | null = null
  onopen: ((event: unknown) => void) | null = null
  onmessage: ((event: { data: unknown }) => void) | null = null
  onerror: ((event: unknown) => void) | null = null
  onclose: ((event: unknown) => void) | null = null
  sent: Array<Record<string, unknown>> = []
  closed = false
  constructor(readonly url: string) {
    FakeSocket.last = this
  }
  send(data: string) {
    this.sent.push(JSON.parse(data))
  }
  close() {
    this.closed = true
  }
  open() {
    this.onopen?.({})
  }
  frame(value: Record<string, unknown>) {
    this.onmessage?.({ data: JSON.stringify(value) })
  }
}

function start(over: Record<string, unknown> = {}) {
  const pending = readRunTranscript(CONFIG, {
    sessionId: RUN,
    teamId: `team-1`,
    ownerUserId: `user-1`,
    WebSocket: FakeSocket,
    ...over,
  })
  const socket = FakeSocket.last!
  socket.open()
  return { pending, socket }
}

function ticketOf(socket: FakeSocket) {
  const ticket = new URL(socket.url).searchParams.get(`ticket`)!
  const verified = verifySteerTicket(ticket, CONFIG.secret)
  if (!verified.ok) throw new Error(verified.reason)
  return verified.claims
}

afterEach(() => {
  vi.useRealTimers()
  FakeSocket.last = null
})

describe(`readRunTranscript`, () => {
  it(`joins as a viewer and collects activity until activity_synced`, async () => {
    const { pending, socket } = start()
    expect(socket.url.startsWith(`wss://relay.test/ws?ticket=`)).toBe(true)
    expect(socket.sent).toEqual([{ t: `join`, channel: `activity` }])
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, seq: 3, event: { kind: `narration`, text: `hi` } })
    socket.frame({ t: `keepalive` })
    socket.frame({ t: `activity`, seq: 4, event: { kind: `user_message`, text: `yo` } })
    socket.frame({ t: `activity_synced`, firstSeq: 3, lastSeq: 4, truncated: true })
    await expect(pending).resolves.toEqual({
      events: [
        { seq: 3, event: { kind: `narration`, text: `hi` } },
        { seq: 4, event: { kind: `user_message`, text: `yo` } },
      ],
      firstSeq: 3,
      lastSeq: 4,
      truncated: true,
    })
    expect(socket.sent.at(-1)).toEqual({ t: `bye` })
    expect(socket.closed).toBe(true)
  })

  it(`mints a viewer ticket, naming the device only when asked to`, async () => {
    const live = start()
    const claims = ticketOf(live.socket)
    expect(claims).toMatchObject({
      sub: `user-1`,
      team: `team-1`,
      sessionId: RUN,
      role: `viewer`,
    })
    expect(claims).not.toHaveProperty(`deviceId`)
    live.socket.frame({ t: `activity_synced`, firstSeq: 0, lastSeq: 0 })
    await live.pending

    const ended = start({ deviceId: `dev-1`, deviceOwnerId: `host-1` })
    expect(ticketOf(ended.socket)).toMatchObject({
      deviceId: `dev-1`,
      deviceOwnerId: `host-1`,
    })
    ended.socket.frame({ t: `history_pending` })
    ended.socket.frame({ t: `activity`, seq: 0, event: { kind: `narration`, text: `a` } })
    ended.socket.frame({ t: `activity_synced`, firstSeq: 0, lastSeq: 0 })
    await expect(ended.pending).resolves.toMatchObject({ events: [{ seq: 0 }] })
  })

  it(`dials the internal relay url when one is configured`, async () => {
    const pending = readRunTranscript(
      { ...CONFIG, internalUrl: `http://steer-relay:4002` },
      { sessionId: RUN, teamId: `t`, ownerUserId: `u`, WebSocket: FakeSocket }
    )
    const socket = FakeSocket.last!
    expect(socket.url.startsWith(`ws://steer-relay:4002/ws?ticket=`)).toBe(true)
    socket.open()
    socket.frame({ t: `activity_synced`, firstSeq: 0, lastSeq: 0 })
    await pending
  })

  it(`drops what arrived before an activity_reset`, async () => {
    const { pending, socket } = start()
    socket.frame({ t: `activity`, seq: 1, event: { kind: `narration`, text: `old` } })
    socket.frame({ t: `activity_reset` })
    socket.frame({ t: `activity`, seq: 1, event: { kind: `narration`, text: `new` } })
    socket.frame({ t: `activity_synced`, firstSeq: 1, lastSeq: 1 })
    const read = await pending
    expect(read.events).toEqual([{ seq: 1, event: { kind: `narration`, text: `new` } }])
    expect(read.truncated).toBe(false)
  })

  it.each([
    [`device_offline`, `offline`],
    [`history_unavailable`, `history unavailable`],
    [`no_such_session`, `not connected`],
  ])(`rejects a relay %s error readably`, async (code, text) => {
    const { pending, socket } = start()
    socket.frame({ t: `error`, code })
    socket.frame({ t: `bye`, outcome: code })
    const error = await pending.catch((e: unknown) => e)
    expect(error).toBeInstanceOf(TranscriptReadError)
    expect((error as TranscriptReadError).code).toBe(code)
    expect((error as Error).message).toContain(text)
    expect(socket.closed).toBe(true)
  })

  it(`resolves with what arrived when the room closes before a span`, async () => {
    const { pending, socket } = start()
    socket.frame({ t: `activity`, seq: 5, event: { kind: `narration`, text: `x` } })
    socket.frame({ t: `bye`, outcome: `ended` })
    await expect(pending).resolves.toMatchObject({ firstSeq: 5, lastSeq: 5 })
  })

  it(`rejects a close before anything arrived`, async () => {
    const { pending, socket } = start()
    socket.onclose?.({})
    await expect(pending).rejects.toMatchObject({ code: `closed` })
  })

  it(`times out`, async () => {
    vi.useFakeTimers()
    const { pending, socket } = start({ timeoutMs: 1_000 })
    const settled = pending.catch((e: unknown) => e)
    await vi.advanceTimersByTimeAsync(1_000)
    const error = await settled
    expect(error).toMatchObject({ code: `timeout` })
    expect(socket.closed).toBe(true)
  })
})
