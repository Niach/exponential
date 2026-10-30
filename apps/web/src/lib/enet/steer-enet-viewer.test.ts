// @vitest-environment node
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/steer-enet.json" with { type: "json" }
import { encodeSteerEnvelope, parseSteerEnvelope, type SteerFrame } from "@exp/domain-contract/steer-enet"
import { CLOSE_ENET_LOST, openEnetSteer, type EnetSteerSocket, type SteerCarrier } from "./steer-enet-viewer"

// VAPP-33: the web viewer side of fixtures/steer-enet.json against a fake
// carrier (the node, or the bridge to it).

const SESSION = `s1`

/** A carrier whose devices answer joins per a script. */
function fakeCarrier(devices: Record<string, (frames: SteerFrame[], session: string) => SteerFrame[] | `fail` | `mute`>) {
  const listeners = new Set<(from: string, body: Uint8Array) => void>()
  const sent: Array<{ to: string; session: string; frames: SteerFrame[] }> = []
  const allowed = new Map<string, number>()
  const deliver = (from: string, body: Uint8Array) => {
    for (const listener of [...listeners]) listener(from, body)
  }
  const carrier: SteerCarrier = {
    send: async (to, body) => {
      const envelope = parseSteerEnvelope(body)!
      sent.push({ to, session: envelope.sessionId, frames: envelope.frames })
      const answer = devices[to]?.(envelope.frames, envelope.sessionId) ?? `fail`
      if (answer === `fail`) throw new Error(`unreachable`)
      if (answer === `mute` || answer.length === 0) return
      // The device dials back on its own stream: a later task.
      setTimeout(() => deliver(to, encodeSteerEnvelope(envelope.sessionId, answer)), 1)
    },
    allow: (id) => {
      allowed.set(id, (allowed.get(id) ?? 0) + 1)
      return () => allowed.set(id, allowed.get(id)! - 1)
    },
    subscribe: (listener) => {
      listeners.add(listener)
      return () => void listeners.delete(listener)
    },
  }
  return { carrier, sent, allowed, deliver, listeners }
}

const REPLAY: SteerFrame[] = [
  { t: `activity_reset` },
  { t: `activity`, seq: 0, event: { kind: `narration`, text: `hi` } },
  { t: `activity_synced`, firstSeq: 0, lastSeq: 0 },
]
const live = (frames: SteerFrame[]) => (frames.some((f) => f.t === `join`) ? REPLAY : [])
const refuse = () => [{ t: `error`, code: `no_such_session` }]
const endpoints = (...ids: string[]) => async () => ids.map((endpointId) => ({ endpointId, kind: `desktop` }))

function attach(socket: EnetSteerSocket) {
  const frames: Array<Record<string, unknown>> = []
  const closes: number[] = []
  let opened = 0
  socket.onopen = () => {
    opened++
    socket.send(JSON.stringify({ t: `join`, channel: `activity` }))
  }
  socket.onmessage = (event) => frames.push(JSON.parse(event.data) as Record<string, unknown>)
  socket.onclose = (event) => closes.push(event.code)
  return { frames, closes, opened: () => opened }
}

beforeEach(() => {
  vi.useFakeTimers()
})
afterEach(() => {
  vi.useRealTimers()
})

describe(`fixture replay`, () => {
  for (const c of fixture.parse) {
    it(`parse: ${c.name}`, () => {
      const body = `bodyText` in c ? (c.bodyText as string) : JSON.stringify(c.body)
      const envelope = parseSteerEnvelope(new TextEncoder().encode(body))
      expect(envelope !== null).toBe(c.ok)
      if (envelope) expect(envelope.frames.map((f) => f.t)).toEqual(c.types)
    })
  }
  it(`frames on the fixture's lists are the ones the viewer sends and applies`, () => {
    expect(fixture.viewerFrames).toEqual(
      expect.arrayContaining([`join`, `leave`, `input`, `answer`, `interrupt`, `unqueue`, `set_mode`, `history_page`])
    )
    expect(fixture.deviceFrames).toEqual(
      expect.arrayContaining([`activity_reset`, `activity`, `activity_synced`, `history_chunk`, `keepalive`, `bye`, `error`])
    )
  })
})

describe(`transport choice`, () => {
  it(`joins the first endpoint, pins it, hands the replay over on the store's join`, async () => {
    const { carrier, sent, allowed } = fakeCarrier({ d1: live })
    const opening = openEnetSteer(SESSION, { carrier: async () => carrier, forSession: endpoints(`d1`) })
    await vi.advanceTimersByTimeAsync(5)
    const socket = (await opening)!
    expect(socket.endpointId).toBe(`d1`)
    expect(allowed.get(`d1`)).toBe(1)
    expect(sent).toEqual([{ to: `d1`, session: SESSION, frames: [{ t: `join`, channel: `activity` }] }])
    const view = attach(socket)
    await vi.advanceTimersByTimeAsync(0)
    expect(view.opened()).toBe(1)
    // The store's join does not go out again: the replay that proved it lands.
    expect(sent).toHaveLength(1)
    expect(view.frames.map((f) => f.t)).toEqual([`activity_reset`, `activity`, `activity_synced`])
  })

  it(`on no_such_session tries the next endpoint`, async () => {
    const { carrier, sent, allowed } = fakeCarrier({ d1: refuse, d2: live })
    const opening = openEnetSteer(SESSION, { carrier: async () => carrier, forSession: endpoints(`d1`, `d2`) })
    await vi.advanceTimersByTimeAsync(10)
    const socket = (await opening)!
    expect(socket.endpointId).toBe(`d2`)
    expect(sent.map((s) => [s.to, s.frames[0]!.t])).toEqual([
      [`d1`, `join`],
      [`d1`, `leave`],
      [`d2`, `join`],
    ])
    expect(allowed.get(`d1`)).toBe(0)
    expect(allowed.get(`d2`)).toBe(1)
  })

  it(`an unreachable endpoint counts as refused`, async () => {
    const { carrier } = fakeCarrier({ d2: live })
    const opening = openEnetSteer(SESSION, { carrier: async () => carrier, forSession: endpoints(`d1`, `d2`) })
    await vi.advanceTimersByTimeAsync(10)
    expect((await opening)?.endpointId).toBe(`d2`)
  })
})

describe(`fallback to the relay (null)`, () => {
  it(`no node`, async () => {
    const forSession = vi.fn(endpoints(`d1`))
    expect(await openEnetSteer(SESSION, { carrier: async () => undefined, forSession })).toBeNull()
    expect(forSession).not.toHaveBeenCalled()
  })

  it(`no endpoint, or discovery failing`, async () => {
    const { carrier } = fakeCarrier({})
    expect(await openEnetSteer(SESSION, { carrier: async () => carrier, forSession: endpoints() })).toBeNull()
    const failing = async () => {
      throw new Error(`FORBIDDEN`)
    }
    expect(await openEnetSteer(SESSION, { carrier: async () => carrier, forSession: failing })).toBeNull()
  })

  it(`every endpoint refused`, async () => {
    const { carrier } = fakeCarrier({ d1: refuse, d2: () => [{ t: `error`, code: `unauthorized` }] })
    const opening = openEnetSteer(SESSION, { carrier: async () => carrier, forSession: endpoints(`d1`, `d2`) })
    await vi.advanceTimersByTimeAsync(10)
    expect(await opening).toBeNull()
  })

  it(`no activity_synced within joinTimeoutMs`, async () => {
    const { carrier, allowed } = fakeCarrier({ d1: () => [{ t: `activity_reset` }], d2: live })
    const opening = openEnetSteer(SESSION, { carrier: async () => carrier, forSession: endpoints(`d1`, `d2`) })
    await vi.advanceTimersByTimeAsync(fixture.joinTimeoutMs - 1)
    let done = false
    void opening.then(() => (done = true))
    await vi.advanceTimersByTimeAsync(0)
    expect(done).toBe(false)
    await vi.advanceTimersByTimeAsync(2)
    expect(await opening).toBeNull()
    expect(allowed.get(`d1`)).toBe(0)
  })
})

describe(`a joined feed`, () => {
  async function joined(devices: Parameters<typeof fakeCarrier>[0] = { d1: live }) {
    const fake = fakeCarrier(devices)
    const opening = openEnetSteer(SESSION, { carrier: async () => fake.carrier, forSession: endpoints(`d1`) })
    await vi.advanceTimersByTimeAsync(5)
    const socket = (await opening)!
    const view = attach(socket)
    await vi.advanceTimersByTimeAsync(0)
    view.frames.length = 0
    fake.sent.length = 0
    return { ...fake, socket, view }
  }

  it(`sends every client frame as is; one tick = one envelope`, async () => {
    const { socket, sent } = await joined()
    socket.send(JSON.stringify({ t: `input`, data: `hello` }))
    socket.send(JSON.stringify({ t: `input`, data: `\r` }))
    await vi.advanceTimersByTimeAsync(0)
    for (const frame of [
      { t: `answer`, questionId: `q`, keys: [`1`] },
      { t: `interrupt` },
      { t: `unqueue`, id: `u` },
      { t: `set_config`, id: `c`, value: `v` },
      { t: `set_mode`, id: `plan` },
      { t: `kill` },
      { t: `history_page`, requestId: `p1`, beforeSeq: 10, limit: 50 },
    ]) {
      socket.send(JSON.stringify(frame))
      await vi.advanceTimersByTimeAsync(0)
    }
    expect(sent.map((s) => s.frames.map((f) => f.t))).toEqual([
      [`input`, `input`],
      [`answer`],
      [`interrupt`],
      [`unqueue`],
      [`set_config`],
      [`set_mode`],
      [`kill`],
      [`history_page`],
    ])
    expect(sent[0]!.frames).toEqual([
      { t: `input`, data: `hello` },
      { t: `input`, data: `\r` },
    ])
    expect(sent.every((s) => s.to === `d1` && s.session === SESSION)).toBe(true)
  })

  it(`applies only the joined session from the joined endpoint`, async () => {
    const { deliver, view } = await joined()
    const frame = (n: number) => [{ t: `activity`, seq: n, event: { kind: `narration`, text: `${n}` } }]
    deliver(`d1`, encodeSteerEnvelope(SESSION, frame(1)))
    deliver(`d2`, encodeSteerEnvelope(SESSION, frame(2)))
    deliver(`d1`, encodeSteerEnvelope(`other`, frame(3)))
    deliver(`d1`, new TextEncoder().encode(`exp-enet-ping/1`))
    expect(view.frames.map((f) => f.seq)).toEqual([1])
  })

  it(`bye closes the socket like the relay does`, async () => {
    const { deliver, view, socket } = await joined()
    deliver(`d1`, encodeSteerEnvelope(SESSION, [{ t: `bye`, outcome: `ended` }, { t: `keepalive` }]))
    await vi.advanceTimersByTimeAsync(0)
    expect(view.frames).toEqual([{ t: `bye`, outcome: `ended` }])
    expect(view.closes).toEqual([1000])
    expect(socket.readyState).toBe(3)
  })

  it(`close sends leave and releases the pin`, async () => {
    const { socket, sent, allowed, view } = await joined()
    socket.close()
    await vi.advanceTimersByTimeAsync(0)
    expect(sent.map((s) => s.frames)).toEqual([[{ t: `leave` }]])
    expect(allowed.get(`d1`)).toBe(0)
    expect(view.closes).toEqual([1000])
  })

  it(`re-joins after staleMs of silence; keepalives hold it`, async () => {
    const { socket, sent, deliver, view } = await joined()
    await vi.advanceTimersByTimeAsync(fixture.staleMs - 10)
    deliver(`d1`, encodeSteerEnvelope(SESSION, [{ t: `keepalive` }]))
    await vi.advanceTimersByTimeAsync(fixture.staleMs - 1)
    expect(sent).toHaveLength(0)
    await vi.advanceTimersByTimeAsync(2)
    expect(sent.map((s) => s.frames)).toEqual([[{ t: `join`, channel: `activity` }]])
    // The device answers the re-join with a full replay, like a relay reconnect.
    await vi.advanceTimersByTimeAsync(5)
    expect(view.frames.map((f) => f.t)).toEqual([`keepalive`, `activity_reset`, `activity`, `activity_synced`])
    expect(socket.readyState).toBe(1)
  })

  it(`a re-join nobody answers loses the feed (silent redial)`, async () => {
    let joins = 0
    const { view } = await joined({ d1: (frames) => (++joins > 1 ? `mute` : live(frames)) })
    await vi.advanceTimersByTimeAsync(fixture.staleMs + fixture.joinTimeoutMs + 1)
    expect(view.closes).toEqual([CLOSE_ENET_LOST])
  })

  it(`a failed send loses the feed`, async () => {
    const { socket, view } = await joined({ d1: (frames) => (frames[0]!.t === `join` ? live(frames) : `fail`) })
    socket.send(JSON.stringify({ t: `interrupt` }))
    await vi.advanceTimersByTimeAsync(1)
    expect(view.closes).toEqual([CLOSE_ENET_LOST])
  })
})
