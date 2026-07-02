import { describe, expect, test } from "bun:test"
import type { SteerTicketClaims } from "@exp/steer-ticket"
import { Hub, RingBuffer, type RelaySocket } from "./hub"
import { OUTPUT_OPCODE } from "./protocol"

class FakeSocket implements RelaySocket {
  sent: (string | Uint8Array)[] = []
  closed: { code?: number; reason?: string } | null = null
  buffered = 0

  send(data: string | Uint8Array) {
    this.sent.push(data)
  }
  close(code?: number, reason?: string) {
    this.closed = { code, reason }
  }
  bufferedAmount() {
    return this.buffered
  }

  /** JSON control frames sent to this socket. */
  frames(): { t: string; [k: string]: unknown }[] {
    return this.sent
      .filter((d): d is string => typeof d === `string`)
      .map((d) => JSON.parse(d))
  }
  /** Binary output payloads (0x01 stripped). */
  outputs(): Uint8Array[] {
    return this.sent
      .filter((d): d is Uint8Array => typeof d !== `string`)
      .map((d) => {
        expect(d[0]).toBe(OUTPUT_OPCODE)
        return d.subarray(1)
      })
  }
  lastFrame(t: string) {
    return this.frames().filter((f) => f.t === t).at(-1)
  }
}

function claims(overrides: Partial<SteerTicketClaims>): SteerTicketClaims {
  const now = Math.floor(Date.now() / 1000)
  return {
    sub: `user-1`,
    ws: `ws-1`,
    role: `viewer`,
    perm: `view`,
    iat: now,
    exp: now + 60,
    ...overrides,
  }
}

function connectPublisher(hub: Hub, sessionId = `sess-1`) {
  const sock = new FakeSocket()
  hub.onOpen(sock, claims({ role: `publisher`, sessionId, perm: `view` }))
  hub.onMessage(
    sock,
    JSON.stringify({ t: `hello`, sessionId, issueId: `issue-1`, cols: 120, rows: 40 })
  )
  return sock
}

function connectViewer(
  hub: Hub,
  opts: { sub?: string; name?: string; perm?: `view` | `steer`; sessionId?: string } = {}
) {
  const sock = new FakeSocket()
  hub.onOpen(
    sock,
    claims({
      role: `viewer`,
      sub: opts.sub ?? `viewer-1`,
      name: opts.name ?? `Viewer`,
      perm: opts.perm ?? `steer`,
      sessionId: opts.sessionId ?? `sess-1`,
    })
  )
  hub.onMessage(sock, JSON.stringify({ t: `join` }))
  return sock
}

function output(hub: Hub, pub: FakeSocket, text: string) {
  const payload = new TextEncoder().encode(text)
  const framed = new Uint8Array(payload.byteLength + 1)
  framed[0] = OUTPUT_OPCODE
  framed.set(payload, 1)
  hub.onMessage(pub, framed)
}

describe(`RingBuffer`, () => {
  test(`evicts oldest past the cap`, () => {
    const ring = new RingBuffer(10)
    ring.push(new Uint8Array(6))
    ring.push(new Uint8Array(6))
    expect(ring.replay().length).toBe(1)
    expect(ring.bytes).toBe(6)
  })
})

describe(`device presence + remote start`, () => {
  test(`online registers, startSession routes, close evicts`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(
      desktop,
      JSON.stringify({ t: `online`, deviceId: `dev-1`, deviceLabel: `MacBook` })
    )

    expect(hub.devicesFor(`owner`)).toMatchObject([
      { deviceId: `dev-1`, deviceLabel: `MacBook` },
    ])

    const routed = hub.startSession(`owner`, `dev-1`, `issue-9`)
    expect(routed).toEqual({ ok: true })
    expect(desktop.lastFrame(`start_session`)).toMatchObject({ issueId: `issue-9` })

    expect(hub.startSession(`owner`, `dev-404`, `issue-9`)).toEqual({
      ok: false,
      reason: `device_offline`,
    })

    hub.onClose(desktop)
    expect(hub.devicesFor(`owner`)).toEqual([])
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
    expect(hub.devicesFor(`owner`).length).toBe(1)
  })
})

describe(`session rooms`, () => {
  test(`viewer gets geometry + ring replay + live output`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    output(hub, pub, `before-join`)

    const viewer = connectViewer(hub)
    expect(viewer.lastFrame(`resize`)).toMatchObject({ cols: 120, rows: 40 })
    expect(
      viewer.outputs().map((o) => new TextDecoder().decode(o))
    ).toEqual([`before-join`])

    output(hub, pub, `live`)
    expect(
      viewer.outputs().map((o) => new TextDecoder().decode(o))
    ).toEqual([`before-join`, `live`])
  })

  test(`join on a dead session errors + closes`, () => {
    const hub = new Hub()
    const viewer = connectViewer(hub, { sessionId: `nope` })
    expect(viewer.lastFrame(`error`)).toMatchObject({ code: `no_such_session` })
    expect(viewer.closed?.code).toBe(4001)
  })

  test(`single-steerer claim gates input forwarding`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectViewer(hub, { sub: `s`, perm: `steer` })
    const watcher = connectViewer(hub, { sub: `w`, perm: `view` })

    // Unclaimed input is dropped.
    hub.onMessage(steerer, JSON.stringify({ t: `input`, data: `x` }))
    expect(pub.lastFrame(`input`)).toBeUndefined()

    // view-perm claim is ignored.
    hub.onMessage(watcher, JSON.stringify({ t: `claim` }))
    expect(
      steerer.lastFrame(`presence`)
    ).toMatchObject({ steererId: null })

    hub.onMessage(steerer, JSON.stringify({ t: `claim` }))
    expect(steerer.lastFrame(`presence`)).toMatchObject({ steererId: `s` })

    hub.onMessage(steerer, JSON.stringify({ t: `input`, data: `ls\n` }))
    expect(pub.lastFrame(`input`)).toMatchObject({ data: `ls\n` })

    // Non-holder input never reaches the publisher.
    hub.onMessage(watcher, JSON.stringify({ t: `input`, data: `rm -rf /\n` }))
    expect(pub.frames().filter((f) => f.t === `input`).length).toBe(1)

    // Second steer-perm claim while held loses.
    const rival = connectViewer(hub, { sub: `r`, perm: `steer` })
    hub.onMessage(rival, JSON.stringify({ t: `claim` }))
    expect(rival.lastFrame(`presence`)).toMatchObject({ steererId: `s` })

    // Release frees the claim.
    hub.onMessage(steerer, JSON.stringify({ t: `release` }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: null })
  })

  test(`publisher release/claim force-clears an active viewer claim (take over)`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectViewer(hub, { sub: `s`, perm: `steer` })

    hub.onMessage(steerer, JSON.stringify({ t: `claim` }))
    expect(steerer.lastFrame(`presence`)).toMatchObject({ steererId: `s` })

    // "Take over" on the desktop sends release-then-claim on the publisher
    // socket — the local user wins immediately.
    hub.onMessage(pub, JSON.stringify({ t: `release` }))
    expect(steerer.lastFrame(`presence`)).toMatchObject({ steererId: null })
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: null })

    hub.onMessage(pub, JSON.stringify({ t: `claim` }))
    // The publisher never becomes steererId — local input doesn't flow
    // through the relay.
    expect(steerer.lastFrame(`presence`)).toMatchObject({ steererId: null })
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: null })

    // The evicted viewer's keystrokes no longer flow.
    hub.onMessage(steerer, JSON.stringify({ t: `input`, data: `x` }))
    expect(pub.lastFrame(`input`)).toBeUndefined()

    // Viewer semantics are unchanged: it can re-claim afterwards.
    hub.onMessage(steerer, JSON.stringify({ t: `claim` }))
    expect(steerer.lastFrame(`presence`)).toMatchObject({ steererId: `s` })

    // A publisher claim while a fresh viewer claim is held also clears it.
    hub.onMessage(pub, JSON.stringify({ t: `claim` }))
    expect(steerer.lastFrame(`presence`)).toMatchObject({ steererId: null })
  })

  test(`kill requires steer perm and reaches the publisher`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const watcher = connectViewer(hub, { perm: `view`, sub: `w` })
    hub.onMessage(watcher, JSON.stringify({ t: `kill` }))
    expect(pub.lastFrame(`kill`)).toBeUndefined()

    const steerer = connectViewer(hub, { perm: `steer`, sub: `s` })
    hub.onMessage(steerer, JSON.stringify({ t: `kill` }))
    expect(pub.lastFrame(`kill`)).toMatchObject({ t: `kill` })
  })

  test(`bye closes the room and evicts viewers`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const viewer = connectViewer(hub)
    hub.onMessage(pub, JSON.stringify({ t: `bye`, outcome: `done` }))
    expect(viewer.lastFrame(`bye`)).toMatchObject({ outcome: `done` })
    expect(viewer.closed?.code).toBe(4001)
    expect(hub.sessionInfo(`sess-1`)).toEqual({ live: false })
  })

  test(`publisher drop marks stale; re-hello resumes the same room`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const viewer = connectViewer(hub)
    hub.onClose(pub)
    expect(hub.sessionInfo(`sess-1`)).toMatchObject({ live: false, viewers: 1 })

    const pub2 = connectPublisher(hub)
    expect(hub.sessionInfo(`sess-1`)).toMatchObject({ live: true, viewers: 1 })
    output(hub, pub2, `resumed`)
    expect(
      viewer.outputs().map((o) => new TextDecoder().decode(o)).at(-1)
    ).toBe(`resumed`)
  })

  test(`slow consumer gets frames dropped, then a resync on recovery`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const viewer = connectViewer(hub)

    viewer.buffered = 10 * 1024 * 1024 // saturated
    output(hub, pub, `dropped`)
    expect(viewer.outputs().length).toBe(0)

    viewer.buffered = 0 // drained
    output(hub, pub, `after`)
    expect(
      viewer.outputs().map((o) => new TextDecoder().decode(o))
    ).toEqual([`after`])
    // Publisher was asked for a full repaint for that viewer.
    expect(pub.lastFrame(`resync`)).toMatchObject({ t: `resync` })
  })

  test(`viewers cannot forge output frames`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const viewer = connectViewer(hub)
    const other = connectViewer(hub, { sub: `other` })
    output(hub, viewer as unknown as FakeSocket, `forged`)
    expect(other.outputs().length).toBe(0)
    expect(pub.outputs().length).toBe(0)
  })
})
