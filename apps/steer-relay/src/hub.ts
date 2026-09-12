// The relay hub: device presence + session rooms, all in memory (masterplan
// §3.1/§3.4). No DB, no persistence — if the relay restarts, sockets reconnect
// and re-announce; nothing durable is lost because nothing durable existed.

import type { SteerTicketClaims } from "@exp/steer-ticket"
import {
  CLOSE_PUBLISHER_IDLE,
  CLOSE_REPLACED,
  CLOSE_SESSION_ENDED,
  CLOSE_SLOW_CONSUMER,
  describeClientFrameRejection,
  parseClientFrame,
  type ActivityEvent,
  type ClientFrame,
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
  // EXP-637: resume an ended run — see the ServerFrame arm in protocol.ts.
  | {
      resumeSessionId: string
      teamId: string
      issueId?: string
      actionId?: string
      actionName?: string
      branch?: string
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
  /** Last time this socket's rejected-frame warning was logged (rate limit). */
  lastFrameWarnAt?: number
}

/** One replayable activity event, pre-serialized once: the same string feeds
 *  the live fan-out and every later join replay. */
interface ActivityEntry {
  framed: string
  bytes: number
  /** EXP-748: set to the subagentId when this entry is a subagent's tool
   *  headline — the class of event the caps give up FIRST. */
  subagentTool?: string
  /** EXP-783: the publisher's own monotonic index, echoed verbatim inside
   *  `framed`. Kept here too so `activity_synced` can name the span the log
   *  covers without re-parsing it. */
  seq?: number
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
  /** EXP-748: live subagent tool entries in activityLog, by subagentId — one
   *  subagent keeps at most SUBAGENT_TOOL_CAP of them. */
  subagentToolCounts: Map<string, number>
  /** Their total, so the count cap knows in O(1) whether anything is left to
   *  give up before it starts dropping the main transcript. */
  subagentToolEntries: number
  /** Scan hint: no subagent tool entry lives BELOW this index, so the search
   *  for the oldest one starts here instead of at the head. */
  subagentScanFrom: number
  /** EXP-773: the room exists only to catch a device's history replay — no
   *  publisher has hello'd yet, and its `historyTimer` closes it if none
   *  does. A viewer that joins one is told `history_pending` instead of
   *  `activity_synced`. */
  pendingHistory: boolean
  /** EXP-773: fires HISTORY_TIMEOUT_MS after the ask went down the device's
   *  control socket — the device is online but produced nothing. */
  historyTimer: ReturnType<typeof setTimeout> | null
  /** EXP-746: latest-wins STATE by kind (`diff`, `config_state`, `usage`,
   *  `rate_limit`, `turn`) —
   *  the newest one replaces its predecessor, stays OUT of the count/byte
   *  budget, and the join replay sends them after the log. Each schema
   *  already caps the payload (a diff at 512KB, a config at 8 options). */
  lastByKind: Map<string, ActivityEntry>
  /** EXP-783: this log has EVICTED at least one event since it was last
   *  cleared, so it is a tail rather than the whole run. Reported on
   *  `activity_synced` so a client knows the pages below it must be asked
   *  for from the device (`history_page`) and are not simply absent. */
  activityTruncated: boolean
  /** EXP-783: the `history_page` asks in flight, by the RELAY-issued id the
   *  publisher answers with (EXP-795: every viewer numbers its own asks from
   *  `p1`, so two viewers in one room would otherwise share a key and one
   *  would receive the other's page). Dropped with the viewer, with the
   *  publisher the ask went to, with the room, and by its own timer. */
  historyPages: Map<string, HistoryPageAsk>
  /** EXP-796: the machine this history room was opened AGAINST — the ticket's
   *  device, under the account it is registered to. Set by `requestHistory`
   *  and looked up LIVE on every page ask, so a device that re-dialed its
   *  control socket in between still answers. Absent on a room a publisher
   *  opened itself. */
  historyDevice: { userId: string; deviceId: string } | null
  /** EXP-796: the device replayed its journal and said `bye {outcome:
   *  'history'}`; the room now LINGERS with no publisher so the parked
   *  viewers can page ("Load earlier") down the device's control socket.
   *  Closed by `lingerTimer`, by the last viewer leaving, or by the device
   *  going away with a page in flight. */
  historyServed: boolean
  /** EXP-796: fires HISTORY_ROOM_LINGER_MS after the replay's `bye`. */
  lingerTimer: ReturnType<typeof setTimeout> | null
}

/** One in-flight `history_page`: who gets the chunks, under which id THEY
 *  asked, and the timer that frees the slot if no chunk ever comes. */
interface HistoryPageAsk {
  viewer: Conn
  requestId: string
  timer: ReturnType<typeof setTimeout>
}

// EXP-249: full-history re-publish on reconnect means a long session's log is
// the whole session. Bounded twice — a count cap for pathological chatter and
// a byte budget for pathological size; the byte budget is a hard bound and
// evicts oldest-first, and both always leave at least one event.
//
// EXP-748: the count cap is TWO-TIER. A parallelising agent spends most of a
// long run inside subagents, so the flat cap used to evict the main
// transcript — the narration a viewer actually reads — to make room for a
// fan-out's tool spam. So the count cap gives up subagent tool headlines
// FIRST (oldest first, across every subagent) and only drops the head once
// none are left; on top of that ONE subagent keeps at most
// SUBAGENT_TOOL_CAP of them, so a single runaway fan-out cannot spend the
// whole budget. The `subagent` card's `toolCalls` (protocol.ts) is what keeps
// the count honest once entries are evicted.
const ACTIVITY_LOG_CAP = 2000
const ACTIVITY_BYTE_CAP = 4 * 1024 * 1024
const SUBAGENT_TOOL_CAP = 50

// EXP-746: kinds that are latest-wins STATE rather than transcript rows.
// Appending them would burn the budgets above on stale snapshots — a `usage`
// frame per turn on a long run would evict real transcript events out of the
// replay window. EXP-784 adds `rate_limit`; a `tool_update` (EXP-785) is a
// plain log row, deliberately NOT here — it folds into its tool row. EXP-848
// adds `turn`, so a late joiner learns whether the agent is mid-turn.
const LATEST_WINS_KINDS = new Set([`diff`, `config_state`, `usage`, `rate_limit`, `turn`])
// Replay order for the latest-wins slots — `diff` stays LAST, exactly where
// it replayed before this became a map.
const LATEST_REPLAY_ORDER = [`config_state`, `usage`, `rate_limit`, `turn`, `diff`] as const

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
// EXP-648: viewers (web/iOS/Android) redial a nominally live socket that has
// been silent for 45s — three of these ticks — because nothing else tells
// them a quiet socket from a dead one: the desktop's 30s ping is a WS control
// frame Bun hands to `ping`, never to `message`, so while an agent waits on a
// question or plan approval the relay used to send viewers NOTHING, and every
// foreground/push/network kick cost a mint + activity_reset + full replay.
// Same 3-missed-ticks ratio as the publisher's 30s ping / 90s idle window.
const VIEWER_KEEPALIVE_INTERVAL_MS = 15_000
// A frame the schema rejects is dropped on the floor; without a log line a
// publisher/client version skew is invisible from the relay side. One warning
// per SOCKET per window keeps a chatty or hostile client from filling the
// log.
const FRAME_WARN_INTERVAL_MS = 30_000
// EXP-773: how long a pending history room waits for the device to hello. It
// has to mint a publisher ticket (a tRPC round trip) and read a file back, so
// the window is generous; past it the viewer is told the transcript is not
// coming rather than left spinning.
const HISTORY_TIMEOUT_MS = 20_000
/** EXP-773: the `bye` outcome a history replay ends with (steer's
 *  `HISTORY_OUTCOME`) — the room answers `activity_synced` before closing,
 *  because the parked viewers have just been sent a complete transcript. */
const HISTORY_OUTCOME = `history`
/** EXP-783: how many `history_page` asks one room may have outstanding. A
 *  viewer pages one screen at a time and a chunk is bounded by
 *  `HISTORY_PAGE_MAX`, so this only exists to stop a misbehaving client from
 *  making a device replay its journal in parallel forever. */
const HISTORY_PAGES_IN_FLIGHT = 4
/** EXP-795: how long one `history_page` ask may wait for its chunk. The
 *  publisher answers inline off a bounded file read, so a slot still open
 *  after this belongs to a publisher that is gone — freed, or the room's
 *  in-flight cap would fill with dead asks and refuse every viewer. */
const HISTORY_PAGE_TIMEOUT_MS = 20_000
/** EXP-796: how long a history room stays up after the device's replay said
 *  `bye {outcome:'history'}`. The parked viewers keep their sockets and page
 *  older transcript through the device's control socket; the room goes when
 *  this expires or the last viewer leaves, whichever is first, and viewers
 *  never re-dial to page. */
export const HISTORY_ROOM_LINGER_MS = 5 * 60_000

function frame(msg: ServerFrame): string {
  return JSON.stringify(msg)
}

/** Serialized once: every tick fans the same bytes to every joined viewer. */
const KEEPALIVE_FRAME = frame({ t: `keepalive` })

/** EXP-700: server-injected input chunk size — mirrors the browser viewer's
 * INPUT_CHUNK_CHARS (steer-session-store.ts). Well under the socket's
 * `maxPayloadLength` (1 MiB, index.ts): the chunking keeps injected text in
 * paste-sized pieces the publisher forwards smoothly, not at a frame cap. */
const INPUT_CHUNK_CHARS = 4096
/** EXP-656: end-of-join-replay marker (see `ServerFrame`). EXP-783 made it
 *  room-dependent — it names the span the replay covered — so it is built per
 *  room by `Hub.activitySyncedFrame` rather than serialized once. */
function activitySyncedFrame(room: Room): string {
  const first = room.activityLog[0]?.seq
  const last = room.activityLog[room.activityLog.length - 1]?.seq
  // The log is a tail when this room evicted — and (EXP-795) when the
  // publisher's own replay started above zero: its in-memory journal is a
  // bounded tail of the file it wrote, and a resumed run inherits its
  // predecessor's lines. Either way the pages below `firstSeq` exist on the
  // device, and a client gated on this flag alone could never ask for them.
  const truncated =
    room.activityTruncated || (first !== undefined && first > 0)
  return frame({
    t: `activity_synced`,
    firstSeq: first,
    lastSeq: last,
    truncated: truncated || undefined,
  })
}

export class Hub {
  private conns = new Map<RelaySocket, Conn>()
  /** userId → deviceId → control connection: the routing table `/start` and
   *  `/nudge` send down, and nothing else. EXP-672 dropped the outbound
   *  presence listing (label/connectedAt/caps) — the web server reads the
   *  persisted `devices` row for everything it shows or gates on. */
  private devices = new Map<string, Map<string, Conn>>()
  /** sessionId (== coding_sessions.id) → room. */
  private rooms = new Map<string, Room>()
  /** REV2-X: Periodic check for idle publishers. */
  private idleCheckInterval: ReturnType<typeof setInterval>
  /** EXP-648: the viewer keepalive tick. */
  private keepaliveInterval: ReturnType<typeof setInterval>

  // EXP-553: monotonic counters for the admin performance page — exposed via
  // counters() on the secret-gated /stats endpoint (stats() stays gauges-only
  // because /healthz is public).
  private connectionsAccepted = 0
  private activityFramesFanned = 0
  private startsRouted = 0
  private slowConsumerEvictions = 0
  // EXP-656: churn diagnostics — a viewer that redials every ~30s shows up as
  // a join rate, a desktop the idle detector keeps detaching as idle closes.
  private viewerJoins = 0
  private publisherIdleCloses = 0
  // EXP-773: how often a viewer asked a device for a stored transcript, and
  // the two ways that ask fails.
  private historyRequests = 0
  private historyDeviceOffline = 0
  private historyTimeouts = 0
  /** EXP-783: `history_page` asks routed to a publisher, and (EXP-795) how
   *  many of them the publisher never answered. */
  private historyPageRequests = 0
  private historyPageTimeouts = 0
  /** EXP-795: numbers the relay-issued `history_page` ids. */
  private historyPageSeq = 0
  /** EXP-796: `history_page` asks routed down a device's CONTROL socket (a
   *  lingering history room has no publisher), and the two ways a lingering
   *  room ends without its viewers leaving — the linger timer, and the device
   *  going offline with a page in flight. */
  private historyPagesViaDevice = 0
  private historyRoomLingerExpiries = 0
  private historyRoomDeviceLost = 0

  constructor() {
    // REV2-X: Start the idle publisher detector — checks every 30s for
    // publishers that haven't sent ANY frame (including pings) in 90s.
    this.idleCheckInterval = setInterval(() => {
      this.checkIdlePublishers()
    }, PUBLISHER_IDLE_CHECK_INTERVAL_MS)
    this.keepaliveInterval = setInterval(() => {
      this.sendViewerKeepalives()
    }, VIEWER_KEEPALIVE_INTERVAL_MS)
  }

  /** Stop the periodic ticks (for clean shutdown/tests). */
  destroy() {
    clearInterval(this.idleCheckInterval)
    clearInterval(this.keepaliveInterval)
  }

  // ── Socket lifecycle (called from the Bun ws handlers) ────────────────────

  onOpen(sock: RelaySocket, claims: SteerTicketClaims) {
    const conn: Conn = { sock, claims }
    this.conns.set(sock, conn)
    this.connectionsAccepted += 1

    if (claims.role === `publisher` && claims.sessionId) {
      // Room attaches on `hello` — nothing yet.
    } else if (claims.role === `viewer` && claims.sessionId) {
      // Viewers join on `join` — nothing yet.
    }
  }

  onMessage(sock: RelaySocket, data: string) {
    const conn = this.conns.get(sock)
    if (!conn) return

    // REV2-X: Update last activity timestamp for publisher connections — ANY
    // message (including pings) counts, so an idle-but-connected publisher
    // (plan mode) stays alive while a truly dead publisher times out.
    if (conn.claims.role === `publisher` && conn.sessionId) {
      const room = this.rooms.get(conn.sessionId)
      if (room && room.publisher === conn) {
        room.lastPublisherActivity = Date.now()
      }
    }

    const msg = parseClientFrame(data)
    if (!msg) {
      this.warnRejectedFrame(conn, data)
      return
    }
    this.onControl(conn, msg)
  }

  /** One rate-limited line for a frame the schema rejected: the socket's role,
   *  the frame's `t`, and the first zod issue's path — structure only, never
   *  frame CONTENT (the activity channel is scrubbed and stays that way). */
  private warnRejectedFrame(conn: Conn, raw: string) {
    const now = Date.now()
    if (
      conn.lastFrameWarnAt !== undefined &&
      now - conn.lastFrameWarnAt < FRAME_WARN_INTERVAL_MS
    ) {
      return
    }
    conn.lastFrameWarnAt = now
    const { t, issue } = describeClientFrameRejection(raw)
    console.warn(
      `[hub] dropped ${conn.claims.role} frame (t=${t ?? `?`}): ${issue}`
    )
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
      if (byDevice?.get(conn.deviceId) === conn) {
        byDevice!.delete(conn.deviceId)
        if (byDevice!.size === 0) this.devices.delete(conn.claims.sub)
        // EXP-796: a page ask went down THIS socket and nothing will answer
        // it — the device is off the table, not merely replaced.
        this.deviceLeftHistoryRooms(conn.claims.sub, conn.deviceId)
      }
    }

    if (!conn.sessionId) return
    const room = this.rooms.get(conn.sessionId)
    if (!room) return

    if (room.publisher === conn) {
      // Publisher dropped without bye → grace period for reconnect.
      room.publisher = null
      // EXP-795: every page ask went down THIS socket; none is coming back.
      this.dropHistoryPages(room)
      room.staleTimer ??= setTimeout(() => {
        this.closeRoom(room, `publisher_lost`)
      }, PUBLISHER_GRACE_MS)
      return
    }

    room.activityMembers.delete(conn)
    // EXP-783: a page this viewer asked for has nowhere to go now.
    this.dropHistoryPages(room, conn)
    // EXP-796: a lingering history room exists for its viewers alone — the
    // last one leaving takes it down (nobody is left to tell).
    if (room.historyServed && room.activityMembers.size === 0) {
      this.closeRoom(room, HISTORY_OUTCOME)
    }
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
          if (byDevice?.get(conn.deviceId) === conn) {
            byDevice!.delete(conn.deviceId)
          }
        }
        conn.deviceId = msg.deviceId
        if (!byDevice) {
          byDevice = new Map()
          this.devices.set(conn.claims.sub, byDevice)
        }
        // A reconnect for the same device replaces the old socket.
        const prior = byDevice.get(msg.deviceId)
        if (prior && prior !== conn) {
          prior.sock.close(CLOSE_REPLACED, `replaced`)
        }
        byDevice.set(msg.deviceId, conn)
        return
      }

      case `hello`: {
        if (conn.claims.role !== `publisher`) return
        const sessionId = conn.claims.sessionId
        if (!sessionId || msg.sessionId !== sessionId) return
        conn.sessionId = sessionId
        let room = this.rooms.get(sessionId)
        if (!room) {
          room = this.newRoom(sessionId, conn, msg.issueId)
          this.rooms.set(sessionId, room)
        } else {
          // EXP-773: the publisher this room was WAITING for (a history
          // replay, or a live publisher that raced the ask) — the room stops
          // being pending and the timeout is off.
          if (room.historyTimer) {
            clearTimeout(room.historyTimer)
            room.historyTimer = null
          }
          // EXP-796: a publisher into a LINGERING room (a second replay from
          // a redial) takes it back: the linger is off and the log is a
          // replay's again, so the flow below starts it empty.
          if (room.lingerTimer) {
            clearTimeout(room.lingerTimer)
            room.lingerTimer = null
          }
          room.pendingHistory = false
          room.historyServed = false
          // Re-hello after a drop, a live publisher taking a replay's room
          // back, or a second replay: every publisher opens with its own
          // `activity_reset`, which clears this log and tells the viewers —
          // the relay never guesses what survived the gap.
          if (room.staleTimer) {
            clearTimeout(room.staleTimer)
            room.staleTimer = null
          }
          if (room.publisher && room.publisher !== conn) {
            room.publisher.sock.close(CLOSE_REPLACED, `replaced`)
          }
          // EXP-795: a page ask addressed to the previous socket is dead.
          this.dropHistoryPages(room)
          room.publisher = conn
          room.lastPublisherActivity = Date.now() // REV2-X: reconnect resets the timer
        }
        return
      }

      case `join`: {
        const sessionId = conn.claims.sessionId
        if (!sessionId) return
        if (conn.claims.role !== `viewer`) return

        let room = this.rooms.get(sessionId)
        if (!room) {
          // EXP-773: the room is not up, but the ticket may name the machine
          // that RAN the session — the transcript lives on that device's
          // disk, never here. Ask it for one; the room opens PENDING and the
          // viewer parks until the replay arrives (or the timer gives up).
          room = this.requestHistory(conn, sessionId)
          if (!room) return // an error frame went out and the socket is closed
        }
        conn.sessionId = sessionId

        room.activityMembers.add(conn)
        // `activity_reset` first: a reconnecting client keeps rendering its
        // old feed until the relay says otherwise, so the replay that follows
        // is always a complete, self-contained picture.
        conn.sock.send(frame({ t: `activity_reset` }))
        this.replayActivity(room, conn)
        if (room.pendingHistory) {
          // EXP-773: a second viewer joining a pending room gets whatever has
          // landed so far and the same "still fetching" marker — never
          // `activity_synced`, which would claim a complete picture.
          conn.sock.send(frame({ t: `history_pending` }))
        } else {
          // EXP-656: unconditional — an empty log still ends with the marker,
          // so a client never waits out its quiet fallback on a fresh room.
          conn.sock.send(activitySyncedFrame(room))
        }
        this.viewerJoins += 1
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

      case `set_config`: {
        const room = this.roomFor(conn)
        if (!room || !room.publisher) return
        // EXP-746: same gating as `input` — membership is only ever granted
        // by the `join` arm, which checks claims.role === 'viewer', so the
        // role test is transitive (regression-locked by the stale
        // public_viewer test).
        if (!room.activityMembers.has(conn)) return
        room.publisher.sock.send(
          frame({ t: `set_config`, id: msg.id, value: msg.value })
        )
        return
      }

      case `set_mode`: {
        const room = this.roomFor(conn)
        if (!room || !room.publisher) return
        // Same gating as `input`.
        if (!room.activityMembers.has(conn)) return
        room.publisher.sock.send(frame({ t: `set_mode`, id: msg.id }))
        return
      }

      case `activity`: {
        // Publisher-only: the desktop's scrubbed event stream.
        const room = this.roomFor(conn)
        if (!room || room.publisher !== conn) return
        const entry = this.entryFor(msg.event, msg.seq)
        if (LATEST_WINS_KINDS.has(msg.event.kind)) {
          room.lastByKind.set(msg.event.kind, entry)
        } else {
          this.appendActivity(room, entry)
        }
        this.fanoutActivity(room, entry.framed)
        return
      }

      // EXP-783: viewer → "give me the page below `beforeSeq`". The room's
      // replay log is a TAIL; the whole run only exists on the device, so the
      // ask goes to the LIVE publisher, which reads its journal file. EXP-795:
      // forwarded under a relay-issued id (viewers number their own asks, so
      // two in one room collide). EXP-796: a LINGERING history room (the
      // device replayed and left) routes the ask down that device's control
      // socket instead, and the chunk comes back naming the session; a room
      // with neither takes no asks. A lingering room whose device has gone
      // offline is answered the way a join finds an offline device:
      // `error device_offline`, then a terminal `bye` — for every viewer,
      // since no page can ever come.
      case `history_page`: {
        const room = this.roomFor(conn)
        if (!room || !room.activityMembers.has(conn)) return
        let target = room.publisher
        let viaDevice = false
        if (!target && room.historyServed && room.historyDevice) {
          target =
            this.devices
              .get(room.historyDevice.userId)
              ?.get(room.historyDevice.deviceId) ?? null
          if (!target) {
            this.historyRoomDeviceLost += 1
            this.closeHistoryRoom(room, `device_offline`)
            return
          }
          viaDevice = true
        }
        if (!target) return
        if (room.historyPages.size >= HISTORY_PAGES_IN_FLIGHT) return
        const relayId = `h${++this.historyPageSeq}`
        const timer = setTimeout(() => {
          if (room.historyPages.delete(relayId)) this.historyPageTimeouts += 1
        }, HISTORY_PAGE_TIMEOUT_MS)
        room.historyPages.set(relayId, {
          viewer: conn,
          requestId: msg.requestId,
          timer,
        })
        target.sock.send(
          frame({
            t: `history_page`,
            sessionId: room.sessionId,
            requestId: relayId,
            beforeSeq: msg.beforeSeq,
            limit: msg.limit,
          })
        )
        this.historyPageRequests += 1
        if (viaDevice) this.historyPagesViaDevice += 1
        return
      }

      // EXP-783: publisher → one page of older transcript, delivered to the
      // ONE viewer that asked for it under the id IT used. Deliberately never
      // appended to `activityLog`: these events are older than everything in
      // it, and the log is the join tail, not the run.
      //
      // EXP-796: a device's CONTROL socket answers too — a lingering history
      // room has no publisher — and then `sessionId` names the room. Only the
      // device the room was opened against may answer it (same owner, same
      // deviceId), and only a room that is actually lingering.
      case `history_chunk`: {
        const room =
          conn.claims.role === `control`
            ? this.historyRoomServedBy(conn, msg.sessionId)
            : this.roomFor(conn)
        if (!room) return
        if (conn.claims.role !== `control` && room.publisher !== conn) return
        const ask = room.historyPages.get(msg.requestId)
        if (!ask) return
        if (msg.done) this.dropHistoryPage(room, msg.requestId)
        if (!room.activityMembers.has(ask.viewer)) return
        ask.viewer.sock.send(
          frame({
            t: `history_chunk`,
            requestId: ask.requestId,
            events: msg.events,
            seqs: msg.seqs,
            done: msg.done,
          })
        )
        return
      }

      case `activity_reset`: {
        // Publisher-only: the desktop is about to re-publish its full history.
        const room = this.roomFor(conn)
        if (!room || room.publisher !== conn) return
        this.clearActivityLog(room)
        this.fanoutActivity(room, frame({ t: `activity_reset` }))
        return
      }

      case `bye`: {
        const room = this.roomFor(conn)
        if (!room || room.publisher !== conn) return
        // EXP-773: a history replay ends with a COMPLETE transcript, so the
        // parked viewers are told the picture is whole before the room goes.
        // Any other outcome is a live session ending and gets none.
        if (msg.outcome === HISTORY_OUTCOME) {
          const synced = activitySyncedFrame(room)
          for (const member of room.activityMembers.keys()) {
            member.sock.send(synced)
          }
          // EXP-796: the room LINGERS without its publisher so the viewers
          // can page older transcript ("Load earlier") through the device's
          // control socket — nobody re-dials. A room a publisher opened
          // itself (no device to page through) still closes as before.
          if (room.historyDevice) {
            this.lingerHistoryRoom(room, conn)
            return
          }
        }
        this.closeRoom(room, msg.outcome ?? `ended`)
        return
      }
    }
  }

  // ── Admin (server-to-server HTTP, secret-authed) ──────────────────────────

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
    const control = this.devices.get(userId)?.get(deviceId)
    if (!control) return { ok: false, reason: `device_offline` }
    // Build each variant explicitly — a raw union spread won't narrow for the
    // ServerFrame `frame()` call. Single-issue key order (t, issueId, options)
    // stays byte-for-byte with the pre-batch frame.
    const payload: ServerFrame =
      `resumeSessionId` in subject
        ? {
            t: `start_session`,
            resumeSessionId: subject.resumeSessionId,
            teamId: subject.teamId,
            ...(subject.issueId ? { issueId: subject.issueId } : {}),
            ...(subject.actionId ? { actionId: subject.actionId } : {}),
            ...(subject.actionName ? { actionName: subject.actionName } : {}),
            ...(subject.branch ? { branch: subject.branch } : {}),
            // A resume carries no launch options — only the attribution and
            // (EXP-679) the agent-started marker.
            ...(options.startedBy ? { startedBy: options.startedBy } : {}),
            ...(options.startedReason
              ? { startedReason: options.startedReason }
              : {}),
          }
        : `issueId` in subject
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
    control.sock.send(frame(payload))
    this.startsRouted += 1
    return { ok: true }
  }

  /** EXP-481: fire-and-forget `check_in` nudge to a device's control socket —
   * "the server persisted new work for you, heartbeat now". Returns whether a
   * live socket received it; the heartbeat pickup is the durable path either
   * way. */
  nudge(userId: string, deviceId: string): boolean {
    const control = this.devices.get(userId)?.get(deviceId)
    if (!control) return false
    control.sock.send(frame({ t: `check_in` }))
    return true
  }

  /** Server-side kill (steer.killSession fallback path). */
  killSession(sessionId: string): boolean {
    const room = this.rooms.get(sessionId)
    if (!room?.publisher) return false
    room.publisher.sock.send(frame({ t: `kill` }))
    return true
  }

  /** EXP-700: server-side text injection — the web app relays a parent/child
   * message into the session's agent as if the owner typed it. Same
   * convention as the browser viewer: text chunked at ≤4096 chars (never
   * splitting a surrogate pair, frames cap at 8 KiB), then a SEPARATE `\r`
   * submit frame — bundled, TUI apps treat the trailing return as a paste. */
  injectInput(sessionId: string, text: string): boolean {
    const room = this.rooms.get(sessionId)
    if (!room?.publisher) return false
    for (let i = 0; i < text.length; ) {
      let end = Math.min(i + INPUT_CHUNK_CHARS, text.length)
      const last = end < text.length ? text.charCodeAt(end - 1) : 0
      if (last >= 0xd800 && last <= 0xdbff) end += 1
      room.publisher.sock.send(frame({ t: `input`, data: text.slice(i, end) }))
      i = end
    }
    room.publisher.sock.send(frame({ t: `input`, data: `\r` }))
    return true
  }

  stats() {
    return {
      connections: this.conns.size,
      devices: [...this.devices.values()].reduce((n, m) => n + m.size, 0),
      rooms: this.rooms.size,
    }
  }

  /** EXP-553: monotonic counters since process start (see the field block). */
  counters() {
    return {
      connectionsAccepted: this.connectionsAccepted,
      activityFramesFanned: this.activityFramesFanned,
      startsRouted: this.startsRouted,
      slowConsumerEvictions: this.slowConsumerEvictions,
      viewerJoins: this.viewerJoins,
      publisherIdleCloses: this.publisherIdleCloses,
      historyRequests: this.historyRequests,
      historyDeviceOffline: this.historyDeviceOffline,
      historyTimeouts: this.historyTimeouts,
      historyPageRequests: this.historyPageRequests,
      historyPageTimeouts: this.historyPageTimeouts,
      historyPagesViaDevice: this.historyPagesViaDevice,
      historyRoomLingerExpiries: this.historyRoomLingerExpiries,
      historyRoomDeviceLost: this.historyRoomDeviceLost,
    }
  }

  // ── Internals ──────────────────────────────────────────────────────────────

  private roomFor(conn: Conn): Room | undefined {
    return conn.sessionId ? this.rooms.get(conn.sessionId) : undefined
  }

  /** EXP-795: retire one `history_page` ask — answered, or given up on. */
  private dropHistoryPage(room: Room, relayId: string) {
    const ask = room.historyPages.get(relayId)
    if (!ask) return
    clearTimeout(ask.timer)
    room.historyPages.delete(relayId)
  }

  /** Retire every in-flight ask (the publisher they went to is gone, or the
   *  room is), or only `viewer`'s (it left). */
  private dropHistoryPages(room: Room, viewer?: Conn) {
    for (const [relayId, ask] of room.historyPages) {
      if (viewer === undefined || ask.viewer === viewer) {
        this.dropHistoryPage(room, relayId)
      }
    }
  }

  /** EXP-796: the device's replay is complete — detach it and keep the room
   *  up for its viewers, for HISTORY_ROOM_LINGER_MS at most. The publisher
   *  socket is the device's short-lived replay connection; it closes itself
   *  right after the `bye`, and `onClose` then finds it is no member. */
  private lingerHistoryRoom(room: Room, publisher: Conn) {
    room.publisher = null
    publisher.sessionId = undefined
    // Asks addressed to the replay socket are dead with it.
    this.dropHistoryPages(room)
    if (room.staleTimer) {
      clearTimeout(room.staleTimer)
      room.staleTimer = null
    }
    room.historyServed = true
    if (room.lingerTimer) clearTimeout(room.lingerTimer)
    room.lingerTimer = setTimeout(() => {
      room.lingerTimer = null
      // A publisher took the room back (a second replay), or it closed and
      // reopened under the same id: this linger is not its linger.
      if (!room.historyServed || room.publisher) return
      if (this.rooms.get(room.sessionId) !== room) return
      this.historyRoomLingerExpiries += 1
      this.closeRoom(room, HISTORY_OUTCOME)
    }, HISTORY_ROOM_LINGER_MS)
    // Nobody stayed for the replay: nothing to linger for.
    if (room.activityMembers.size === 0) this.closeRoom(room, HISTORY_OUTCOME)
  }

  /** EXP-796: end a lingering history room with a TERMINAL answer — `error
   *  code` to every viewer, then the `bye` + close `closeRoom` sends. The
   *  same shape `requestHistory` and the 20s timer use, so every client
   *  lands in Ended instead of reading a plain drop and redialing. */
  private closeHistoryRoom(room: Room, code: string) {
    const err = frame({ t: `error`, code })
    for (const member of room.activityMembers.keys()) {
      member.sock.send(err)
    }
    this.closeRoom(room, code)
  }

  /** EXP-796: the lingering history room a control socket may answer a
   *  `history_chunk` for: `sessionId` names it, it has no publisher, and it
   *  was opened against THIS device under THIS account. */
  private historyRoomServedBy(
    conn: Conn,
    sessionId: string | undefined
  ): Room | undefined {
    if (!sessionId || !conn.deviceId) return undefined
    const room = this.rooms.get(sessionId)
    if (!room || room.publisher || !room.historyServed) return undefined
    const device = room.historyDevice
    if (
      !device ||
      device.userId !== conn.claims.sub ||
      device.deviceId !== conn.deviceId
    ) {
      return undefined
    }
    return room
  }

  /** EXP-796: a device left the presence table. Every lingering history room
   *  opened against it that has a page IN FLIGHT is answered `device_offline`
   *  (terminal — the ask went down the socket that just died); a lingering
   *  room with nothing pending stays up, and its next ask finds the device
   *  missing and answers the same way. */
  private deviceLeftHistoryRooms(userId: string, deviceId: string) {
    for (const room of [...this.rooms.values()]) {
      if (!room.historyServed || room.historyPages.size === 0) continue
      const device = room.historyDevice
      if (!device || device.userId !== userId || device.deviceId !== deviceId) {
        continue
      }
      this.historyRoomDeviceLost += 1
      this.closeHistoryRoom(room, `device_offline`)
    }
  }

  /** Drop the replay log and everything derived from it. The caller decides
   *  whether members hear an `activity_reset` about it. */
  private clearActivityLog(room: Room) {
    room.activityLog = []
    room.activityBytes = 0
    room.subagentToolCounts.clear()
    room.subagentToolEntries = 0
    room.subagentScanFrom = 0
    room.lastByKind.clear()
    room.activityTruncated = false
  }

  /** A fresh room. `publisher: null` + `pendingHistory: true` is the EXP-773
   *  shape: a room opened by a VIEWER, waiting for the device to replay. */
  private newRoom(
    sessionId: string,
    publisher: Conn | null,
    issueId?: string
  ): Room {
    return {
      sessionId,
      issueId,
      publisher,
      staleTimer: null,
      lastPublisherActivity: Date.now(), // REV2-X
      pendingHistory: publisher === null,
      historyTimer: null,
      activityMembers: new Set(),
      activityLog: [],
      activityBytes: 0,
      subagentToolCounts: new Map(),
      subagentToolEntries: 0,
      subagentScanFrom: 0,
      lastByKind: new Map(),
      activityTruncated: false,
      historyPages: new Map(),
      historyDevice: null,
      historyServed: false,
      lingerTimer: null,
    }
  }

  /** EXP-773: a viewer joined a session with no live room. If its ticket
   *  names the device that ran the session AND that device's control socket
   *  is online under the SAME user, open a pending room and ask the device
   *  for its stored transcript. Otherwise answer and close, as before.
   *  Returns the room to park the viewer in, or `undefined` when the socket
   *  has already been answered with an error. */
  private requestHistory(conn: Conn, sessionId: string): Room | undefined {
    const deviceId = conn.claims.deviceId
    if (!deviceId) {
      // No device claim (an older web build, or a session that never named
      // one): nothing can serve the transcript — the pre-EXP-773 answer.
      conn.sock.send(frame({ t: `error`, code: `no_such_session` }))
      conn.sock.close(CLOSE_SESSION_ENDED, `no_such_session`)
      return undefined
    }
    // Devices are indexed under their OWNER, and for a shared-device run
    // (EXP-432) that is the HOST, not the requester the ticket was minted
    // for — so the mint names the account to look the device up under. Both
    // ids come from the signed ticket, so this still reaches only the machine
    // the web app decided ran this session.
    const deviceOwnerId = conn.claims.deviceOwnerId ?? conn.claims.sub
    const control = this.devices.get(deviceOwnerId)?.get(deviceId)
    if (!control) {
      conn.sock.send(frame({ t: `error`, code: `device_offline` }))
      // The `bye` is not decoration: a viewer that sees a close with no `bye`
      // reads it as a transport drop and redials forever (the desktop's
      // `ConnEnd::Dropped`). This is terminal — the transcript is a file on a
      // machine that is not here — so answer it exactly like the timeout path
      // does through `closeRoom`: error, then `bye` naming the outcome, then
      // the close. Every viewer lands in Ended.
      conn.sock.send(frame({ t: `bye`, outcome: `device_offline` }))
      conn.sock.close(CLOSE_SESSION_ENDED, `device_offline`)
      this.historyDeviceOffline += 1
      return undefined
    }
    const room = this.newRoom(sessionId, null)
    // EXP-796: remembered so the room can page through the device once the
    // replay is over and the publisher socket is gone.
    room.historyDevice = { userId: deviceOwnerId, deviceId }
    room.historyTimer = setTimeout(() => {
      room.historyTimer = null
      for (const member of room.activityMembers.keys()) {
        member.sock.send(frame({ t: `error`, code: `history_unavailable` }))
      }
      this.historyTimeouts += 1
      this.closeRoom(room, `history_unavailable`)
    }, HISTORY_TIMEOUT_MS)
    this.rooms.set(sessionId, room)
    control.sock.send(frame({ t: `history_request`, sessionId }))
    this.historyRequests += 1
    return room
  }

  private entryFor(event: ActivityEvent, seq?: number): ActivityEntry {
    const framed = frame({ t: `activity`, event, seq })
    const entry: ActivityEntry = {
      framed,
      bytes: Buffer.byteLength(framed, `utf8`),
      seq,
    }
    // EXP-748: tag the entries the count cap is allowed to sacrifice.
    // EXP-773: prose a subagent wrote renders inside that subagent's card, so
    // it is second class exactly like the calls around it.
    if (
      (event.kind === `tool` || event.kind === `narration`) &&
      event.subagentId
    ) {
      entry.subagentTool = event.subagentId
    }
    return entry
  }

  private appendActivity(room: Room, entry: ActivityEntry) {
    room.activityLog.push(entry)
    room.activityBytes += entry.bytes
    if (entry.subagentTool) {
      const count = (room.subagentToolCounts.get(entry.subagentTool) ?? 0) + 1
      room.subagentToolCounts.set(entry.subagentTool, count)
      room.subagentToolEntries += 1
      // Per-subagent tier: one fan-out never owns more than its share.
      if (count > SUBAGENT_TOOL_CAP) {
        this.evictOldestSubagentTool(room, entry.subagentTool)
      }
    }
    // Count tier: spend subagent tool calls before the main transcript.
    while (room.activityLog.length > ACTIVITY_LOG_CAP) {
      if (!this.evictOldestSubagentTool(room)) this.dropHead(room)
    }
    // Byte tier: a hard bound, so it stays plain oldest-first.
    while (
      room.activityBytes > ACTIVITY_BYTE_CAP &&
      room.activityLog.length > 1
    ) {
      this.dropHead(room)
    }
  }

  /** Evict the oldest subagent tool entry — of `subagentId` when given, of
   *  any subagent otherwise. Returns false when there is none left to give
   *  up, which is the count cap's signal to fall back to the head. */
  private evictOldestSubagentTool(room: Room, subagentId?: string): boolean {
    if (room.subagentToolEntries === 0) return false
    const log = room.activityLog
    let i = Math.min(room.subagentScanFrom, log.length)
    for (; i < log.length; i++) {
      const tag = log[i]!.subagentTool
      if (tag && (subagentId === undefined || tag === subagentId)) break
    }
    if (i >= log.length) return false
    const [entry] = log.splice(i, 1)
    room.activityBytes -= entry!.bytes
    room.activityTruncated = true
    this.forgetSubagentTool(room, entry!.subagentTool!)
    // Only the un-filtered scan proves nothing older survives below `i`; a
    // per-subagent hit may sit past an older entry of another subagent.
    if (subagentId === undefined) room.subagentScanFrom = i
    return true
  }

  /** Drop the oldest event outright: the byte budget's hard bound, and the
   *  count cap once no subagent tool call is left to sacrifice. */
  private dropHead(room: Room) {
    const entry = room.activityLog.shift()
    if (!entry) return
    room.activityBytes -= entry.bytes
    room.activityTruncated = true
    if (entry.subagentTool) this.forgetSubagentTool(room, entry.subagentTool)
    if (room.subagentScanFrom > 0) room.subagentScanFrom -= 1
  }

  private forgetSubagentTool(room: Room, subagentId: string) {
    const left = (room.subagentToolCounts.get(subagentId) ?? 1) - 1
    if (left > 0) room.subagentToolCounts.set(subagentId, left)
    else room.subagentToolCounts.delete(subagentId)
    room.subagentToolEntries -= 1
  }

  /** Fan one pre-serialized text frame to the activity audience. Activity is
   *  low-volume JSON, so a saturated socket is evicted outright — there is no
   *  drop-and-recover path. */
  private fanoutActivity(room: Room, framed: string) {
    for (const member of room.activityMembers.keys()) {
      if (member.sock.bufferedAmount() > VIEWER_HIGH_WATER) {
        member.sock.close(CLOSE_SLOW_CONSUMER, `slow_consumer`)
        this.slowConsumerEvictions += 1
        continue
      }
      member.sock.send(framed)
      this.activityFramesFanned += 1
    }
  }

  /** Replay the scrubbed event log, then the latest-wins state slots, to one
   *  socket. EXP-746: the slots come last and in a fixed order so a joining
   *  viewer paints its chips, meter and diff from the end of the burst. */
  private replayActivity(room: Room, conn: Conn) {
    for (const entry of room.activityLog) {
      conn.sock.send(entry.framed)
    }
    for (const kind of LATEST_REPLAY_ORDER) {
      const entry = room.lastByKind.get(kind)
      if (entry) conn.sock.send(entry.framed)
    }
  }

  private closeRoom(room: Room, outcome: string) {
    if (room.staleTimer) clearTimeout(room.staleTimer)
    if (room.historyTimer) clearTimeout(room.historyTimer)
    if (room.lingerTimer) clearTimeout(room.lingerTimer)
    this.dropHistoryPages(room)
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
        this.publisherIdleCloses += 1
        room.publisher.sock.close(
          CLOSE_PUBLISHER_IDLE,
          `publisher_idle_${Math.floor(idleMs / 1000)}s`
        )
      }
    }
  }

  /** EXP-648: one `keepalive` frame to every JOINED viewer. Only
   *  activityMembers, never publishers or control sockets — and membership
   *  is granted synchronously before the join's `activity_reset` + replay go
   *  out, so a keepalive can never be a socket's first frame (every client
   *  disarms its join-ack deadline on the first frame, whatever it is).
   *  Rooms whose publisher dropped (grace window) still get one: the frame
   *  vouches for the SOCKET, and a redial there would replay the same log.
   *  A saturated socket is skipped, not evicted — eviction is the activity
   *  fan-out's call, and none of the activity counters move here. */
  private sendViewerKeepalives() {
    for (const room of this.rooms.values()) {
      for (const member of room.activityMembers.keys()) {
        if (member.sock.bufferedAmount() > VIEWER_HIGH_WATER) continue
        member.sock.send(KEEPALIVE_FRAME)
      }
    }
  }
}
