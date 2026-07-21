// End-to-end smoke through the real Bun server: ticket auth on upgrade,
// hello/join, binary output fan-out with ring replay, claim + input, and the
// secret-authed admin endpoints.

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

function ticket(overrides: Partial<SteerTicketClaims>): string {
  const now = Math.floor(Date.now() / 1000)
  return signSteerTicket(
    {
      sub: `user-1`,
      team: `team-1`,
      role: `viewer`,
      perm: `view`,
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

describe(`steer relay end-to-end`, () => {
  test(`rejects bad tickets at upgrade`, async () => {
    const res = await fetch(`${base.replace(`http`, `http`)}/ws?ticket=garbage`, {
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
      perm: `view`,
      sessionId: `sess-x`,
    } as unknown as Partial<SteerTicketClaims>)
    const res = await fetch(`${base}/ws?ticket=${encodeURIComponent(stale)}`, {
      headers: { upgrade: `websocket`, connection: `upgrade` },
    })
    expect(res.status).toBe(401)
  })

  test(`healthz is open, admin requires the secret`, async () => {
    const health = await fetch(`${base}/healthz`)
    expect(health.ok).toBe(true)

    const noAuth = await fetch(`${base}/devices/u1`)
    expect(noAuth.status).toBe(401)

    const authed = await fetch(`${base}/devices/u1`, {
      headers: { "x-relay-secret": `integration-secret` },
    })
    expect(authed.ok).toBe(true)
    expect(await authed.json()).toEqual({ devices: [] })
  })

  test(`publisher → viewer: replay, live output, claim, input, kill, bye`, async () => {
    const sessionId = `sess-e2e`

    const pub = await connect(
      ticket({ role: `publisher`, sub: `desktop-user`, sessionId })
    )
    const pubIn = collector(pub)
    pub.send(JSON.stringify({ t: `hello`, sessionId, issueId: `i1`, cols: 100, rows: 30 }))
    // hello triggers a presence broadcast to the publisher.
    expect(await pubIn.nextJson()).toMatchObject({ t: `presence` })

    // Emit output BEFORE the viewer joins (ring replay coverage).
    pub.send(new Uint8Array([0x01, ...new TextEncoder().encode(`early `)]))

    const viewer = await connect(
      ticket({ role: `viewer`, sub: `phone-user`, name: `Phone`, perm: `steer`, sessionId })
    )
    const viewerIn = collector(viewer)
    viewer.send(JSON.stringify({ t: `join` }))

    expect(await viewerIn.nextJson()).toMatchObject({ t: `resize`, cols: 100, rows: 30 })
    const replay = await viewerIn.next()
    expect(typeof replay).not.toBe(`string`)
    expect(new TextDecoder().decode((replay as Uint8Array).subarray(1))).toBe(`early `)
    expect(await viewerIn.nextJson()).toMatchObject({ t: `presence` })

    // Live output flows.
    pub.send(new Uint8Array([0x01, ...new TextEncoder().encode(`live!`)]))
    const live = await viewerIn.next()
    expect(new TextDecoder().decode((live as Uint8Array).subarray(1))).toBe(`live!`)

    // Presence reached the publisher too.
    expect(await pubIn.nextJson()).toMatchObject({
      t: `presence`,
      viewers: [{ userId: `phone-user`, name: `Phone`, perm: `steer` }],
    })

    // Claim + input forwarding.
    viewer.send(JSON.stringify({ t: `claim` }))
    expect(await viewerIn.nextJson()).toMatchObject({ t: `presence`, steererId: `phone-user` })
    expect(await pubIn.nextJson()).toMatchObject({ t: `presence`, steererId: `phone-user` })
    viewer.send(JSON.stringify({ t: `input`, data: `yes\n` }))
    expect(await pubIn.nextJson()).toMatchObject({ t: `input`, data: `yes\n` })

    // Kill from a steer-perm viewer reaches the publisher.
    viewer.send(JSON.stringify({ t: `kill` }))
    expect(await pubIn.nextJson()).toMatchObject({ t: `kill` })

    // Publisher ends the session; viewer gets bye + close.
    const closed = new Promise<number>((resolve) => {
      viewer.onclose = (e) => resolve(e.code)
    })
    pub.send(JSON.stringify({ t: `bye`, outcome: `done` }))
    expect(await viewerIn.nextJson()).toMatchObject({ t: `bye`, outcome: `done` })
    expect(await closed).toBe(4001)

    pub.close()
  })

  test(`activity channel: replays + steers for members; legacy activityPublic flag is ignored`, async () => {
    const sessionId = `sess-activity-e2e`

    // Older desktops still send the removed activityPublic flag in hello —
    // non-strict parsing must ignore it (EXP-90).
    const pub = await connect(
      ticket({ role: `publisher`, sub: `desktop-user`, sessionId })
    )
    const pubIn = collector(pub)
    pub.send(
      JSON.stringify({
        t: `hello`,
        sessionId,
        issueId: `i2`,
        cols: 80,
        rows: 24,
        activityPublic: false,
      })
    )
    expect(await pubIn.nextJson()).toMatchObject({ t: `presence`, viewers: [] })

    // Activity emitted before anyone joins → the replayable log + lastDiff.
    pub.send(
      JSON.stringify({
        t: `activity`,
        event: { kind: `narration`, text: `thinking` },
      })
    )
    pub.send(
      JSON.stringify({ t: `activity`, event: { kind: `diff`, diff: `+ line` } })
    )

    // A team member joins the activity channel on an ordinary viewer
    // ticket: replay (log, then latest diff), then presence — no resize, no
    // binary ring.
    const member = await connect(
      ticket({
        role: `viewer`,
        sub: `member-user`,
        name: `Member`,
        perm: `steer`,
        sessionId,
      })
    )
    const memberIn = collector(member)
    member.send(JSON.stringify({ t: `join`, channel: `activity` }))

    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `narration`, text: `thinking` },
    })
    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `diff`, diff: `+ line` },
    })
    expect(await memberIn.nextJson()).toMatchObject({
      t: `presence`,
      viewers: [{ userId: `member-user`, name: `Member`, perm: `steer` }],
      steererId: null,
    })

    // Live activity reaches the member.
    pub.send(
      JSON.stringify({
        t: `activity`,
        event: { kind: `tool`, name: `Edit`, detail: `a.ts` },
      })
    )
    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `tool`, name: `Edit` },
    })

    // claim{steal:true} → the member holds the claim; input reaches the
    // publisher.
    member.send(JSON.stringify({ t: `claim`, steal: true }))
    expect(await memberIn.nextJson()).toMatchObject({
      t: `presence`,
      steererId: `member-user`,
    })
    // The publisher heard the member join, then the claim, then the input.
    expect(await pubIn.nextJson()).toMatchObject({
      t: `presence`,
      viewers: [{ userId: `member-user` }],
      steererId: null,
    })
    expect(await pubIn.nextJson()).toMatchObject({
      t: `presence`,
      steererId: `member-user`,
    })
    member.send(JSON.stringify({ t: `input`, data: `y\n` }))
    expect(await pubIn.nextJson()).toMatchObject({ t: `input`, data: `y\n` })

    pub.send(JSON.stringify({ t: `bye`, outcome: `done` }))
    expect(await memberIn.nextJson()).toMatchObject({ t: `bye`, outcome: `done` })
    pub.close()
  })

  test(`remote start routes through the control socket`, async () => {
    const desktop = await connect(
      ticket({ role: `control`, sub: `owner-1`, deviceLabel: `Test Box` })
    )
    const desktopIn = collector(desktop)
    desktop.send(JSON.stringify({ t: `online`, deviceId: `dev-9`, deviceLabel: `Test Box` }))

    // Presence shows up on the admin endpoint (poll until registered).
    let devices: { deviceId: string }[] = []
    for (let i = 0; i < 20 && devices.length === 0; i++) {
      const res = await fetch(`${base}/devices/owner-1`, {
        headers: { "x-relay-secret": `integration-secret` },
      })
      devices = ((await res.json()) as { devices: { deviceId: string }[] }).devices
      if (devices.length === 0) await new Promise((r) => setTimeout(r, 25))
    }
    expect(devices).toMatchObject([{ deviceId: `dev-9` }])

    const start = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({ userId: `owner-1`, deviceId: `dev-9`, issueId: `issue-42` }),
    })
    expect(start.ok).toBe(true)
    expect(await desktopIn.nextJson()).toEqual({ t: `start_session`, issueId: `issue-42` })

    // Launch options (EXP-149) ride the same frame.
    const startWithOptions = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({
        userId: `owner-1`,
        deviceId: `dev-9`,
        issueId: `issue-43`,
        model: `opus`,
        effort: `high`,
        ultracode: false,
        planMode: true,
      }),
    })
    expect(startWithOptions.ok).toBe(true)
    expect(await desktopIn.nextJson()).toEqual({
      t: `start_session`,
      issueId: `issue-43`,
      model: `opus`,
      effort: `high`,
      ultracode: false,
      planMode: true,
    })

    const offline = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({ userId: `owner-1`, deviceId: `gone`, issueId: `issue-42` }),
    })
    expect(offline.status).toBe(404)

    desktop.close()
  })

  test(`batch remote start routes a fat frame; bad shapes are 400`, async () => {
    const desktop = await connect(
      ticket({ role: `control`, sub: `owner-2`, deviceLabel: `Batch Box` })
    )
    const desktopIn = collector(desktop)
    desktop.send(JSON.stringify({ t: `online`, deviceId: `dev-batch` }))

    let devices: { deviceId: string }[] = []
    for (let i = 0; i < 20 && devices.length === 0; i++) {
      const res = await fetch(`${base}/devices/owner-2`, {
        headers: { "x-relay-secret": `integration-secret` },
      })
      devices = ((await res.json()) as { devices: { deviceId: string }[] }).devices
      if (devices.length === 0) await new Promise((r) => setTimeout(r, 25))
    }
    expect(devices).toMatchObject([{ deviceId: `dev-batch` }])

    const repo = {
      repositoryId: `repo-1`,
      fullName: `acme/api`,
      defaultBranch: `main`,
    }
    const postStart = (body: unknown) =>
      fetch(`${base}/start`, {
        method: `POST`,
        headers: {
          "x-relay-secret": `integration-secret`,
          "content-type": `application/json`,
        },
        body: JSON.stringify(body),
      })

    const batch = await postStart({
      userId: `owner-2`,
      deviceId: `dev-batch`,
      issueIds: [`issue-1`, `issue-2`],
      teamId: `team-1`,
      repo,
      ultracode: true,
    })
    expect(batch.ok).toBe(true)
    expect(await desktopIn.nextJson()).toEqual({
      t: `start_session`,
      issueIds: [`issue-1`, `issue-2`],
      teamId: `team-1`,
      repo,
      ultracode: true,
    })

    // 400 cases — every one is rejected before the hub is touched.
    const bothSubjects = await postStart({
      userId: `owner-2`,
      deviceId: `dev-batch`,
      issueId: `issue-1`,
      issueIds: [`issue-2`],
      teamId: `team-1`,
      repo,
    })
    expect(bothSubjects.status).toBe(400)

    const noTeam = await postStart({
      userId: `owner-2`,
      deviceId: `dev-batch`,
      issueIds: [`issue-1`],
      repo,
    })
    expect(noTeam.status).toBe(400)

    const noRepo = await postStart({
      userId: `owner-2`,
      deviceId: `dev-batch`,
      issueIds: [`issue-1`],
      teamId: `team-1`,
    })
    expect(noRepo.status).toBe(400)

    const tooMany = await postStart({
      userId: `owner-2`,
      deviceId: `dev-batch`,
      issueIds: Array.from({ length: 31 }, (_, i) => `issue-${i}`),
      teamId: `team-1`,
      repo,
    })
    expect(tooMany.status).toBe(400)

    const repoMissingBranch = await postStart({
      userId: `owner-2`,
      deviceId: `dev-batch`,
      issueIds: [`issue-1`],
      teamId: `team-1`,
      repo: { repositoryId: `repo-1`, fullName: `acme/api` },
    })
    expect(repoMissingBranch.status).toBe(400)

    const nonStringMember = await postStart({
      userId: `owner-2`,
      deviceId: `dev-batch`,
      issueIds: [`issue-1`, 42],
      teamId: `team-1`,
      repo,
    })
    expect(nonStringMember.status).toBe(400)

    desktop.close()
  })

  // Runs LAST: it deliberately drains the shared failed-auth bucket (no
  // TRUST_PROXY here, so every request keys to the `unknown` fallback), which
  // would 429 any later bad-ticket assertions.
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
    const ws = await connect(ticket({ sessionId: `sess-flood` }))
    ws.close()
  })
})
