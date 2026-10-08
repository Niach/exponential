// VAPP-91: the framework-agnostic host runtime for TypeScript renderers.
// It owns the transport, the router, one SurfaceStore per surface, the
// source subscriptions, the function registry and the policy hooks; a
// renderer (React: `@exponential-at/ui-react` `<HostSurface>`) paints a
// store and hands its interactions back through `dispatch`.

import { reduceSurface } from "../reducer"
import type { ExtensionDef, FlatComponent, ReduceIssue, UiNode } from "../types"
import { actionMessage, errorMessage } from "./contract"
import type { ClientMessage, HostOp } from "./contract"
import { clientCapabilities } from "./package"
import type { PackageIssue, VappPackage } from "./package"
import { combineDecisions, decideFunction, decideUrl, mediaRequest, packagePolicy } from "./policy"
import type { FunctionDecision, FunctionPolicy, MediaOptions, MediaRequest, UrlPolicy } from "./policy"
import { HostRouter } from "./router"
import { parseSource } from "./sources"
import type { SourceResolvers } from "./sources"

export type TransportStatus = `connecting` | `open` | `closed` | `error`

/** Messages in, client messages out. Adapters: `transports.ts`. */
export interface Transport {
  /** Start delivering messages; report the connection state. */
  start(receive: (message: unknown) => void, status: (status: TransportStatus, detail?: string) => void): void
  send(message: ClientMessage): void | Promise<void>
  close(): void
}

export interface FunctionCallInfo {
  surfaceId: string
  componentId: string
  name: string
  args: Record<string, unknown>
}

export type HostFunction = (args: Record<string, unknown>, call: FunctionCallInfo) => unknown | Promise<unknown>

export interface HostPolicy {
  /** The declarative gate; `ask` decisions go to onFunctionCall. */
  functions?: FunctionPolicy
  /** The consent hook: true runs the call. Unset = `ask` is denied. */
  onFunctionCall?: (call: FunctionCallInfo) => boolean | Promise<boolean>
  urls?: UrlPolicy
  /** Opens an allowed url (default: `window.open(url, "_blank")`). */
  openUrl?: (url: string) => void
  media?: MediaOptions
}

export interface HostOptions {
  transport?: Transport
  functions?: Record<string, HostFunction>
  sources?: SourceResolvers
  extensions?: readonly ExtensionDef[]
  packages?: readonly VappPackage[]
  policy?: HostPolicy
  /** Every client message that leaves (after the transport got it). */
  onSend?: (message: ClientMessage) => void
  /** Ops the host performs (tests, logging). */
  onOp?: (op: HostOp) => void
}

export interface FunctionOutcome {
  decision: FunctionDecision
  result?: unknown
  error?: string
}

type Listener = () => void

/** One surface's state: the flat components, the data model, the reduced
 *  tree. Mutations notify subscribers; `root` is reduced lazily. */
export class SurfaceStore {
  readonly surfaceId: string
  catalogId: string
  packageId?: string
  sendDataModel = false
  components: FlatComponent[] = []
  data: Record<string, unknown> = {}
  private reduced: { root: UiNode | null; issues: ReduceIssue[] } | null = null
  private listeners = new Set<Listener>()
  version = 0

  constructor(surfaceId: string, catalogId: string, private extensions: () => readonly ExtensionDef[]) {
    this.surfaceId = surfaceId
    this.catalogId = catalogId
  }

  get root(): UiNode | null {
    return this.reduce().root
  }

  get issues(): ReduceIssue[] {
    return this.reduce().issues
  }

  private reduce() {
    if (!this.reduced)
      this.reduced = this.components.length ? reduceSurface(this.components, { catalogId: this.catalogId, extensions: this.extensions() }) : { root: null, issues: [] }
    return this.reduced
  }

  setComponents(components: readonly FlatComponent[]): void {
    const byId = new Map(this.components.map((c) => [c.id, c]))
    for (const c of components) byId.set(c.id, c)
    this.components = [...byId.values()]
    this.reduced = null
    this.notify()
  }

  setData(pointer: string, value: unknown): void {
    this.data = setPointerImmutable(this.data, pointer, value) as Record<string, unknown>
    this.notify()
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  notify(): void {
    this.version += 1
    for (const l of [...this.listeners]) l()
  }
}

function tokensOf(pointer: string): string[] {
  if (!pointer) return []
  return pointer
    .replace(/^\//, ``)
    .split(`/`)
    .map((t) => t.replace(/~1/g, `/`).replace(/~0/g, `~`))
}

/** The same pointer write every renderer's data model does (absent value =
 *  remove), returning a new object. */
export function setPointerImmutable(data: unknown, pointer: string, value: unknown): unknown {
  const tokens = tokensOf(pointer)
  if (!tokens.length) return value === undefined ? {} : value
  const put = (cur: unknown, i: number): unknown => {
    const token = tokens[i]!
    const last = i === tokens.length - 1
    const base: unknown = cur !== null && typeof cur === `object` ? cur : /^\d+$/.test(token) ? [] : {}
    if (Array.isArray(base)) {
      const next = [...base]
      const idx = token === `-` ? next.length : Number(token)
      if (last) {
        if (value === undefined) next.splice(idx, 1)
        else next[idx] = value
      } else next[idx] = put(next[idx], i + 1)
      return next
    }
    const next = { ...(base as Record<string, unknown>) }
    if (last) {
      if (value === undefined) delete next[token]
      else next[token] = value
    } else next[token] = put(next[token], i + 1)
    return next
  }
  return put(data, 0)
}

export class ExponentialHost {
  readonly router: HostRouter
  private stores = new Map<string, SurfaceStore>()
  private subscriptions = new Map<string, (() => void)[]>()
  private listeners = new Set<Listener>()
  private functions: Record<string, HostFunction>
  private sources: SourceResolvers
  private extensions: ExtensionDef[]
  status: TransportStatus = `closed`
  statusDetail?: string
  /** The last UNSUPPORTED_CATALOG id seen (the catalog-update banner). */
  unsupportedCatalog?: string

  constructor(private options: HostOptions = {}) {
    this.extensions = [...(options.extensions ?? [])]
    this.router = new HostRouter({ extensionIds: this.extensions.map((e) => e.id) })
    this.functions = { ...(options.functions ?? {}) }
    this.sources = { ...(options.sources ?? {}) }
    for (const pkg of options.packages ?? []) this.installPackage(pkg)
  }

  // --- negotiation + registration ----------------------------------------

  get supportedCatalogIds(): string[] {
    return this.router.supportedCatalogIds
  }

  clientCapabilities() {
    return clientCapabilities(this.extensions.map((e) => e.id))
  }

  get extensionDefs(): readonly ExtensionDef[] {
    return this.extensions
  }

  registerExtension(ext: ExtensionDef): void {
    if (this.extensions.some((e) => e.id === ext.id)) return
    this.extensions.push(ext)
    this.router.registerExtension(ext.id)
  }

  registerFunction(name: string, fn: HostFunction): void {
    this.functions[name] = fn
  }

  registerSource(scheme: string, resolver: SourceResolvers[string]): void {
    this.sources[scheme.toLowerCase()] = resolver
  }

  installPackage(pkg: VappPackage): PackageIssue[] {
    return this.router.installPackage(pkg)
  }

  // --- surfaces -----------------------------------------------------------

  surface(id: string): SurfaceStore | undefined {
    return this.stores.get(id)
  }

  surfaceIds(): string[] {
    return [...this.stores.keys()]
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  private notify(): void {
    for (const l of [...this.listeners]) l()
  }

  // --- transport ----------------------------------------------------------

  connect(): void {
    const t = this.options.transport
    if (!t) return
    t.start(
      (m) => this.receive(m),
      (status, detail) => {
        this.status = status
        this.statusDetail = detail
        this.notify()
      }
    )
  }

  close(): void {
    this.options.transport?.close()
    for (const id of this.surfaceIds()) this.perform({ op: `delete`, surfaceId: id })
  }

  /** One server message (a transport calls this; tests and in-memory hosts
   *  may call it directly). */
  receive(message: unknown): HostOp[] {
    const ops = this.router.route(message)
    for (const op of ops) this.perform(op)
    return ops
  }

  send(message: ClientMessage): void {
    void this.options.transport?.send(message)
    this.options.onSend?.(message)
  }

  private perform(op: HostOp): void {
    this.options.onOp?.(op)
    switch (op.op) {
      case `create`: {
        this.unbind(op.surfaceId)
        const store = new SurfaceStore(op.surfaceId, op.catalogId, () => this.extensions)
        store.sendDataModel = op.sendDataModel === true
        store.packageId = this.router.surface(op.surfaceId)?.packageId
        this.stores.set(op.surfaceId, store)
        this.notify()
        return
      }
      case `components`:
        this.stores.get(op.surfaceId)?.setComponents(op.components)
        return
      case `data`:
        this.stores.get(op.surfaceId)?.setData(op.path, op.value)
        return
      case `bind`: {
        const store = this.stores.get(op.surfaceId)
        const source = parseSource(op.source)
        const resolver = source ? this.sources[source.scheme] : undefined
        if (!store || !source) return
        if (!resolver) {
          this.send(errorMessage(`VALIDATION_FAILED`, op.surfaceId, `no resolver for the source scheme ${source.scheme}`, op.path || `/`))
          return
        }
        const cancel = resolver(source, (value) => this.stores.get(op.surfaceId)?.setData(op.path, value))
        if (cancel) this.subscriptions.set(op.surfaceId, [...(this.subscriptions.get(op.surfaceId) ?? []), cancel])
        return
      }
      case `delete`:
        this.unbind(op.surfaceId)
        this.stores.delete(op.surfaceId)
        this.notify()
        return
      case `send`: {
        const err = `error` in op.message ? op.message.error : undefined
        if (err?.code === `UNSUPPORTED_CATALOG`) {
          this.unsupportedCatalog = err.message.replace(/^catalog (\S+).*$/, `$1`)
          this.notify()
        }
        this.send(op.message)
        return
      }
    }
  }

  private unbind(surfaceId: string): void {
    for (const cancel of this.subscriptions.get(surfaceId) ?? []) cancel()
    this.subscriptions.delete(surfaceId)
  }

  // --- interactions out ---------------------------------------------------

  /** A component's server event: the A2UI client action message. */
  action(event: { surfaceId: string; componentId: string; name: string; context?: Record<string, unknown>; payload?: Record<string, unknown>; timestamp?: string }): ClientMessage {
    const message = actionMessage({
      name: event.name,
      surfaceId: event.surfaceId,
      sourceComponentId: event.componentId,
      timestamp: event.timestamp ?? new Date().toISOString(),
      context: event.context ?? {},
      payload: event.payload,
    })
    this.send(message)
    return message
  }

  /** The policy decision for a call, before any consent hook. */
  decide(surfaceId: string, name: string): FunctionDecision {
    const registered = name in this.functions
    let decision = decideFunction(this.options.policy?.functions, name, registered)
    const pkgId = this.stores.get(surfaceId)?.packageId
    const pkg = pkgId ? this.router.package(pkgId) : undefined
    if (pkg) decision = combineDecisions(decision, decideFunction(packagePolicy(pkg.functions), name, registered))
    return decision
  }

  /** An action `functionCall` to a host function: gate, consent, run. A
   *  built-in other than openUrl has no effect as an action. */
  async callFunction(call: FunctionCallInfo): Promise<FunctionOutcome> {
    if (call.name === `openUrl`) {
      const url = typeof call.args.url === `string` ? call.args.url : ``
      return { decision: this.openUrl(url) ? `allow` : `deny` }
    }
    let decision = this.decide(call.surfaceId, call.name)
    if (decision === `not_found`) {
      this.send(errorMessage(`FUNCTION_NOT_FOUND`, call.surfaceId, `no function ${call.name}`))
      return { decision }
    }
    if (decision === `ask`) {
      const ok = (await this.options.policy?.onFunctionCall?.(call)) === true
      if (!ok) decision = `deny`
    }
    if (decision === `deny`) {
      this.send(errorMessage(`FUNCTION_DENIED`, call.surfaceId, `${call.name} was not allowed`))
      return { decision }
    }
    const fn = this.functions[call.name]
    if (!fn) return { decision: `allow` }
    try {
      return { decision: `allow`, result: await fn(call.args, call) }
    } catch (e) {
      return { decision: `allow`, error: e instanceof Error ? e.message : String(e) }
    }
  }

  /** openUrl / Link through the URL policy. True when it opened. */
  openUrl(url: string): boolean {
    const d = decideUrl({ baseUrl: this.options.policy?.media?.baseUrl, ...this.options.policy?.urls }, url)
    if (!d.allowed || !d.url) return false
    const open = this.options.policy?.openUrl ?? ((u: string) => globalThis.window?.open(u, `_blank`, `noopener`))
    open(d.url)
    return true
  }

  /** The image loader's request (absolute url + auth headers). */
  mediaRequest(url: string): MediaRequest | null {
    return mediaRequest(url, this.options.policy?.media)
  }
}
