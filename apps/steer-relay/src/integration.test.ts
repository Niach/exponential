// End-to-end smoke through the real Bun server: ticket auth on upgrade,
// hello/join, the scrubbed activity channel (reset + replay + live fan-out),
// input + semantic answers, and the secret-authed admin endpoints.

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
      role: `viewer`,
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

/** EXP-783: every publisher numbers its activity frames, so the tests do too
 *  — one 0-based counter per publisher socket, exactly like a device's
 *  recorder. Returns the seq it sent, which is what the relay echoes back and
 *  what `activity_synced` names its span with. */
const publisherSeqs = new WeakMap<WebSocket, number>()
function sendActivity(pub: WebSocket, event: unknown): number {
  const seq = publisherSeqs.get(pub) ?? 0
  publisherSeqs.set(pub, seq + 1)
  pub.send(JSON.stringify({ t: `activity`, event, seq }))
  return seq
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

    const noAuth = await fetch(`${base}/sessions/no-such`)
    expect(noAuth.status).toBe(401)

    const authed = await fetch(`${base}/sessions/no-such`, {
      headers: { "x-relay-secret": `integration-secret` },
    })
    expect(authed.ok).toBe(true)
    expect(await authed.json()).toEqual({ live: false })
  })

  test(`activity channel: reset + replay, live fan-out, input, answer, kill, bye`, async () => {
    const sessionId = `sess-e2e`

    // Publishers still send the removed public-activity feature's
    // activityPublic flag in hello — non-strict parsing must ignore it.
    const pub = await connect(
      ticket({ role: `publisher`, sub: `desktop-user`, sessionId })
    )
    const pubIn = collector(pub)
    pub.send(
      JSON.stringify({
        t: `hello`,
        sessionId,
        issueId: `i1`,
        activityPublic: false,
      })
    )
    // Activity emitted BEFORE anyone joins → the replayable log + lastDiff.
    const thinkingSeq = sendActivity(pub, { kind: `narration`, text: `thinking` })
    sendActivity(pub, { kind: `diff`, diff: `+ line` })

    const member = await connect(
      ticket({ role: `viewer`, sub: `member-user`, sessionId })
    )
    const memberIn = collector(member)
    member.send(JSON.stringify({ t: `join`, channel: `activity` }))

    // The relay owns the "clear your feed" moment — it arrives BEFORE the
    // replay, so clients never wipe on dial/open themselves.
    expect(await memberIn.nextJson()).toEqual({ t: `activity_reset` })
    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `narration`, text: `thinking` },
    })
    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `diff`, diff: `+ line` },
    })
    // EXP-656: the join replay ends with an explicit marker. EXP-783: it names
    // the span of the LOG it replayed (the latest-wins diff is not in it).
    expect(await memberIn.nextJson()).toEqual({
      t: `activity_synced`,
      firstSeq: thinkingSeq,
      lastSeq: thinkingSeq,
    })

    // Live activity — including the v2 kinds — reaches the member intact.
    const question = {
      kind: `question`,
      text: `Which color?`,
      options: [
        { label: `Red`, key: `1`, description: `warm` },
        { label: `Blue`, key: `2` },
      ],
      id: `toolu_1#0`,
      askId: `toolu_1`,
      index: 1,
      total: 2,
      header: `Palette`,
    }
    const questionSeq = sendActivity(pub, question)
    expect(await memberIn.nextJson()).toEqual({
      t: `activity`,
      event: question,
      seq: questionSeq,
    })

    // A joined viewer's input and answers reach the publisher directly
    // (EXP-312: steering is seamless and owner-only — no claim, no perm).
    member.send(JSON.stringify({ t: `input`, data: `yes\n` }))
    expect(await pubIn.nextJson()).toMatchObject({ t: `input`, data: `yes\n` })

    member.send(
      JSON.stringify({
        t: `answer`,
        questionId: `toolu_1#0`,
        askId: `toolu_1`,
        keys: [`2`],
      })
    )
    expect(await pubIn.nextJson()).toEqual({
      t: `answer`,
      questionId: `toolu_1#0`,
      askId: `toolu_1`,
      keys: [`2`],
    })

    // The publisher acks + resolves the card; both are ordinary activity.
    sendActivity(pub, {
      kind: `answer_ack`,
      id: `toolu_1#0`,
      askId: `toolu_1`,
    })
    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `answer_ack`, id: `toolu_1#0` },
    })
    sendActivity(pub, {
      kind: `question_resolved`,
      id: `toolu_1#0`,
      answers: [`Blue`],
    })
    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `question_resolved`, answers: [`Blue`] },
    })

    // activity_reset from the publisher clears the log and tells the audience.
    pub.send(JSON.stringify({ t: `activity_reset` }))
    expect(await memberIn.nextJson()).toEqual({ t: `activity_reset` })
    // The publisher keeps counting across its own reset (its journal is one
    // run), so the republished log starts ABOVE zero — which is exactly what
    // makes the late joiner's span `truncated`.
    const republishedSeq = sendActivity(pub, {
      kind: `narration`,
      text: `republished`,
    })
    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `narration`, text: `republished` },
    })

    // EXP-746: config_state and usage are latest-wins STATE — the room keeps
    // only the newest of each, out of the log, and replays them AFTER it, so
    // a late joiner paints its chips and meter from one frame apiece.
    const configState = {
      kind: `config_state`,
      options: [
        {
          id: `model`,
          label: `Model`,
          category: `model`,
          value: `opus`,
          values: [
            { id: `opus`, label: `Opus` },
            { id: `sonnet`, label: `Sonnet` },
          ],
        },
      ],
      currentMode: `plan`,
      modes: [{ id: `plan`, label: `Plan`, description: `Read-only` }],
      commands: [{ name: `compact`, description: `Compact the context` }],
    }
    const usage = {
      kind: `usage`,
      contextUsed: 124_000,
      contextSize: 200_000,
      costUsd: 1.24,
    }
    sendActivity(pub, { ...configState, currentMode: `code` })
    expect(await memberIn.nextJson()).toMatchObject({ t: `activity` })
    const configSeq = sendActivity(pub, configState)
    expect(await memberIn.nextJson()).toEqual({
      t: `activity`,
      event: configState,
      seq: configSeq,
    })
    const usageSeq = sendActivity(pub, usage)
    expect(await memberIn.nextJson()).toEqual({
      t: `activity`,
      event: usage,
      seq: usageSeq,
    })

    // EXP-850 §2/§3: the two new latest-wins kinds ride the same rails — the
    // workflow cards keyed PER ID, replayed between `turn` and the strip.
    const workflow = {
      kind: `workflow`,
      id: `toolu_017aGvi2moAfSykrRA4LmyT4`,
      name: `wire-probe`,
      description: `Probe the workflow progress wire`,
      status: `running`,
      phases: [
        { index: 1, title: `Alpha` },
        { index: 2, title: `Beta` },
      ],
      agents: [
        {
          index: 1,
          label: `alpha:one`,
          phaseIndex: 1,
          agentId: `a0ce244c651aaa623`,
          model: `claude-haiku-4-5-20251001`,
          state: `running`,
        },
      ],
    }
    const backgroundTasks = {
      kind: `background_tasks`,
      tasks: [
        {
          id: `w5zr2977l`,
          kind: `workflow`,
          description: `Probe the workflow progress wire`,
          toolId: `toolu_017aGvi2moAfSykrRA4LmyT4`,
        },
      ],
    }
    const workflowSeq = sendActivity(pub, workflow)
    expect(await memberIn.nextJson()).toEqual({
      t: `activity`,
      event: workflow,
      seq: workflowSeq,
    })
    const tasksSeq = sendActivity(pub, backgroundTasks)
    expect(await memberIn.nextJson()).toEqual({
      t: `activity`,
      event: backgroundTasks,
      seq: tasksSeq,
    })

    const late = await connect(
      ticket({ role: `viewer`, sub: `late-user`, sessionId })
    )
    const lateIn = collector(late)
    late.send(JSON.stringify({ t: `join`, channel: `activity` }))
    expect(await lateIn.nextJson()).toEqual({ t: `activity_reset` })
    expect(await lateIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `narration`, text: `republished` },
    })
    // The newest snapshot only, and after the log.
    expect(await lateIn.nextJson()).toEqual({
      t: `activity`,
      event: configState,
      seq: configSeq,
    })
    expect(await lateIn.nextJson()).toEqual({
      t: `activity`,
      event: usage,
      seq: usageSeq,
    })
    expect(await lateIn.nextJson()).toEqual({
      t: `activity`,
      event: workflow,
      seq: workflowSeq,
    })
    expect(await lateIn.nextJson()).toEqual({
      t: `activity`,
      event: backgroundTasks,
      seq: tasksSeq,
    })
    // The log this room holds starts above zero, so the span it names is a
    // TAIL: the pages below it live in the device's journal.
    expect(await lateIn.nextJson()).toEqual({
      t: `activity_synced`,
      firstSeq: republishedSeq,
      lastSeq: republishedSeq,
      truncated: true,
    })
    late.close()

    // EXP-746: live-config steering rides the same viewer gate, and a BLANK
    // value (the "CLI default" choice) survives the round trip.
    member.send(
      JSON.stringify({ t: `set_config`, id: `model`, value: `opus` })
    )
    expect(await pubIn.nextJson()).toEqual({
      t: `set_config`,
      id: `model`,
      value: `opus`,
    })
    member.send(JSON.stringify({ t: `set_config`, id: `model`, value: `` }))
    expect(await pubIn.nextJson()).toEqual({
      t: `set_config`,
      id: `model`,
      value: ``,
    })
    member.send(JSON.stringify({ t: `set_mode`, id: `plan` }))
    expect(await pubIn.nextJson()).toEqual({ t: `set_mode`, id: `plan` })

    // Kill from the joined viewer reaches the publisher.
    member.send(JSON.stringify({ t: `kill` }))
    expect(await pubIn.nextJson()).toMatchObject({ t: `kill` })

    // Publisher ends the session; the member gets bye + close.
    const closed = new Promise<number>((resolve) => {
      member.onclose = (e) => resolve(e.code)
    })
    pub.send(JSON.stringify({ t: `bye`, outcome: `done` }))
    expect(await memberIn.nextJson()).toMatchObject({ t: `bye`, outcome: `done` })
    expect(await closed).toBe(4001)

    pub.close()
  })

  test(`an unknown frame is dropped and the room survives`, async () => {
    const sessionId = `sess-legacy`
    const pub = await connect(
      ticket({ role: `publisher`, sub: `desktop-user`, sessionId })
    )
    pub.send(JSON.stringify({ t: `hello`, sessionId }))

    const member = await connect(
      ticket({ role: `viewer`, sub: `member-user`, sessionId })
    )
    const memberIn = collector(member)
    member.send(JSON.stringify({ t: `join`, channel: `activity` }))
    expect(await memberIn.nextJson()).toEqual({ t: `activity_reset` })
    // Nothing to replay: the span is the empty one at zero.
    expect(await memberIn.nextJson()).toEqual({
      t: `activity_synced`,
      firstSeq: 0,
      lastSeq: 0,
    })

    // A frame no schema in the union matches fails the parse and is dropped;
    // nothing is relayed and the socket stays open.
    pub.send(JSON.stringify({ t: `resize`, cols: 200, rows: 50 }))
    sendActivity(pub, { kind: `narration`, text: `after the unknown frame` })
    // The very next frame is the activity event.
    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `narration`, text: `after the unknown frame` },
    })

    const info = await fetch(`${base}/sessions/${sessionId}`, {
      headers: { "x-relay-secret": `integration-secret` },
    })
    expect(await info.json()).toMatchObject({ live: true, viewers: 1 })

    pub.send(JSON.stringify({ t: `bye`, outcome: `done` }))
    pub.close()
    member.close()
  })

  // EXP-700: server-side input injection — POST /sessions/:id/input relays
  // text into the publisher as owner input, then a separate `\r` submit frame.
  test(`server input injection reaches the publisher`, async () => {
    const sessionId = `sess-inject`
    const pub = await connect(
      ticket({ role: `publisher`, sub: `desktop-user`, sessionId })
    )
    const pubIn = collector(pub)
    pub.send(JSON.stringify({ t: `hello`, sessionId }))

    // The hello rides the socket — wait for the room before POSTing.
    for (let i = 0; i < 20; i++) {
      const res = await fetch(`${base}/sessions/${sessionId}`, {
        headers: { "x-relay-secret": `integration-secret` },
      })
      if (((await res.json()) as { live?: boolean }).live) break
      await new Promise((r) => setTimeout(r, 25))
    }

    const post = (body: unknown, headers: Record<string, string> = {}) =>
      fetch(`${base}/sessions/${sessionId}/input`, {
        method: `POST`,
        headers: {
          "content-type": `application/json`,
          "x-relay-secret": `integration-secret`,
          ...headers,
        },
        body: JSON.stringify(body),
      })

    const ok = await post({ text: `hello there` })
    expect(await ok.json()).toEqual({ ok: true, delivered: true })
    expect(await pubIn.nextJson()).toEqual({ t: `input`, data: `hello there` })
    expect(await pubIn.nextJson()).toEqual({ t: `input`, data: `\r` })

    // Same secret gate as every admin endpoint.
    const noAuth = await fetch(`${base}/sessions/${sessionId}/input`, {
      method: `POST`,
      headers: { "content-type": `application/json` },
      body: JSON.stringify({ text: `x` }),
    })
    expect(noAuth.status).toBe(401)

    // Malformed bodies are refused, not silently dropped.
    expect((await post({})).status).toBe(400)
    expect((await post({ text: 42 })).status).toBe(400)
    expect((await post({ text: `` })).status).toBe(400)
    expect((await post({ text: `a`.repeat(16 * 1024 + 1) })).status).toBe(400)

    // No live publisher reads as not-delivered — the kill contract.
    const gone = await fetch(`${base}/sessions/no-such-session/input`, {
      method: `POST`,
      headers: {
        "content-type": `application/json`,
        "x-relay-secret": `integration-secret`,
      },
      body: JSON.stringify({ text: `x` }),
    })
    expect(await gone.json()).toEqual({ ok: true, delivered: false })

    pub.send(JSON.stringify({ t: `bye`, outcome: `done` }))
    pub.close()
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
    const ws = await connect(ticket({ sessionId: `sess-flood` }))
    ws.close()
  })

  test(`admin endpoints throttle wrong-secret attempts`, async () => {
    let saw429 = false
    for (let i = 0; i < 150 && !saw429; i++) {
      const res = await fetch(`${base}/sessions/u1`, {
        headers: { "x-relay-secret": `wrong-${i}` },
      })
      expect([401, 429]).toContain(res.status)
      saw429 = res.status === 429
    }
    expect(saw429).toBe(true)

    // The secret-bearing web server keeps working through the flood — only
    // failed auth is throttled.
    const authed = await fetch(`${base}/sessions/u1`, {
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
