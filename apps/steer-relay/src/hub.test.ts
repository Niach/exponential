import { describe, expect, test } from "bun:test"
import type { SteerTicketClaims } from "@exp/steer-ticket"
import { Hub, type RelaySocket } from "./hub"
import { CLOSE_SESSION_ENDED, CLOSE_SLOW_CONSUMER } from "./protocol"

class FakeSocket implements RelaySocket {
  sent: string[] = []
  closed: { code?: number; reason?: string } | null = null
  buffered = 0

  send(data: string) {
    this.sent.push(data)
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
  events(): { kind: string; [k: string]: unknown }[] {
    return this.framesOf(`activity`).map(
      (f) => f.event as { kind: string; [k: string]: unknown }
    )
  }
}

function claims(overrides: Partial<SteerTicketClaims>): SteerTicketClaims {
  const now = Math.floor(Date.now() / 1000)
  return {
    sub: `user-1`,
    team: `team-1`,
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
    JSON.stringify({ t: `hello`, sessionId, issueId: `issue-1` })
  )
  return sock
}

/** An authenticated member on a viewer ticket joining the scrubbed activity
 *  channel — the ONLY audience since EXP-249 removed the PTY mirror. */
function connectMember(
  hub: Hub,
  opts: { sub?: string; name?: string; perm?: `view` | `steer`; sessionId?: string } = {}
) {
  const sock = new FakeSocket()
  hub.onOpen(
    sock,
    claims({
      role: `viewer`,
      sub: opts.sub ?? `member-1`,
      name: opts.name ?? `Member`,
      perm: opts.perm ?? `steer`,
      sessionId: opts.sessionId ?? `sess-1`,
    })
  )
  hub.onMessage(sock, JSON.stringify({ t: `join`, channel: `activity` }))
  return sock
}

/** A socket carrying the REMOVED anonymous public_viewer role (EXP-90) — the
 *  Bun upgrade layer 401s these; if one ever reaches the hub anyway it must
 *  stay outside every audience. */
function connectStalePublicViewer(hub: Hub, sessionId = `sess-1`) {
  const sock = new FakeSocket()
  hub.onOpen(
    sock,
    claims({
      role: `public_viewer`,
      sub: `anon`,
      perm: `view`,
      sessionId,
    } as unknown as Partial<SteerTicketClaims>)
  )
  hub.onMessage(sock, JSON.stringify({ t: `join`, channel: `activity` }))
  return sock
}

const activity = (hub: Hub, pub: FakeSocket, event: unknown) =>
  hub.onMessage(pub, JSON.stringify({ t: `activity`, event }))

/** A legacy desktop's binary PTY output frame (opcode 0x01 + bytes). */
function binaryOutput(hub: Hub, sock: FakeSocket, text: string) {
  const payload = new TextEncoder().encode(text)
  const framed = new Uint8Array(payload.byteLength + 1)
  framed[0] = 0x01
  framed.set(payload, 1)
  hub.onMessage(sock, framed)
}

interface RoomInternals {
  activityLog: { framed: string; bytes: number }[]
  activityBytes: number
  lastDiff: { framed: string } | null
  lastPublisherActivity: number
}

function room(hub: Hub, sessionId = `sess-1`): RoomInternals {
  return (
    hub as unknown as { rooms: Map<string, RoomInternals> }
  ).rooms.get(sessionId)!
}

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
      // EXP-201: no advertisement on the online frame ⇒ claude-only (the
      // old-desktop compat default).
      { deviceId: `dev-1`, deviceLabel: `MacBook`, agents: [`claude`] },
    ])

    const routed = hub.startSession(`owner`, `dev-1`, { issueId: `issue-9` })
    expect(routed).toEqual({ ok: true })
    // Option-less start stays byte-identical to the pre-options frame.
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueId: `issue-9`,
    })

    expect(
      hub.startSession(`owner`, `dev-404`, { issueId: `issue-9` })
    ).toEqual({
      ok: false,
      reason: `device_offline`,
    })

    hub.onClose(desktop)
    expect(hub.devicesFor(`owner`)).toEqual([])
  })

  test(`startSession passes launch options through to the frame`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(desktop, JSON.stringify({ t: `online`, deviceId: `dev-1` }))

    const routed = hub.startSession(`owner`, `dev-1`, { issueId: `issue-9` }, {
      model: `opus`,
      effort: ``,
      ultracode: true,
      planMode: false,
    })
    expect(routed).toEqual({ ok: true })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueId: `issue-9`,
      model: `opus`,
      effort: ``,
      ultracode: true,
      planMode: false,
    })

    // Partial options: undefined fields never reach the wire.
    hub.startSession(`owner`, `dev-1`, { issueId: `issue-10` }, {
      model: `sonnet`,
    })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueId: `issue-10`,
      model: `sonnet`,
    })

    // EXP-201: agent + skipPermissions ride the frame like any option.
    hub.startSession(`owner`, `dev-1`, { issueId: `issue-11` }, {
      agent: `codex`,
      skipPermissions: true,
    })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueId: `issue-11`,
      agent: `codex`,
      skipPermissions: true,
    })
  })

  test(`online advertises installed agents (EXP-201)`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(
      desktop,
      JSON.stringify({
        t: `online`,
        deviceId: `dev-1`,
        deviceLabel: `MacBook`,
        agents: [`claude`, `pi`],
      })
    )
    expect(hub.devicesFor(`owner`)).toMatchObject([
      { deviceId: `dev-1`, agents: [`claude`, `pi`] },
    ])
  })

  test(`startSession routes a batch subject as a fat start_session frame`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(desktop, JSON.stringify({ t: `online`, deviceId: `dev-1` }))

    const repo = {
      repositoryId: `repo-1`,
      fullName: `acme/api`,
      defaultBranch: `main`,
    }
    const routed = hub.startSession(
      `owner`,
      `dev-1`,
      { issueIds: [`issue-1`, `issue-2`], teamId: `team-1`, repo },
      { ultracode: true }
    )
    expect(routed).toEqual({ ok: true })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueIds: [`issue-1`, `issue-2`],
      teamId: `team-1`,
      repo,
      ultracode: true,
    })

    // Undefined options never reach the batch frame either.
    hub.startSession(`owner`, `dev-1`, {
      issueIds: [`issue-3`],
      teamId: `team-1`,
      repo,
    })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueIds: [`issue-3`],
      teamId: `team-1`,
      repo,
    })
  })

  test(`batch start to an offline device reports device_offline`, () => {
    const hub = new Hub()
    expect(
      hub.startSession(`owner`, `dev-gone`, {
        issueIds: [`issue-1`, `issue-2`],
        teamId: `team-1`,
        repo: {
          repositoryId: `repo-1`,
          fullName: `acme/api`,
          defaultBranch: `main`,
        },
      })
    ).toEqual({ ok: false, reason: `device_offline` })
  })

  test(`online advertises capabilities; absent caps default to [] (EXP-253)`, () => {
    const hub = new Hub()
    const modern = new FakeSocket()
    hub.onOpen(modern, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(
      modern,
      JSON.stringify({
        t: `online`,
        deviceId: `dev-1`,
        agents: [`claude`],
        caps: [`actions`],
      })
    )
    const legacy = new FakeSocket()
    hub.onOpen(legacy, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(legacy, JSON.stringify({ t: `online`, deviceId: `dev-2` }))

    expect(hub.devicesFor(`owner`)).toMatchObject([
      { deviceId: `dev-1`, caps: [`actions`] },
      // Old desktops never advertise ⇒ [] — the web server strictly gates
      // action starts on the capability, so they are never targeted.
      { deviceId: `dev-2`, caps: [] },
    ])
  })

  test(`startSession routes an action subject as a fat start_session frame (EXP-253)`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(desktop, JSON.stringify({ t: `online`, deviceId: `dev-1` }))

    const repo = {
      repositoryId: `repo-1`,
      fullName: `acme/api`,
      defaultBranch: `main`,
    }
    // Repo-backed action with launch options.
    const routed = hub.startSession(
      `owner`,
      `dev-1`,
      {
        actionId: `action-1`,
        actionName: `Code review`,
        teamId: `team-1`,
        repo,
      },
      { model: `opus`, effort: `high` }
    )
    expect(routed).toEqual({ ok: true })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      actionId: `action-1`,
      actionName: `Code review`,
      teamId: `team-1`,
      repo,
      model: `opus`,
      effort: `high`,
    })

    // Repo-less action: the repo key never reaches the wire, nor do
    // undefined options.
    hub.startSession(`owner`, `dev-1`, {
      actionId: `action-2`,
      actionName: `Groom backlog`,
      teamId: `team-1`,
    })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      actionId: `action-2`,
      actionName: `Groom backlog`,
      teamId: `team-1`,
    })
  })

  test(`action start to an offline device reports device_offline`, () => {
    const hub = new Hub()
    expect(
      hub.startSession(`owner`, `dev-gone`, {
        actionId: `action-1`,
        actionName: `Code review`,
        teamId: `team-1`,
      })
    ).toEqual({ ok: false, reason: `device_offline` })
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
  test(`member join: activity_reset, then replay, then presence`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    activity(hub, pub, { kind: `narration`, text: `before-join` })
    activity(hub, pub, { kind: `diff`, diff: `old diff` })
    activity(hub, pub, { kind: `tool`, name: `Bash` })
    activity(hub, pub, { kind: `diff`, diff: `new diff` })

    const member = connectMember(hub)
    const frames = member.frames()
    // The reset lands FIRST: clients no longer wipe their feed on dial/open,
    // so the relay owns the "clear now" moment.
    expect(frames[0]).toEqual({ t: `activity_reset` })
    // Then the log in order, then ONLY the latest diff, then presence.
    expect(member.events().map((e) => e.kind)).toEqual([
      `narration`,
      `tool`,
      `diff`,
    ])
    expect(member.events().at(-1)?.diff).toBe(`new diff`)
    expect(frames.at(-1)).toMatchObject({
      t: `presence`,
      viewers: [{ userId: `member-1`, name: `Member`, perm: `steer` }],
      steererId: null,
    })

    // The publisher's presence broadcast lists the member too.
    expect(pub.lastFrame(`presence`)).toMatchObject({
      viewers: [{ userId: `member-1`, perm: `steer` }],
    })

    activity(hub, pub, { kind: `tool`, name: `Edit`, detail: `src/a.ts` })
    expect(member.events().at(-1)).toMatchObject({ kind: `tool`, name: `Edit` })
  })

  test(`a pty/absent-channel join is refused with pty_removed and joins nothing`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)

    const legacy = new FakeSocket()
    hub.onOpen(
      legacy,
      claims({ role: `viewer`, sub: `old`, perm: `steer`, sessionId: `sess-1` })
    )
    hub.onMessage(legacy, JSON.stringify({ t: `join` }))
    expect(legacy.frames()).toEqual([{ t: `error`, code: `pty_removed` }])
    expect(legacy.closed).toBeNull()

    hub.onMessage(legacy, JSON.stringify({ t: `join`, channel: `pty` }))
    expect(legacy.framesOf(`error`).length).toBe(2)

    // It entered no audience: no presence, and activity never reaches it.
    expect(pub.lastFrame(`presence`)).toMatchObject({ viewers: [] })
    activity(hub, pub, { kind: `narration`, text: `secret` })
    expect(legacy.framesOf(`activity`).length).toBe(0)

    // Nor can it steer.
    hub.onMessage(legacy, JSON.stringify({ t: `claim` }))
    hub.onMessage(legacy, JSON.stringify({ t: `input`, data: `x` }))
    expect(pub.lastFrame(`input`)).toBeUndefined()
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: null })
  })

  test(`join on a dead session errors + closes`, () => {
    const hub = new Hub()
    const member = connectMember(hub, { sessionId: `nope` })
    expect(member.lastFrame(`error`)).toMatchObject({ code: `no_such_session` })
    expect(member.closed?.code).toBe(CLOSE_SESSION_ENDED)
  })

  test(`single-steerer claim gates input forwarding`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectMember(hub, { sub: `s`, perm: `steer` })
    const watcher = connectMember(hub, { sub: `w`, perm: `view` })

    // Unclaimed input is dropped.
    hub.onMessage(steerer, JSON.stringify({ t: `input`, data: `x` }))
    expect(pub.lastFrame(`input`)).toBeUndefined()

    // view-perm claim is ignored.
    hub.onMessage(watcher, JSON.stringify({ t: `claim` }))
    expect(steerer.lastFrame(`presence`)).toMatchObject({ steererId: null })

    hub.onMessage(steerer, JSON.stringify({ t: `claim` }))
    expect(steerer.lastFrame(`presence`)).toMatchObject({ steererId: `s` })

    hub.onMessage(steerer, JSON.stringify({ t: `input`, data: `ls\n` }))
    expect(pub.lastFrame(`input`)).toMatchObject({ data: `ls\n` })

    // Non-holder input never reaches the publisher.
    hub.onMessage(watcher, JSON.stringify({ t: `input`, data: `rm -rf /\n` }))
    expect(pub.framesOf(`input`).length).toBe(1)

    // Second steer-perm claim while held loses.
    const rival = connectMember(hub, { sub: `r`, perm: `steer` })
    hub.onMessage(rival, JSON.stringify({ t: `claim` }))
    expect(rival.lastFrame(`presence`)).toMatchObject({ steererId: `s` })

    // Release frees the claim.
    hub.onMessage(steerer, JSON.stringify({ t: `release` }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: null })
  })

  test(`publisher release/claim force-clears an active viewer claim (take over)`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectMember(hub, { sub: `s`, perm: `steer` })

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
    const watcher = connectMember(hub, { perm: `view`, sub: `w` })
    hub.onMessage(watcher, JSON.stringify({ t: `kill` }))
    expect(pub.lastFrame(`kill`)).toBeUndefined()

    const steerer = connectMember(hub, { perm: `steer`, sub: `s` })
    hub.onMessage(steerer, JSON.stringify({ t: `kill` }))
    expect(pub.lastFrame(`kill`)).toMatchObject({ t: `kill` })
  })

  test(`bye closes the room and evicts members`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub)
    hub.onMessage(pub, JSON.stringify({ t: `bye`, outcome: `done` }))
    expect(member.lastFrame(`bye`)).toMatchObject({ outcome: `done` })
    expect(member.closed?.code).toBe(CLOSE_SESSION_ENDED)
    expect(hub.sessionInfo(`sess-1`)).toEqual({ live: false })
  })

  test(`publisher drop marks stale; re-hello resumes the same room`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub)
    hub.onClose(pub)
    expect(hub.sessionInfo(`sess-1`)).toMatchObject({ live: false, viewers: 1 })

    const pub2 = connectPublisher(hub)
    expect(hub.sessionInfo(`sess-1`)).toMatchObject({ live: true, viewers: 1 })
    activity(hub, pub2, { kind: `narration`, text: `resumed` })
    expect(member.events().at(-1)).toMatchObject({ text: `resumed` })
  })

  test(`disconnect of the steerer clears the claim + presence`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const watcher = connectMember(hub, { sub: `w`, name: `Watcher`, perm: `view` })
    const member = connectMember(hub, { sub: `m`, perm: `steer` })
    hub.onMessage(member, JSON.stringify({ t: `claim` }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `m` })

    hub.onClose(member)
    const cleared = pub.lastFrame(`presence`)
    expect(cleared).toMatchObject({ steererId: null })
    expect(cleared?.viewers).toEqual([
      { userId: `w`, name: `Watcher`, perm: `view` },
    ])
    expect(watcher.lastFrame(`presence`)).toMatchObject({ steererId: null })

    // Frames after the disconnect no longer reach the dead socket.
    const sentBefore = member.sent.length
    activity(hub, pub, { kind: `narration`, text: `after` })
    expect(member.sent.length).toBe(sentBefore)
  })

  test(`a saturated activity socket is evicted as a slow consumer`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub)
    const healthy = connectMember(hub, { sub: `ok` })

    member.buffered = 10 * 1024 * 1024
    activity(hub, pub, { kind: `narration`, text: `flood` })
    expect(member.closed?.code).toBe(CLOSE_SLOW_CONSUMER)
    expect(member.events().length).toBe(0)
    // The healthy member is unaffected.
    expect(healthy.events().at(-1)).toMatchObject({ text: `flood` })
  })

  test(`members cannot forge activity or activity_reset`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    activity(hub, pub, { kind: `narration`, text: `real` })
    const member = connectMember(hub)
    const other = connectMember(hub, { sub: `other` })

    hub.onMessage(
      member,
      JSON.stringify({ t: `activity`, event: { kind: `narration`, text: `forged` } })
    )
    hub.onMessage(member, JSON.stringify({ t: `activity_reset` }))
    expect(other.events().map((e) => e.text)).toEqual([`real`])
    expect(other.framesOf(`activity_reset`).length).toBe(1) // its own join reset
    expect(room(hub).activityLog.length).toBe(1)
  })
})

describe(`PTY mirror removal (EXP-249)`, () => {
  test(`an old desktop's binary output frames are ignored, not fanned out`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub)
    const sentBefore = member.sent.length

    expect(() => binaryOutput(hub, pub, `secret pty bytes`)).not.toThrow()
    expect(member.sent.length).toBe(sentBefore)

    // The room survives and still carries live activity.
    activity(hub, pub, { kind: `narration`, text: `still fine` })
    expect(member.events().at(-1)).toMatchObject({ text: `still fine` })
  })

  test(`a binary frame still counts as publisher liveness`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub, `sess-bin`)
    room(hub, `sess-bin`).lastPublisherActivity = Date.now() - 91_000
    binaryOutput(hub, pub, `output`)
    ;(hub as unknown as { checkIdlePublishers: () => void }).checkIdlePublishers()
    expect(pub.closed).toBeNull()
    hub.destroy()
  })

  test(`an old desktop's resize frame parses and is dropped`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub)
    const sentBefore = member.sent.length
    hub.onMessage(pub, JSON.stringify({ t: `resize`, cols: 80, rows: 24 }))
    expect(member.sent.length).toBe(sentBefore)
    expect(member.lastFrame(`resize`)).toBeUndefined()
  })

  test(`hello still accepts (and ignores) legacy geometry + activityPublic`, () => {
    const hub = new Hub()
    const sock = new FakeSocket()
    hub.onOpen(sock, claims({ role: `publisher`, sessionId: `sess-1`, perm: `view` }))
    hub.onMessage(
      sock,
      JSON.stringify({
        t: `hello`,
        sessionId: `sess-1`,
        cols: 120,
        rows: 40,
        activityPublic: false,
      })
    )
    expect(hub.sessionInfo(`sess-1`)).toMatchObject({ live: true })

    const member = connectMember(hub)
    activity(hub, sock, { kind: `narration`, text: `still flows` })
    expect(member.events().at(-1)).toMatchObject({ text: `still flows` })
    // A re-hello with different geometry no longer broadcasts anything.
    hub.onClose(sock)
    const pub2 = new FakeSocket()
    hub.onOpen(pub2, claims({ role: `publisher`, sessionId: `sess-1`, perm: `view` }))
    const before = member.sent.length
    hub.onMessage(
      pub2,
      JSON.stringify({ t: `hello`, sessionId: `sess-1`, cols: 80, rows: 24 })
    )
    // Only the presence broadcast.
    expect(member.frames().slice(before).map((f) => f.t)).toEqual([`presence`])
  })
})

describe(`removed public_viewer role (EXP-90)`, () => {
  test(`a stale public_viewer socket joins NO audience and receives nothing`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const stale = connectStalePublicViewer(hub)

    activity(hub, pub, { kind: `tool`, name: `Edit`, detail: `src/a.ts` })
    expect(stale.sent.length).toBe(0)
    expect(pub.lastFrame(`presence`)).toMatchObject({ viewers: [] })
  })

  test(`a stale public_viewer cannot steer, kill, answer, or forge activity`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const stale = connectStalePublicViewer(hub)
    const member = connectMember(hub)

    hub.onMessage(stale, JSON.stringify({ t: `claim`, steal: true }))
    hub.onMessage(stale, JSON.stringify({ t: `input`, data: `rm -rf /` }))
    hub.onMessage(stale, JSON.stringify({ t: `kill` }))
    hub.onMessage(
      stale,
      JSON.stringify({ t: `answer`, questionId: `q1`, keys: [`1`] })
    )
    expect(pub.lastFrame(`input`)).toBeUndefined()
    expect(pub.lastFrame(`answer`)).toBeUndefined()
    expect(pub.lastFrame(`kill`)).toBeUndefined()
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: null })

    hub.onMessage(
      stale,
      JSON.stringify({ t: `activity`, event: { kind: `narration`, text: `fake` } })
    )
    expect(member.events().length).toBe(0)
  })
})

describe(`activity event kinds`, () => {
  const userMessage = { kind: `user_message`, text: `fix the login bug` }
  const question = {
    kind: `question`,
    text: `Which color?`,
    options: [
      { label: `Red`, key: `1`, description: `warm` },
      { label: `Blue`, key: `2` },
    ],
    multiSelect: true,
    id: `toolu_1#0`,
    askId: `toolu_1`,
    index: 1,
    total: 2,
    header: `Palette`,
  }
  const planQuestion = {
    kind: `question`,
    text: `## The plan`,
    options: [
      { label: `Approve — auto-accept edits`, key: `1` },
      { label: `No, keep planning`, key: `3` },
    ],
    planMode: true,
    id: `toolu_plan`,
  }

  test(`every v2 kind fans out with its fields intact`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub)

    const events: Record<string, unknown>[] = [
      userMessage,
      question,
      planQuestion,
      { kind: `question_resolved`, id: `toolu_1#0`, answers: [`Red`] },
      { kind: `question_resolved`, askId: `toolu_1`, dismissed: true },
      { kind: `answer_ack`, id: `toolu_1#0`, askId: `toolu_1` },
      {
        kind: `subagent`,
        id: `sub-1`,
        agentType: `code-reviewer`,
        status: `started`,
        detail: `reviewing diff`,
      },
      { kind: `subagent`, id: `sub-1`, agentType: `code-reviewer`, status: `completed` },
      { kind: `tool`, name: `Grep`, detail: `foo`, subagentId: `sub-1` },
      { kind: `permission`, tool: `Bash`, detail: `rm -rf build` },
    ]
    for (const event of events) activity(hub, pub, event)

    // Non-strict zod would silently strip an unlisted key — assert the whole
    // object, not just the kind.
    expect(member.events()).toEqual(events as never)
  })

  test(`replay preserves all kinds in order, after the reset`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    activity(hub, pub, userMessage)
    activity(hub, pub, { kind: `narration`, text: `working` })
    activity(hub, pub, question)
    activity(hub, pub, planQuestion)
    activity(hub, pub, { kind: `answer_ack`, id: `toolu_plan` })

    const member = connectMember(hub)
    expect(member.frames()[0]).toEqual({ t: `activity_reset` })
    expect(member.events().map((e) => e.kind)).toEqual([
      `user_message`,
      `narration`,
      `question`,
      `question`,
      `answer_ack`,
    ])
    expect(member.events()[3]).toEqual(planQuestion as never)
  })

  test(`a re-emitted question with the same id is replayed too (clients replace)`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    activity(hub, pub, planQuestion)
    activity(hub, pub, {
      ...planQuestion,
      options: [...planQuestion.options, { label: `Type something`, key: `t` }],
    })
    const member = connectMember(hub)
    const questions = member.events().filter((e) => e.kind === `question`)
    expect(questions.length).toBe(2)
    expect((questions[1].options as unknown[]).length).toBe(3)
  })

  test(`invalid shapes are dropped by the schema`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub)

    activity(hub, pub, { kind: `question`, text: `no options`, options: [] })
    activity(hub, pub, {
      kind: `question`,
      text: `oversized key`,
      options: [{ label: `A`, key: `x`.repeat(9) }],
    })
    activity(hub, pub, { kind: `answer_ack` }) // id is required
    activity(hub, pub, { kind: `subagent`, id: `s`, agentType: `t`, status: `paused` })
    activity(hub, pub, { kind: `permission` }) // tool is required
    activity(hub, pub, { kind: `unknown_kind`, text: `x` })
    expect(member.events().length).toBe(0)
  })
})

describe(`semantic answers (EXP-249)`, () => {
  test(`only the claim holder's answer reaches the publisher, verbatim`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectMember(hub, { sub: `s`, perm: `steer` })
    const watcher = connectMember(hub, { sub: `w`, perm: `view` })

    // Unclaimed — dropped, exactly like `input`.
    hub.onMessage(
      steerer,
      JSON.stringify({ t: `answer`, questionId: `q1`, keys: [`1`] })
    )
    expect(pub.lastFrame(`answer`)).toBeUndefined()

    hub.onMessage(steerer, JSON.stringify({ t: `claim` }))
    hub.onMessage(
      steerer,
      JSON.stringify({
        t: `answer`,
        questionId: `toolu_1#0`,
        askId: `toolu_1`,
        keys: [`1`, `3`],
      })
    )
    expect(pub.lastFrame(`answer`)).toEqual({
      t: `answer`,
      questionId: `toolu_1#0`,
      askId: `toolu_1`,
      keys: [`1`, `3`],
    })

    // A view-perm member never holds the claim, so its answer never flows.
    hub.onMessage(
      watcher,
      JSON.stringify({ t: `answer`, questionId: `toolu_1#0`, keys: [`2`] })
    )
    expect(pub.framesOf(`answer`).length).toBe(1)

    // askId is omitted when absent — the frame stays minimal.
    hub.onMessage(
      steerer,
      JSON.stringify({ t: `answer`, questionId: `toolu_plan`, keys: [`1`] })
    )
    expect(pub.lastFrame(`answer`)).toEqual({
      t: `answer`,
      questionId: `toolu_plan`,
      keys: [`1`],
    })

    // Answers are never echoed to the audience.
    expect(steerer.framesOf(`answer`).length).toBe(0)
    expect(watcher.framesOf(`answer`).length).toBe(0)
  })

  test(`a stolen claim moves the answer right with it`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const first = connectMember(hub, { sub: `first`, perm: `steer` })
    const boss = connectMember(hub, { sub: `boss`, perm: `steer` })
    hub.onMessage(first, JSON.stringify({ t: `claim` }))
    hub.onMessage(boss, JSON.stringify({ t: `claim`, steal: true }))

    hub.onMessage(
      first,
      JSON.stringify({ t: `answer`, questionId: `q`, keys: [`1`] })
    )
    expect(pub.lastFrame(`answer`)).toBeUndefined()
    hub.onMessage(
      boss,
      JSON.stringify({ t: `answer`, questionId: `q`, keys: [`2`] })
    )
    expect(pub.lastFrame(`answer`)).toMatchObject({ keys: [`2`] })
  })

  test(`malformed answers are dropped by the schema`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectMember(hub, { sub: `s`, perm: `steer` })
    hub.onMessage(steerer, JSON.stringify({ t: `claim` }))

    hub.onMessage(steerer, JSON.stringify({ t: `answer`, keys: [`1`] }))
    hub.onMessage(steerer, JSON.stringify({ t: `answer`, questionId: `q` }))
    hub.onMessage(steerer, JSON.stringify({ t: `answer`, questionId: `q`, keys: [] }))
    hub.onMessage(
      steerer,
      JSON.stringify({ t: `answer`, questionId: `q`, keys: [`x`.repeat(9)] })
    )
    hub.onMessage(
      steerer,
      JSON.stringify({
        t: `answer`,
        questionId: `q`,
        keys: Array.from({ length: 11 }, () => `1`),
      })
    )
    expect(pub.framesOf(`answer`).length).toBe(0)
  })
})

describe(`activity_reset (EXP-249)`, () => {
  test(`publisher reset clears the log + diff and fans out to members`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub)
    activity(hub, pub, { kind: `narration`, text: `first run` })
    activity(hub, pub, { kind: `diff`, diff: `+ old` })

    hub.onMessage(pub, JSON.stringify({ t: `activity_reset` }))
    expect(member.frames().at(-1)).toEqual({ t: `activity_reset` })
    expect(room(hub).activityLog.length).toBe(0)
    expect(room(hub).activityBytes).toBe(0)
    expect(room(hub).lastDiff).toBeNull()

    // The re-published history is all a late joiner sees.
    activity(hub, pub, { kind: `narration`, text: `republished` })
    const late = connectMember(hub, { sub: `late` })
    expect(late.frames()[0]).toEqual({ t: `activity_reset` })
    expect(late.events().map((e) => e.text)).toEqual([`republished`])
  })

  test(`a reconnecting publisher's re-publish does not double the log`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    activity(hub, pub, { kind: `narration`, text: `one` })
    activity(hub, pub, { kind: `narration`, text: `two` })

    hub.onClose(pub)
    const pub2 = connectPublisher(hub)
    hub.onMessage(pub2, JSON.stringify({ t: `activity_reset` }))
    activity(hub, pub2, { kind: `narration`, text: `one` })
    activity(hub, pub2, { kind: `narration`, text: `two` })
    activity(hub, pub2, { kind: `narration`, text: `three` })

    const member = connectMember(hub)
    expect(member.events().map((e) => e.text)).toEqual([`one`, `two`, `three`])
  })
})

describe(`activity log caps (EXP-249)`, () => {
  test(`the count cap evicts the oldest events`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    for (let i = 0; i < 2100; i++) {
      activity(hub, pub, { kind: `narration`, text: `n${i}` })
    }
    expect(room(hub).activityLog.length).toBe(2000)

    const member = connectMember(hub)
    const texts = member.events().map((e) => e.text)
    expect(texts.length).toBe(2000)
    expect(texts[0]).toBe(`n100`)
    expect(texts.at(-1)).toBe(`n2099`)
  })

  test(`the byte budget evicts the oldest events`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    // Just under the 16KB narration budget once the `<i>:` prefix is added.
    const big = `x`.repeat(16 * 1024 - 8)
    // 300 × 16KB ≈ 4.8MiB — well under the 2000-event cap, over the byte one.
    for (let i = 0; i < 300; i++) {
      activity(hub, pub, { kind: `narration`, text: `${i}:${big}` })
    }
    const state = room(hub)
    expect(state.activityLog.length).toBeLessThan(300)
    expect(state.activityBytes).toBeLessThanOrEqual(4 * 1024 * 1024)
    // The tail survives; the head is gone.
    const member = connectMember(hub)
    const texts = member.events().map((e) => (e.text as string).split(`:`)[0])
    expect(texts.at(-1)).toBe(`299`)
    expect(texts[0]).not.toBe(`0`)
    expect(state.activityBytes).toBe(
      state.activityLog.reduce((n, e) => n + e.bytes, 0)
    )
  })

  test(`eviction always keeps at least one event`, () => {
    const hub = new Hub()
    connectPublisher(hub)
    const state = room(hub)
    ;(
      hub as unknown as {
        appendActivity(r: RoomInternals, e: { framed: string; bytes: number }): void
      }
    ).appendActivity(state, { framed: `{}`, bytes: 8 * 1024 * 1024 })
    expect(state.activityLog.length).toBe(1)
  })

  test(`the latest diff is exempt from the log budget`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    activity(hub, pub, { kind: `diff`, diff: `x`.repeat(400 * 1024) })
    expect(room(hub).activityBytes).toBe(0)
    expect(room(hub).activityLog.length).toBe(0)
    expect(room(hub).lastDiff).not.toBeNull()
  })
})

describe(`claim steal (EXP-32)`, () => {
  test(`claim{steal:true} overrides an existing steerer and broadcasts presence`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const first = connectMember(hub, { sub: `first`, perm: `steer` })
    hub.onMessage(first, JSON.stringify({ t: `claim` }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `first` })

    const member = connectMember(hub, { sub: `boss`, perm: `steer` })
    // Plain claim still loses while the claim is held (first-claim-wins).
    hub.onMessage(member, JSON.stringify({ t: `claim` }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `first` })

    // steal:true wins (last-writer-wins) and everyone hears about it.
    hub.onMessage(member, JSON.stringify({ t: `claim`, steal: true }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `boss` })
    expect(first.lastFrame(`presence`)).toMatchObject({ steererId: `boss` })
    expect(member.lastFrame(`presence`)).toMatchObject({ steererId: `boss` })

    // The deposed steerer's input no longer flows; the stealer's does.
    hub.onMessage(first, JSON.stringify({ t: `input`, data: `nope` }))
    expect(pub.lastFrame(`input`)).toBeUndefined()
    hub.onMessage(member, JSON.stringify({ t: `input`, data: `go\n` }))
    expect(pub.lastFrame(`input`)).toMatchObject({ data: `go\n` })

    // And it can be stolen right back.
    hub.onMessage(first, JSON.stringify({ t: `claim`, steal: true }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `first` })
  })

  test(`steal is denied for perm view`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectMember(hub, { sub: `s`, perm: `steer` })
    hub.onMessage(steerer, JSON.stringify({ t: `claim` }))

    const watcher = connectMember(hub, { sub: `w`, perm: `view` })
    hub.onMessage(watcher, JSON.stringify({ t: `claim`, steal: true }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `s` })

    hub.onMessage(watcher, JSON.stringify({ t: `input`, data: `x` }))
    expect(pub.framesOf(`input`).length).toBe(0)
  })

  test(`publisher takeover still trumps a stolen claim`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub, { sub: `m`, perm: `steer` })
    hub.onMessage(member, JSON.stringify({ t: `claim`, steal: true }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `m` })

    hub.onMessage(pub, JSON.stringify({ t: `release` }))
    expect(member.lastFrame(`presence`)).toMatchObject({ steererId: null })
    hub.onMessage(member, JSON.stringify({ t: `input`, data: `x` }))
    expect(pub.lastFrame(`input`)).toBeUndefined()
    hub.onMessage(
      member,
      JSON.stringify({ t: `answer`, questionId: `q`, keys: [`1`] })
    )
    expect(pub.lastFrame(`answer`)).toBeUndefined()
  })
})

// REV2-X regression: the desktop publisher keeps a live-but-quiet session
// (plan mode / a parked agent) alive with protocol-level WebSocket pings.
// Bun delivers those to the `ping` handler, NOT to `message`, so the idle
// detector must be fed from hub.onPing — otherwise it closes the room after
// 90s and the desktop kills the running agent (CLOSE_SESSION_ENDED = terminal).
describe(`idle publisher detection (REV2-X)`, () => {
  const IDLE_MS = 91_000

  function idle(hub: Hub, sessionId: string, ms: number) {
    // Backdate the room's last-activity to simulate `ms` of silence without
    // waiting real time.
    room(hub, sessionId).lastPublisherActivity = Date.now() - ms
  }
  function checkIdle(hub: Hub) {
    ;(hub as unknown as { checkIdlePublishers: () => void }).checkIdlePublishers()
  }

  test(`a genuinely silent publisher (no pings) times out`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub, `sess-idle`)
    idle(hub, `sess-idle`, IDLE_MS)
    checkIdle(hub)
    expect(pub.closed?.code).toBe(CLOSE_SESSION_ENDED)
    hub.destroy()
  })

  test(`onPing refreshes activity so a live-but-quiet publisher survives`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub, `sess-ping`)
    idle(hub, `sess-ping`, IDLE_MS)
    // A protocol ping arrives via the dedicated handler (the whole point of
    // the fix — this frame never reaches onMessage).
    hub.onPing(pub)
    checkIdle(hub)
    expect(pub.closed).toBeNull()
    hub.destroy()
  })

  test(`onPing from a non-publisher socket is a no-op`, () => {
    const hub = new Hub()
    connectPublisher(hub, `sess-1`)
    const member = connectMember(hub)
    expect(() => hub.onPing(member)).not.toThrow()
    hub.destroy()
  })
})
