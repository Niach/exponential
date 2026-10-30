// EXP-1457 step 1 / VAPP-37: this browser install's eNet node. After sign-in
// the app lazily loads exponential-net-wasm (never in the initial bundle),
// starts a RELAY-ONLY node on the identity from identity-store.ts, registers
// it (`installs.register`, on every start) and keeps the node's Access
// allowlist = the endpoint ids `installs.list` gives for every team the user
// is in (on start, every 10 minutes, on team change). Sign-out stops the
// node; an explicit sign-out also revokes this install and forgets its
// identity. VAPP-33: the node's chat messages (steer over eNet) reach
// `onChat` listeners; `pin` keeps a peer allowed past the team lists.

import { buildRegistration, type InstallCrypto, type InstallRegistration } from "./install-registration"
import { forgetIdentity, loadOrCreateIdentity, type IdentityStore } from "./identity-store"

export const ENET_RELAY_URL = `https://relay.exponential.at`
export const ALLOWLIST_REFRESH_MS = 10 * 60 * 1000

/** One node event (crates/exponential-net-wasm `event_js`); only chat is read. */
export type EnetEvent = { kind: `chat`; from: string; id: Uint8Array; timestampMs: number; body: Uint8Array } | { kind: string }

/** The slice of a running exponential-net-wasm `WasmNode` this module uses. */
export type EnetNode = {
  readonly endpointId: string
  allowPeer: (id: string) => void
  revokePeer: (id: string) => void
  /** Calls `callback` for every event until shutdown (one consumer per node). */
  onEvent: (callback: (event: EnetEvent) => void) => void
  /** Resolves once the peer acknowledged the message. */
  sendChat: (to: string, body: Uint8Array) => Promise<unknown>
  shutdown: () => Promise<void>
}

export type ChatListener = (from: string, body: Uint8Array) => void

/** The wasm module, once loaded. */
export type EnetModule = InstallCrypto & {
  generateIdentity: () => Uint8Array
  startNode: (relayUrl: string, identity: Uint8Array) => Promise<EnetNode>
}

export type InstallsApi = {
  register: (input: InstallRegistration) => Promise<{ installId: string }>
  list: (teamId: string) => Promise<Array<{ endpointId: string }>>
  revoke: (installId: string) => Promise<unknown>
}

export type WebInstallDeps = {
  loadEnet: () => Promise<EnetModule>
  api: InstallsApi
  store?: IdentityStore
  relayUrl?: string
}

/** One account's install in this tab. `start` once, `stop` once. */
export class WebInstall {
  private node: EnetNode | undefined
  private installId: string | undefined
  private teamIds: string[] = []
  private allowed = new Set<string>()
  private pins = new Map<string, number>()
  private chatListeners = new Set<ChatListener>()
  private timer: ReturnType<typeof setInterval> | undefined
  private refreshing: Promise<void> = Promise.resolve()
  private stopped = false

  constructor(
    private readonly userId: string,
    private readonly deps: WebInstallDeps
  ) {}

  get endpointId() {
    return this.node?.endpointId
  }

  get allowlist(): ReadonlySet<string> {
    return this.allowed
  }

  /** Starts the node and registers; teams come from `setTeams`. */
  async start(): Promise<void> {
    const enet = await this.deps.loadEnet()
    const identity = await loadOrCreateIdentity(this.userId, enet.generateIdentity, this.deps.store)
    if (this.stopped) return
    const node = await enet.startNode(this.deps.relayUrl ?? ENET_RELAY_URL, identity)
    if (this.stopped) {
      await node.shutdown().catch(() => {})
      return
    }
    this.node = node
    node.onEvent((event) => {
      if (event.kind !== `chat`) return
      const { from, body } = event as Extract<EnetEvent, { kind: `chat` }>
      for (const listener of this.chatListeners) listener(from, body)
    })
    const registration = buildRegistration({
      userId: this.userId,
      identity,
      endpointId: node.endpointId,
      crypto: enet,
    })
    identity.fill(0)
    const { installId } = await this.deps.api.register(registration)
    this.installId = installId
    if (this.stopped) return
    await this.refresh()
    this.timer = setInterval(() => void this.refresh().catch(() => {}), ALLOWLIST_REFRESH_MS)
  }

  /** Whether the node is up (chat can go out). */
  get running(): boolean {
    return this.node !== undefined && !this.stopped
  }

  onChat(listener: ChatListener): () => void {
    this.chatListeners.add(listener)
    return () => void this.chatListeners.delete(listener)
  }

  async sendChat(to: string, body: Uint8Array): Promise<void> {
    if (!this.node || this.stopped) throw new Error(`eNet node is not running`)
    await this.node.sendChat(to, body)
  }

  /** Allows `id` until the release runs, whatever the team lists say. */
  pin(id: string): () => void {
    this.pins.set(id, (this.pins.get(id) ?? 0) + 1)
    this.node?.allowPeer(id)
    let released = false
    return () => {
      if (released) return
      released = true
      const left = (this.pins.get(id) ?? 1) - 1
      if (left > 0) return void this.pins.set(id, left)
      this.pins.delete(id)
      if (!this.allowed.has(id)) this.node?.revokePeer(id)
    }
  }

  /** The user's teams changed: reload the allowlist for the new set. */
  setTeams(teamIds: string[]): Promise<void> {
    this.teamIds = teamIds
    return this.node ? this.refresh() : Promise.resolve()
  }

  /** Allowlist = every non-revoked install of every team the user is in. */
  refresh(): Promise<void> {
    // Serialized, so an older answer never overwrites a newer one.
    this.refreshing = this.refreshing.catch(() => {}).then(() => this.applyAllowlist())
    return this.refreshing
  }

  private async applyAllowlist() {
    const node = this.node
    if (!node || this.stopped) return
    const lists = await Promise.all(this.teamIds.map((teamId) => this.deps.api.list(teamId)))
    if (this.stopped) return
    const next = new Set(lists.flat().map((row) => row.endpointId))
    next.delete(node.endpointId)
    for (const id of this.allowed) if (!next.has(id) && !this.pins.has(id)) node.revokePeer(id)
    for (const id of next) if (!this.allowed.has(id)) node.allowPeer(id)
    this.allowed = next
  }

  /** Stops the node; `revoke` (explicit sign-out) also retires this install. */
  async stop({ revoke = false }: { revoke?: boolean } = {}): Promise<void> {
    this.stopped = true
    clearInterval(this.timer)
    const node = this.node
    this.node = undefined
    await node?.shutdown().catch(() => {})
    // Without an install id (the start never got that far) the identity
    // stays: it is still registered, the next sign-in reuses it.
    if (!revoke || !this.installId) return
    await this.deps.api.revoke(this.installId)
    await forgetIdentity(this.userId, this.deps.store)
  }
}
