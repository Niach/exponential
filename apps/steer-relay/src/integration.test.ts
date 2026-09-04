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

// EXP-710: `Record<string, unknown>` on purpose — a ticket minted seconds
// before a deploy still carries dropped claims (`deviceLabel`), and the relay
// must keep verifying it.
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

/** EXP-672: with the outbound presence listing gone, "has the control socket
 *  registered yet?" is observable only through routing — `/start` answers 404
 *  and sends NOTHING until the `online` frame lands, so retrying the real
 *  call is both the wait and the assertion. */
async function startWhenOnline(body: unknown): Promise<Response> {
  for (let i = 0; i < 40; i++) {
    const res = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify(body),
    })
    if (res.ok) return res
    await res.text()
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
    pub.send(
      JSON.stringify({
        t: `activity`,
        event: { kind: `narration`, text: `thinking` },
      })
    )
    pub.send(
      JSON.stringify({ t: `activity`, event: { kind: `diff`, diff: `+ line` } })
    )

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
    // EXP-656: the join replay ends with an explicit marker.
    expect(await memberIn.nextJson()).toEqual({ t: `activity_synced` })

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
    pub.send(JSON.stringify({ t: `activity`, event: question }))
    expect(await memberIn.nextJson()).toEqual({ t: `activity`, event: question })

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
    pub.send(
      JSON.stringify({
        t: `activity`,
        event: { kind: `answer_ack`, id: `toolu_1#0`, askId: `toolu_1` },
      })
    )
    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `answer_ack`, id: `toolu_1#0` },
    })
    pub.send(
      JSON.stringify({
        t: `activity`,
        event: {
          kind: `question_resolved`,
          id: `toolu_1#0`,
          answers: [`Blue`],
        },
      })
    )
    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `question_resolved`, answers: [`Blue`] },
    })

    // activity_reset from the publisher clears the log and tells the audience.
    pub.send(JSON.stringify({ t: `activity_reset` }))
    expect(await memberIn.nextJson()).toEqual({ t: `activity_reset` })
    pub.send(
      JSON.stringify({
        t: `activity`,
        event: { kind: `narration`, text: `republished` },
      })
    )
    expect(await memberIn.nextJson()).toMatchObject({
      t: `activity`,
      event: { kind: `narration`, text: `republished` },
    })

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
    expect(await memberIn.nextJson()).toEqual({ t: `activity_synced` })

    // A frame no schema in the union matches fails the parse and is dropped;
    // nothing is relayed and the socket stays open.
    pub.send(JSON.stringify({ t: `resize`, cols: 200, rows: 50 }))
    pub.send(
      JSON.stringify({
        t: `activity`,
        event: { kind: `narration`, text: `after the unknown frame` },
      })
    )
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

  test(`remote start routes through the control socket`, async () => {
    const desktop = await connect(
      // A pre-EXP-710 ticket, legacy `deviceLabel` claim and all: verification
      // is tolerant, so an in-flight desktop still connects across the deploy.
      ticket({ role: `control`, sub: `owner-1`, deviceLabel: `Test Box` })
    )
    const desktopIn = collector(desktop)
    desktop.send(
      JSON.stringify({
        t: `online`,
        deviceId: `dev-9`,
        deviceLabel: `Test Box`,
      })
    )

    const start = await startWhenOnline({
      userId: `owner-1`,
      deviceId: `dev-9`,
      issueId: `issue-42`,
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

    // EXP-432: a shared-device start rides startedBy end-to-end. A present
    // but mistyped startedBy is 400 (it would drop the frame desktop-side).
    const sharedStart = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({
        userId: `owner-1`,
        deviceId: `dev-9`,
        issueId: `issue-44`,
        startedBy: `requester-1`,
      }),
    })
    expect(sharedStart.ok).toBe(true)
    expect(await desktopIn.nextJson()).toEqual({
      t: `start_session`,
      issueId: `issue-44`,
      startedBy: `requester-1`,
    })
    const badStartedBy = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({
        userId: `owner-1`,
        deviceId: `dev-9`,
        issueId: `issue-45`,
        startedBy: 42,
      }),
    })
    expect(badStartedBy.status).toBe(400)

    // EXP-679: an agent-started run rides startedReason end-to-end. `agent`
    // is the only accepted value — anything else is a 400 rather than a frame
    // the desktop would drop after /start already answered ok.
    const agentStart = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({
        userId: `owner-1`,
        deviceId: `dev-9`,
        issueId: `issue-46`,
        startedReason: `agent`,
      }),
    })
    expect(agentStart.ok).toBe(true)
    expect(await desktopIn.nextJson()).toEqual({
      t: `start_session`,
      issueId: `issue-46`,
      startedReason: `agent`,
    })
    const badStartedReason = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({
        userId: `owner-1`,
        deviceId: `dev-9`,
        issueId: `issue-47`,
        startedReason: `bogus`,
      }),
    })
    expect(badStartedReason.status).toBe(400)

    const offline = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({ userId: `owner-1`, deviceId: `gone`, issueId: `issue-42` }),
    })
    expect(offline.status).toBe(404)

    // EXP-481: resume rides the start frame untouched.
    const resumeStart = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({
        userId: `owner-1`,
        deviceId: `dev-9`,
        issueId: `issue-46`,
        resume: true,
      }),
    })
    expect(resumeStart.ok).toBe(true)
    expect(await desktopIn.nextJson()).toEqual({
      t: `start_session`,
      issueId: `issue-46`,
      resume: true,
    })

    // EXP-637: a run resume is a subject of its own — it may carry the
    // issueId/actionId display hints, and it carries no launch options.
    const runResume = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({
        userId: `owner-1`,
        deviceId: `dev-9`,
        resumeSessionId: `sess-77`,
        teamId: `team-1`,
        actionId: `act-1`,
        actionName: `Refresh screenshots`,
        branch: `exp/refresh-screenshots-1a2b3c4d`,
      }),
    })
    expect(runResume.ok).toBe(true)
    expect(await desktopIn.nextJson()).toEqual({
      t: `start_session`,
      resumeSessionId: `sess-77`,
      teamId: `team-1`,
      actionId: `act-1`,
      actionName: `Refresh screenshots`,
      branch: `exp/refresh-screenshots-1a2b3c4d`,
    })

    // teamId is required (the relay routes without a DB read), and a present
    // hint key that doesn't parse is 400 like every other pinned shape.
    const resumeNoTeam = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({
        userId: `owner-1`,
        deviceId: `dev-9`,
        resumeSessionId: `sess-78`,
      }),
    })
    expect(resumeNoTeam.status).toBe(400)
    const resumeBadHint = await fetch(`${base}/start`, {
      method: `POST`,
      headers: {
        "x-relay-secret": `integration-secret`,
        "content-type": `application/json`,
      },
      body: JSON.stringify({
        userId: `owner-1`,
        deviceId: `dev-9`,
        resumeSessionId: `sess-79`,
        teamId: `team-1`,
        branch: 42,
      }),
    })
    expect(resumeBadHint.status).toBe(400)

    // EXP-481: the check-in nudge — secret-gated, delivered iff the control
    // socket is live.
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

  test(`batch remote start routes a fat frame; bad shapes are 400`, async () => {
    const desktop = await connect(
      ticket({ role: `control`, sub: `owner-2` })
    )
    const desktopIn = collector(desktop)
    desktop.send(JSON.stringify({ t: `online`, deviceId: `dev-batch` }))

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

    const batch = await startWhenOnline({
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

  test(`action remote start routes a fat frame; bad shapes are 400 (EXP-253)`, async () => {
    const desktop = await connect(
      ticket({ role: `control`, sub: `owner-3` })
    )
    const desktopIn = collector(desktop)
    desktop.send(
      JSON.stringify({
        t: `online`,
        deviceId: `dev-action`,
        caps: [`actions`],
      })
    )

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

    // Repo-backed action with options. The `online` frame above carried the
    // legacy `caps` array (EXP-672 ignores it) — routing still finds the
    // device.
    const repoBacked = await startWhenOnline({
      userId: `owner-3`,
      deviceId: `dev-action`,
      actionId: `action-1`,
      actionName: `Code review`,
      teamId: `team-1`,
      repo,
      model: `opus`,
      effort: `high`,
    })
    expect(repoBacked.ok).toBe(true)
    expect(await desktopIn.nextJson()).toEqual({
      t: `start_session`,
      actionId: `action-1`,
      actionName: `Code review`,
      teamId: `team-1`,
      repo,
      model: `opus`,
      effort: `high`,
    })

    // Repo-less action: no repo key on the frame at all.
    const repoLess = await postStart({
      userId: `owner-3`,
      deviceId: `dev-action`,
      actionId: `action-2`,
      actionName: `Groom backlog`,
      teamId: `team-1`,
    })
    expect(repoLess.ok).toBe(true)
    expect(await desktopIn.nextJson()).toEqual({
      t: `start_session`,
      actionId: `action-2`,
      actionName: `Groom backlog`,
      teamId: `team-1`,
    })

    // EXP-257: resolved input values + full options pass through verbatim.
    const inputs = [
      {
        key: `description`,
        label: `Description`,
        type: `text`,
        value: `Review PRs`,
        display: `Review PRs`,
      },
      {
        key: `repo`,
        label: `Repository`,
        type: `repo`,
        value: `repo-1`,
        display: `acme/api`,
      },
    ]
    const withInputs = await postStart({
      userId: `owner-3`,
      deviceId: `dev-action`,
      actionId: `builtin:create-action`,
      actionName: `Create action`,
      teamId: `team-1`,
      inputs,
      agent: `codex`,
    })
    expect(withInputs.ok).toBe(true)
    expect(await desktopIn.nextJson()).toEqual({
      t: `start_session`,
      actionId: `builtin:create-action`,
      actionName: `Create action`,
      teamId: `team-1`,
      inputs,
      agent: `codex`,
    })

    // A PRESENT but malformed inputs key must 400 (same stance as repo) —
    // and unknown entry keys are stripped by the rebuild, never forwarded.
    const badInputs = await postStart({
      userId: `owner-3`,
      deviceId: `dev-action`,
      actionId: `action-1`,
      actionName: `Code review`,
      teamId: `team-1`,
      inputs: [{ key: `x` }],
    })
    expect(badInputs.status).toBe(400)

    // 400 cases.
    const noTeam = await postStart({
      userId: `owner-3`,
      deviceId: `dev-action`,
      actionId: `action-1`,
      actionName: `Code review`,
    })
    expect(noTeam.status).toBe(400)

    const noName = await postStart({
      userId: `owner-3`,
      deviceId: `dev-action`,
      actionId: `action-1`,
      teamId: `team-1`,
    })
    expect(noName.status).toBe(400)

    const dualSubject = await postStart({
      userId: `owner-3`,
      deviceId: `dev-action`,
      actionId: `action-1`,
      actionName: `Code review`,
      teamId: `team-1`,
      issueId: `issue-1`,
    })
    expect(dualSubject.status).toBe(400)

    // A PRESENT but malformed repo must 400 (a bad repo would silently drop
    // the frame at the desktop's serde parse).
    const badRepo = await postStart({
      userId: `owner-3`,
      deviceId: `dev-action`,
      actionId: `action-1`,
      actionName: `Code review`,
      teamId: `team-1`,
      repo: { repositoryId: `repo-1` },
    })
    expect(badRepo.status).toBe(400)

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
