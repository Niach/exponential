// The relay hub: device presence + session rooms, all in memory (masterplan
// §3.1/§3.4). No DB, no persistence — if the relay restarts, sockets reconnect
// and re-announce; nothing durable is lost because nothing durable existed.

import type { SteerTicketClaims } from "@exp/steer-ticket"
import {
  CLOSE_REPLACED,
  CLOSE_SESSION_ENDED,
  CLOSE_SLOW_CONSUMER,
  OUTPUT_OPCODE,
  parseClientFrame,
  type ClientFrame,
  type PresenceViewer,
  type ServerFrame,
} from "./protocol"

// Abstracted so the hub is unit-testable with fake sockets; the Bun layer
// adapts ServerWebSocket to this.
export interface RelaySocket {
  send(data: string | Uint8Array): void
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
  // viewer lag bookkeeping (see forwardOutput).
  laggingSince?: number
}

interface DeviceEntry {
  conn: Conn
  deviceLabel: string
  connectedAt: number
}

interface Room {
  sessionId: string
  issueId?: string
  publisher: Conn | null
  cols: number
  rows: number
  viewers: Map<Conn, PresenceViewer>
  /** userId of the single steer-claim holder (relay memory only). */
  steerer: Conn | null
  ring: RingBuffer
  /** Publisher dropped without `bye`; room closes when the grace expires. */
  staleTimer: ReturnType<typeof setTimeout> | null
}

const RING_CAP_BYTES = 256 * 1024
// A viewer with more than this queued gets output frames dropped (control
// frames still flow); saturated past the timeout it is evicted.
const VIEWER_HIGH_WATER = 512 * 1024
const VIEWER_LAG_EVICT_MS = 10_000
const PUBLISHER_GRACE_MS = 60_000

export class RingBuffer {
  private chunks: Uint8Array[] = []
  private total = 0

  constructor(private readonly cap = RING_CAP_BYTES) {}

  push(chunk: Uint8Array) {
    this.chunks.push(chunk)
    this.total += chunk.byteLength
    while (this.total > this.cap && this.chunks.length > 1) {
      const evicted = this.chunks.shift()!
      this.total -= evicted.byteLength
    }
  }

  replay(): Uint8Array[] {
    return [...this.chunks]
  }

  get bytes() {
    return this.total
  }
}

function frame(msg: ServerFrame): string {
  return JSON.stringify(msg)
}

function outputFrame(payload: Uint8Array): Uint8Array {
  const buf = new Uint8Array(payload.byteLength + 1)
  buf[0] = OUTPUT_OPCODE
  buf.set(payload, 1)
  return buf
}

export class Hub {
  private conns = new Map<RelaySocket, Conn>()
  /** userId → deviceId → control connection (drives the phone's device picker). */
  private devices = new Map<string, Map<string, DeviceEntry>>()
  /** sessionId (== coding_sessions.id) → room. */
  private rooms = new Map<string, Room>()

  // ── Socket lifecycle (called from the Bun ws handlers) ────────────────────

  onOpen(sock: RelaySocket, claims: SteerTicketClaims) {
    const conn: Conn = { sock, claims }
    this.conns.set(sock, conn)

    if (claims.role === `publisher` && claims.sessionId) {
      // Room attaches on `hello` (which carries geometry) — nothing yet.
    } else if (claims.role === `viewer` && claims.sessionId) {
      // Viewers join on `join` — nothing yet.
    }
  }

  onMessage(sock: RelaySocket, data: string | Uint8Array) {
    const conn = this.conns.get(sock)
    if (!conn) return

    if (typeof data !== `string`) {
      // Binary = terminal output; only the room's publisher may produce it.
      this.onOutput(conn, data)
      return
    }
    const msg = parseClientFrame(data)
    if (!msg) return
    this.onControl(conn, msg)
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
    } else if (room.viewers.has(conn)) {
      room.viewers.delete(conn)
      if (room.steerer === conn) room.steerer = null
      this.broadcastPresence(room)
    }
  }

  // ── Control frames ─────────────────────────────────────────────────────────

  private onControl(conn: Conn, msg: ClientFrame) {
    switch (msg.t) {
      case `online`: {
        if (conn.claims.role !== `control`) return
        conn.deviceId = msg.deviceId
        let byDevice = this.devices.get(conn.claims.sub)
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
            cols: msg.cols ?? 80,
            rows: msg.rows ?? 24,
            viewers: new Map(),
            steerer: null,
            ring: new RingBuffer(),
            staleTimer: null,
          }
          this.rooms.set(sessionId, room)
        } else {
          // Re-hello after a drop: resume the same room.
          if (room.staleTimer) {
            clearTimeout(room.staleTimer)
            room.staleTimer = null
          }
          if (room.publisher && room.publisher !== conn) {
            room.publisher.sock.close(CLOSE_REPLACED, `replaced`)
          }
          room.publisher = conn
          if (msg.cols) room.cols = msg.cols
          if (msg.rows) room.rows = msg.rows
        }
        this.broadcastPresence(room)
        return
      }

      case `join`: {
        if (conn.claims.role !== `viewer`) return
        const sessionId = conn.claims.sessionId
        if (!sessionId) return
        const room = this.rooms.get(sessionId)
        if (!room) {
          conn.sock.send(
            frame({ t: `error`, code: `no_such_session` })
          )
          conn.sock.close(CLOSE_SESSION_ENDED, `no_such_session`)
          return
        }
        conn.sessionId = sessionId
        room.viewers.set(conn, {
          userId: conn.claims.sub,
          name: conn.claims.name ?? conn.claims.sub,
          perm: conn.claims.perm,
        })
        // Current geometry, then scrollback replay, then live tail.
        conn.sock.send(frame({ t: `resize`, cols: room.cols, rows: room.rows }))
        for (const chunk of room.ring.replay()) {
          conn.sock.send(outputFrame(chunk))
        }
        this.broadcastPresence(room)
        return
      }

      case `resize`: {
        const room = this.roomFor(conn)
        if (!room || room.publisher !== conn) return
        room.cols = msg.cols
        room.rows = msg.rows
        for (const viewer of room.viewers.keys()) {
          viewer.sock.send(frame({ t: `resize`, cols: msg.cols, rows: msg.rows }))
        }
        return
      }

      case `input`: {
        const room = this.roomFor(conn)
        if (!room || !room.publisher) return
        // Single-steerer rule: only the claim holder's keystrokes flow.
        if (room.steerer !== conn) return
        room.publisher.sock.send(frame({ t: `input`, data: msg.data }))
        return
      }

      case `claim`: {
        const room = this.roomFor(conn)
        if (!room) return
        if (room.publisher === conn) {
          this.publisherTakeover(room)
          return
        }
        if (!room.viewers.has(conn)) return
        if (conn.claims.perm !== `steer`) return
        if (room.steerer && room.steerer !== conn) return // first claim wins
        room.steerer = conn
        this.broadcastPresence(room)
        return
      }

      case `release`: {
        const room = this.roomFor(conn)
        if (!room) return
        if (room.publisher === conn) {
          this.publisherTakeover(room)
          return
        }
        if (room.steerer === conn) {
          room.steerer = null
          this.broadcastPresence(room)
        }
        return
      }

      case `kill`: {
        const room = this.roomFor(conn)
        if (!room || !room.publisher) return
        if (conn.claims.perm !== `steer`) return
        room.publisher.sock.send(frame({ t: `kill` }))
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

  // ── Output hot path ────────────────────────────────────────────────────────

  private onOutput(conn: Conn, data: Uint8Array) {
    const room = this.roomFor(conn)
    if (!room || room.publisher !== conn) return
    if (data.byteLength < 1 || data[0] !== OUTPUT_OPCODE) return
    const payload = data.subarray(1)
    room.ring.push(payload)

    const framed = outputFrame(payload)
    const now = Date.now()
    for (const viewer of room.viewers.keys()) {
      if (viewer.sock.bufferedAmount() > VIEWER_HIGH_WATER) {
        // Slow consumer: drop output frames; evict after sustained saturation.
        viewer.laggingSince ??= now
        if (now - viewer.laggingSince > VIEWER_LAG_EVICT_MS) {
          viewer.sock.close(CLOSE_SLOW_CONSUMER, `slow_consumer`)
        }
        continue
      }
      if (viewer.laggingSince !== undefined) {
        // Recovered — it missed frames; ask the publisher for a full repaint.
        viewer.laggingSince = undefined
        room.publisher.sock.send(frame({ t: `resync` }))
      }
      viewer.sock.send(framed)
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
    }))
  }

  sessionInfo(sessionId: string) {
    const room = this.rooms.get(sessionId)
    if (!room) return { live: false as const }
    return {
      live: room.publisher !== null,
      viewers: room.viewers.size,
      issueId: room.issueId ?? null,
    }
  }

  /** Route a remote "Start on my desktop" to the device's control socket. */
  startSession(
    userId: string,
    deviceId: string,
    issueId: string
  ): { ok: true } | { ok: false; reason: `device_offline` } {
    const entry = this.devices.get(userId)?.get(deviceId)
    if (!entry) return { ok: false, reason: `device_offline` }
    entry.conn.sock.send(frame({ t: `start_session`, issueId }))
    return { ok: true }
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

  /**
   * The local desktop user always wins (masterplan §3.4): "Take over" sends
   * release-then-claim on the publisher socket, which force-clears any remote
   * steer claim immediately. The publisher never becomes the steerer itself —
   * local input doesn't flow through the relay.
   */
  private publisherTakeover(room: Room) {
    room.steerer = null
    this.broadcastPresence(room)
  }

  private broadcastPresence(room: Room) {
    const msg = frame({
      t: `presence`,
      viewers: [...room.viewers.values()],
      steererId: room.steerer?.claims.sub ?? null,
    })
    room.publisher?.sock.send(msg)
    for (const viewer of room.viewers.keys()) viewer.sock.send(msg)
  }

  private closeRoom(room: Room, outcome: string) {
    if (room.staleTimer) clearTimeout(room.staleTimer)
    this.rooms.delete(room.sessionId)
    const msg = frame({ t: `bye`, outcome })
    for (const viewer of room.viewers.keys()) {
      viewer.sock.send(msg)
      viewer.sock.close(CLOSE_SESSION_ENDED, `session_ended`)
    }
    room.viewers.clear()
  }
}
