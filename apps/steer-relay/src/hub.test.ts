import { describe, expect, test } from "bun:test"
import type { SteerTicketClaims } from "@exp/steer-ticket"
import { Hub, type RelaySocket } from "./hub"
import {
  CLOSE_PUBLISHER_IDLE,
  CLOSE_SESSION_ENDED,
  CLOSE_SLOW_CONSUMER,
} from "./protocol"

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
    iat: now,
    exp: now + 60,
    ...overrides,
  }
}

function connectPublisher(hub: Hub, sessionId = `sess-1`) {
  const sock = new FakeSocket()
  hub.onOpen(sock, claims({ role: `publisher`, sessionId }))
  hub.onMessage(
    sock,
    JSON.stringify({ t: `hello`, sessionId, issueId: `issue-1` })
  )
  return sock
}

/** The session owner on a viewer ticket (EXP-312: viewer tickets are minted
 *  owner-only) joining the scrubbed activity channel — the ONLY audience
 *  since EXP-249 removed the PTY mirror. */
function connectMember(
  hub: Hub,
  opts: { sub?: string; sessionId?: string } = {}
) {
  const sock = new FakeSocket()
  hub.onOpen(
    sock,
    claims({
      role: `viewer`,
      sub: opts.sub ?? `member-1`,
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
      sessionId,
    } as unknown as Partial<SteerTicketClaims>)
  )
  hub.onMessage(sock, JSON.stringify({ t: `join`, channel: `activity` }))
  return sock
}

const activity = (hub: Hub, pub: FakeSocket, event: unknown) =>
  hub.onMessage(pub, JSON.stringify({ t: `activity`, event }))

interface RoomInternals {
  activityLog: { framed: string; bytes: number }[]
  activityBytes: number
  lastDiff: { framed: string } | null
  lastPublisherActivity: number
  publisher: unknown
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
      { deviceId: `dev-1`, deviceLabel: `MacBook` },
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

  test(`startSession passes resume through to the frame (EXP-481)`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(desktop, JSON.stringify({ t: `online`, deviceId: `dev-1` }))

    hub.startSession(
      `owner`,
      `dev-1`,
      { issueId: `issue-9` },
      { resume: true }
    )
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueId: `issue-9`,
      resume: true,
    })

    // Absent resume: the frame stays byte-identical to the legacy wire.
    hub.startSession(`owner`, `dev-1`, { issueId: `issue-10` })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueId: `issue-10`,
    })
  })

  test(`startSession routes a resume subject and drops launch options (EXP-637)`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(desktop, JSON.stringify({ t: `online`, deviceId: `dev-1` }))

    hub.startSession(
      `owner`,
      `dev-1`,
      {
        resumeSessionId: `sess-1`,
        teamId: `team-1`,
        actionId: `act-1`,
        actionName: `Refresh screenshots`,
        branch: `exp/refresh-screenshots-1a2b3c4d`,
      },
      // A resumed run keeps the options it was started with — the device
      // reads them off its own run registry, so anything passed here must
      // NOT reach the frame and contradict it. Only attribution survives.
      { agent: `codex`, model: `gpt-5.4`, startedBy: `mate` }
    )
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      resumeSessionId: `sess-1`,
      teamId: `team-1`,
      actionId: `act-1`,
      actionName: `Refresh screenshots`,
      branch: `exp/refresh-screenshots-1a2b3c4d`,
      startedBy: `mate`,
    })

    // Hints are optional: a bare resume names only the run and its team.
    hub.startSession(`owner`, `dev-1`, {
      resumeSessionId: `sess-2`,
      teamId: `team-1`,
    })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      resumeSessionId: `sess-2`,
      teamId: `team-1`,
    })

    // Every other subject stays byte-identical.
    hub.startSession(`owner`, `dev-1`, { issueId: `issue-9` })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueId: `issue-9`,
    })
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

  test(`startSession passes startedBy through to the frame (EXP-432)`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(desktop, JSON.stringify({ t: `online`, deviceId: `dev-1` }))

    // A shared-device start rides the requester's userId as an option —
    // dumb pass-through, the daemon threads it into codingSessions.start.
    const routed = hub.startSession(
      `owner`,
      `dev-1`,
      { issueId: `issue-9` },
      { startedBy: `requester-1` }
    )
    expect(routed).toEqual({ ok: true })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueId: `issue-9`,
      startedBy: `requester-1`,
    })

    // Absent startedBy: the frame stays byte-identical to the legacy wire.
    hub.startSession(`owner`, `dev-1`, { issueId: `issue-10` })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueId: `issue-10`,
    })
  })

  // EXP-679: a start asked for by another coding session. Pure pass-through
  // like startedBy — the device writes it as coding_sessions.started_reason,
  // so the run is unattended and its own close-out ends it. It rides EVERY
  // subject, the resume arm (which drops launch options) included.
  test(`startSession passes startedReason through to the frame (EXP-679)`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(desktop, JSON.stringify({ t: `online`, deviceId: `dev-1` }))

    hub.startSession(
      `owner`,
      `dev-1`,
      { issueId: `issue-9` },
      { startedReason: `agent` }
    )
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueId: `issue-9`,
      startedReason: `agent`,
    })

    hub.startSession(
      `owner`,
      `dev-1`,
      { resumeSessionId: `sess-1`, teamId: `team-1` },
      // Launch options still die on a resume; the marker survives with the
      // attribution.
      { agent: `codex`, startedBy: `mate`, startedReason: `agent` }
    )
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      resumeSessionId: `sess-1`,
      teamId: `team-1`,
      startedBy: `mate`,
      startedReason: `agent`,
    })

    // Absent: the frame stays byte-identical to the human-start wire.
    hub.startSession(`owner`, `dev-1`, { issueId: `issue-10` })
    expect(desktop.lastFrame(`start_session`)).toEqual({
      t: `start_session`,
      issueId: `issue-10`,
    })
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
        caps: [`actions`],
      })
    )
    const legacy = new FakeSocket()
    hub.onOpen(legacy, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(legacy, JSON.stringify({ t: `online`, deviceId: `dev-2` }))

    expect(hub.devicesFor(`owner`)).toMatchObject([
      { deviceId: `dev-1`, caps: [`actions`] },
      // No advertisement ⇒ [] — the web server strictly gates action starts
      // on the capability, so such a device is never targeted.
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
    expect(hub.devicesFor(`owner`)).toMatchObject([{ deviceId: `dev-999` }])
    // The abandoned ids are gone, not ghosts routing into this socket.
    expect(hub.startSession(`owner`, `dev-0`, { issueId: `issue-9` })).toEqual({
      ok: false,
      reason: `device_offline`,
    })
    expect(hub.stats().devices).toBe(1)

    hub.onClose(desktop)
    expect(hub.devicesFor(`owner`)).toEqual([])
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
    expect(hub.startSession(`owner`, `dev-1`, { issueId: `issue-9` })).toEqual({
      ok: true,
    })
    expect(second.lastFrame(`start_session`)).toMatchObject({
      issueId: `issue-9`,
    })
    hub.onClose(first)
    hub.onClose(second)
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
  test(`member join: activity_reset, then replay`, () => {
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
    // Then the log in order, then ONLY the latest diff.
    expect(member.events().map((e) => e.kind)).toEqual([
      `narration`,
      `tool`,
      `diff`,
    ])
    expect(member.events().at(-1)?.diff).toBe(`new diff`)

    activity(hub, pub, { kind: `tool`, name: `Edit`, detail: `src/a.ts` })
    expect(member.events().at(-1)).toMatchObject({ kind: `tool`, name: `Edit` })
  })

  // EXP-656: the replay ends with an explicit marker so a client can stage
  // the whole reset+replay behind its visible feed and commit once.
  test(`a join replay ends with activity_synced`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    activity(hub, pub, { kind: `narration`, text: `one` })
    activity(hub, pub, { kind: `tool`, name: `Bash` })
    activity(hub, pub, { kind: `narration`, text: `three` })
    activity(hub, pub, { kind: `diff`, diff: `d` })

    const member = connectMember(hub)
    expect(member.frames().map((f) => f.t)).toEqual([
      `activity_reset`,
      `activity`,
      `activity`,
      `activity`,
      `activity`,
      `activity_synced`,
    ])
    expect(hub.counters().viewerJoins).toBe(1)
    hub.destroy()
  })

  test(`an empty room still ends its join with activity_synced`, () => {
    const hub = new Hub()
    connectPublisher(hub)
    const member = connectMember(hub)
    expect(member.frames().map((f) => f.t)).toEqual([`activity_reset`, `activity_synced`])
    hub.destroy()
  })

  test(`activity_synced goes only to the joining viewer`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const early = connectMember(hub)
    activity(hub, pub, { kind: `narration`, text: `hi` })
    const late = connectMember(hub)
    expect(late.framesOf(`activity_synced`)).toHaveLength(1)
    // The earlier member only ever saw its own join's marker.
    expect(early.framesOf(`activity_synced`)).toHaveLength(1)
    expect(pub.framesOf(`activity_synced`)).toHaveLength(0)
    hub.destroy()
  })

  test(`a publisher-driven activity_reset does NOT emit activity_synced`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub)
    hub.onMessage(pub, JSON.stringify({ t: `activity_reset` }))
    activity(hub, pub, { kind: `narration`, text: `republished` })
    // Old desktops give the relay no end-of-republish signal, so the client
    // owns that commit (quiet timer); the marker stays join-only.
    expect(member.framesOf(`activity_synced`)).toHaveLength(1)
    expect(member.frames().at(-1)).toMatchObject({ t: `activity` })
    hub.destroy()
  })

  test(`join on a dead session errors + closes`, () => {
    const hub = new Hub()
    const member = connectMember(hub, { sessionId: `nope` })
    expect(member.lastFrame(`error`)).toMatchObject({ code: `no_such_session` })
    expect(member.closed?.code).toBe(CLOSE_SESSION_ENDED)
  })

  test(`room membership gates input forwarding — no claim, no perm tier (EXP-312)`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectMember(hub, { sub: `s` })

    // A joined viewer's input flows immediately — seamless.
    hub.onMessage(steerer, JSON.stringify({ t: `input`, data: `ls\n` }))
    expect(pub.lastFrame(`input`)).toMatchObject({ data: `ls\n` })

    // A viewer that never joined the room is dropped.
    const stranger = new FakeSocket()
    hub.onOpen(stranger, claims({ role: `viewer`, sub: `x`, sessionId: `sess-1` }))
    hub.onMessage(stranger, JSON.stringify({ t: `input`, data: `rm -rf /\n` }))
    expect(pub.framesOf(`input`).length).toBe(1)

    // Multiple joined sockets steer concurrently — no single operator.
    const second = connectMember(hub, { sub: `s2` })
    hub.onMessage(second, JSON.stringify({ t: `input`, data: `pwd\n` }))
    expect(pub.lastFrame(`input`)).toMatchObject({ data: `pwd\n` })
    hub.onMessage(steerer, JSON.stringify({ t: `input`, data: `id\n` }))
    expect(pub.lastFrame(`input`)).toMatchObject({ data: `id\n` })
  })

  test(`kill requires room membership and reaches the publisher`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const stranger = new FakeSocket()
    hub.onOpen(stranger, claims({ role: `viewer`, sub: `x`, sessionId: `sess-1` }))
    hub.onMessage(stranger, JSON.stringify({ t: `kill` }))
    expect(pub.lastFrame(`kill`)).toBeUndefined()

    const steerer = connectMember(hub, { sub: `s` })
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

  test(`disconnect evicts the member from the activity audience`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub, { sub: `m` })

    hub.onClose(member)

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

describe(`hello leftovers (EXP-90)`, () => {
  test(`hello still accepts (and ignores) activityPublic`, () => {
    const hub = new Hub()
    const sock = new FakeSocket()
    hub.onOpen(sock, claims({ role: `publisher`, sessionId: `sess-1` }))
    hub.onMessage(
      sock,
      JSON.stringify({
        t: `hello`,
        sessionId: `sess-1`,
        activityPublic: false,
      })
    )
    expect(hub.sessionInfo(`sess-1`)).toMatchObject({ live: true })

    const member = connectMember(hub)
    activity(hub, sock, { kind: `narration`, text: `still flows` })
    expect(member.events().at(-1)).toMatchObject({ text: `still flows` })
    // A re-hello broadcasts nothing to the joined member.
    hub.onClose(sock)
    const pub2 = new FakeSocket()
    hub.onOpen(pub2, claims({ role: `publisher`, sessionId: `sess-1` }))
    const before = member.sent.length
    hub.onMessage(
      pub2,
      JSON.stringify({ t: `hello`, sessionId: `sess-1` })
    )
    expect(member.frames().slice(before)).toEqual([])
  })
})

describe(`removed public_viewer role (EXP-90)`, () => {
  test(`a stale public_viewer socket joins NO audience and receives nothing`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const stale = connectStalePublicViewer(hub)

    activity(hub, pub, { kind: `tool`, name: `Edit`, detail: `src/a.ts` })
    expect(stale.sent.length).toBe(0)
  })

  test(`a stale public_viewer cannot steer, kill, answer, or forge activity`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const stale = connectStalePublicViewer(hub)
    const member = connectMember(hub)

    hub.onMessage(stale, JSON.stringify({ t: `input`, data: `rm -rf /` }))
    hub.onMessage(stale, JSON.stringify({ t: `kill` }))
    hub.onMessage(
      stale,
      JSON.stringify({ t: `answer`, questionId: `q1`, keys: [`1`] })
    )
    expect(pub.lastFrame(`input`)).toBeUndefined()
    expect(pub.lastFrame(`answer`)).toBeUndefined()
    expect(pub.lastFrame(`kill`)).toBeUndefined()

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
      // EXP-513: the synthetic free-text row must survive re-serialization.
      { label: `Type something.`, key: `3`, freeText: true },
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

  test(`an anchored narration keeps beforeQuestionId in fan-out AND replay (EXP-483)`, () => {
    // The relay re-serializes the zod-PARSED event — a field missing from
    // the schema would be silently stripped here.
    const anchored = {
      kind: `narration`,
      text: `Here is the summary of my findings.`,
      beforeQuestionId: `toolu_1`,
    }
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const live = connectMember(hub)
    activity(hub, pub, question)
    activity(hub, pub, anchored)
    expect(live.events()[1]).toEqual(anchored as never)

    const late = connectMember(hub)
    expect(late.events()[1]).toEqual(anchored as never)
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
  test(`a joined viewer's answer reaches the publisher, verbatim`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectMember(hub, { sub: `s` })

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

    // A viewer that never joined the room is dropped.
    const stranger = new FakeSocket()
    hub.onOpen(stranger, claims({ role: `viewer`, sub: `x`, sessionId: `sess-1` }))
    hub.onMessage(
      stranger,
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
  })

  test(`a free-text answer rides its typed reply to the publisher (EXP-513)`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectMember(hub, { sub: `s` })

    hub.onMessage(
      steerer,
      JSON.stringify({
        t: `answer`,
        questionId: `toolu_1#0`,
        askId: `toolu_1`,
        keys: [`4`],
        text: `purple`,
      })
    )
    expect(pub.lastFrame(`answer`)).toEqual({
      t: `answer`,
      questionId: `toolu_1#0`,
      askId: `toolu_1`,
      keys: [`4`],
      text: `purple`,
    })

    // Oversized replies are dropped by the schema, not truncated.
    hub.onMessage(
      steerer,
      JSON.stringify({
        t: `answer`,
        questionId: `toolu_1#0`,
        keys: [`4`],
        text: `x`.repeat(4001),
      })
    )
    expect(pub.framesOf(`answer`).length).toBe(1)
  })

  test(`answers from every joined socket flow — no single operator`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const first = connectMember(hub, { sub: `first` })
    const boss = connectMember(hub, { sub: `boss` })

    hub.onMessage(
      first,
      JSON.stringify({ t: `answer`, questionId: `q`, keys: [`1`] })
    )
    expect(pub.lastFrame(`answer`)).toMatchObject({ keys: [`1`] })
    hub.onMessage(
      boss,
      JSON.stringify({ t: `answer`, questionId: `q`, keys: [`2`] })
    )
    expect(pub.lastFrame(`answer`)).toMatchObject({ keys: [`2`] })
  })

  test(`malformed answers are dropped by the schema`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectMember(hub, { sub: `s` })

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

// REV2-X regression: the desktop publisher keeps a live-but-quiet session
// (plan mode / a parked agent) alive with protocol-level WebSocket pings.
// Bun delivers those to the `ping` handler, NOT to `message`, so the idle
// detector must be fed from hub.onPing — otherwise it detaches the publisher
// after 90s (a churny reconnect; EXP-283 made the idle close non-terminal).
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

  test(`a genuinely silent publisher (no pings) times out — with the NON-terminal idle code`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub, `sess-idle`)
    idle(hub, `sess-idle`, IDLE_MS)
    checkIdle(hub)
    // EXP-283: MUST be CLOSE_PUBLISHER_IDLE, never CLOSE_SESSION_ENDED —
    // desktops treat 4001 as a remote kill and would tear down a live agent
    // that merely slept through the idle window (laptop suspend).
    expect(pub.closed?.code).toBe(CLOSE_PUBLISHER_IDLE)
    // EXP-656: idle detaches are counted — a desktop the detector keeps
    // dropping (and whose republish yanks every viewer) shows up in stats.
    expect(hub.counters().publisherIdleCloses).toBe(1)
    hub.destroy()
  })

  test(`an idle-detached publisher can re-hello and resume its room`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub, `sess-resume`)
    idle(hub, `sess-resume`, IDLE_MS)
    checkIdle(hub)
    expect(pub.closed?.code).toBe(CLOSE_PUBLISHER_IDLE)
    hub.onClose(pub)

    // The woken desktop reconnects and re-hellos into the SAME room.
    const again = connectPublisher(hub, `sess-resume`)
    expect(again.closed).toBeNull()
    expect(room(hub, `sess-resume`).publisher).not.toBeNull()
    // The re-hello reset the idle clock — the next check must not re-fire.
    checkIdle(hub)
    expect(again.closed).toBeNull()
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

describe(`stats counters (EXP-553)`, () => {
  test(`counters start at zero and track opens, starts, fanned frames`, () => {
    const hub = new Hub()
    expect(hub.counters()).toEqual({
      connectionsAccepted: 0,
      activityFramesFanned: 0,
      startsRouted: 0,
      slowConsumerEvictions: 0,
      viewerJoins: 0,
      publisherIdleCloses: 0,
    })

    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(
      desktop,
      JSON.stringify({ t: `online`, deviceId: `dev-1`, deviceLabel: `Mac` })
    )
    hub.startSession(`owner`, `dev-1`, { issueId: `issue-1` })
    // Offline device: routing fails, counter must not move.
    hub.startSession(`owner`, `dev-404`, { issueId: `issue-1` })

    const pub = connectPublisher(hub)
    const member = connectMember(hub)
    activity(hub, pub, { kind: `narration`, text: `working` })

    const counters = hub.counters()
    expect(counters.connectionsAccepted).toBe(3)
    expect(counters.startsRouted).toBe(1)
    expect(counters.activityFramesFanned).toBeGreaterThanOrEqual(1)
    expect(counters.slowConsumerEvictions).toBe(0)

    // A saturated member socket is evicted, not sent to.
    member.buffered = 10 * 1024 * 1024
    activity(hub, pub, { kind: `narration`, text: `again` })
    expect(hub.counters().slowConsumerEvictions).toBe(1)
    hub.destroy()
  })
})

// EXP-648: an agent parked on a question or plan approval sends nothing for
// minutes, and the desktop's 30s ping is a WS control frame Bun never hands
// to `message`, so viewers used to hear NOTHING from a healthy socket and
// redialed it (mint + activity_reset + full replay) on every wake past 45s.
// The relay now ticks a `keepalive` to every joined viewer.
describe(`viewer keepalive (EXP-648)`, () => {
  function tick(hub: Hub) {
    ;(hub as unknown as { sendViewerKeepalives: () => void }).sendViewerKeepalives()
  }

  test(`a joined viewer gets one per tick; publisher and control sockets get none`, () => {
    const hub = new Hub()
    const desktop = new FakeSocket()
    hub.onOpen(desktop, claims({ role: `control`, sub: `owner` }))
    hub.onMessage(
      desktop,
      JSON.stringify({ t: `online`, deviceId: `dev-1`, deviceLabel: `Mac` })
    )
    const pub = connectPublisher(hub)
    const member = connectMember(hub)
    expect(member.framesOf(`keepalive`)).toHaveLength(0)

    tick(hub)
    tick(hub)
    expect(member.framesOf(`keepalive`)).toHaveLength(2)
    expect(member.frames().at(-1)).toEqual({ t: `keepalive` })
    expect(pub.framesOf(`keepalive`)).toHaveLength(0)
    expect(desktop.framesOf(`keepalive`)).toHaveLength(0)
    hub.destroy()
  })

  test(`a keepalive is never a socket's first frame — unjoined viewers get none`, () => {
    const hub = new Hub()
    connectPublisher(hub)
    // Upgraded, ticket-valid, but the join has not been sent yet: every
    // client disarms its join-ack deadline on the FIRST frame, so a
    // keepalive here would let a mute join hang forever.
    const lurker = new FakeSocket()
    hub.onOpen(lurker, claims({ role: `viewer`, sessionId: `sess-1` }))
    tick(hub)
    expect(lurker.sent).toHaveLength(0)

    // Joining answers with activity_reset first; keepalives follow it.
    hub.onMessage(lurker, JSON.stringify({ t: `join`, channel: `activity` }))
    tick(hub)
    expect(lurker.frames().map((f) => f.t)).toEqual([
      `activity_reset`,
      `activity_synced`,
      `keepalive`,
    ])
    hub.destroy()
  })

  test(`a saturated viewer is skipped, not evicted, and no activity counter moves`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub)
    activity(hub, pub, { kind: `narration`, text: `working` })
    const before = hub.counters()

    member.buffered = 10 * 1024 * 1024
    tick(hub)
    expect(member.closed).toBeNull()
    expect(member.framesOf(`keepalive`)).toHaveLength(0)
    expect(hub.counters()).toEqual(before)

    // Drained: the next tick reaches it again.
    member.buffered = 0
    tick(hub)
    expect(member.framesOf(`keepalive`)).toHaveLength(1)
    hub.destroy()
  })

  test(`viewers of a room whose publisher dropped (grace window) still get one`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectMember(hub)
    hub.onClose(pub)
    // Grace window: the room stays, viewers hear nothing until it expires.
    expect(room(hub).publisher).toBeNull()
    expect(member.framesOf(`bye`)).toHaveLength(0)
    expect(member.closed).toBeNull()

    tick(hub)
    // The frame vouches for the SOCKET: a redial here would only replay the
    // same log while the desktop reconnects.
    expect(member.framesOf(`keepalive`)).toHaveLength(1)
    hub.destroy()
  })

  test(`the tick is a real interval that destroy() stops`, () => {
    const timers: { fn: () => void; ms: number }[] = []
    const cleared: unknown[] = []
    const realSet = globalThis.setInterval
    const realClear = globalThis.clearInterval
    globalThis.setInterval = ((fn: () => void, ms: number) => {
      timers.push({ fn, ms })
      return timers.length as unknown as ReturnType<typeof setInterval>
    }) as typeof setInterval
    globalThis.clearInterval = ((id: unknown) => {
      cleared.push(id)
    }) as typeof clearInterval
    try {
      const hub = new Hub()
      connectPublisher(hub)
      const member = connectMember(hub)
      const keepalive = timers.find((t) => t.ms === 15_000)
      expect(keepalive).toBeDefined()
      keepalive!.fn()
      expect(member.framesOf(`keepalive`)).toHaveLength(1)
      hub.destroy()
      // Both the idle detector and the keepalive tick are cleared.
      expect(cleared).toHaveLength(timers.length)
    } finally {
      globalThis.setInterval = realSet
      globalThis.clearInterval = realClear
    }
  })
})
