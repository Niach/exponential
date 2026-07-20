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
    JSON.stringify({
      t: `hello`,
      sessionId,
      issueId: `issue-1`,
      cols: 120,
      rows: 40,
    })
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
  hub.onMessage(sock, JSON.stringify({ t: `join` }))
  return sock
}

/** An authenticated member on an ordinary viewer ticket joining the scrubbed
 *  activity channel ({ t: `join`, channel: `activity` }). */
function connectActivityMember(
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

  test(`re-hello with changed geometry broadcasts resize to attached viewers`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub) // 120x40
    const viewer = connectViewer(hub)
    expect(viewer.lastFrame(`resize`)).toMatchObject({ cols: 120, rows: 40 })

    // Publisher drops, gets resized while disconnected, re-hellos at 80x24.
    hub.onClose(pub)
    const pub2 = new FakeSocket()
    hub.onOpen(pub2, claims({ role: `publisher`, sessionId: `sess-1`, perm: `view` }))
    hub.onMessage(
      pub2,
      JSON.stringify({ t: `hello`, sessionId: `sess-1`, cols: 80, rows: 24 })
    )
    expect(viewer.lastFrame(`resize`)).toMatchObject({ cols: 80, rows: 24 })

    // A same-geometry re-hello stays quiet.
    const resizes = () => viewer.frames().filter((f) => f.t === `resize`).length
    const before = resizes()
    hub.onClose(pub2)
    const pub3 = new FakeSocket()
    hub.onOpen(pub3, claims({ role: `publisher`, sessionId: `sess-1`, perm: `view` }))
    hub.onMessage(
      pub3,
      JSON.stringify({ t: `hello`, sessionId: `sess-1`, cols: 80, rows: 24 })
    )
    expect(resizes()).toBe(before)
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

describe(`removed public_viewer role (EXP-90)`, () => {
  const activity = (hub: Hub, pub: FakeSocket, event: unknown) =>
    hub.onMessage(pub, JSON.stringify({ t: `activity`, event }))

  test(`a stale public_viewer socket joins NO audience and receives nothing`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const stale = connectStalePublicViewer(hub)

    output(hub, pub, `secret pty bytes`)
    activity(hub, pub, { kind: `tool`, name: `Edit`, detail: `src/a.ts` })
    hub.onMessage(pub, JSON.stringify({ t: `resize`, cols: 80, rows: 24 }))

    expect(stale.sent.length).toBe(0)
    // It never entered presence either.
    expect(pub.lastFrame(`presence`)).toMatchObject({ viewers: [] })
  })

  test(`a stale public_viewer cannot steer, kill, or forge output/activity`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const stale = connectStalePublicViewer(hub)
    const member = connectActivityMember(hub)

    hub.onMessage(stale, JSON.stringify({ t: `claim`, steal: true }))
    hub.onMessage(stale, JSON.stringify({ t: `input`, data: `rm -rf /` }))
    hub.onMessage(stale, JSON.stringify({ t: `kill` }))
    expect(pub.lastFrame(`input`)).toBeUndefined()
    expect(pub.lastFrame(`kill`)).toBeUndefined()
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: null })

    output(hub, stale as unknown as FakeSocket, `forged`)
    hub.onMessage(stale, JSON.stringify({ t: `activity`, event: { kind: `narration`, text: `fake` } }))
    expect(member.frames().filter((f) => f.t === `activity`).length).toBe(0)
  })

  test(`hello with the legacy activityPublic flag still parses and runs the room`, () => {
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

    const member = connectActivityMember(hub)
    activity(hub, sock, { kind: `narration`, text: `still flows` })
    expect(member.lastFrame(`activity`)).toMatchObject({
      event: { kind: `narration`, text: `still flows` },
    })
  })

  test(`pty viewers never receive activity frames`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const viewer = connectViewer(hub)
    activity(hub, pub, { kind: `narration`, text: `working on it` })
    expect(viewer.lastFrame(`activity`)).toBeUndefined()
  })
})

describe(`member activity channel (EXP-32)`, () => {
  const activity = (hub: Hub, pub: FakeSocket, event: unknown) =>
    hub.onMessage(pub, JSON.stringify({ t: `activity`, event }))

  test(`member join replays log + latest diff + presence, never binary/resize/ring`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    output(hub, pub, `secret pty scrollback`) // fills the ring
    activity(hub, pub, { kind: `narration`, text: `one` })
    activity(hub, pub, { kind: `diff`, diff: `old diff` })
    activity(hub, pub, { kind: `tool`, name: `Bash` })
    activity(hub, pub, { kind: `diff`, diff: `new diff` })

    const member = connectActivityMember(hub)
    const frames = member.frames()
    const events = frames
      .filter((f) => f.t === `activity`)
      .map((f) => f.event as { kind: string; diff?: string })
    // Replay order: log, then ONLY the latest diff, then presence.
    expect(events.map((e) => e.kind)).toEqual([`narration`, `tool`, `diff`])
    expect(events.at(-1)?.diff).toBe(`new diff`)
    expect(frames.at(-1)).toMatchObject({
      t: `presence`,
      viewers: [{ userId: `member-1`, name: `Member`, perm: `steer` }],
      steererId: null,
    })
    // NEVER the PTY audience's frames: no ring replay, no geometry.
    expect(member.outputs().length).toBe(0)
    expect(member.lastFrame(`resize`)).toBeUndefined()

    // The publisher's presence broadcast lists the activity member too.
    expect(pub.lastFrame(`presence`)).toMatchObject({
      viewers: [{ userId: `member-1`, perm: `steer` }],
    })

    // Live: activity flows, binary output does not.
    output(hub, pub, `live pty`)
    activity(hub, pub, { kind: `tool`, name: `Edit`, detail: `src/a.ts` })
    expect(member.outputs().length).toBe(0)
    expect(member.lastFrame(`activity`)).toMatchObject({
      event: { kind: `tool`, name: `Edit` },
    })
  })

  test(`pty viewers get no activity`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const viewer = connectViewer(hub, { sub: `pty-v` })
    const member = connectActivityMember(hub)

    activity(hub, pub, { kind: `narration`, text: `working` })
    expect(viewer.lastFrame(`activity`)).toBeUndefined()
    expect(member.lastFrame(`activity`)).toMatchObject({
      event: { kind: `narration` },
    })
  })

  test(`an activity-member steerer's input reaches the publisher`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectActivityMember(hub, { sub: `m`, perm: `steer` })

    hub.onMessage(member, JSON.stringify({ t: `claim` }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `m` })
    expect(member.lastFrame(`presence`)).toMatchObject({ steererId: `m` })

    hub.onMessage(member, JSON.stringify({ t: `input`, data: `ls\n` }))
    expect(pub.lastFrame(`input`)).toMatchObject({ data: `ls\n` })
  })

  test(`disconnect of the activity steerer clears the claim + presence`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const watcher = connectViewer(hub, { sub: `w`, perm: `view` })
    const member = connectActivityMember(hub, { sub: `m`, perm: `steer` })
    hub.onMessage(member, JSON.stringify({ t: `claim` }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `m` })

    hub.onClose(member)
    const cleared = pub.lastFrame(`presence`)
    expect(cleared).toMatchObject({ steererId: null })
    // The departed member left the viewers list too.
    expect(cleared?.viewers).toEqual([
      { userId: `w`, name: `Viewer`, perm: `view` },
    ])
    expect(watcher.lastFrame(`presence`)).toMatchObject({ steererId: null })
  })

  test(`disconnect evicts a socket that joined BOTH the pty and activity channels`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const dual = connectViewer(hub, { sub: `dual`, name: `Dual` })
    hub.onMessage(dual, JSON.stringify({ t: `join`, channel: `activity` }))
    // The socket now sits in both audiences; presence lists it twice.
    expect(pub.lastFrame(`presence`)?.viewers).toEqual([
      { userId: `dual`, name: `Dual`, perm: `steer` },
      { userId: `dual`, name: `Dual`, perm: `steer` },
    ])

    hub.onClose(dual)
    // No ghost entry survives in either map.
    expect(pub.lastFrame(`presence`)?.viewers).toEqual([])

    // Activity frames after the disconnect no longer reach the dead socket.
    const sentBefore = dual.sent.length
    hub.onMessage(
      pub,
      JSON.stringify({ t: `activity`, event: { kind: `narration`, text: `after` } })
    )
    output(hub, pub, `after`)
    expect(dual.sent.length).toBe(sentBefore)
  })

  test(`room close sends bye to activity members and evicts them`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectActivityMember(hub)
    hub.onMessage(pub, JSON.stringify({ t: `bye`, outcome: `done` }))
    expect(member.lastFrame(`bye`)).toMatchObject({ outcome: `done` })
    expect(member.closed?.code).toBe(4001)
  })
})

describe(`member-only activity kinds (EXP-78)`, () => {
  const activity = (hub: Hub, pub: FakeSocket, event: unknown) =>
    hub.onMessage(pub, JSON.stringify({ t: `activity`, event }))
  const userMessage = { kind: `user_message`, text: `fix the login bug` }
  const question = {
    kind: `question`,
    text: `Which color?`,
    options: [
      { label: `Red`, key: `1` },
      { label: `Blue`, key: `2` },
    ],
    multiSelect: true,
  }
  const planQuestion = {
    kind: `question`,
    text: `## The plan`,
    options: [
      { label: `Approve — auto-accept edits`, key: `1` },
      { label: `No, keep planning`, key: `3` },
    ],
    planMode: true,
  }

  test(`user_message and question fan out to activity members with fields intact`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectActivityMember(hub)

    activity(hub, pub, userMessage)
    expect(member.lastFrame(`activity`)).toMatchObject({ event: userMessage })

    activity(hub, pub, question)
    expect(member.lastFrame(`activity`)).toMatchObject({ event: question })

    // The planMode marker must survive the schema parse + re-serialization
    // (EXP-97) — a non-strict zod would silently strip an unlisted key.
    activity(hub, pub, planQuestion)
    const plan = member.lastFrame(`activity`).event as { planMode?: boolean }
    expect(plan.planMode).toBe(true)
  })

  test(`replay preserves all kinds in order for members`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    activity(hub, pub, userMessage)
    activity(hub, pub, { kind: `narration`, text: `working` })
    activity(hub, pub, question)
    activity(hub, pub, planQuestion)

    const member = connectActivityMember(hub)
    const replayed = member.frames().filter((f) => f.t === `activity`)
    expect(replayed.map((f) => (f.event as { kind: string }).kind)).toEqual([
      `user_message`,
      `narration`,
      `question`,
      `question`,
    ])
    // planMode survives the activityLog replay path too (EXP-97).
    expect((replayed[3].event as { planMode?: boolean }).planMode).toBe(true)
  })

  test(`a question with an invalid shape is dropped by the schema`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectActivityMember(hub)

    activity(hub, pub, { kind: `question`, text: `no options`, options: [] })
    activity(hub, pub, {
      kind: `question`,
      text: `oversized key`,
      options: [{ label: `A`, key: `x`.repeat(9) }],
    })
    expect(member.frames().filter((f) => f.t === `activity`).length).toBe(0)
  })
})

describe(`claim steal (EXP-32)`, () => {
  test(`claim{steal:true} overrides an existing steerer and broadcasts presence`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const first = connectViewer(hub, { sub: `first`, perm: `steer` })
    hub.onMessage(first, JSON.stringify({ t: `claim` }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `first` })

    const member = connectActivityMember(hub, { sub: `boss`, perm: `steer` })
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

    // A PTY viewer with steer perm can steal it right back.
    hub.onMessage(first, JSON.stringify({ t: `claim`, steal: true }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `first` })
  })

  test(`steal is denied for perm view (either audience)`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const steerer = connectViewer(hub, { sub: `s`, perm: `steer` })
    hub.onMessage(steerer, JSON.stringify({ t: `claim` }))

    const watcher = connectViewer(hub, { sub: `w`, perm: `view` })
    hub.onMessage(watcher, JSON.stringify({ t: `claim`, steal: true }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `s` })

    const viewMember = connectActivityMember(hub, { sub: `vm`, perm: `view` })
    hub.onMessage(viewMember, JSON.stringify({ t: `claim`, steal: true }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `s` })

    // And their input never flows.
    hub.onMessage(watcher, JSON.stringify({ t: `input`, data: `x` }))
    hub.onMessage(viewMember, JSON.stringify({ t: `input`, data: `x` }))
    expect(pub.lastFrame(`input`)).toBeUndefined()
  })

  test(`publisher takeover still trumps a stolen claim`, () => {
    const hub = new Hub()
    const pub = connectPublisher(hub)
    const member = connectActivityMember(hub, { sub: `m`, perm: `steer` })
    hub.onMessage(member, JSON.stringify({ t: `claim`, steal: true }))
    expect(pub.lastFrame(`presence`)).toMatchObject({ steererId: `m` })

    hub.onMessage(pub, JSON.stringify({ t: `release` }))
    expect(member.lastFrame(`presence`)).toMatchObject({ steererId: null })
    hub.onMessage(member, JSON.stringify({ t: `input`, data: `x` }))
    expect(pub.lastFrame(`input`)).toBeUndefined()
  })
})
