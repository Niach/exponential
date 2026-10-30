// The relay hub: device presence, the control routes down a device's control
// socket (check_in, kill, interrupt, compact_request) and (NEW-55) voice call
// rooms, all in memory (masterplan §3.1/§3.4). No DB, no persistence: if the
// relay restarts, sockets reconnect and re-announce; nothing durable is lost
// because nothing durable existed.
//
// VAPP-43: the hub carries NO session content. Session rooms, the activity
// log + replay, viewers, keepalives and the stored-transcript machinery are
// gone: every viewer reads a session over eNet from the device that runs it
// (fixtures/steer-enet.json). A content frame (`CONTENT_FRAME_TYPES`) from any
// socket is answered `error content_refused` and dropped, never forwarded and
// never logged beyond its type.

import type { SteerTicketClaims } from "@exp/steer-ticket"
import {
  CALL_FRAME_HEADER_BYTES,
  CALL_KIND_DOWN,
  CALL_KIND_UP,
  CALL_IDLE_TIMEOUT_MS,
  CALL_MAX_FRAME_BYTES,
  CALL_MAX_FRAMES_PER_SECOND,
  CALL_MAX_MS,
  CALL_RING_TIMEOUT_MS,
  CLOSE_REPLACED,
  CLOSE_SESSION_ENDED,
  describeClientFrameRejection,
  isContentFrameType,
  parseClientFrame,
  type ClientFrame,
  type CompactRefusal,
  type ServerFrame,
} from "./protocol"

// Abstracted so the hub is unit-testable with fake sockets; the Bun layer
// adapts ServerWebSocket to this. Text-only except NEW-55's voice calls,
// whose Opus frames go out through `sendBinary` and only ever to a call
// room's peer.
export interface RelaySocket {
  send(data: string): void
  /** NEW-55: one binary WebSocket frame (a call's audio). */
  sendBinary(data: Uint8Array): void
  close(code?: number, reason?: string): void
  /** Bytes queued on the socket, for call backpressure. */
  bufferedAmount(): number
}

interface Conn {
  sock: RelaySocket
  claims: SteerTicketClaims
  // control sockets announce a deviceId via `online`.
  deviceId?: string
  /** NEW-55: caller/call_host sockets belong to a call room once it opened
   *  (caller) or they were admitted to it (host). Cleared when the room ends,
   *  so the socket's own close is inert afterwards. */
  callId?: string
  /** NEW-55 review: this socket's binary-frame token bucket (tokens left,
   *  and when it was last refilled). */
  frameTokens?: number
  frameTokensAt?: number
  /** Last time this socket's rejected-frame warning was logged (rate limit). */
  lastFrameWarnAt?: number
}

/** NEW-55: one voice call — a `caller` streaming to the member's voice
 *  device, which answers with a `call_host` socket. Exactly two sockets,
 *  nothing replayed, nothing logged. */
interface CallRoom {
  callId: string
  /** The voice device's owner (the caller's `deviceOwnerId ?? sub`): the
   *  only account whose `call_host` ticket may answer. */
  ownerId: string
  deviceId: string
  caller: Conn
  host: Conn | null
  /** Fires CALL_RING_TIMEOUT_MS after the ring when no host joined. */
  ringTimer: ReturnType<typeof setTimeout> | null
  /** NEW-55 review: once paired, ends the call at CALL_MAX_MS. */
  lengthTimer: ReturnType<typeof setTimeout> | null
  /** NEW-55 review: once paired, checks for CALL_IDLE_TIMEOUT_MS without a
   *  forwarded frame. */
  idleTimer: ReturnType<typeof setInterval> | null
  /** The last forwarded binary frame (or the pairing). */
  lastFrameAt: number
}

/** NEW-55: a peer with more than this queued SKIPS audio frames instead of
 *  buffering them — a call is realtime, a late frame is worthless, and the
 *  queue must not grow. ~2 s of the 32 kbps downlink. */
const CALL_HIGH_WATER = 8 * 1024

function callDeviceKey(ownerId: string, deviceId: string): string {
  return `${ownerId}\n${deviceId}`
}

/** The device a control route names: the account it is registered under
 *  (the session's host) and its steer deviceId. */
export interface DeviceRef {
  userId: string
  deviceId: string
}

/** EXP-936: what `POST /sessions/:id/compact` gets back. `delivered: false`
 *  = the device's control socket is not connected (`device_offline`), it
 *  never answered (`no_verdict`: a host older than the frame), or another ask
 *  for the session still waits (`busy`); the web server turns that into a
 *  tool error, never into a verdict. */
export type CompactOutcome =
  | { delivered: false; reason: `device_offline` | `no_verdict` | `busy` }
  | { delivered: true; accepted: boolean; refusedBecause?: CompactRefusal }

/** One in-flight compaction ask: the control socket it went down, and the
 *  device (owner + deviceId) whose verdict alone settles it. */
interface CompactAsk {
  control: Conn
  ownerId: string
  deviceId: string
  resolve: (outcome: CompactOutcome) => void
  timer: ReturnType<typeof setTimeout>
}

// A frame the schema rejects is dropped on the floor; without a log line a
// client version skew is invisible from the relay side. One warning per
// SOCKET per window keeps a chatty or hostile client from filling the log.
const FRAME_WARN_INTERVAL_MS = 30_000
/** EXP-936: how long a `compact_request` may wait for the host's verdict.
 *  The device answers off in-memory state the moment the frame lands, so an
 *  ask still open after this is a host that does not speak the frame; the
 *  web server's own fetch timeout sits above this, so the relay always
 *  answers first with a structured `no_verdict`. */
export const COMPACT_VERDICT_TIMEOUT_MS = 4_000

function frame(msg: ServerFrame): string {
  return JSON.stringify(msg)
}

/** Serialized once: the answer to every refused content frame. */
const CONTENT_REFUSED_FRAME = frame({ t: `error`, code: `content_refused` })

export class Hub {
  private conns = new Map<RelaySocket, Conn>()
  /** userId → deviceId → control connection: the routing table nudge, kill,
   *  interrupt, compact and call rings send down, and nothing else. EXP-672
   *  dropped the outbound presence listing — the web server reads the
   *  persisted `devices` row for everything it shows or gates on. */
  private devices = new Map<string, Map<string, Conn>>()
  /** EXP-936: sessionId → its ONE in-flight compaction ask. */
  private compactAsks = new Map<string, CompactAsk>()
  /** NEW-55: callId → call room, and the voice device (owner + deviceId) →
   *  its ONE running callId (a second caller reads `busy`). */
  private calls = new Map<string, CallRoom>()
  private callsByDevice = new Map<string, string>()

  // EXP-553: monotonic counters for the admin performance page — exposed via
  // counters() on the secret-gated /stats endpoint (stats() stays gauges-only
  // because /healthz is public).
  private connectionsAccepted = 0
  /** VAPP-43: session content frames refused (any role). */
  private contentFramesRefused = 0
  /** EXP-936: `compact_request`s sent down a control socket, and how many
   *  the host never answered (a host older than the frame). */
  private compactRequests = 0
  private compactVerdictTimeouts = 0
  /** NEW-55: calls rung, the three ways a ring fails, and the audio frames
   *  forwarded / dropped (wrong kind, oversized, not paired) / skipped (a
   *  saturated peer). */
  private callRings = 0
  private callDeviceOffline = 0
  private callBusy = 0
  private callNoAnswer = 0
  private callsExpired = 0
  private callFramesForwarded = 0
  private callFramesDropped = 0
  private callFramesSkipped = 0

  /** Stop every timer (for clean shutdown/tests). */
  destroy() {
    // NEW-55: every call's ring, length and idle timers die with the hub.
    for (const call of [...this.calls.values()]) {
      this.removeCall(call)
    }
    for (const sessionId of [...this.compactAsks.keys()]) {
      this.settleCompactAsk(sessionId, { delivered: false, reason: `device_offline` })
    }
  }

  // ── Socket lifecycle (called from the Bun ws handlers) ────────────────────

  onOpen(sock: RelaySocket, claims: SteerTicketClaims) {
    const conn: Conn = { sock, claims }
    this.conns.set(sock, conn)
    this.connectionsAccepted += 1

    if (claims.role === `caller`) {
      // NEW-55: a caller rings on connect — its ticket is the whole ask.
      this.ringCall(conn)
    } else if (claims.role === `call_host`) {
      this.answerCall(conn)
    }
  }

  onMessage(sock: RelaySocket, data: string) {
    const conn = this.conns.get(sock)
    if (!conn) return

    // VAPP-43: session content never crosses the relay. Checked on the frame
    // TYPE alone, before any schema: a content frame is refused whatever it
    // carries, and its content is never parsed further nor logged.
    const t = frameType(data)
    if (t !== null && isContentFrameType(t)) {
      this.refuseContent(conn, t)
      return
    }

    const msg = parseClientFrame(data)
    if (!msg) {
      this.warnRejectedFrame(conn, data)
      return
    }
    this.onControl(conn, msg)
  }

  /** NEW-55: one BINARY frame (Bun hands binary messages over as a Buffer).
   *  Only a paired call room carries them: a caller's `up` frame goes to the
   *  host, a host's `down` frame to the caller, each at most
   *  CALL_MAX_FRAME_BYTES. Everything else — any other role, a wrong kind, an
   *  oversized or empty frame — is dropped. Never decoded, never stored. */
  onBinary(sock: RelaySocket, data: Uint8Array) {
    const conn = this.conns.get(sock)
    if (!conn) return
    const call = this.callFor(conn)
    if (!call) return this.dropBinary(conn, `no_call`, data.length)
    const up = call.caller === conn
    const peer = up ? call.host : call.caller
    // A caller may start capturing while it rings: expected, not worth a log.
    if (!peer) {
      this.callFramesDropped += 1
      return
    }
    if (data.length <= CALL_FRAME_HEADER_BYTES) {
      return this.dropBinary(conn, `short`, data.length)
    }
    if (data.length > CALL_MAX_FRAME_BYTES) {
      return this.dropBinary(conn, `oversized`, data.length)
    }
    if (data[0] !== (up ? CALL_KIND_UP : CALL_KIND_DOWN)) {
      return this.dropBinary(conn, `wrong_kind`, data.length)
    }
    if (!this.takeFrameToken(conn)) {
      return this.dropBinary(conn, `rate`, data.length)
    }
    call.lastFrameAt = Date.now()
    // Realtime: a saturated peer skips the frame rather than queueing it.
    if (peer.sock.bufferedAmount() > CALL_HIGH_WATER) {
      this.callFramesSkipped += 1
      return
    }
    peer.sock.sendBinary(data)
    this.callFramesForwarded += 1
  }

  /** NEW-55 review: CALL_MAX_FRAMES_PER_SECOND per socket, a bucket that
   *  holds one second's worth. */
  private takeFrameToken(conn: Conn): boolean {
    const now = Date.now()
    const elapsed = (now - (conn.frameTokensAt ?? now)) / 1000
    const tokens = Math.min(
      CALL_MAX_FRAMES_PER_SECOND,
      (conn.frameTokens ?? CALL_MAX_FRAMES_PER_SECOND) + elapsed * CALL_MAX_FRAMES_PER_SECOND
    )
    conn.frameTokensAt = now
    if (tokens < 1) {
      conn.frameTokens = tokens
      return false
    }
    conn.frameTokens = tokens - 1
    return true
  }

  private dropBinary(conn: Conn, why: string, bytes: number) {
    this.callFramesDropped += 1
    if (!this.mayWarn(conn)) return
    console.warn(
      `[hub] dropped ${conn.claims.role} binary frame (${bytes} bytes): ${why}`
    )
  }

  /** The per-socket rate limit every dropped-frame warning shares. */
  private mayWarn(conn: Conn): boolean {
    const now = Date.now()
    if (
      conn.lastFrameWarnAt !== undefined &&
      now - conn.lastFrameWarnAt < FRAME_WARN_INTERVAL_MS
    ) {
      return false
    }
    conn.lastFrameWarnAt = now
    return true
  }

  /** One rate-limited line for a frame the schema rejected: the socket's role,
   *  the frame's `t`, and the first zod issue's path — structure only, never
   *  frame CONTENT. */
  private warnRejectedFrame(conn: Conn, raw: string) {
    if (!this.mayWarn(conn)) return
    const { t, issue } = describeClientFrameRejection(raw)
    console.warn(
      `[hub] dropped ${conn.claims.role} frame (t=${t ?? `?`}): ${issue}`
    )
  }

  /** VAPP-43: answer a session content frame with `content_refused` on the
   *  socket that sent it and drop it. The socket stays open (a control
   *  socket's presence must survive a stray frame from an old build); the
   *  log names the role and the type, never a byte of the frame. */
  private refuseContent(conn: Conn, t: string) {
    this.contentFramesRefused += 1
    conn.sock.send(CONTENT_REFUSED_FRAME)
    if (!this.mayWarn(conn)) return
    console.warn(`[hub] refused ${conn.claims.role} content frame (t=${t})`)
  }

  onClose(sock: RelaySocket) {
    const conn = this.conns.get(sock)
    if (!conn) return
    this.conns.delete(sock)

    // NEW-55: either side of a call going away ends it for the other.
    const call = this.callFor(conn)
    if (call) {
      this.endCall(call, conn, `dropped`, false)
      return
    }

    // A compaction ask that went down THIS socket has nobody left to answer.
    for (const [sessionId, ask] of [...this.compactAsks]) {
      if (ask.control === conn) {
        this.settleCompactAsk(sessionId, { delivered: false, reason: `device_offline` })
      }
    }

    // Device presence eviction.
    if (conn.deviceId) {
      const byDevice = this.devices.get(conn.claims.sub)
      if (byDevice?.get(conn.deviceId) === conn) {
        byDevice.delete(conn.deviceId)
        if (byDevice.size === 0) this.devices.delete(conn.claims.sub)
      }
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
        // announced id, so without this every extra `online` frame leaked an
        // entry forever (unbounded heap for a hostile control client, and
        // ghost devices whose nudge sends into a dead socket).
        // One control socket owns at most one presence entry.
        if (conn.deviceId && conn.deviceId !== msg.deviceId) {
          if (byDevice?.get(conn.deviceId) === conn) {
            byDevice.delete(conn.deviceId)
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

      // EXP-936: the host's answer to the session's ONE pending
      // `compact_request`, on the CONTROL socket of the device it was sent
      // to (same owner + deviceId); any other socket's verdict is ignored.
      // Never forwarded: the verdict belongs to the web server's awaiting
      // HTTP call alone. A verdict with no ask open is dropped.
      case `compact_verdict`: {
        if (conn.claims.role !== `control` || !conn.deviceId) return
        const ask = this.compactAsks.get(msg.sessionId)
        if (!ask) return
        if (ask.ownerId !== conn.claims.sub || ask.deviceId !== conn.deviceId) return
        this.settleCompactAsk(
          msg.sessionId,
          msg.accepted
            ? { delivered: true, accepted: true }
            : {
                delivered: true,
                accepted: false,
                refusedBecause: msg.refusedBecause ?? `cooldown`,
              }
        )
        return
      }

      // NEW-55: the caller's microphone toggled — to the host, once paired.
      case `call_mute`: {
        const call = this.callFor(conn)
        if (!call?.host || call.caller !== conn) return
        call.host.sock.send(frame({ t: `call_mute`, muted: msg.muted }))
        return
      }

      // NEW-55: one flow event from the host — to the caller.
      case `call_event`: {
        const call = this.callFor(conn)
        if (!call || call.host !== conn) return
        call.caller.sock.send(frame({ t: `call_event`, event: msg.event }))
        return
      }

      // NEW-55: either side hangs up (a ringing caller included).
      case `call_end`: {
        const call = this.callFor(conn)
        if (!call) return
        this.endCall(call, conn, msg.reason, true)
        return
      }
    }
  }

  // ── Admin (server-to-server HTTP, secret-authed) ──────────────────────────

  /** The control socket of a device, or undefined when it is not connected. */
  private controlFor(device: DeviceRef): Conn | undefined {
    return this.devices.get(device.userId)?.get(device.deviceId)
  }

  /** EXP-481: fire-and-forget check-in nudge to a device's control socket.
   * Returns whether a live socket received it; the heartbeat pickup is the
   * durable path either way. */
  nudge(userId: string, deviceId: string): boolean {
    const control = this.controlFor({ userId, deviceId })
    if (!control) return false
    control.sock.send(frame({ t: `check_in` }))
    return true
  }

  /** Server-side kill (every end the server makes): `kill {sessionId}` down
   *  the control socket of the device running the session. The DB row flip
   *  the device watches is the durable path; this only cuts its latency. */
  killSession(sessionId: string, device: DeviceRef): boolean {
    const control = this.controlFor(device)
    if (!control) return false
    control.sock.send(frame({ t: `kill`, sessionId }))
    return true
  }

  /** EXP-1291: server-side Stop (`sessions.stop`): interrupt the running
   *  turn on the device; the session stays live. */
  interruptSession(sessionId: string, device: DeviceRef): boolean {
    const control = this.controlFor(device)
    if (!control) return false
    control.sock.send(frame({ t: `interrupt`, sessionId }))
    return true
  }

  /** EXP-936: relay the run's own `exponential_sessions_compact` down its
   *  device's control socket and wait for the host's verdict. The host
   *  decides — it holds the context meter, the turn count since the last
   *  compaction and the open-fold state — and the relay only carries the
   *  answer back. One ask per session at a time: a second one while the
   *  first waits reads `busy` rather than queueing behind a verdict that
   *  will answer it too. */
  requestCompaction(
    sessionId: string,
    device: DeviceRef,
    keep?: string
  ): Promise<CompactOutcome> {
    const control = this.controlFor(device)
    if (!control) {
      return Promise.resolve({ delivered: false, reason: `device_offline` })
    }
    if (this.compactAsks.has(sessionId)) {
      return Promise.resolve({ delivered: false, reason: `busy` })
    }
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.compactVerdictTimeouts += 1
        this.settleCompactAsk(sessionId, { delivered: false, reason: `no_verdict` })
      }, COMPACT_VERDICT_TIMEOUT_MS)
      this.compactAsks.set(sessionId, {
        control,
        ownerId: device.userId,
        deviceId: device.deviceId,
        resolve,
        timer,
      })
      this.compactRequests += 1
      control.sock.send(
        frame({
          t: `compact_request`,
          sessionId,
          ...(keep === undefined ? {} : { keep }),
        })
      )
    })
  }

  /** EXP-936: answer (or give up on) a session's pending compaction ask. A
   *  no-op when none is open, so every exit path may call it. */
  private settleCompactAsk(sessionId: string, outcome: CompactOutcome) {
    const ask = this.compactAsks.get(sessionId)
    if (!ask) return
    clearTimeout(ask.timer)
    this.compactAsks.delete(sessionId)
    ask.resolve(outcome)
  }

  stats() {
    return {
      connections: this.conns.size,
      devices: [...this.devices.values()].reduce((n, m) => n + m.size, 0),
      calls: this.calls.size,
    }
  }

  /** EXP-553: monotonic counters since process start (see the field block). */
  counters() {
    return {
      connectionsAccepted: this.connectionsAccepted,
      contentFramesRefused: this.contentFramesRefused,
      compactRequests: this.compactRequests,
      compactVerdictTimeouts: this.compactVerdictTimeouts,
      callRings: this.callRings,
      callDeviceOffline: this.callDeviceOffline,
      callBusy: this.callBusy,
      callNoAnswer: this.callNoAnswer,
      callsExpired: this.callsExpired,
      callFramesForwarded: this.callFramesForwarded,
      callFramesDropped: this.callFramesDropped,
      callFramesSkipped: this.callFramesSkipped,
    }
  }

  // ── NEW-55: voice calls ────────────────────────────────────────────────────

  /** The call room this socket is a MEMBER of (a refused socket never is). */
  private callFor(conn: Conn): CallRoom | undefined {
    if (!conn.callId) return undefined
    const call = this.calls.get(conn.callId)
    if (!call || (call.caller !== conn && call.host !== conn)) return undefined
    return call
  }

  /** A terminal answer on a call socket: error, `bye` with the same outcome,
   *  close — the shape EXP-773's history refusals use, so a client never
   *  mistakes it for a drop. */
  private refuseCall(conn: Conn, code: string) {
    conn.sock.send(frame({ t: `error`, code }))
    conn.sock.send(frame({ t: `bye`, outcome: code }))
    conn.sock.close(CLOSE_SESSION_ENDED, code)
  }

  /** A caller connected: ring the voice device its ticket names, on that
   *  device's CONTROL socket under its OWNER (`deviceOwnerId ?? sub`, the
   *  EXP-773 lookup) — both ids come from the signed ticket, so a caller can
   *  only ever ring the device the web app decided is the member's. */
  private ringCall(conn: Conn) {
    const { callId, deviceId, agentId } = conn.claims
    if (!callId || !deviceId || !agentId) {
      this.refuseCall(conn, `no_such_call`)
      return
    }
    const ownerId = conn.claims.deviceOwnerId ?? conn.claims.sub
    const control = this.devices.get(ownerId)?.get(deviceId)
    if (!control) {
      this.callDeviceOffline += 1
      this.refuseCall(conn, `device_offline`)
      return
    }
    const key = callDeviceKey(ownerId, deviceId)
    // One call per voice device; a replayed ticket (same callId) reads the
    // same way.
    if (this.callsByDevice.has(key) || this.calls.has(callId)) {
      this.callBusy += 1
      this.refuseCall(conn, `busy`)
      return
    }
    const call: CallRoom = {
      callId,
      ownerId,
      deviceId,
      caller: conn,
      host: null,
      ringTimer: null,
      lengthTimer: null,
      idleTimer: null,
      lastFrameAt: 0,
    }
    call.ringTimer = setTimeout(() => {
      call.ringTimer = null
      if (this.calls.get(callId) !== call) return
      this.callNoAnswer += 1
      this.removeCall(call)
      conn.callId = undefined
      this.refuseCall(conn, `no_answer`)
    }, CALL_RING_TIMEOUT_MS)
    conn.callId = callId
    this.calls.set(callId, call)
    this.callsByDevice.set(key, callId)
    control.sock.send(frame({ t: `call_ring`, callId, agentId }))
    this.callRings += 1
  }

  /** The voice device answers: admitted only into a RINGING room, and only
   *  when the ticket's account IS the device's owner. */
  private answerCall(conn: Conn) {
    const callId = conn.claims.callId
    const call = callId ? this.calls.get(callId) : undefined
    if (
      !call ||
      call.host ||
      call.ownerId !== conn.claims.sub ||
      conn.claims.deviceId !== call.deviceId
    ) {
      this.refuseCall(conn, `no_such_call`)
      return
    }
    if (call.ringTimer) {
      clearTimeout(call.ringTimer)
      call.ringTimer = null
    }
    call.host = conn
    conn.callId = call.callId
    // NEW-55 review: a paired call ends at CALL_MAX_MS, or after
    // CALL_IDLE_TIMEOUT_MS without a frame either way.
    call.lastFrameAt = Date.now()
    call.lengthTimer = setTimeout(() => {
      call.lengthTimer = null
      this.expireCall(call, `max_length`)
    }, CALL_MAX_MS)
    call.idleTimer = setInterval(
      () => {
        if (Date.now() - call.lastFrameAt >= CALL_IDLE_TIMEOUT_MS) this.expireCall(call, `idle`)
      },
      Math.min(CALL_IDLE_TIMEOUT_MS, 5_000)
    )
    const ready = frame({ t: `call_ready` })
    call.caller.sock.send(ready)
    conn.sock.send(ready)
  }

  /** Hang up: the OTHER side gets `call_end` (the sender's reason, or
   *  `dropped` when its socket simply went away) and both sockets close; the
   *  room and its timer are gone. `closeFrom` = false when `from` is the
   *  socket already closing. */
  private endCall(
    call: CallRoom,
    from: Conn,
    reason: string | undefined,
    closeFrom: boolean
  ) {
    this.removeCall(call)
    const end = frame({
      t: `call_end`,
      ...(reason === undefined ? {} : { reason }),
    })
    for (const member of [call.caller, call.host]) {
      if (!member) continue
      member.callId = undefined
      if (member === from) {
        if (closeFrom) member.sock.close(CLOSE_SESSION_ENDED, `call_end`)
        continue
      }
      member.sock.send(end)
      member.sock.close(CLOSE_SESSION_ENDED, `call_end`)
    }
  }

  /** NEW-55 review: the relay itself ends a paired call (too long, idle):
   *  both sides get `call_end` with the reason and close. */
  private expireCall(call: CallRoom, reason: string) {
    if (this.calls.get(call.callId) !== call) return
    this.callsExpired += 1
    this.removeCall(call)
    const end = frame({ t: `call_end`, reason })
    for (const member of [call.caller, call.host]) {
      if (!member) continue
      member.callId = undefined
      member.sock.send(end)
      member.sock.close(CLOSE_SESSION_ENDED, `call_end`)
    }
  }

  private removeCall(call: CallRoom) {
    if (call.ringTimer) {
      clearTimeout(call.ringTimer)
      call.ringTimer = null
    }
    if (call.lengthTimer) {
      clearTimeout(call.lengthTimer)
      call.lengthTimer = null
    }
    if (call.idleTimer) {
      clearInterval(call.idleTimer)
      call.idleTimer = null
    }
    if (this.calls.get(call.callId) === call) this.calls.delete(call.callId)
    const key = callDeviceKey(call.ownerId, call.deviceId)
    if (this.callsByDevice.get(key) === call.callId) {
      this.callsByDevice.delete(key)
    }
  }
}

/** A text frame's `t`, when it is JSON with a string `t`; null otherwise. */
function frameType(raw: string): string | null {
  try {
    const t = (JSON.parse(raw) as { t?: unknown } | null)?.t
    return typeof t === `string` ? t : null
  } catch {
    return null
  }
}
