// VAPP-33: which carrier a tab's steer viewer gets. The eNet node runs only
// in the tab holding the Web Lock (use-web-install.ts), so every OTHER tab of
// the profile reaches it over a BroadcastChannel: the node's tab serves the
// bridge (sends chat for the others, pins their device endpoints, rebroadcasts
// every inbound steer envelope), the others are clients. Decision: bridge,
// not relay-only for non-node tabs. A user's sessions open in whichever tab
// they clicked, so relay-only there would leave most views on the relay the
// switch-off (#VAPP-43) removes; the bridge is a few message kinds and the
// viewer rule (joined session + joined endpoint) already filters what each
// tab applies. Every tab shares ONE endpoint id, so the node's tab counts
// joins per (device, session) across tabs and forwards a `leave` only when
// the last tab leaves; a second tab's `join` restarts the feed for all of
// them, which each store takes as a relay-style re-join (staged, one commit).
// No node tab answering within WHO_TIMEOUT_MS = no carrier = the relay.

import { encodeSteerEnvelope, parseSteerEnvelope, STEER_ENET_SEND_TIMEOUT_MS } from "@exp/domain-contract/steer-enet"
import type { SteerCarrier } from "./steer-enet-viewer"
import type { ChatListener } from "./web-install"

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
  | { op: `send`; tab: string; req: number; to: string; body: Uint8Array }
  | { op: `sent`; tab: string; req: number; error?: string }
  | { op: `allow`; tab: string; pin: number; id: string }
  | { op: `release`; tab: string; pin: number }
  | { op: `gone`; tab: string }
  | { op: `chat`; from: string; body: Uint8Array }

/** What the router needs from the running node (WebInstall). */
export type SteerNode = {
  onChat: (listener: ChatListener) => () => void
  sendChat: (to: string, body: Uint8Array) => Promise<void>
  pin: (id: string) => () => void
}

/** The node tab's view of every tab's steer joins. */
export class SteerRouter {
  private joins = new Map<string, Set<string>>()
  private pins = new Map<string, () => void>()

  constructor(private readonly node: SteerNode) {}

  /** `tab`'s chat message; a steer `leave` another tab still needs is held back. */
  async send(tab: string, to: string, body: Uint8Array): Promise<void> {
    const envelope = parseSteerEnvelope(body)
    if (!envelope) return this.node.sendChat(to, body)
    const key = `${to} ${envelope.sessionId}`
    const frames = envelope.frames.filter((frame) => {
      if (frame.t === `join`) {
        const tabs = this.joins.get(key) ?? new Set<string>()
        tabs.add(tab)
        this.joins.set(key, tabs)
      } else if (frame.t === `leave`) {
        const tabs = this.joins.get(key)
        tabs?.delete(tab)
        if (tabs && tabs.size > 0) return false
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
    for (const [key, tabs] of this.joins) {
      if (!tabs.delete(tab) || tabs.size > 0) continue
      this.joins.delete(key)
      const [to, sessionId] = key.split(` `) as [string, string]
      void this.node.sendChat(to, encodeSteerEnvelope(sessionId, [{ t: `leave` }])).catch(() => {})
    }
  }

  carrier(tab: string = SELF): SteerCarrier {
    let pins = 0
    return {
      send: (to, body) => this.send(tab, to, body),
      allow: (id) => this.allow(tab, `l${++pins}`, id),
      subscribe: (listener) => this.node.onChat(listener),
    }
  }
}

/** The node tab: serves the others; returns its own carrier and the stop. */
export function serveSteerBridge(node: SteerNode, channel: BridgeChannel) {
  const router = new SteerRouter(node)
  const offChat = node.onChat((from, body) => {
    if (parseSteerEnvelope(body)) channel.postMessage({ op: `chat`, from, body })
  })
  channel.onmessage = ({ data }) => {
    switch (data.op) {
      case `who`:
        return channel.postMessage({ op: `here` })
      case `send`:
        return void router.send(data.tab, data.to, data.body).then(
          () => channel.postMessage({ op: `sent`, tab: data.tab, req: data.req }),
          (error: unknown) =>
            channel.postMessage({ op: `sent`, tab: data.tab, req: data.req, error: String(error) })
        )
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
        for (const listener of this.listeners) listener(data.from, data.body)
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
      send: (to, body) => this.send(to, body),
      allow: (id) => {
        const pin = ++this.pins
        this.channel.postMessage({ op: `allow`, tab: this.tab, pin, id })
        return () => this.channel.postMessage({ op: `release`, tab: this.tab, pin })
      },
      subscribe: (listener) => {
        this.listeners.add(listener)
        return () => void this.listeners.delete(listener)
      },
    }
  }

  /** This tab is going away (pagehide). */
  gone() {
    this.channel.postMessage({ op: `gone`, tab: this.tab })
  }

  private send(to: string, body: Uint8Array): Promise<void> {
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
      this.channel.postMessage({ op: `send`, tab: this.tab, req, to, body })
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

/** The node here, else the bridge to the node's tab, else none (= the relay). */
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
