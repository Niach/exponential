// End-to-end smoke through the real Bun server: ticket auth on upgrade (the
// role allowlist: control + the call pair), device presence, the
// secret-authed admin routes down the control socket, the VAPP-43 content
// refusal, and a voice call.

import { afterAll, beforeAll, describe, expect, test } from "bun:test"
import { signSteerTicket, type SteerTicketClaims } from "@exp/steer-ticket"

process.env.STEER_RELAY_SECRET = `integration-secret`

const { default: serverConfig } = await import("./index")

let server: ReturnType<typeof Bun.serve>
let base: string
let wsBase: string

beforeAll(() => {
  server = Bun.serve({ ...serverConfig, port: 0 })
  base = `http://localhost:${server.port}`
  wsBase = `ws://localhost:${server.port}`
})

afterAll(() => {
  server.stop(true)
})

// `Record<string, unknown>` on purpose: verification tolerates unknown claims.
function ticket(
  overrides: Partial<SteerTicketClaims> & Record<string, unknown>
): string {
  const now = Math.floor(Date.now() / 1000)
  return signSteerTicket(
    {
      sub: `user-1`,
      team: `team-1`,
      role: `control`,
      iat: now,
      exp: now + 60,
      ...overrides,
    },
    process.env.STEER_RELAY_SECRET!
  )
}

function connect(t: string): Promise<WebSocket> {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`${wsBase}/ws?ticket=${encodeURIComponent(t)}`)
    ws.binaryType = `arraybuffer`
    ws.onopen = () => resolve(ws)
    ws.onerror = (e) => reject(e)
  })
}

/** Collects incoming messages; lets tests await the next one. */
function collector(ws: WebSocket) {
  const queue: (string | Uint8Array)[] = []
  const waiters: ((msg: string | Uint8Array) => void)[] = []
  ws.onmessage = (event) => {
    const msg =
      typeof event.data === `string`
        ? event.data
        : new Uint8Array(event.data as ArrayBuffer)
    const waiter = waiters.shift()
    if (waiter) waiter(msg)
    else queue.push(msg)
  }
  return {
    next(timeoutMs = 2000): Promise<string | Uint8Array> {
      const queued = queue.shift()
      if (queued !== undefined) return Promise.resolve(queued)
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`timeout`)), timeoutMs)
        waiters.push((msg) => {
          clearTimeout(timer)
          resolve(msg)
        })
      })
    },
    async nextJson(timeoutMs = 2000): Promise<Record<string, unknown>> {
      const msg = await this.next(timeoutMs)
      if (typeof msg !== `string`) throw new Error(`expected text frame`)
      return JSON.parse(msg)
    },
  }
}

/** EXP-672: with the outbound presence listing gone, "has the control socket
 *  registered yet?" is observable only through routing — the nudge answers
 *  `delivered: false` until the `online` frame lands, so retrying the real
 *  call is both the wait and the assertion. */
async function nudgeWhenOnline(userId: string, deviceId: string): Promise<void> {
  for (let i = 0; i < 40; i++) {
    const res = await fetch(`${base}/devices/${userId}/${deviceId}/nudge`, {
      method: `POST`,
      headers: { "x-relay-secret": `integration-secret` },
    })
    const body = (await res.json()) as { delivered?: boolean }
    if (body.delivered) return
    await new Promise((r) => setTimeout(r, 25))
  }
  throw new Error(`control socket never registered`)
}

const admin = (path: string, body?: unknown) =>
  fetch(`${base}${path}`, {
    method: `POST`,
    headers: {
      "x-relay-secret": `integration-secret`,
      "content-type": `application/json`,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  })

describe(`steer relay end-to-end`, () => {
  test(`rejects bad tickets at upgrade`, async () => {
    const res = await fetch(`${base}/ws?ticket=garbage`, {
      headers: { upgrade: `websocket`, connection: `upgrade` },
    })
    expect(res.status).toBe(401)
  })

  test(`rejects signature-valid tickets with the removed public_viewer role`, async () => {
    // EXP-90 skew guard: a stale web instance that still mints the anonymous
    // public-activity role gets 401 at upgrade, never a socket.
    const stale = ticket({
      role: `public_viewer`,
      sub: `anon`,
      sessionId: `sess-x`,
    } as unknown as Partial<SteerTicketClaims>)
    const res = await fetch(`${base}/ws?ticket=${encodeURIComponent(stale)}`, {
      headers: { upgrade: `websocket`, connection: `upgrade` },
    })
    expect(res.status).toBe(401)
  })

  // VAPP-43: session content goes over eNet; an old build's publisher or
  // viewer ticket never gets a socket.
  test(`rejects publisher and viewer tickets at upgrade (VAPP-43)`, async () => {
    for (const role of [`publisher`, `viewer`]) {
      const stale = ticket({
        role,
        sessionId: `sess-x`,
      } as unknown as Partial<SteerTicketClaims>)
      const res = await fetch(`${base}/ws?ticket=${encodeURIComponent(stale)}`, {
        headers: { upgrade: `websocket`, connection: `upgrade` },
      })
      expect(res.status).toBe(401)
      expect(await res.text()).toBe(`Unauthorized: unknown_role`)
    }
  })

  test(`healthz is open, admin requires the secret`, async () => {
    const health = await fetch(`${base}/healthz`)
    expect(health.ok).toBe(true)

    const noAuth = await fetch(`${base}/stats`)
    expect(noAuth.status).toBe(401)

    const authed = await fetch(`${base}/stats`, {
      headers: { "x-relay-secret": `integration-secret` },
    })
    expect(authed.ok).toBe(true)
    const stats = (await authed.json()) as Record<string, unknown>
    expect(stats).not.toHaveProperty(`rooms`)
  })

  test(`the removed session routes are gone (VAPP-43)`, async () => {
    const info = await fetch(`${base}/sessions/sess-1`, {
      headers: { "x-relay-secret": `integration-secret` },
    })
    expect(info.status).toBe(404)
    const input = await admin(`/sessions/sess-1/input`, { text: `hi` })
    expect(input.status).toBe(404)
  })

  test(`kill, interrupt and compact go down the device's control socket`, async () => {
    const desktop = await connect(ticket({ role: `control`, sub: `host-1` }))
    const desktopIn = collector(desktop)
    desktop.send(JSON.stringify({ t: `online`, deviceId: `dev-k` }))
    await nudgeWhenOnline(`host-1`, `dev-k`)
    expect(await desktopIn.nextJson()).toEqual({ t: `check_in` })
    const device = { userId: `host-1`, deviceId: `dev-k` }

    // A body that names no device is a 400, not a guess.
    expect((await admin(`/sessions/sess-k/kill`)).status).toBe(400)
    expect((await admin(`/sessions/sess-k/kill`, { userId: `host-1` })).status).toBe(400)
    expect((await admin(`/sessions/sess-k/interrupt`, {})).status).toBe(400)
    expect((await admin(`/sessions/sess-k/compact`, { deviceId: `dev-k` })).status).toBe(400)

    const interrupt = await admin(`/sessions/sess-k/interrupt`, device)
    expect(await interrupt.json()).toEqual({ ok: true, delivered: true })
    expect(await desktopIn.nextJson()).toEqual({ t: `interrupt`, sessionId: `sess-k` })

    const compact = admin(`/sessions/sess-k/compact`, { ...device, keep: `the plan` })
    expect(await desktopIn.nextJson()).toEqual({
      t: `compact_request`,
      sessionId: `sess-k`,
      keep: `the plan`,
    })
    desktop.send(
      JSON.stringify({ t: `compact_verdict`, sessionId: `sess-k`, accepted: true })
    )
    expect(await (await compact).json()).toEqual({
      ok: true,
      delivered: true,
      accepted: true,
    })

    const kill = await admin(`/sessions/sess-k/kill`, device)
    expect(await kill.json()).toEqual({ ok: true, delivered: true })
    expect(await desktopIn.nextJson()).toEqual({ t: `kill`, sessionId: `sess-k` })

    // Another device (or another owner) is not connected: not delivered.
    const offline = await admin(`/sessions/sess-k/kill`, { userId: `host-2`, deviceId: `dev-k` })
    expect(await offline.json()).toEqual({ ok: true, delivered: false })
    const compactOffline = await admin(`/sessions/sess-k/compact`, {
      userId: `host-1`,
      deviceId: `dev-gone`,
    })
    expect(await compactOffline.json()).toEqual({
      ok: true,
      delivered: false,
      reason: `device_offline`,
    })
    desktop.close()
  })

  test(`a content frame on a control socket is refused and presence survives`, async () => {
    const desktop = await connect(ticket({ role: `control`, sub: `owner-c` }))
    const desktopIn = collector(desktop)
    desktop.send(JSON.stringify({ t: `online`, deviceId: `dev-c` }))
    await nudgeWhenOnline(`owner-c`, `dev-c`)
    expect(await desktopIn.nextJson()).toEqual({ t: `check_in` })

    desktop.send(
      JSON.stringify({ t: `activity`, event: { kind: `narration`, text: `secret` }, seq: 0 })
    )
    expect(await desktopIn.nextJson()).toEqual({ t: `error`, code: `content_refused` })
    // Still registered: the nudge reaches it.
    await nudgeWhenOnline(`owner-c`, `dev-c`)
    expect(await desktopIn.nextJson()).toEqual({ t: `check_in` })
    desktop.close()
  })

  test(`the check-in nudge reaches the control socket`, async () => {
    const desktop = await connect(ticket({ role: `control`, sub: `owner-1` }))
    const desktopIn = collector(desktop)
    desktop.send(JSON.stringify({ t: `online`, deviceId: `dev-9` }))
    await nudgeWhenOnline(`owner-1`, `dev-9`)
    expect(await desktopIn.nextJson()).toEqual({ t: `check_in` })

    // EXP-481: secret-gated, delivered iff the control socket is live.
    const noAuthNudge = await fetch(`${base}/devices/owner-1/dev-9/nudge`, {
      method: `POST`,
    })
    expect(noAuthNudge.status).toBe(401)
    const nudge = await fetch(`${base}/devices/owner-1/dev-9/nudge`, {
      method: `POST`,
      headers: { "x-relay-secret": `integration-secret` },
    })
    expect(await nudge.json()).toEqual({ ok: true, delivered: true })
    expect(await desktopIn.nextJson()).toEqual({ t: `check_in` })
    const nudgeOffline = await fetch(`${base}/devices/owner-1/gone/nudge`, {
      method: `POST`,
      headers: { "x-relay-secret": `integration-secret` },
    })
    expect(await nudgeOffline.json()).toEqual({ ok: true, delivered: false })

    desktop.close()
  })

  // The last two tests run LAST: they deliberately drain the shared failed-auth
  // bucket (no TRUST_PROXY here, so every request keys to the `unknown`
  // fallback), which would 429 any later bad-ticket/bad-secret assertions.
  test(`failed-auth floods never starve ticket-valid connects`, async () => {
    let saw429 = false
    for (let i = 0; i < 150 && !saw429; i++) {
      const res = await fetch(`${base}/ws?ticket=garbage-${i}`, {
        headers: { upgrade: `websocket`, connection: `upgrade` },
      })
      expect([401, 429]).toContain(res.status)
      saw429 = res.status === 429
    }
    expect(saw429).toBe(true)

    // A valid ticket still upgrades — it counts against a separate, larger
    // per-IP bucket (mirrors push-relay's failed-auth-only philosophy).
    const ws = await connect(ticket({ role: `control` }))
    ws.close()
  })

  test(`admin endpoints throttle wrong-secret attempts`, async () => {
    let saw429 = false
    for (let i = 0; i < 150 && !saw429; i++) {
      const res = await fetch(`${base}/stats`, {
        headers: { "x-relay-secret": `wrong-${i}` },
      })
      expect([401, 429]).toContain(res.status)
      saw429 = res.status === 429
    }
    expect(saw429).toBe(true)

    // The secret-bearing web server keeps working through the flood — only
    // failed auth is throttled.
    const authed = await fetch(`${base}/stats`, {
      headers: { "x-relay-secret": `integration-secret` },
    })
    expect(authed.status).toBe(200)
  })
})

// NEW-55: a voice call through the real server — the call roles pass the
// upgrade allowlist, the ring rides the control socket, and BINARY frames
// cross the pair untouched (a wrong kind never does).
describe(`voice call end-to-end (NEW-55)`, () => {
  test(`ring, answer, audio both ways, hang up`, async () => {
    const device = await connect(ticket({ role: `control`, sub: `voice-owner`, team: `` }))
    const deviceIn = collector(device)
    device.send(JSON.stringify({ t: `online`, deviceId: `mint` }))
    await nudgeWhenOnline(`voice-owner`, `mint`)
    expect(await deviceIn.nextJson()).toEqual({ t: `check_in` })

    const caller = await connect(
      ticket({
        role: `caller`,
        sub: `voice-owner`,
        team: ``,
        callId: `call-e2e`,
        agentId: `agent-1`,
        deviceId: `mint`,
        deviceOwnerId: `voice-owner`,
      })
    )
    const callerIn = collector(caller)
    expect(await deviceIn.nextJson()).toEqual({
      t: `call_ring`,
      callId: `call-e2e`,
      agentId: `agent-1`,
    })

    const host = await connect(
      ticket({ role: `call_host`, sub: `voice-owner`, team: ``, callId: `call-e2e`, deviceId: `mint` })
    )
    const hostIn = collector(host)
    expect(await hostIn.nextJson()).toEqual({ t: `call_ready` })
    expect(await callerIn.nextJson()).toEqual({ t: `call_ready` })

    // A caller's `down` frame is dropped; its `up` frame arrives byte-exact.
    caller.send(new Uint8Array([2, 0, 0, 9, 9]))
    const up = new Uint8Array([1, 0, 1, 0xfc, 0xff, 0xfe])
    caller.send(up)
    expect(await hostIn.next()).toEqual(up)

    const down = new Uint8Array([2, 0xff, 0xff, 1, 2, 3, 4])
    host.send(down)
    expect(await callerIn.next()).toEqual(down)

    host.send(JSON.stringify({ t: `call_event`, event: { t: `replyFinished` } }))
    expect(await callerIn.nextJson()).toEqual({
      t: `call_event`,
      event: { t: `replyFinished` },
    })

    const hostClosed = new Promise<number>((resolve) => {
      host.onclose = (e) => resolve(e.code)
    })
    caller.send(JSON.stringify({ t: `call_end` }))
    expect(await hostIn.nextJson()).toEqual({ t: `call_end` })
    expect(await hostClosed).toBe(4001)
    device.close()
  })

  test(`a caller for an offline device is told so`, async () => {
    const caller = await connect(
      ticket({
        role: `caller`,
        sub: `nobody`,
        team: ``,
        callId: `call-off`,
        agentId: `agent-1`,
        deviceId: `ghost`,
      })
    )
    const callerIn = collector(caller)
    expect(await callerIn.nextJson()).toEqual({ t: `error`, code: `device_offline` })
    expect(await callerIn.nextJson()).toEqual({ t: `bye`, outcome: `device_offline` })
  })
})
