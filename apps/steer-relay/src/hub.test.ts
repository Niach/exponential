import { describe, expect, spyOn, test } from "bun:test"
import type { SteerTicketClaims } from "@exp/steer-ticket"
import { COMPACT_VERDICT_TIMEOUT_MS, Hub, type RelaySocket } from "./hub"
import {
  CALL_IDLE_TIMEOUT_MS,
  CALL_MAX_FRAME_BYTES,
  CALL_MAX_FRAMES_PER_SECOND,
  CALL_MAX_MS,
  CALL_RING_TIMEOUT_MS,
  CLOSE_SESSION_ENDED,
  CONTENT_FRAME_TYPES,
  clientFrame,
} from "./protocol"

class FakeSocket implements RelaySocket {
  sent: string[] = []
  /** NEW-55: binary frames (a call's audio). */
  binary: Uint8Array[] = []
  closed: { code?: number; reason?: string } | null = null
  buffered = 0

  send(data: string) {
    this.sent.push(data)
  }
  sendBinary(data: Uint8Array) {
    this.binary.push(data)
  }
  close(code?: number, reason?: string) {
    this.closed = { code, reason }
  }
  bufferedAmount() {
    return this.buffered
  }

  frames(): { t: string; [k: string]: unknown }[] {
    return this.sent.map((d) => JSON.parse(d))
  }
  framesOf(t: string) {
    return this.frames().filter((f) => f.t === t)
  }
  lastFrame(t: string) {
    return this.framesOf(t).at(-1)
  }
}

function claims(overrides: Partial<SteerTicketClaims>): SteerTicketClaims {
  const now = Math.floor(Date.now() / 1000)
  return {
    sub: `user-1`,
    team: ``,
    role: `control`,
    iat: now,
    exp: now + 60,
    ...overrides,
  }
}

/** A device's control socket, announced as `deviceId` under `owner`. */
function connectDevice(hub: Hub, owner = `owner`, deviceId = `dev-1`) {
  const sock = new FakeSocket()
  hub.onOpen(sock, claims({ role: `control`, sub: owner }))
  hub.onMessage(sock, JSON.stringify({ t: `online`, deviceId }))
  return sock
}

/** Silences the hub's rate-limited warnings for one block. */
function quietly(fn: () => void) {
  const warn = spyOn(console, `warn`).mockImplementation(() => {})
  try {
    fn()
  } finally {
    warn.mockRestore()
  }
}

describe(`device presence`, () => {
  test(`online registers, nudge routes, close evicts`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(desktop, JSON.stringify({ t: `online`, deviceId: `dev-1` }))

    expect(hub.stats().devices).toBe(1)
    expect(hub.nudge(`owner`, `dev-1`)).toBe(true)
    expect(hub.nudge(`owner`, `dev-404`)).toBe(false)

    hub.onClose(desktop)
    expect(hub.stats().devices).toBe(0)
    expect(hub.nudge(`owner`, `dev-1`)).toBe(false)
  })

  test(`nudge delivers a check_in frame to the control socket (EXP-481)`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(desktop, JSON.stringify({ t: `online`, deviceId: `dev-1` }))

    expect(hub.nudge(`owner`, `dev-1`)).toBe(true)
    expect(desktop.lastFrame(`check_in`)).toEqual({ t: `check_in` })

    // Offline device / wrong owner: not delivered, nothing sent anywhere.
    expect(hub.nudge(`owner`, `dev-404`)).toBe(false)
    expect(hub.nudge(`someone-else`, `dev-1`)).toBe(false)
    expect(desktop.framesOf(`check_in`)).toHaveLength(1)

    hub.onClose(desktop)
    expect(hub.nudge(`owner`, `dev-1`)).toBe(false)
  })

  // The schema stays non-strict: a `.strict()` object would fail the parse
  // on an unknown key, the frame would be dropped, and the device would be
  // unreachable.
  test(`an online frame with unknown keys still registers`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(
      desktop,
      JSON.stringify({ t: `online`, deviceId: `dev-1`, someday: true })
    )

    expect(hub.stats().devices).toBe(1)
    expect(hub.nudge(`owner`, `dev-1`)).toBe(true)
  })

  test(`re-announcing a new deviceId on one socket evicts the previous entry (REV-11)`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    // A hostile client streams online frames with fresh deviceIds on ONE
    // connection — each must replace, never accumulate.
    for (let i = 0; i < 1000; i++) {
      hub.onMessage(
        desktop,
        JSON.stringify({ t: `online`, deviceId: `dev-${i}` })
      )
    }
    expect(hub.nudge(`owner`, `dev-999`)).toBe(true)
    // The abandoned ids are gone, not ghosts routing into this socket.
    expect(hub.nudge(`owner`, `dev-0`)).toBe(false)
    expect(hub.stats().devices).toBe(1)

    hub.onClose(desktop)
    expect(hub.stats().devices).toBe(0)
  })

  test(`re-announce eviction never removes an entry another socket took over`, () => {
    const hub = new Hub()
    const first = new FakeSocket()
    hub.onOpen(first, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(first, JSON.stringify({ t: `online`, deviceId: `dev-1` }))
    // A reconnect claims dev-1 (closes `first`), but `first`'s close hasn't
    // been processed yet when it re-announces a different id.
    const second = new FakeSocket()
    hub.onOpen(second, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(second, JSON.stringify({ t: `online`, deviceId: `dev-1` }))
    hub.onMessage(first, JSON.stringify({ t: `online`, deviceId: `dev-2` }))

    // dev-1 still belongs to the second socket — the eviction only removes
    // entries the re-announcing conn itself owns.
    expect(hub.nudge(`owner`, `dev-1`)).toBe(true)
    expect(second.lastFrame(`check_in`)).toEqual({ t: `check_in` })
    hub.onClose(first)
    hub.onClose(second)
    expect(hub.stats().devices).toBe(0)
  })

  test(`same-device reconnect replaces the old socket`, () => {
    const hub = new Hub()
    const first = new FakeSocket()
    hub.onOpen(first, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(first, JSON.stringify({ t: `online`, deviceId: `dev-1` }))
    const second = new FakeSocket()
    hub.onOpen(second, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(second, JSON.stringify({ t: `online`, deviceId: `dev-1` }))

    expect(first.closed?.code).toBe(4002)
    expect(hub.stats().devices).toBe(1)
  })
})


// VAPP-43: kill, interrupt and compact name the DEVICE (owner + deviceId) and
// go down its CONTROL socket; there are no session rooms any more.
describe(`session control via the device's control socket`, () => {
  const device = { userId: `owner`, deviceId: `dev-1` }

  test(`kill and interrupt reach the named device's control socket`, () => {
    const hub = new Hub()
    const desktop = connectDevice(hub)
    const other = connectDevice(hub, `owner`, `dev-2`)

    expect(hub.killSession(`sess-1`, device)).toBe(true)
    expect(desktop.lastFrame(`kill`)).toEqual({ t: `kill`, sessionId: `sess-1` })
    expect(hub.interruptSession(`sess-1`, device)).toBe(true)
    expect(desktop.lastFrame(`interrupt`)).toEqual({ t: `interrupt`, sessionId: `sess-1` })
    expect(other.frames()).toEqual([])

    // Not connected (another owner, another device, gone): not delivered.
    expect(hub.killSession(`sess-1`, { userId: `someone`, deviceId: `dev-1` })).toBe(false)
    expect(hub.interruptSession(`sess-1`, { userId: `owner`, deviceId: `dev-9` })).toBe(false)
    hub.onClose(desktop)
    expect(hub.killSession(`sess-1`, device)).toBe(false)
    hub.destroy()
  })

  test(`compact is relayed as one compact_request and answered by the device's verdict`, async () => {
    const hub = new Hub()
    const desktop = connectDevice(hub)
    const pending = hub.requestCompaction(`sess-1`, device, `the plan`)
    expect(desktop.lastFrame(`compact_request`)).toEqual({
      t: `compact_request`,
      sessionId: `sess-1`,
      keep: `the plan`,
    })
    // One ask per session in flight; another session is independent.
    expect(await hub.requestCompaction(`sess-1`, device)).toEqual({
      delivered: false,
      reason: `busy`,
    })
    hub.onMessage(
      desktop,
      JSON.stringify({ t: `compact_verdict`, sessionId: `sess-1`, accepted: true })
    )
    expect(await pending).toEqual({ delivered: true, accepted: true })
    expect(hub.counters().compactRequests).toBe(1)

    // Settled: a fresh ask goes out; no keep = no key.
    const refused = hub.requestCompaction(`sess-1`, device)
    expect(desktop.lastFrame(`compact_request`)).toEqual({
      t: `compact_request`,
      sessionId: `sess-1`,
    })
    hub.onMessage(
      desktop,
      JSON.stringify({
        t: `compact_verdict`,
        sessionId: `sess-1`,
        accepted: false,
        refusedBecause: `too_early`,
      })
    )
    expect(await refused).toEqual({
      delivered: true,
      accepted: false,
      refusedBecause: `too_early`,
    })
    hub.destroy()
  })

  test(`a device that is not connected reads device_offline`, async () => {
    const hub = new Hub()
    connectDevice(hub, `owner`, `dev-2`)
    expect(await hub.requestCompaction(`sess-1`, device)).toEqual({
      delivered: false,
      reason: `device_offline`,
    })
    hub.destroy()
  })

  test(`verdicts from another device, another owner or a non-control socket are ignored`, async () => {
    const hub = new Hub()
    const desktop = connectDevice(hub)
    const sibling = connectDevice(hub, `owner`, `dev-2`)
    const stranger = connectDevice(hub, `someone`, `dev-1`)
    const caller = new FakeSocket()
    hub.onOpen(caller, claims({ role: `caller`, sub: `owner` }))
    const unannounced = new FakeSocket()
    hub.onOpen(unannounced, claims({ role: `control`, sub: `owner` }))

    let settled: unknown = null
    const pending = hub.requestCompaction(`sess-1`, device).then((o) => (settled = o))
    const verdict = JSON.stringify({
      t: `compact_verdict`,
      sessionId: `sess-1`,
      accepted: true,
    })
    for (const sock of [sibling, stranger, caller, unannounced]) {
      hub.onMessage(sock, verdict)
    }
    await Promise.resolve()
    expect(settled).toBeNull()
    // A verdict for ANOTHER session from the right device settles nothing.
    hub.onMessage(
      desktop,
      JSON.stringify({ t: `compact_verdict`, sessionId: `sess-2`, accepted: true })
    )
    await Promise.resolve()
    expect(settled).toBeNull()
    hub.onMessage(desktop, verdict)
    await pending
    expect(settled).toEqual({ delivered: true, accepted: true })
    hub.destroy()
  })

  test(`the control socket closing settles the ask device_offline`, async () => {
    const hub = new Hub()
    const desktop = connectDevice(hub)
    const pending = hub.requestCompaction(`sess-1`, device)
    hub.onClose(desktop)
    expect(await pending).toEqual({ delivered: false, reason: `device_offline` })
    hub.destroy()
  })

  test(`a replaced control socket settles its ask; the new one may be asked again`, async () => {
    const hub = new Hub()
    const first = connectDevice(hub)
    const pending = hub.requestCompaction(`sess-1`, device)
    const second = connectDevice(hub)
    expect(first.closed?.code).toBe(4002)
    hub.onClose(first)
    expect(await pending).toEqual({ delivered: false, reason: `device_offline` })
    const again = hub.requestCompaction(`sess-1`, device)
    expect(second.lastFrame(`compact_request`)).toEqual({
      t: `compact_request`,
      sessionId: `sess-1`,
    })
    hub.onMessage(
      second,
      JSON.stringify({ t: `compact_verdict`, sessionId: `sess-1`, accepted: true })
    )
    expect(await again).toEqual({ delivered: true, accepted: true })
    hub.destroy()
  })

  test(`a host that never answers reads no_verdict after the timeout`, async () => {
    const hub = new Hub()
    connectDevice(hub)
    const realSetTimeout = globalThis.setTimeout
    let fire: (() => void) | null = null
    globalThis.setTimeout = ((fn: () => void, ms: number) => {
      if (ms === COMPACT_VERDICT_TIMEOUT_MS) {
        fire = fn
        return 0 as unknown as ReturnType<typeof setTimeout>
      }
      return realSetTimeout(fn, ms)
    }) as typeof setTimeout
    let pending: Promise<unknown>
    try {
      pending = hub.requestCompaction(`sess-1`, device)
    } finally {
      globalThis.setTimeout = realSetTimeout
    }
    expect(fire).not.toBeNull()
    fire!()
    expect(await pending!).toEqual({ delivered: false, reason: `no_verdict` })
    expect(hub.counters().compactVerdictTimeouts).toBe(1)
    hub.destroy()
  })
})

// VAPP-43: session content never crosses the relay. Every content frame type,
// from every socket role, is answered `content_refused` on that socket and
// goes nowhere else; a control socket stays registered.
describe(`content refusal (VAPP-43)`, () => {
  /** A representative frame per type: shaped like the real thing, so a
   *  refusal never depends on a parse failure. */
  function contentFrame(t: string): string {
    switch (t) {
      case `activity`:
        return JSON.stringify({ t, event: { kind: `narration`, text: `secret` }, seq: 0 })
      case `hello`:
        return JSON.stringify({ t, sessionId: `sess-1` })
      case `join`:
        return JSON.stringify({ t, channel: `activity` })
      case `input`:
        return JSON.stringify({ t, data: `rm -rf /\r` })
      case `answer`:
        return JSON.stringify({ t, questionId: `q-1`, keys: [`1`] })
      case `history_page`:
        return JSON.stringify({ t, requestId: `r-1`, beforeSeq: 10, limit: 5 })
      case `history_chunk`:
        return JSON.stringify({ t, sessionId: `sess-1`, requestId: `r-1`, events: [], seqs: [], done: true })
      default:
        return JSON.stringify({ t, sessionId: `sess-1`, id: `x`, value: `y` })
    }
  }

  test(`the refusal list is the old relay session protocol`, () => {
    for (const t of [`hello`, `join`, `input`, `answer`, `kill`, `interrupt`, `unqueue`, `set_config`, `set_mode`, `bye`, `activity`, `activity_reset`, `history_page`, `history_chunk`, `history_none`]) {
      expect(CONTENT_FRAME_TYPES as readonly string[]).toContain(t)
    }
    // None of them is a relay client frame any more.
    for (const t of CONTENT_FRAME_TYPES) {
      expect(clientFrame.safeParse(JSON.parse(contentFrame(t))).success).toBe(false)
    }
  })

  test(`every content frame from every role is refused and forwarded nowhere`, () => {
    quietly(() => {
      const hub = new Hub()
      // A full house: an online device, a paired call, and a second device.
      const desktop = connectDevice(hub, `owner`, `mint`)
      const bystander = connectDevice(hub, `owner`, `dev-2`)
      const caller = new FakeSocket()
      hub.onOpen(
        caller,
        claims({
          role: `caller`,
          sub: `owner`,
          callId: `call-1`,
          agentId: `agent-1`,
          deviceId: `mint`,
          deviceOwnerId: `owner`,
        })
      )
      const host = new FakeSocket()
      hub.onOpen(host, claims({ role: `call_host`, sub: `owner`, callId: `call-1`, deviceId: `mint` }))
      const all = [desktop, bystander, caller, host]
      const before = all.map((sock) => sock.sent.length)

      for (const sender of [desktop, caller, host]) {
        for (const t of CONTENT_FRAME_TYPES) {
          const sent = all.map((sock) => sock.sent.length)
          hub.onMessage(sender, contentFrame(t))
          for (const [i, sock] of all.entries()) {
            const added = sock.frames().slice(sent[i])
            if (sock === sender) {
              expect(added).toEqual([{ t: `error`, code: `content_refused` }])
            } else {
              expect(added).toEqual([])
            }
          }
        }
      }
      const perSender = CONTENT_FRAME_TYPES.length
      expect(desktop.sent.length - before[0]!).toBe(perSender)
      expect(bystander.sent.length).toBe(before[1]!)
      expect(hub.counters().contentFramesRefused).toBe(perSender * 3)
      // Nobody was closed, no binary moved, the call and presence stand.
      for (const sock of all) {
        expect(sock.closed).toBeNull()
        expect(sock.binary).toHaveLength(0)
      }
      expect(hub.stats().calls).toBe(1)
      expect(hub.nudge(`owner`, `mint`)).toBe(true)
      hub.destroy()
    })
  })

  test(`a refusal logs the role and the type, never the content`, () => {
    const hub = new Hub()
    const desktop = connectDevice(hub)
    const warn = spyOn(console, `warn`).mockImplementation(() => {})
    try {
      hub.onMessage(
        desktop,
        JSON.stringify({ t: `input`, data: `hunter2-password` })
      )
      const logged = warn.mock.calls.map((call) => call.join(` `)).join(`\n`)
      expect(logged).toContain(`t=input`)
      expect(logged).not.toContain(`hunter2`)
    } finally {
      warn.mockRestore()
    }
    hub.destroy()
  })

  test(`a stale publisher/viewer socket that reached the hub gets nothing`, () => {
    quietly(() => {
      const hub = new Hub()
      const desktop = connectDevice(hub)
      for (const role of [`publisher`, `viewer`]) {
        const sock = new FakeSocket()
        hub.onOpen(sock, claims({ role, sessionId: `sess-1` } as unknown as Partial<SteerTicketClaims>))
        hub.onMessage(sock, JSON.stringify({ t: `online`, deviceId: `dev-x` }))
        hub.onMessage(sock, JSON.stringify({ t: `compact_verdict`, sessionId: `sess-1`, accepted: true }))
        hub.onMessage(sock, JSON.stringify({ t: `join`, channel: `activity` }))
        expect(sock.frames()).toEqual([{ t: `error`, code: `content_refused` }])
      }
      expect(hub.stats().devices).toBe(1)
      expect(desktop.frames()).toEqual([])
      hub.destroy()
    })
  })
})

describe(`stats counters (EXP-553)`, () => {
  test(`counters start at zero and track opens`, () => {
    const hub = new Hub()
    expect(hub.counters()).toEqual({
      connectionsAccepted: 0,
      contentFramesRefused: 0,
      compactRequests: 0,
      compactVerdictTimeouts: 0,
      callRings: 0,
      callDeviceOffline: 0,
      callBusy: 0,
      callNoAnswer: 0,
      callsExpired: 0,
      callFramesForwarded: 0,
      callFramesDropped: 0,
      callFramesSkipped: 0,
    })
    expect(hub.stats()).toEqual({ connections: 0, devices: 0, calls: 0 })
    connectDevice(hub)
    connectDevice(hub, `owner`, `dev-2`)
    expect(hub.counters().connectionsAccepted).toBe(2)
    expect(hub.stats()).toEqual({ connections: 2, devices: 2, calls: 0 })
    hub.destroy()
  })
})

describe(`voice calls (NEW-55)`, () => {
  function onlineDevice(hub: Hub, owner = `owner`, deviceId = `mint`) {
    const sock = new FakeSocket()
    hub.onOpen(sock, claims({ role: `control`, sub: owner, team: `` }))
    hub.onMessage(sock, JSON.stringify({ t: `online`, deviceId }))
    return sock
  }
  function dial(
    hub: Hub,
    opts: { callId?: string; owner?: string; deviceId?: string } = {}
  ) {
    const sock = new FakeSocket()
    hub.onOpen(
      sock,
      claims({
        role: `caller`,
        sub: opts.owner ?? `owner`,
        team: ``,
        callId: opts.callId ?? `call-1`,
        agentId: `agent-1`,
        deviceId: opts.deviceId ?? `mint`,
        deviceOwnerId: opts.owner ?? `owner`,
      })
    )
    return sock
  }
  function answer(hub: Hub, opts: { callId?: string; sub?: string; deviceId?: string } = {}) {
    const sock = new FakeSocket()
    hub.onOpen(
      sock,
      claims({
        role: `call_host`,
        sub: opts.sub ?? `owner`,
        team: ``,
        callId: opts.callId ?? `call-1`,
        deviceId: opts.deviceId ?? `mint`,
      })
    )
    return sock
  }
  function audio(kind: number, seq: number, bytes = 40) {
    const out = new Uint8Array(3 + bytes)
    out[0] = kind
    out[1] = (seq >> 8) & 0xff
    out[2] = seq & 0xff
    out.fill(0xab, 3)
    return out
  }
  function paired() {
    const hub = new Hub()
    const device = onlineDevice(hub)
    const caller = dial(hub)
    const host = answer(hub)
    return { hub, device, caller, host }
  }

  test(`a caller rings the voice device's control socket`, () => {
    const hub = new Hub()
    const device = onlineDevice(hub)
    const caller = dial(hub)
    expect(device.lastFrame(`call_ring`)).toEqual({
      t: `call_ring`,
      callId: `call-1`,
      agentId: `agent-1`,
    })
    expect(caller.closed).toBeNull()
    expect(hub.stats().calls).toBe(1)
    expect(hub.counters().callRings).toBe(1)
    hub.destroy()
  })

  // `destroy()` retires every call (ring, length and idle timers alike), not
  // just the ones still ringing.
  test(`destroy drops every call with its timers`, () => {
    const { hub } = paired()
    expect(hub.stats().calls).toBe(1)
    hub.destroy()
    expect(hub.stats().calls).toBe(0)
  })

  test(`an offline voice device answers device_offline, terminally`, () => {
    const hub = new Hub()
    onlineDevice(hub, `owner`, `other-device`)
    const caller = dial(hub)
    expect(caller.frames()).toEqual([
      { t: `error`, code: `device_offline` },
      { t: `bye`, outcome: `device_offline` },
    ])
    expect(caller.closed).toEqual({
      code: CLOSE_SESSION_ENDED,
      reason: `device_offline`,
    })
    expect(hub.stats().calls).toBe(0)
    hub.destroy()
  })

  test(`the device is looked up under its owner, not another account`, () => {
    const hub = new Hub()
    onlineDevice(hub, `someone-else`, `mint`)
    const caller = dial(hub)
    expect(caller.lastFrame(`error`)).toMatchObject({ code: `device_offline` })
    hub.destroy()
  })

  test(`a second call to the same device is busy`, () => {
    const hub = new Hub()
    const device = onlineDevice(hub)
    dial(hub)
    const second = dial(hub, { callId: `call-2` })
    expect(second.frames()).toEqual([
      { t: `error`, code: `busy` },
      { t: `bye`, outcome: `busy` },
    ])
    expect(second.closed?.code).toBe(CLOSE_SESSION_ENDED)
    expect(device.framesOf(`call_ring`)).toHaveLength(1)
    // The refused socket closing does not touch the running call.
    hub.onClose(second)
    expect(hub.stats().calls).toBe(1)
    expect(hub.counters().callBusy).toBe(1)
    hub.destroy()
  })

  test(`no host within the ring timeout answers no_answer`, () => {
    const hub = new Hub()
    onlineDevice(hub)
    const timers: { fn: () => void; ms?: number }[] = []
    const realSetTimeout = globalThis.setTimeout
    globalThis.setTimeout = ((fn: () => void, ms?: number) => {
      timers.push({ fn, ms })
      return 0 as unknown as ReturnType<typeof setTimeout>
    }) as typeof setTimeout
    let caller: FakeSocket
    try {
      caller = dial(hub)
    } finally {
      globalThis.setTimeout = realSetTimeout
    }
    const ring = timers.find((t) => t.ms === CALL_RING_TIMEOUT_MS)
    expect(ring).toBeDefined()
    expect(CALL_RING_TIMEOUT_MS).toBe(10_000)
    ring!.fn()
    expect(caller.frames()).toEqual([
      { t: `error`, code: `no_answer` },
      { t: `bye`, outcome: `no_answer` },
    ])
    expect(caller.closed?.code).toBe(CLOSE_SESSION_ENDED)
    expect(hub.stats().calls).toBe(0)
    expect(hub.counters().callNoAnswer).toBe(1)
    // The device is free again.
    const again = dial(hub, { callId: `call-2` })
    expect(again.closed).toBeNull()
    hub.destroy()
  })

  test(`the owner's host joins and both get call_ready`, () => {
    const { hub, caller, host } = paired()
    expect(caller.framesOf(`call_ready`)).toHaveLength(1)
    expect(host.frames()).toEqual([{ t: `call_ready` }])
    expect(host.closed).toBeNull()
    hub.destroy()
  })

  // NEW-55 review: the host ticket is bound to its device.
  test(`a host ticket for another device is refused`, () => {
    const hub = new Hub()
    onlineDevice(hub)
    const caller = dial(hub)
    const host = answer(hub, { deviceId: `other` })
    expect(host.frames()).toContainEqual({ t: `error`, code: `no_such_call` })
    expect(caller.framesOf(`call_ready`)).toHaveLength(0)
    hub.destroy()
  })

  test(`a socket may send at most CALL_MAX_FRAMES_PER_SECOND frames a second`, () => {
    const { hub, caller, host } = paired()
    const before = host.binary.length
    for (let seq = 0; seq < CALL_MAX_FRAMES_PER_SECOND + 20; seq += 1) hub.onBinary(caller, audio(1, seq))
    expect(host.binary.length - before).toBe(CALL_MAX_FRAMES_PER_SECOND)
    hub.destroy()
  })

  function pairedWithTimers() {
    const timeouts: { fn: () => void; ms: number }[] = []
    const intervals: { fn: () => void; ms: number }[] = []
    const [realTimeout, realInterval] = [globalThis.setTimeout, globalThis.setInterval]
    globalThis.setTimeout = ((fn: () => void, ms: number) => {
      timeouts.push({ fn, ms })
      return 0 as unknown as ReturnType<typeof setTimeout>
    }) as typeof setTimeout
    globalThis.setInterval = ((fn: () => void, ms: number) => {
      intervals.push({ fn, ms })
      return 0 as unknown as ReturnType<typeof setInterval>
    }) as typeof setInterval
    try {
      return { ...paired(), timeouts, intervals }
    } finally {
      globalThis.setTimeout = realTimeout
      globalThis.setInterval = realInterval
    }
  }

  test(`a paired call ends at CALL_MAX_MS with max_length`, () => {
    const { hub, caller, host, timeouts } = pairedWithTimers()
    const length = timeouts.find((t) => t.ms === CALL_MAX_MS)
    expect(length).toBeDefined()
    length!.fn()
    expect(caller.lastFrame(`call_end`)).toEqual({ t: `call_end`, reason: `max_length` })
    expect(host.lastFrame(`call_end`)).toEqual({ t: `call_end`, reason: `max_length` })
    expect(hub.stats().calls).toBe(0)
    expect(hub.counters().callsExpired).toBe(1)
    hub.destroy()
  })

  test(`a paired call with no frame for CALL_IDLE_TIMEOUT_MS ends with idle`, () => {
    const { hub, caller, host, intervals } = pairedWithTimers()
    const idle = intervals.find((t) => t.ms === Math.min(CALL_IDLE_TIMEOUT_MS, 5_000))
    expect(idle).toBeDefined()
    const now = Date.now
    try {
      // A frame keeps it alive.
      hub.onBinary(caller, audio(1, 0))
      idle!.fn()
      expect(hub.stats().calls).toBe(1)
      Date.now = () => now() + CALL_IDLE_TIMEOUT_MS + 1
      idle!.fn()
    } finally {
      Date.now = now
    }
    expect(caller.lastFrame(`call_end`)).toEqual({ t: `call_end`, reason: `idle` })
    expect(host.closed?.code).toBe(CLOSE_SESSION_ENDED)
    expect(hub.stats().calls).toBe(0)
    hub.destroy()
  })

  test(`a host on another account is refused and the call keeps ringing`, () => {
    const hub = new Hub()
    onlineDevice(hub)
    const caller = dial(hub)
    const intruder = answer(hub, { sub: `attacker` })
    expect(intruder.frames()).toEqual([
      { t: `error`, code: `no_such_call` },
      { t: `bye`, outcome: `no_such_call` },
    ])
    expect(intruder.closed?.code).toBe(CLOSE_SESSION_ENDED)
    hub.onClose(intruder)
    expect(caller.framesOf(`call_ready`)).toHaveLength(0)
    expect(caller.closed).toBeNull()
    // The real host still gets in.
    const host = answer(hub)
    expect(host.frames()).toEqual([{ t: `call_ready` }])
    hub.destroy()
  })

  test(`a host for an unknown or already answered call is refused`, () => {
    const { hub } = paired()
    const late = answer(hub)
    expect(late.lastFrame(`error`)).toMatchObject({ code: `no_such_call` })
    const stray = answer(hub, { callId: `nope` })
    expect(stray.lastFrame(`error`)).toMatchObject({ code: `no_such_call` })
    hub.destroy()
  })

  test(`audio flows up caller → host and down host → caller, untouched`, () => {
    const { hub, caller, host } = paired()
    const up = audio(1, 7)
    hub.onBinary(caller, up)
    expect(host.binary).toEqual([up])
    const down = audio(2, 65535)
    hub.onBinary(host, down)
    expect(caller.binary).toEqual([down])
    expect(hub.counters().callFramesForwarded).toBe(2)
    hub.destroy()
  })

  test(`a wrong kind, an oversized or an empty frame is dropped`, () => {
    const { hub, caller, host } = paired()
    const warn = spyOn(console, `warn`).mockImplementation(() => {})
    try {
      hub.onBinary(caller, audio(2, 1)) // a caller may only send `up`
      hub.onBinary(host, audio(1, 1)) // a host may only send `down`
      hub.onBinary(caller, audio(1, 2, CALL_MAX_FRAME_BYTES)) // 3 + 1500 bytes
      hub.onBinary(caller, new Uint8Array([1, 0, 3])) // header only
      expect(host.binary).toHaveLength(0)
      expect(caller.binary).toHaveLength(0)
      // Exactly the limit passes.
      hub.onBinary(caller, audio(1, 3, CALL_MAX_FRAME_BYTES - 3))
      expect(host.binary).toHaveLength(1)
      expect(hub.counters().callFramesDropped).toBe(4)
      // Rate-limited: one warning per socket per window.
      expect(warn).toHaveBeenCalledTimes(2)
    } finally {
      warn.mockRestore()
    }
    hub.destroy()
  })

  test(`audio before the host joined goes nowhere`, () => {
    const hub = new Hub()
    onlineDevice(hub)
    const caller = dial(hub)
    hub.onBinary(caller, audio(1, 1))
    expect(hub.counters().callFramesDropped).toBe(1)
    hub.destroy()
  })

  test(`a saturated peer skips frames instead of queueing them`, () => {
    const { hub, caller, host } = paired()
    host.buffered = 64 * 1024
    hub.onBinary(caller, audio(1, 1))
    expect(host.binary).toHaveLength(0)
    expect(hub.counters().callFramesSkipped).toBe(1)
    host.buffered = 0
    hub.onBinary(caller, audio(1, 2))
    expect(host.binary).toHaveLength(1)
    hub.destroy()
  })

  test(`binary frames from every other role are still dropped`, () => {
    const hub = new Hub()
    const warn = spyOn(console, `warn`).mockImplementation(() => {})
    try {
      const device = onlineDevice(hub)
      for (const sock of [device]) hub.onBinary(sock, audio(1, 1))
      for (const sock of [device]) expect(sock.binary).toHaveLength(0)
      expect(hub.counters().callFramesForwarded).toBe(0)
    } finally {
      warn.mockRestore()
    }
    hub.destroy()
  })

  test(`call_mute goes to the host, call_event to the caller`, () => {
    const { hub, caller, host } = paired()
    hub.onMessage(caller, JSON.stringify({ t: `call_mute`, muted: true }))
    expect(host.lastFrame(`call_mute`)).toEqual({ t: `call_mute`, muted: true })
    const event = { t: `said`, text: `what is still open` }
    hub.onMessage(host, JSON.stringify({ t: `call_event`, event }))
    expect(caller.lastFrame(`call_event`)).toEqual({ t: `call_event`, event })
    // Wrong direction: a host cannot mute, a caller cannot send events.
    hub.onMessage(host, JSON.stringify({ t: `call_mute`, muted: false }))
    hub.onMessage(caller, JSON.stringify({ t: `call_event`, event }))
    expect(caller.framesOf(`call_mute`)).toHaveLength(0)
    expect(host.framesOf(`call_event`)).toHaveLength(0)
    hub.destroy()
  })

  test(`an oversized call_event is rejected`, () => {
    const { hub, caller, host } = paired()
    const warn = spyOn(console, `warn`).mockImplementation(() => {})
    try {
      const event = { t: `partial`, text: `x`.repeat(9 * 1024) }
      hub.onMessage(host, JSON.stringify({ t: `call_event`, event }))
      expect(caller.framesOf(`call_event`)).toHaveLength(0)
    } finally {
      warn.mockRestore()
    }
    hub.destroy()
  })

  test(`call_end reaches the peer and closes the room`, () => {
    const { hub, device, caller, host } = paired()
    hub.onMessage(caller, JSON.stringify({ t: `call_end`, reason: `hangup` }))
    expect(host.lastFrame(`call_end`)).toEqual({ t: `call_end`, reason: `hangup` })
    expect(host.closed?.code).toBe(CLOSE_SESSION_ENDED)
    expect(caller.closed?.code).toBe(CLOSE_SESSION_ENDED)
    expect(caller.framesOf(`call_end`)).toHaveLength(0)
    expect(hub.stats().calls).toBe(0)
    // The sockets' own closes afterwards are inert, and the device is free.
    hub.onClose(caller)
    hub.onClose(host)
    const next = dial(hub, { callId: `call-2` })
    expect(next.closed).toBeNull()
    expect(device.framesOf(`call_ring`)).toHaveLength(2)
    hub.destroy()
  })

  test(`either socket closing ends the call for the other`, () => {
    const a = paired()
    a.hub.onClose(a.host)
    expect(a.caller.lastFrame(`call_end`)).toEqual({ t: `call_end`, reason: `dropped` })
    expect(a.caller.closed?.code).toBe(CLOSE_SESSION_ENDED)
    expect(a.hub.stats().calls).toBe(0)
    a.hub.destroy()

    const b = paired()
    b.hub.onClose(b.caller)
    expect(b.host.lastFrame(`call_end`)).toEqual({ t: `call_end`, reason: `dropped` })
    expect(b.host.closed?.code).toBe(CLOSE_SESSION_ENDED)
    expect(b.hub.stats().calls).toBe(0)
    b.hub.destroy()
  })

  test(`a ringing caller hanging up frees the device`, () => {
    const hub = new Hub()
    onlineDevice(hub)
    const caller = dial(hub)
    hub.onClose(caller)
    expect(hub.stats().calls).toBe(0)
    const host = answer(hub)
    expect(host.lastFrame(`error`)).toMatchObject({ code: `no_such_call` })
    hub.destroy()
  })

  test(`call frames from other roles do nothing`, () => {
    const hub = new Hub()
    const device = onlineDevice(hub)
    hub.onMessage(device, JSON.stringify({ t: `call_end` }))
    hub.onMessage(device, JSON.stringify({ t: `call_mute`, muted: true }))
    expect(device.closed).toBeNull()
    expect(device.frames()).toEqual([])
    hub.destroy()
  })
})
