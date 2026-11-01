// EXP-1527: ONE session feed over two carriers, shaped like the WebSocket
// the steer store drives (fixtures/steer-enet.json `fallback`). eNet is the
// primary; the sealed relay tunnel (steer-tunnel.ts) opens only when eNet
// stalls (`stallMs`), has no feed or drops, and eNet keeps redialing in the
// background and takes over on its `activity_reset`. Which carrier does what
// is `steerCarrierStep` (the `carrierCases` every viewer replays): this file
// only runs its actions. Carriers are invisible: the store sees one socket,
// frames of the active carrier only, and a carrier switch as a re-join
// (`activity_reset` + replay, staged into one commit).
//
// The feed closes (CLOSE_ENET_LOST) once NEITHER carrier is active and none
// is on its way: the store's existing drop rule (LOST BANNER) takes it from
// there, with `downSince` = when the last carrier went, so the grace counts
// across both. Its redial is a fresh feed (eNet first, the relay after the
// stall), so a failed redial is one where BOTH failed. A relay that dropped
// while active is retried by that redial (`reopenRelayLater`).

import {
  STEER_FALLBACK_STALL_MS,
  STEER_RELAY_RETRY_MS,
  steerCarrierStep,
  type SteerCarrierAction,
  type SteerCarrierEvent,
  type SteerCarrierState,
  type SteerConnPath,
  type SteerFrame,
} from "@exp/domain-contract/steer-enet"
import { CLOSE_ENET_LOST, type EnetNoFeed } from "./steer-enet-viewer"
import { TunnelLeg, type TunnelDeps } from "./steer-tunnel"

/** A joined eNet feed (EnetSteerSocket), as the feed drives it. */
export type EnetLeg = {
  readyState: number
  onmessage: ((event: { data: string }) => void) | null
  onclose: ((event: { code: number }) => void) | null
  send: (data: string) => void
  close: () => void
}

/** What the feed reports for connection analytics (conn-stats.ts). */
export type FeedStats = {
  /** A frame applied to the store, from the carrier on `path`. */
  applied: (path: SteerConnPath, reset: boolean) => void
  /** The relay leg opened. */
  fallback: () => void
  /** A carrier failed (connStats `errorClasses`). */
  failure: (errorClass: string) => void
}

export type SteerFeedDeps = {
  /** One eNet attempt (openEnetSteer); `why` names a no-feed. */
  enet: (sessionId: string, why: (reason: EnetNoFeed) => void) => Promise<EnetLeg | null>
  /** The tunnel leg's deps; null = this viewer has no relay leg at all. */
  tunnel: TunnelDeps | null
  stallMs?: number
  /** The background eNet redial's backoff while the relay carries. */
  retryMs?: readonly number[]
  stats?: FeedStats
}

/** The store's close event: `downSince` = when the last carrier went. */
export type SteerFeedClose = { code: number; downSince?: number }

const OPEN = 1
const CLOSED = 3
const JOIN: SteerFrame = { t: `join`, channel: `activity` }
const HELD_MAX = 200

/** Dials a feed; null = NO FEED on both carriers. */
export function openSteerFeed(sessionId: string, deps: SteerFeedDeps): Promise<SteerFeed | null> {
  return new SteerFeed(sessionId, deps).ready
}

export class SteerFeed {
  readyState = OPEN
  onmessage: ((event: { data: string }) => void) | null = null
  onclose: ((event: SteerFeedClose) => void) | null = null
  readonly ready: Promise<SteerFeed | null>

  private state: SteerCarrierState = { active: `none`, relay: `closed` }
  private settle: ((feed: SteerFeed | null) => void) | null = null
  private events: Array<[SteerCarrierEvent, SteerFrame | undefined]> = []
  private stepping = false
  private wasActive = false
  private downSince: number | null = null

  private enet: EnetLeg | null = null
  private enetGen = 0
  private enetDialing = false
  private enetTimer: ReturnType<typeof setTimeout> | undefined
  private enetRetries = 0

  private relay: TunnelLeg | null = null
  private relayGen = 0
  /** The relay said it is not there for this feed: never re-asked (the store's redial does). */
  private relayGone = false

  private stallTimer: ReturnType<typeof setTimeout> | undefined
  /** Frames for the store before its own `join` (its onopen). */
  private pending: SteerFrame[] | null = []
  /** Steering frames sent while no carrier was active. */
  private held: SteerFrame[] = []
  private outbox: SteerFrame[] = []

  constructor(
    readonly sessionId: string,
    private readonly deps: SteerFeedDeps
  ) {
    this.ready = new Promise((resolve) => (this.settle = resolve))
    this.step(`start`)
  }

  /** The carrier frames apply from (tests; never shown). */
  get carrier(): SteerCarrierState[`active`] {
    return this.state.active
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

  /** One steer client frame (`{t, ...}` JSON), on the ACTIVE carrier. */
  send(data: string): void {
    if (this.readyState !== OPEN) return
    const frame = JSON.parse(data) as SteerFrame
    if (frame.t === `join`) {
      if (this.pending) {
        const replay = this.pending
        void Promise.resolve().then(() => {
          if (this.pending !== replay) return
          this.pending = null
          for (const f of replay) this.deliver(f)
        })
        return
      }
      if (this.state.active === `enet`) this.enet?.send(data)
      else if (this.state.active === `relay`) this.relay?.rejoin()
      return
    }
    this.route(frame)
  }

  close(): void {
    this.shut(1000)
  }

  // ── The rule ──────────────────────────────────────────────────────────────

  /** Events queue: a carrier answering inside an action never re-enters. */
  private step(event: SteerCarrierEvent, frame?: SteerFrame) {
    this.events.push([event, frame])
    if (this.stepping) return
    this.stepping = true
    try {
      while (this.events.length > 0 && this.readyState === OPEN) {
        const [next, nextFrame] = this.events.shift()!
        const { state, actions } = steerCarrierStep(this.state, next)
        this.state = state
        for (const action of actions) this.run(action, nextFrame)
        this.check()
      }
    } finally {
      this.stepping = false
      this.events = []
    }
  }

  private run(action: SteerCarrierAction, frame: SteerFrame | undefined) {
    switch (action) {
      case `dialEnet`:
        return this.dialEnet()
      case `armStall`:
        clearTimeout(this.stallTimer)
        this.stallTimer = setTimeout(() => this.step(`stall`), this.deps.stallMs ?? STEER_FALLBACK_STALL_MS)
        return
      case `openRelay`:
        return this.openRelay()
      case `rejoinRelay`:
        return this.relay ? this.relay.rejoin() : this.openRelay()
      case `closeRelay`:
        return this.closeRelay()
      case `redialEnetLater`:
        return this.redialEnetLater()
      case `reopenRelayLater`:
        // The feed closes (check): the store's redial reopens both.
        return
      case `apply`:
        return frame && this.apply(frame)
      case `drop`:
        return
    }
  }

  /** Nothing active and nothing on its way: no feed (dialing) or lost (after a frame). */
  private check() {
    if (this.readyState !== OPEN) return
    const { active, relay } = this.state
    if (active !== `none`) {
      this.downSince = null
      if (this.settle) {
        this.settle(this)
        this.settle = null
      }
      this.flushHeld()
      return
    }
    if (this.wasActive && this.downSince === null) this.downSince = Date.now()
    if (relay === `open`) return
    if (!this.wasActive && this.enetDialing) return
    this.shut(CLOSE_ENET_LOST)
  }

  // ── eNet ──────────────────────────────────────────────────────────────────

  private dialEnet() {
    if (this.enet || this.enetDialing) return
    clearTimeout(this.enetTimer)
    this.enetTimer = undefined
    const gen = ++this.enetGen
    this.enetDialing = true
    let why: EnetNoFeed = `join_timeout`
    void this.deps
      .enet(this.sessionId, (reason) => (why = reason))
      .catch(() => null)
      .then((sock) => {
        if (gen !== this.enetGen || this.readyState !== OPEN) {
          sock?.close()
          return
        }
        this.enetDialing = false
        if (!sock) {
          this.deps.stats?.failure(why)
          return this.step(`enetNoFeed`)
        }
        this.enet = sock
        sock.onmessage = ({ data }) => {
          if (gen !== this.enetGen) return
          const f = parseFrame(data)
          if (f) this.step(f.t === `activity_reset` ? `enetReset` : `enetSynced`, f)
        }
        sock.onclose = ({ code }) => {
          if (gen !== this.enetGen) return
          this.enet = null
          this.deps.stats?.failure(code === CLOSE_ENET_LOST ? `stale` : `closed`)
          this.step(`enetDropped`)
        }
        // Delivers the replay that proved the join.
        sock.send(JSON.stringify(JOIN))
      })
  }

  private redialEnetLater() {
    if (this.enet || this.enetDialing || this.enetTimer) return
    const steps = this.deps.retryMs ?? STEER_RELAY_RETRY_MS
    const delay = steps[Math.min(this.enetRetries++, steps.length - 1)]
    this.enetTimer = setTimeout(() => {
      this.enetTimer = undefined
      if (this.readyState === OPEN) this.dialEnet()
    }, delay)
  }

  // ── The relay ─────────────────────────────────────────────────────────────

  private openRelay() {
    this.closeRelay()
    const tunnel = this.deps.tunnel
    if (this.relayGone || !tunnel || !tunnel.keys(this.sessionId)) {
      this.relayGone = true
      return this.step(`relayUnavailable`)
    }
    this.deps.stats?.fallback()
    const gen = ++this.relayGen
    this.relay = new TunnelLeg(this.sessionId, tunnel, {
      frame: (f) => {
        if (gen === this.relayGen) this.step(f.t === `activity_reset` ? `relayReset` : `relaySynced`, f)
      },
      down: (how, why) => {
        if (gen !== this.relayGen) return
        this.relay = null
        this.deps.stats?.failure(why)
        if (how === `unavailable`) this.relayGone = true
        this.step(how === `dropped` ? `relayDropped` : `relayUnavailable`)
      },
    })
  }

  private closeRelay() {
    this.relayGen++
    this.relay?.close()
    this.relay = null
  }

  // ── Frames ────────────────────────────────────────────────────────────────

  private apply(frame: SteerFrame) {
    const active = this.state.active
    if (active === `none`) return
    const reset = frame.t === `activity_reset`
    if (reset && active === `enet`) this.enetRetries = 0
    this.wasActive = true
    this.deps.stats?.applied(active === `enet` ? `iroh_relay` : `server_fallback`, reset)
    if (this.pending) this.pending.push(frame)
    else this.deliver(frame)
  }

  private deliver(frame: SteerFrame) {
    if (this.readyState !== OPEN) return
    this.onmessage?.({ data: JSON.stringify(frame) })
    // The device dropped this viewer after its `bye`.
    if (frame.t === `bye`) this.shut(1000)
  }

  private route(frame: SteerFrame) {
    const active = this.state.active
    if (active === `enet` && this.enet) return this.enet.send(JSON.stringify(frame))
    if (active === `relay` && this.relay) {
      // Frames sent in one tick share an envelope (a message = its chunks + CR).
      if (this.outbox.length === 0) {
        void Promise.resolve().then(() => {
          const frames = this.outbox
          this.outbox = []
          if (this.state.active === `relay`) this.relay?.send(frames)
          else for (const f of frames) this.hold(f)
        })
      }
      this.outbox.push(frame)
      return
    }
    this.hold(frame)
  }

  private hold(frame: SteerFrame) {
    if (this.held.length < HELD_MAX) this.held.push(frame)
  }

  private flushHeld() {
    if (this.held.length === 0) return
    const held = this.held
    this.held = []
    for (const frame of held) this.route(frame)
  }

  private shut(code: number) {
    if (this.readyState === CLOSED) return
    this.readyState = CLOSED
    clearTimeout(this.stallTimer)
    clearTimeout(this.enetTimer)
    this.enetGen++
    const enet = this.enet
    this.enet = null
    if (enet) {
      enet.onmessage = null
      enet.onclose = null
      enet.close()
    }
    this.closeRelay()
    this.pending = null
    this.held = []
    this.outbox = []
    if (this.settle) {
      this.settle(null)
      this.settle = null
      return
    }
    const downSince = this.downSince ?? undefined
    void Promise.resolve().then(() => this.onclose?.({ code, downSince }))
  }
}

function parseFrame(data: unknown): SteerFrame | null {
  if (typeof data !== `string`) return null
  try {
    const frame = JSON.parse(data) as SteerFrame
    return frame && typeof frame === `object` && typeof frame.t === `string` ? frame : null
  } catch {
    return null
  }
}
