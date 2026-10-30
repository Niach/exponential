// VAPP-33: the web steer viewer over eNet's chat channel
// (packages/domain-contract/fixtures/steer-enet.json is the spec). One
// attempt per dial: `installs.forSession` → join the first endpoint → on
// `error no_such_session` (or `unauthorized`, or a send that never lands)
// the next → the first `activity_synced` wins. No carrier, no endpoint,
// every endpoint refused, or no `activity_synced` within joinTimeoutMs =
// null, and the store dials the steer relay exactly as before (the fallback
// until #VAPP-43).
//
// A won attempt is handed to the store as a WebSocket-shaped object
// (`EnetSteerSocket`): the store sends and parses the relay's `{t, ...}`
// frames unchanged, and they ride the envelope as they are. The replay that proved the join is buffered and
// delivered when the store sends its own `join` (its onopen), so the store's
// staging sees the same reset → replay → synced sequence a relay join gives.

import {
  encodeSteerEnvelope,
  parseSteerEnvelope,
  STEER_ENET_JOIN_TIMEOUT_MS,
  STEER_ENET_STALE_MS,
  type SteerFrame,
} from "@exp/domain-contract/steer-enet"

/** What the viewer needs from this browser's eNet node (the node's own tab,
 *  or the bridge to it: steer-bridge.ts). */
export type SteerCarrier = {
  /** One chat message; resolves once the peer acknowledged it. */
  send: (to: string, body: Uint8Array) => Promise<void>
  /** Keeps `endpointId` on the node's allowlist until the release runs: the
   *  device dials its frames back, so the node must accept it. */
  allow: (endpointId: string) => () => void
  /** Every inbound chat message. */
  subscribe: (listener: (from: string, body: Uint8Array) => void) => () => void
}

export type SteerEndpoint = { endpointId: string; kind: string }

export type EnetSteerDeps = {
  carrier: () => Promise<SteerCarrier | undefined>
  forSession: (sessionId: string) => Promise<SteerEndpoint[]>
  joinTimeoutMs?: number
  staleMs?: number
}

/** The eNet feed went silent past a re-join: the store redials silently
 *  (eNet again, else the relay), like the relay's slow-consumer eviction. */
export const CLOSE_ENET_LOST = 4901

const OPEN = 1
const CLOSED = 3
const JOIN: SteerFrame = { t: `join`, channel: `activity` }

type Outcome = { kind: `joined`; frames: SteerFrame[] } | { kind: `refused` } | { kind: `timeout` }

/** One eNet attempt for a dial; null = use the steer relay. */
export async function openEnetSteer(sessionId: string, deps: EnetSteerDeps): Promise<EnetSteerSocket | null> {
  const joinTimeoutMs = deps.joinTimeoutMs ?? STEER_ENET_JOIN_TIMEOUT_MS
  const carrier = await deps.carrier().catch(() => undefined)
  if (!carrier) return null
  const endpoints = await withDeadline(deps.forSession(sessionId), joinTimeoutMs).catch(() => undefined)
  if (!endpoints || endpoints.length === 0) return null
  for (const { endpointId } of endpoints) {
    const release = carrier.allow(endpointId)
    const buffered: SteerFrame[] = []
    let settle: (outcome: Outcome) => void = () => {}
    const outcome = new Promise<Outcome>((resolve) => (settle = resolve))
    const unsubscribe = carrier.subscribe((from, body) => {
      const envelope = steerEnvelopeFor(sessionId, endpointId, from, body)
      if (!envelope) return
      for (const frame of envelope.frames) {
        if (frame.t === `error` || frame.t === `bye`) return settle({ kind: `refused` })
        buffered.push(frame)
        if (frame.t === `activity_synced`) return settle({ kind: `joined`, frames: buffered })
      }
    })
    carrier.send(endpointId, encodeSteerEnvelope(sessionId, [JOIN])).catch(() => settle({ kind: `refused` }))
    const timer = setTimeout(() => settle({ kind: `timeout` }), joinTimeoutMs)
    const result = await outcome
    clearTimeout(timer)
    unsubscribe()
    if (result.kind === `joined`) {
      return new EnetSteerSocket(sessionId, endpointId, carrier, release, result.frames, deps.staleMs, joinTimeoutMs)
    }
    void carrier.send(endpointId, encodeSteerEnvelope(sessionId, [{ t: `leave` }])).catch(() => {})
    release()
    if (result.kind === `timeout`) return null
  }
  return null
}

/** The viewer-side rule: only the joined session, only from the joined endpoint. */
function steerEnvelopeFor(sessionId: string, endpointId: string, from: string, body: Uint8Array) {
  if (from !== endpointId) return null
  const envelope = parseSteerEnvelope(body)
  return envelope && envelope.sessionId === sessionId ? envelope : null
}

/** A joined eNet feed, shaped like the relay WebSocket the store drives. */
export class EnetSteerSocket {
  readyState = OPEN
  onmessage: ((event: { data: string }) => void) | null = null
  onclose: ((event: { code: number }) => void) | null = null
  /** The join replay, until the store's own `join` asks for it. */
  private pending: SteerFrame[] | null
  private outbox: SteerFrame[] = []
  private sending: Promise<void> = Promise.resolve()
  private unsubscribe: () => void
  private staleTimer: ReturnType<typeof setTimeout> | undefined
  private rejoinTimer: ReturnType<typeof setTimeout> | undefined

  constructor(
    readonly sessionId: string,
    readonly endpointId: string,
    private readonly carrier: SteerCarrier,
    private readonly release: () => void,
    replay: SteerFrame[],
    private readonly staleMs = STEER_ENET_STALE_MS,
    private readonly joinTimeoutMs = STEER_ENET_JOIN_TIMEOUT_MS
  ) {
    this.pending = replay
    this.unsubscribe = carrier.subscribe((from, body) => {
      const envelope = steerEnvelopeFor(sessionId, endpointId, from, body)
      if (envelope) this.receive(envelope.frames)
    })
    this.armStale()
  }

  private openHandler: (() => void) | null = null
  private opened = false

  get onopen() {
    return this.openHandler
  }

  /** Like a WebSocket: open fires once, after the caller attached its handlers. */
  set onopen(handler: (() => void) | null) {
    this.openHandler = handler
    if (!handler || this.opened) return
    this.opened = true
    void Promise.resolve().then(() => {
      if (this.readyState === OPEN) this.openHandler?.()
    })
  }

  /** One relay-shaped client frame (`{t, ...}` JSON). */
  send(data: string): void {
    if (this.readyState !== OPEN) return
    const frame = JSON.parse(data) as SteerFrame
    if (frame.t === `join` && this.pending) {
      // Already joined: deliver the replay that proved it, after the
      // caller's onopen finished arming its own join deadline.
      const replay = this.pending
      void Promise.resolve().then(() => {
        if (this.pending !== replay) return
        this.pending = null
        this.deliver(replay)
      })
      return
    }
    // Frames sent in one tick share an envelope (a message = its text
    // chunks + the submitting CR).
    if (this.outbox.length === 0) void Promise.resolve().then(() => this.flush())
    this.outbox.push(frame)
  }

  close(): void {
    this.shut(1000, true)
  }

  private flush() {
    const frames = this.outbox
    this.outbox = []
    if (frames.length === 0 || this.readyState !== OPEN) return
    const body = encodeSteerEnvelope(this.sessionId, frames)
    // Chained: the chat channel keeps order per stream, this keeps it per call.
    this.sending = this.sending.then(() =>
      this.carrier.send(this.endpointId, body).catch(() => this.shut(CLOSE_ENET_LOST, false))
    )
  }

  private receive(frames: SteerFrame[]) {
    this.armStale()
    if (this.pending) {
      // Still before the store's join: keep the order.
      this.pending.push(...frames)
      return
    }
    this.deliver(frames)
  }

  private deliver(frames: SteerFrame[]) {
    for (const frame of frames) {
      if (this.readyState !== OPEN) return
      this.onmessage?.({ data: JSON.stringify(frame) })
      // The device dropped this viewer (the relay closes after its `bye`).
      if (frame.t === `bye`) return this.shut(1000, false)
    }
  }

  /** `staleMs` without a frame: re-send `join`; no answer = the feed is lost. */
  private armStale() {
    clearTimeout(this.staleTimer)
    clearTimeout(this.rejoinTimer)
    this.rejoinTimer = undefined
    this.staleTimer = setTimeout(() => {
      if (this.readyState !== OPEN) return
      this.send(JSON.stringify({ t: `join`, channel: `activity` }))
      this.rejoinTimer = setTimeout(() => this.shut(CLOSE_ENET_LOST, true), this.joinTimeoutMs)
    }, this.staleMs)
  }

  private shut(code: number, leave: boolean) {
    if (this.readyState === CLOSED) return
    this.readyState = CLOSED
    this.pending = null
    clearTimeout(this.staleTimer)
    clearTimeout(this.rejoinTimer)
    this.unsubscribe()
    if (leave) {
      const body = encodeSteerEnvelope(this.sessionId, [{ t: `leave` }])
      void this.sending.then(() => this.carrier.send(this.endpointId, body)).catch(() => {})
    }
    this.release()
    void Promise.resolve().then(() => this.onclose?.({ code }))
  }
}

function withDeadline<T>(promise: Promise<T>, ms: number): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timed out`)), ms)
    promise.then(
      (value) => {
        clearTimeout(timer)
        resolve(value)
      },
      (error: unknown) => {
        clearTimeout(timer)
        reject(error)
      }
    )
  })
}
