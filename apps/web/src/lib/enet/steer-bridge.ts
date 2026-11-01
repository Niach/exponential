// VAPP-33: which carrier a tab's steer viewer gets. The eNet node runs only
// in the tab holding the Web Lock (use-web-install.ts), so every OTHER tab of
// the profile reaches it over a BroadcastChannel: the node's tab serves the
// bridge (sends chat for the others, pins their device endpoints, rebroadcasts
// every inbound steer envelope), the others are clients. A user's sessions
// open in whichever tab they clicked, and since VAPP-43 the steer relay
// carries no session content, so a tab without the bridge would have no feed
// at all; the bridge is a few message kinds and the
// viewer rule (joined session + joined endpoint) already filters what each
// tab applies. Every tab shares ONE endpoint id, so the node's tab counts
// joins per (device, session) across tabs and forwards a `leave` only when
// the last tab leaves; a second tab's `join` restarts the feed for all of
// them, which each store takes as a re-join (staged, one commit).
// EXP-1531: joins are counted per SOCKET (`${tab}:${sub}`), not per tab, so a
// superseded socket's leave never ends a newer socket's feed in the same tab;
// a lost feed only `forget`s its count (no leave goes out). The node's
// `peerDisconnected` and each peer's path ride the bridge like chat does.
// No node tab answering within WHO_TIMEOUT_MS = no carrier = no feed.

import { encodeSteerEnvelope, parseSteerEnvelope, STEER_ENET_SEND_TIMEOUT_MS } from "@exp/domain-contract/steer-enet"
import type { SteerCarrier } from "./steer-enet-viewer"
import type { ChatListener, EnetPathKind } from "./web-install"

export const STEER_BRIDGE_CHANNEL = `exp-enet-steer`
export const WHO_TIMEOUT_MS = 500

/** The node tab's own id in the join counts. */
const SELF = `self`

/** The slice of BroadcastChannel the bridge uses. */
export type BridgeChannel = {
  postMessage: (message: BridgeMessage) => void
  onmessage: ((event: { data: BridgeMessage }) => void) | null
  close: () => void
}

export type BridgeMessage =
  | { op: `who` }
  | { op: `here` }
  | { op: `send`; tab: string; req: number; to: string; body: Uint8Array; sub?: string }
  | { op: `forget`; tab: string; to: string; sessionId: string; sub: string }
  | { op: `sent`; tab: string; req: number; error?: string }
  | { op: `allow`; tab: string; pin: number; id: string }
  | { op: `release`; tab: string; pin: number }
  | { op: `gone`; tab: string }
  | { op: `chat`; from: string; body: Uint8Array; path?: EnetPathKind }
  | { op: `peerGone`; peer: string }

/** What the router needs from the running node (WebInstall). */
export type SteerNode = {
  onChat: (listener: ChatListener) => () => void
  sendChat: (to: string, body: Uint8Array) => Promise<void>
  pin: (id: string) => () => void
  /** EXP-1531 (optional: fakes and older nodes lack them). */
  onPeerDisconnected?: (listener: (peer: string) => void) => () => void
  pathKind?: (id: string) => EnetPathKind | null
}

/** The node tab's view of every tab's steer joins. */
export class SteerRouter {
  private joins = new Map<string, Set<string>>()
  private pins = new Map<string, () => void>()

  constructor(private readonly node: SteerNode) {}

  /** `tab`'s chat message (from its socket `sub`); a steer `leave` another
   *  socket (of any tab) still needs is held back. */
  async send(tab: string, to: string, body: Uint8Array, sub = ``): Promise<void> {
    const envelope = parseSteerEnvelope(body)
    if (!envelope) return this.node.sendChat(to, body)
    const key = `${to} ${envelope.sessionId}`
    const member = `${tab}:${sub}`
    const frames = envelope.frames.filter((frame) => {
      if (frame.t === `join`) {
        const subs = this.joins.get(key) ?? new Set<string>()
        subs.add(member)
        this.joins.set(key, subs)
      } else if (frame.t === `leave`) {
        const subs = this.joins.get(key)
        subs?.delete(member)
        if (subs && subs.size > 0) return false
        this.joins.delete(key)
      }
      return true
    })
    if (frames.length === 0) return
    await this.node.sendChat(
      to,
      frames.length === envelope.frames.length ? body : encodeSteerEnvelope(envelope.sessionId, frames)
    )
  }

  /** A lost feed's socket stops counting; no leave goes out (the feed is gone). */
  forget(tab: string, to: string, sessionId: string, sub: string) {
    const key = `${to} ${sessionId}`
    const subs = this.joins.get(key)
    subs?.delete(`${tab}:${sub}`)
    if (subs?.size === 0) this.joins.delete(key)
  }

  allow(tab: string, pin: number | string, id: string): () => void {
    const key = `${tab} ${pin}`
    this.pins.get(key)?.()
    const release = this.node.pin(id)
    this.pins.set(key, release)
    return () => this.release(tab, pin)
  }

  release(tab: string, pin: number | string) {
    const key = `${tab} ${pin}`
    this.pins.get(key)?.()
    this.pins.delete(key)
  }

  /** A tab closed: its pins go, its joins count down (the last one leaves). */
  dropTab(tab: string) {
    for (const [key, release] of this.pins) {
      if (!key.startsWith(`${tab} `)) continue
      release()
      this.pins.delete(key)
    }
    for (const [key, subs] of this.joins) {
      let had = false
      for (const member of subs) {
        if (!member.startsWith(`${tab}:`)) continue
        subs.delete(member)
        had = true
      }
      if (!had || subs.size > 0) continue
      this.joins.delete(key)
      const [to, sessionId] = key.split(` `) as [string, string]
      void this.node.sendChat(to, encodeSteerEnvelope(sessionId, [{ t: `leave` }])).catch(() => {})
    }
  }

  carrier(tab: string = SELF): SteerCarrier {
    let pins = 0
    return {
      send: (to, body, sub) => this.send(tab, to, body, sub),
      forget: (to, sessionId, sub) => this.forget(tab, to, sessionId, sub),
      allow: (id) => this.allow(tab, `l${++pins}`, id),
      subscribe: (listener) => this.node.onChat(listener),
      onPeerGone: (listener) => this.node.onPeerDisconnected?.(listener) ?? (() => {}),
      pathKind: (id) => this.node.pathKind?.(id) ?? null,
    }
  }
}

/** The node tab: serves the others; returns its own carrier and the stop. */
export function serveSteerBridge(node: SteerNode, channel: BridgeChannel) {
  const router = new SteerRouter(node)
  const offChat = node.onChat((from, body) => {
    if (parseSteerEnvelope(body)) channel.postMessage({ op: `chat`, from, body, path: node.pathKind?.(from) ?? undefined })
  })
  const offGone = node.onPeerDisconnected?.((peer) => channel.postMessage({ op: `peerGone`, peer }))
  channel.onmessage = ({ data }) => {
    switch (data.op) {
      case `who`:
        return channel.postMessage({ op: `here` })
      case `send`:
        return void router.send(data.tab, data.to, data.body, data.sub).then(
          () => channel.postMessage({ op: `sent`, tab: data.tab, req: data.req }),
          (error: unknown) =>
            channel.postMessage({ op: `sent`, tab: data.tab, req: data.req, error: String(error) })
        )
      case `forget`:
        return router.forget(data.tab, data.to, data.sessionId, data.sub)
      case `allow`:
        return void router.allow(data.tab, data.pin, data.id)
      case `release`:
        return router.release(data.tab, data.pin)
      case `gone`:
        return router.dropTab(data.tab)
    }
  }
  channel.postMessage({ op: `here` })
  return {
    carrier: router.carrier(),
    stop() {
      offChat()
      offGone?.()
      router.dropTab(SELF)
      channel.onmessage = null
      channel.close()
    },
  }
}

/** A tab without the node: reaches it through the node tab. */
export class SteerBridgeClient {
  readonly tab = `t${Math.random().toString(36).slice(2)}`
  private listeners = new Set<ChatListener>()
  private goneListeners = new Set<(peer: string) => void>()
  /** The node tab's last word on each peer's path (EXP-1531). */
  private paths = new Map<string, EnetPathKind>()
  private waiting = new Map<number, { resolve: () => void; reject: (error: Error) => void }>()
  private heard: Array<() => void> = []
  private req = 0
  private pins = 0

  constructor(
    private readonly channel: BridgeChannel,
    private readonly timeouts = { whoMs: WHO_TIMEOUT_MS, sendMs: STEER_ENET_SEND_TIMEOUT_MS }
  ) {
    channel.onmessage = ({ data }) => {
      if (data.op === `here`) {
        const heard = this.heard
        this.heard = []
        for (const resolve of heard) resolve()
      } else if (data.op === `chat`) {
        if (data.path) this.paths.set(data.from, data.path)
        for (const listener of this.listeners) listener(data.from, data.body)
      } else if (data.op === `peerGone`) {
        this.paths.delete(data.peer)
        for (const listener of [...this.goneListeners]) listener(data.peer)
      } else if (data.op === `sent` && data.tab === this.tab) {
        const pending = this.waiting.get(data.req)
        this.waiting.delete(data.req)
        if (data.error) pending?.reject(new Error(data.error))
        else pending?.resolve()
      }
    }
  }

  /** The bridge, when a node tab answers in time. */
  async carrier(): Promise<SteerCarrier | undefined> {
    const answered = await new Promise<boolean>((resolve) => {
      const timer = setTimeout(() => resolve(false), this.timeouts.whoMs)
      this.heard.push(() => {
        clearTimeout(timer)
        resolve(true)
      })
      this.channel.postMessage({ op: `who` })
    })
    if (!answered) return undefined
    return {
      send: (to, body, sub) => this.send(to, body, sub),
      forget: (to, sessionId, sub) => this.channel.postMessage({ op: `forget`, tab: this.tab, to, sessionId, sub }),
      allow: (id) => {
        const pin = ++this.pins
        this.channel.postMessage({ op: `allow`, tab: this.tab, pin, id })
        return () => this.channel.postMessage({ op: `release`, tab: this.tab, pin })
      },
      subscribe: (listener) => {
        this.listeners.add(listener)
        return () => void this.listeners.delete(listener)
      },
      onPeerGone: (listener) => {
        this.goneListeners.add(listener)
        return () => void this.goneListeners.delete(listener)
      },
      pathKind: (id) => this.pathKind(id),
    }
  }

  /** The path the node tab last reported for `id` (null = none / gone). */
  pathKind(id: string): EnetPathKind | null {
    return this.paths.get(id) ?? null
  }

  /** This tab is going away (pagehide). */
  gone() {
    this.channel.postMessage({ op: `gone`, tab: this.tab })
  }

  private send(to: string, body: Uint8Array, sub?: string): Promise<void> {
    const req = ++this.req
    return new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiting.delete(req)
        reject(new Error(`bridge send timed out`))
      }, this.timeouts.sendMs)
      this.waiting.set(req, {
        resolve: () => {
          clearTimeout(timer)
          resolve()
        },
        reject: (error) => {
          clearTimeout(timer)
          reject(error)
        },
      })
      this.channel.postMessage({ op: `send`, tab: this.tab, req, to, body, sub })
    })
  }
}

// ── This tab's carrier ───────────────────────────────────────────────────────

let local: SteerCarrier | undefined
let client: SteerBridgeClient | undefined

/** use-web-install.ts: this tab runs the node (undefined = not any more). */
export function setLocalSteerCarrier(carrier: SteerCarrier | undefined) {
  local = carrier
}

/** The node here, else the bridge to the node's tab, else none (= no feed). */
export function currentSteerCarrier(): Promise<SteerCarrier | undefined> {
  if (local) return Promise.resolve(local)
  if (typeof BroadcastChannel === `undefined`) return Promise.resolve(undefined)
  if (!client) {
    const bridge = new SteerBridgeClient(new BroadcastChannel(STEER_BRIDGE_CHANNEL) as unknown as BridgeChannel)
    client = bridge
    globalThis.addEventListener?.(`pagehide`, () => bridge.gone())
  }
  return client.carrier()
}

/** EXP-1531 (connStats): how this browser's node reaches `endpointId` right
 *  now; null = no open connection, no node, or an older wasm build. */
export function enetPathKind(endpointId: string): EnetPathKind | null {
  if (local) return local.pathKind?.(endpointId) ?? null
  return client?.pathKind(endpointId) ?? null
}
