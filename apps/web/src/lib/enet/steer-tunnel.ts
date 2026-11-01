// EXP-1527: the web viewer's SEALED RELAY TUNNEL, eNet's fallback carrier
// (fixtures/steer-enet.json `fallback`). The same exp-steer envelopes ride a
// steer-relay WebSocket instead of an eNet chat stream, every one sealed to
// the session TREE key (fixtures/sealed-content.json): `data` =
// `v1:<epoch>:<b64url push_seal(tree_public, utf8(envelope JSON))>` both
// ways, opened with openBody(`exp-sealed:` + data). The relay sees the
// session id, its tunnel id and ciphertext, nothing else.
//
// AVAILABILITY: only when the ROOT carries a seal key, this install holds
// that epoch's tree secret AND `steer.tunnelTicket` hands out a ticket;
// anything else is `unavailable` (the viewer stays eNet-only). steer-feed.ts
// decides WHEN this leg opens (steerCarrierStep); this file is the leg.

import { openBody, sealBody, SEALED_BODY_PREFIX, type TreeKeys } from "@exp/domain-contract/sealed-content"
import {
  encodeSteerEnvelope,
  parseSteerEnvelope,
  STEER_ENET_JOIN_TIMEOUT_MS,
  STEER_ENET_STALE_MS,
  STEER_TUNNEL_MAX_DATA_BYTES,
  type SteerFrame,
} from "@exp/domain-contract/steer-enet"

/** The key a tunnel seals to and the secrets it opens with (the ROOT's). */
export type TunnelKeys = { epoch: number; publicKey: string; keys: TreeKeys }

/** `steer.tunnelTicket` (#EXP-1525); a throw counts as no ticket. */
export type TunnelTicket = { disabled: true } | { unavailable: true } | { ticket: string; url: string }

/** The slice of WebSocket the tunnel uses. */
export type TunnelSocket = {
  readyState: number
  onopen: (() => void) | null
  onmessage: ((event: { data: unknown }) => void) | null
  onclose: ((event: { code: number }) => void) | null
  send: (data: string) => void
  close: () => void
}

export type TunnelDeps = {
  /** The root's current seal key with this install's secrets; null = keyless or no secret. */
  keys: (sessionId: string) => TunnelKeys | null
  ticket: (sessionId: string) => Promise<TunnelTicket>
  socket: (url: string) => TunnelSocket
  joinTimeoutMs?: number
  staleMs?: number
}

/** Why the leg is down: an analytics error class (connStats `errorClasses`). */
export type TunnelDown = `no_key` | `ticket_failed` | `device_offline` | `device_unsupported` | `refused` | `join_timeout` | `stale` | `relay_closed`

export type TunnelHandlers = {
  /** One device frame, in order. */
  frame: (frame: SteerFrame) => void
  /** The leg is gone: `unavailable` before its first `activity_reset`, `dropped` after. */
  down: (how: `unavailable` | `dropped`, why: TunnelDown) => void
}

const OPEN = 1
const JOIN: SteerFrame = { t: `join`, channel: `activity` }
const decoder = new TextDecoder()

/** Seals one envelope for the wire (`data` = the sealed value after its prefix). */
export function sealTunnelData(sessionId: string, frames: SteerFrame[], key: Pick<TunnelKeys, `epoch` | `publicKey`>): string {
  const json = decoder.decode(encodeSteerEnvelope(sessionId, frames))
  return sealBody(key.publicKey, key.epoch, json).slice(SEALED_BODY_PREFIX.length)
}

/** Opens one tunnel `data`: the frames of `sessionId`'s envelope, else null (dropped). */
export function openTunnelData(sessionId: string, data: unknown, keys: TreeKeys): SteerFrame[] | null {
  if (typeof data !== `string` || data.length === 0 || data.length > STEER_TUNNEL_MAX_DATA_BYTES) return null
  const opened = openBody(`${SEALED_BODY_PREFIX}${data}`, keys)
  if (opened.status !== `opened`) return null
  const envelope = parseSteerEnvelope(opened.text)
  return envelope && envelope.sessionId === sessionId ? envelope.frames : null
}

/** One tunnel leg; `down` fires exactly once. */
export class TunnelLeg {
  private socket: TunnelSocket | null = null
  private gone = false
  private reset = false
  private staleTimer: ReturnType<typeof setTimeout> | undefined
  private joinTimer: ReturnType<typeof setTimeout> | undefined
  private readonly joinTimeoutMs: number
  private readonly staleMs: number

  constructor(
    readonly sessionId: string,
    private readonly deps: TunnelDeps,
    private readonly on: TunnelHandlers
  ) {
    this.joinTimeoutMs = deps.joinTimeoutMs ?? STEER_ENET_JOIN_TIMEOUT_MS
    this.staleMs = deps.staleMs ?? STEER_ENET_STALE_MS
    void this.open()
  }

  private key: TunnelKeys | null = null

  private async open() {
    this.key = this.deps.keys(this.sessionId)
    if (!this.key) return this.fail(`no_key`)
    const ticket = await this.deps.ticket(this.sessionId).catch(() => null)
    if (this.gone) return
    if (!ticket || !(`url` in ticket)) {
      return this.fail(ticket && `unavailable` in ticket ? `device_offline` : `ticket_failed`)
    }
    let socket: TunnelSocket
    try {
      socket = this.deps.socket(ticket.url)
    } catch {
      return this.fail(`relay_closed`)
    }
    this.socket = socket
    socket.onopen = () => {
      if (this.gone) return
      this.send([JOIN])
      this.armJoin()
    }
    socket.onmessage = ({ data }) => this.receive(data)
    socket.onclose = () => this.fail(`relay_closed`)
  }

  /** Steer client frames, one sealed envelope. */
  send(frames: SteerFrame[]): void {
    const socket = this.socket
    if (this.gone || !socket || socket.readyState !== OPEN || !this.key || frames.length === 0) return
    const data = sealTunnelData(this.sessionId, frames, this.key)
    if (data.length > STEER_TUNNEL_MAX_DATA_BYTES) return
    socket.send(JSON.stringify({ t: `sealed`, data }))
  }

  /** Re-send `join` (a stand-by tunnel taking over): the device restarts the feed. */
  rejoin(): void {
    this.send([JOIN])
    this.armJoin()
  }

  close(): void {
    if (this.gone) return
    this.gone = true
    this.stop()
  }

  private receive(raw: unknown) {
    if (this.gone || typeof raw !== `string`) return
    let message: { t?: unknown; data?: unknown; code?: unknown }
    try {
      message = JSON.parse(raw)
    } catch {
      return
    }
    if (message?.t === `tunnel_error`) {
      return this.fail(message.code === `device_unsupported` ? `device_unsupported` : `device_offline`)
    }
    if (message?.t !== `sealed`) return
    const frames = openTunnelData(this.sessionId, message.data, this.key?.keys ?? new Map())
    if (!frames) return
    clearTimeout(this.joinTimer)
    this.armStale()
    for (const frame of frames) {
      if (this.gone) return
      if (!this.reset) {
        // Before the replay: a refusal ends this leg (the access rule, no such session).
        if (frame.t === `error` || frame.t === `bye`) return this.fail(`refused`)
        if (frame.t !== `activity_reset`) continue
        this.reset = true
      }
      this.on.frame(frame)
    }
  }

  private armJoin() {
    clearTimeout(this.joinTimer)
    this.joinTimer = setTimeout(() => this.fail(this.reset ? `stale` : `join_timeout`), this.joinTimeoutMs)
  }

  /** `staleMs` without a frame: re-join; no answer = the leg is lost. */
  private armStale() {
    clearTimeout(this.staleTimer)
    this.staleTimer = setTimeout(() => this.rejoin(), this.staleMs)
  }

  private fail(why: TunnelDown) {
    if (this.gone) return
    this.gone = true
    this.stop()
    this.on.down(this.reset ? `dropped` : `unavailable`, why)
  }

  private stop() {
    clearTimeout(this.staleTimer)
    clearTimeout(this.joinTimer)
    const socket = this.socket
    this.socket = null
    if (!socket) return
    socket.onopen = null
    socket.onmessage = null
    socket.onclose = null
    try {
      socket.close()
    } catch {
      // Already gone.
    }
  }
}
