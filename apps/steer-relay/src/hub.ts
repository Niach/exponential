// The relay hub: device presence + session rooms, all in memory (masterplan
// §3.1/§3.4). No DB, no persistence — if the relay restarts, sockets reconnect
// and re-announce; nothing durable is lost because nothing durable existed.

import type { SteerTicketClaims } from "@exp/steer-ticket"
import {
  CLOSE_PUBLISHER_IDLE,
  CLOSE_REPLACED,
  CLOSE_SESSION_ENDED,
  CLOSE_SLOW_CONSUMER,
  parseClientFrame,
  type ActivityEvent,
  type ClientFrame,
  type LaunchDefaults,
  type ServerFrame,
  type StartInput,
  type StartRepoGroup,
  type StartSessionOptions,
} from "./protocol"

// A remote start's subject: a single issue (wire-unchanged), a batch group,
// or an action run (EXP-253 — repo absent for repo-less actions; `inputs`
// carries the EXP-257 resolved input values).
export type StartSubject =
  | { issueId: string }
  | { issueIds: string[]; teamId: string; repo: StartRepoGroup }
  | {
      actionId: string
      actionName: string
      teamId: string
      repo?: StartRepoGroup
      inputs?: StartInput[]
    }

// Abstracted so the hub is unit-testable with fake sockets; the Bun layer
// adapts ServerWebSocket to this. Text-only since EXP-249 removed the binary
// PTY mirror.
export interface RelaySocket {
  send(data: string): void
  close(code?: number, reason?: string): void
  /** Bytes queued on the socket, for viewer backpressure. */
  bufferedAmount(): number
}

interface Conn {
  sock: RelaySocket
  claims: SteerTicketClaims
  // control sockets announce a deviceId via `online`.
  deviceId?: string
  // publisher/viewer sockets belong to a room after hello/join.
  sessionId?: string
}

interface DeviceEntry {
  conn: Conn
  deviceLabel: string
  connectedAt: number
  /** EXP-201: agent CLIs the device advertised in its `online` frame.
   * Absent on old desktops ⇒ defaulted to ["claude"] at store time.
   * Since EXP-409: runnable (installed AND signed in). */
  agents: string[]
  /** EXP-409: agents installed but signed out on the device — unusable,
   * passed through so the web can explain instead of hiding them. */
  unauthedAgents: string[]
  /** EXP-253: feature capabilities (`actions`) the device advertised.
   * Absent on old desktops ⇒ [] — the web server strictly gates action
   * starts on this. */
  caps: string[]
  /** EXP-437: the machine's per-agent launch defaults, passed through
   * verbatim so remote Start-coding dialogs can seed from the selected
   * device. Absent on old desktops ⇒ stays absent on the wire. */
  launchDefaults?: LaunchDefaults
}

/** One replayable activity event, pre-serialized once: the same string feeds
 *  the live fan-out and every later join replay. */
interface ActivityEntry {
  framed: string
  bytes: number
}

interface Room {
  sessionId: string
  issueId?: string
  publisher: Conn | null
  /** Publisher dropped without `bye`; room closes when the grace expires. */
  staleTimer: ReturnType<typeof setTimeout> | null
  /** REV2-X: Last time we received ANY message from the publisher (including
   * pings). Updated on every publisher frame to detect dead publishers during
   * idle periods (plan mode). */
  lastPublisherActivity: number
  // ── Scrubbed activity channel (tool headlines / narration / diffs) ──────
  // activityMembers: authenticated viewer tickets that joined with
  // channel:'activity' — the only audience there is (EXP-90 removed the
  // anonymous one, EXP-249 the PTY mirror).
  activityMembers: Set<Conn>
  /** Replayable scrubbed event log, capped by count AND bytes. */
  activityLog: ActivityEntry[]
  /** Running serialized size of activityLog (the byte-budget accumulator). */
  activityBytes: number
  /** Latest worktree diff — replaces rather than appends (replay stays small),
   *  and stays OUT of the byte budget: the schema already caps it at 512KB. */
  lastDiff: ActivityEntry | null
}

// EXP-249: full-history re-publish on reconnect means a long session's log is
// the whole session. Bounded twice — a count cap for pathological chatter and
// a byte budget for pathological size; eviction is oldest-first and always
// leaves at least one event.
const ACTIVITY_LOG_CAP = 2000
const ACTIVITY_BYTE_CAP = 4 * 1024 * 1024

// An activity socket with more than this queued is evicted: activity is
// low-volume JSON, so saturation means the consumer is gone, not lagging.
const VIEWER_HIGH_WATER = 512 * 1024
const PUBLISHER_GRACE_MS = 60_000
// REV2-X: If we receive no frames (including pings) from a publisher for this
// long, assume the connection is dead and close the room. Desktop pings every
// 30s, so 90s = 3 missed pings = definitely dead. Fixes the plan-mode hang
// where a dropped connection sits undetected and UIs retry `no_such_session`.
const PUBLISHER_IDLE_TIMEOUT_MS = 90_000
const PUBLISHER_IDLE_CHECK_INTERVAL_MS = 30_000

function frame(msg: ServerFrame): string {
  return JSON.stringify(msg)
}

export class Hub {
  private conns = new Map<RelaySocket, Conn>()
  /** userId → deviceId → control connection (drives the phone's device picker). */
  private devices = new Map<string, Map<string, DeviceEntry>>()
  /** sessionId (== coding_sessions.id) → room. */
  private rooms = new Map<string, Room>()
  /** REV2-X: Periodic check for idle publishers. */
  private idleCheckInterval: ReturnType<typeof setInterval>

  constructor() {
    // REV2-X: Start the idle publisher detector — checks every 30s for
    // publishers that haven't sent ANY frame (including pings) in 90s.
    this.idleCheckInterval = setInterval(() => {
      this.checkIdlePublishers()
    }, PUBLISHER_IDLE_CHECK_INTERVAL_MS)
  }

  /** REV2-X: Stop the idle check interval (for clean shutdown/tests). */
  destroy() {
    clearInterval(this.idleCheckInterval)
  }

  // ── Socket lifecycle (called from the Bun ws handlers) ────────────────────

  onOpen(sock: RelaySocket, claims: SteerTicketClaims) {
    const conn: Conn = { sock, claims }
    this.conns.set(sock, conn)

    if (claims.role === `publisher` && claims.sessionId) {
      // Room attaches on `hello` — nothing yet.
    } else if (claims.role === `viewer` && claims.sessionId) {
      // Viewers join on `join` — nothing yet.
    }
  }

  onMessage(sock: RelaySocket, data: string | Uint8Array) {
    const conn = this.conns.get(sock)
    if (!conn) return

    // REV2-X: Update last activity timestamp for publisher connections — ANY
    // message (including pings and the ignored legacy binary frames) counts,
    // so an idle-but-connected publisher (plan mode) stays alive while a truly
    // dead publisher times out.
    if (conn.claims.role === `publisher` && conn.sessionId) {
      const room = this.rooms.get(conn.sessionId)
      if (room && room.publisher === conn) {
        room.lastPublisherActivity = Date.now()
      }
    }

    // EXP-249: binary frames were the PTY mirror. Old desktops still push
    // them — they count as liveness (above) and are otherwise dropped.
    if (typeof data !== `string`) return

    const msg = parseClientFrame(data)
    if (!msg) return
    this.onControl(conn, msg)
  }

  /** REV2-X: Bun delivers protocol-level ping frames to a dedicated `ping`
   * handler, NOT to `message`, so an idle publisher's 30s keepalive pings
   * never reach onMessage. Without this hook, lastPublisherActivity would
   * stop refreshing during a live-but-quiet session (plan mode / a parked
   * agent), and checkIdlePublishers would wrongly detach the publisher after
   * 90s (a churny reconnect at best — EXP-283 made the idle close
   * non-terminal). Bump activity here exactly as onMessage does for data
   * frames. */
  onPing(sock: RelaySocket) {
    const conn = this.conns.get(sock)
    if (!conn) return
    if (conn.claims.role === `publisher` && conn.sessionId) {
      const room = this.rooms.get(conn.sessionId)
      if (room && room.publisher === conn) {
        room.lastPublisherActivity = Date.now()
      }
    }
  }

  onClose(sock: RelaySocket) {
    const conn = this.conns.get(sock)
    if (!conn) return
    this.conns.delete(sock)

    // Device presence eviction.
    if (conn.deviceId) {
      const byDevice = this.devices.get(conn.claims.sub)
      const entry = byDevice?.get(conn.deviceId)
      if (entry?.conn === conn) {
        byDevice!.delete(conn.deviceId)
        if (byDevice!.size === 0) this.devices.delete(conn.claims.sub)
      }
    }

    if (!conn.sessionId) return
    const room = this.rooms.get(conn.sessionId)
    if (!room) return

    if (room.publisher === conn) {
      // Publisher dropped without bye → grace period for reconnect.
      room.publisher = null
      room.staleTimer ??= setTimeout(() => {
        this.closeRoom(room, `publisher_lost`)
      }, PUBLISHER_GRACE_MS)
      return
    }

    room.activityMembers.delete(conn)
  }

  // ── Control frames ─────────────────────────────────────────────────────────

  private onControl(conn: Conn, msg: ClientFrame) {
    switch (msg.t) {
      case `online`: {
        if (conn.claims.role !== `control`) return
        let byDevice = this.devices.get(conn.claims.sub)
        // REV-11: a socket re-announcing under a NEW deviceId must evict the
        // entry it registered before — onClose only reclaims the LAST
        // announced id, so without this every extra `online` frame leaked a
        // DeviceEntry forever (unbounded heap for a hostile control client,
        // and ghost devices whose startSession sends into a dead socket).
        // One control socket owns at most one presence entry.
        if (conn.deviceId && conn.deviceId !== msg.deviceId) {
          const prev = byDevice?.get(conn.deviceId)
          if (prev?.conn === conn) byDevice!.delete(conn.deviceId)
        }
        conn.deviceId = msg.deviceId
        if (!byDevice) {
          byDevice = new Map()
          this.devices.set(conn.claims.sub, byDevice)
        }
        // A reconnect for the same device replaces the old socket.
        const prior = byDevice.get(msg.deviceId)
        if (prior && prior.conn !== conn) {
          prior.conn.sock.close(CLOSE_REPLACED, `replaced`)
        }
        byDevice.set(msg.deviceId, {
          conn,
          deviceLabel:
            msg.deviceLabel ?? conn.claims.deviceLabel ?? `Desktop`,
          connectedAt: Date.now(),
          // Absent = old desktop that predates the advertisement — it can
          // run exactly what every desktop could before EXP-201: claude.
          agents: msg.agents ?? [`claude`],
          // Absent = old sender that predates EXP-409.
          unauthedAgents: msg.unauthedAgents ?? [],
          // Absent = old desktop with no action launch path (EXP-253).
          caps: msg.caps ?? [],
          // Absent (old desktop / malformed blob caught to undefined) =
          // no defaults advertised — clients seed statically (EXP-437).
          launchDefaults: msg.launchDefaults,
        })
        return
      }

      case `hello`: {
        if (conn.claims.role !== `publisher`) return
        const sessionId = conn.claims.sessionId
        if (!sessionId || msg.sessionId !== sessionId) return
        conn.sessionId = sessionId
        let room = this.rooms.get(sessionId)
        if (!room) {
          room = {
            sessionId,
            issueId: msg.issueId,
            publisher: conn,
            staleTimer: null,
            lastPublisherActivity: Date.now(), // REV2-X
            activityMembers: new Set(),
            activityLog: [],
            activityBytes: 0,
            lastDiff: null,
          }
          this.rooms.set(sessionId, room)
        } else {
          // Re-hello after a drop: resume the same room. The publisher clears
          // and re-publishes its own history via `activity_reset` — the relay
          // never guesses what survived the gap.
          if (room.staleTimer) {
            clearTimeout(room.staleTimer)
            room.staleTimer = null
          }
          if (room.publisher && room.publisher !== conn) {
            room.publisher.sock.close(CLOSE_REPLACED, `replaced`)
          }
          room.publisher = conn
          room.lastPublisherActivity = Date.now() // REV2-X: reconnect resets the timer
        }
        return
      }

      case `join`: {
        const sessionId = conn.claims.sessionId
        if (!sessionId) return
        if (conn.claims.role !== `viewer`) return

        // EXP-249: the PTY mirror is gone. An absent channel (or the legacy
        // `pty`) gets a typed error and joins NOTHING, so an old client shows
        // an update prompt instead of an empty terminal.
        if (msg.channel !== `activity`) {
          conn.sock.send(frame({ t: `error`, code: `pty_removed` }))
          return
        }

        const room = this.rooms.get(sessionId)
        if (!room) {
          conn.sock.send(frame({ t: `error`, code: `no_such_session` }))
          conn.sock.close(CLOSE_SESSION_ENDED, `no_such_session`)
          return
        }
        conn.sessionId = sessionId

        room.activityMembers.add(conn)
        // `activity_reset` first: a reconnecting client keeps rendering its
        // old feed until the relay says otherwise, so the replay that follows
        // is always a complete, self-contained picture.
        conn.sock.send(frame({ t: `activity_reset` }))
        this.replayActivity(room, conn)
        return
      }

      case `resize`: {
        // Legacy PTY geometry from old desktops — parsed, then dropped.
        return
      }

      case `input`: {
        const room = this.roomFor(conn)
        if (!room || !room.publisher) return
        // Steering is seamless and owner-only (EXP-312): tickets are minted
        // exclusively for the session owner, so a joined viewer's keystrokes
        // just flow — no operator claim, no perm tier.
        if (!room.activityMembers.has(conn)) return
        room.publisher.sock.send(frame({ t: `input`, data: msg.data }))
        return
      }

      case `answer`: {
        const room = this.roomFor(conn)
        if (!room || !room.publisher) return
        // Same gating as `input`.
        if (!room.activityMembers.has(conn)) return
        room.publisher.sock.send(
          frame({
            t: `answer`,
            questionId: msg.questionId,
            ...(msg.askId !== undefined ? { askId: msg.askId } : {}),
            keys: msg.keys,
            ...(msg.text !== undefined ? { text: msg.text } : {}),
          })
        )
        return
      }

      case `kill`: {
        const room = this.roomFor(conn)
        if (!room || !room.publisher) return
        // Same gating as `input` — a joined (owner-minted) viewer.
        if (!room.activityMembers.has(conn)) return
        room.publisher.sock.send(frame({ t: `kill` }))
        return
      }

      case `activity`: {
        // Publisher-only: the desktop's scrubbed event stream.
        const room = this.roomFor(conn)
        if (!room || room.publisher !== conn) return
        const entry = this.entryFor(msg.event)
        if (msg.event.kind === `diff`) {
          room.lastDiff = entry
        } else {
          this.appendActivity(room, entry)
        }
        this.fanoutActivity(room, entry.framed)
        return
      }

      case `activity_reset`: {
        // Publisher-only: the desktop is about to re-publish its full history.
        const room = this.roomFor(conn)
        if (!room || room.publisher !== conn) return
        room.activityLog = []
        room.activityBytes = 0
        room.lastDiff = null
        this.fanoutActivity(room, frame({ t: `activity_reset` }))
        return
      }

      case `bye`: {
        const room = this.roomFor(conn)
        if (!room || room.publisher !== conn) return
        this.closeRoom(room, msg.outcome ?? `ended`)
        return
      }
    }
  }

  // ── Admin (server-to-server HTTP, secret-authed) ──────────────────────────

  devicesFor(userId: string) {
    const byDevice = this.devices.get(userId)
    if (!byDevice) return []
    return [...byDevice.entries()].map(([deviceId, entry]) => ({
      deviceId,
      deviceLabel: entry.deviceLabel,
      connectedAt: entry.connectedAt,
      agents: entry.agents,
      unauthedAgents: entry.unauthedAgents,
      caps: entry.caps,
      // `undefined` drops out of JSON.stringify — old desktops' rows keep
      // their exact pre-EXP-437 wire shape.
      launchDefaults: entry.launchDefaults,
    }))
  }

  sessionInfo(sessionId: string) {
    const room = this.rooms.get(sessionId)
    if (!room) return { live: false as const }
    return {
      live: room.publisher !== null,
      viewers: room.activityMembers.size,
      issueId: room.issueId ?? null,
    }
  }

  /** Route a remote "Start on my desktop" to the device's control socket.
   * `subject` is a single issue (wire-unchanged), a batch group (issueIds +
   * teamId + repo), or an action run (actionId + actionName + teamId +
   * optional repo — EXP-253). `options` fields are optional launch options
   * (EXP-149); undefineds are dropped by JSON.stringify, so an option-less
   * single-issue start stays byte-identical to the pre-options frame. */
  startSession(
    userId: string,
    deviceId: string,
    subject: StartSubject,
    options: StartSessionOptions = {}
  ): { ok: true } | { ok: false; reason: `device_offline` } {
    const entry = this.devices.get(userId)?.get(deviceId)
    if (!entry) return { ok: false, reason: `device_offline` }
    // Build each variant explicitly — a raw union spread won't narrow for the
    // ServerFrame `frame()` call. Single-issue key order (t, issueId, options)
    // stays byte-for-byte with the pre-batch frame.
    const payload: ServerFrame =
      `issueId` in subject
        ? { t: `start_session`, issueId: subject.issueId, ...options }
        : `actionId` in subject
          ? {
              t: `start_session`,
              actionId: subject.actionId,
              actionName: subject.actionName,
              teamId: subject.teamId,
              ...(subject.repo ? { repo: subject.repo } : {}),
              ...(subject.inputs ? { inputs: subject.inputs } : {}),
              ...options,
            }
          : {
              t: `start_session`,
              issueIds: subject.issueIds,
              teamId: subject.teamId,
              repo: subject.repo,
              ...options,
            }
    entry.conn.sock.send(frame(payload))
    return { ok: true }
  }

  /** EXP-481: fire-and-forget `check_in` nudge to a device's control socket —
   * "the server persisted new work for you, heartbeat now". Returns whether a
   * live socket received it; the heartbeat pickup is the durable path either
   * way. */
  nudge(userId: string, deviceId: string): boolean {
    const entry = this.devices.get(userId)?.get(deviceId)
    if (!entry) return false
    entry.conn.sock.send(frame({ t: `check_in` }))
    return true
  }

  /** Server-side kill (steer.killSession fallback path). */
  killSession(sessionId: string): boolean {
    const room = this.rooms.get(sessionId)
    if (!room?.publisher) return false
    room.publisher.sock.send(frame({ t: `kill` }))
    return true
  }

  stats() {
    return {
      connections: this.conns.size,
      devices: [...this.devices.values()].reduce((n, m) => n + m.size, 0),
      rooms: this.rooms.size,
    }
  }

  // ── Internals ──────────────────────────────────────────────────────────────

  private roomFor(conn: Conn): Room | undefined {
    return conn.sessionId ? this.rooms.get(conn.sessionId) : undefined
  }

  private entryFor(event: ActivityEvent): ActivityEntry {
    const framed = frame({ t: `activity`, event })
    return { framed, bytes: Buffer.byteLength(framed, `utf8`) }
  }

  private appendActivity(room: Room, entry: ActivityEntry) {
    room.activityLog.push(entry)
    room.activityBytes += entry.bytes
    while (
      room.activityLog.length > ACTIVITY_LOG_CAP ||
      (room.activityBytes > ACTIVITY_BYTE_CAP && room.activityLog.length > 1)
    ) {
      room.activityBytes -= room.activityLog.shift()!.bytes
    }
  }

  /** Fan one pre-serialized text frame to the activity audience. Activity is
   *  low-volume JSON, so a saturated socket is evicted outright — there is no
   *  drop-and-recover path. */
  private fanoutActivity(room: Room, framed: string) {
    for (const member of room.activityMembers.keys()) {
      if (member.sock.bufferedAmount() > VIEWER_HIGH_WATER) {
        member.sock.close(CLOSE_SLOW_CONSUMER, `slow_consumer`)
        continue
      }
      member.sock.send(framed)
    }
  }

  /** Replay the scrubbed event log then the latest diff to one socket. */
  private replayActivity(room: Room, conn: Conn) {
    for (const entry of room.activityLog) {
      conn.sock.send(entry.framed)
    }
    if (room.lastDiff) conn.sock.send(room.lastDiff.framed)
  }

  private closeRoom(room: Room, outcome: string) {
    if (room.staleTimer) clearTimeout(room.staleTimer)
    this.rooms.delete(room.sessionId)
    const msg = frame({ t: `bye`, outcome })
    for (const member of room.activityMembers.keys()) {
      member.sock.send(msg)
      member.sock.close(CLOSE_SESSION_ENDED, `session_ended`)
    }
    room.activityMembers.clear()
  }

  /** REV2-X: Periodic check for publishers that have sent no frames (including
   * pings) within PUBLISHER_IDLE_TIMEOUT_MS. A silent publisher is a dead
   * publisher — detach it so viewers stop retrying `no_such_session`.
   *
   * EXP-283: the close code MUST NOT be CLOSE_SESSION_ENDED — publishers
   * treated 4001 as a terminal remote kill (tear down the live agent + the
   * whole terminal tab), so a publisher that merely slept through the idle
   * window (laptop suspend, network stall) had its live coding session
   * killed the moment it woke up and read the close frame. An idle socket is
   * a transport-level determination, never a session end: close with the
   * dedicated CLOSE_PUBLISHER_IDLE, which every desktop (old and new) treats
   * as a plain drop and reconnects from. */
  private checkIdlePublishers() {
    const now = Date.now()
    for (const room of this.rooms.values()) {
      if (!room.publisher) continue // already detached
      const idleMs = now - room.lastPublisherActivity
      if (idleMs >= PUBLISHER_IDLE_TIMEOUT_MS) {
        console.log(
          `[hub] publisher for session ${room.sessionId} idle for ${Math.floor(idleMs / 1000)}s — detaching publisher`
        )
        // Closing the socket triggers onClose's grace-period logic: if the
        // publisher reconnects within the grace window, re-hello resets
        // lastPublisherActivity so the next idle check won't re-fire; if it
        // doesn't reconnect, the grace timer closes the room.
        room.publisher.sock.close(
          CLOSE_PUBLISHER_IDLE,
          `publisher_idle_${Math.floor(idleMs / 1000)}s`
        )
      }
    }
  }
}
