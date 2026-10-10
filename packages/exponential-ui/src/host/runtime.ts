// VAPP-91: the framework-agnostic host runtime for TypeScript renderers.
// It owns the transport, the router, one SurfaceStore per surface, the
// source subscriptions, the function registry and the policy hooks; a
// renderer (React: `@exponential-at/ui-react` `<HostSurface>`) paints a
// store and hands its interactions back through `dispatch`.

import { writePointer } from "../dynamic"
import { reduceSurface } from "../reducer"
import { tryLoadTheme } from "../theme"
import type { ResolvedTheme } from "../theme-types"
import { BUILTIN_THEME_IDS, BUILTIN_THEMES, builtinTheme } from "../themes"
import type { ExtensionDef, FlatComponent, ReduceIssue, UiNode } from "../types"
import { RENDER_FAILED, actionMessage, errorMessage } from "./contract"
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
  /** Every problem the host meets (a package that failed validation, an
   *  error it answered a message with, an invalid surface theme). A
   *  transport-less host has no server to tell: this is where they land. */
  onIssue?: (issue: HostIssue) => void
}

/** One problem the host met; `host.issues` keeps the latest ones. */
export interface HostIssue {
  /** A host error code (`TEMPLATE_NOT_FOUND`…) or `PACKAGE_INVALID`. */
  code: string
  message: string
  surfaceId?: string
  /** A JSON pointer (into the package or the message). */
  path?: string
  packageId?: string
}

/** A package passed in `HostOptions.packages` that failed validation. */
export class PackageError extends Error {
  constructor(
    readonly packageId: string,
    readonly issues: PackageIssue[]
  ) {
    super(`exponential-ui: package ${packageId} is unusable: ${issues.map((i) => `${i.path || `/`} ${i.message}`).join(`; `)}`)
    this.name = `PackageError`
  }
}

const MAX_ISSUES = 100

/** A `createSurface.theme` (a built-in id or a theme JSON) → the resolved
 *  theme, or why it is unusable. */
export function resolveSurfaceTheme(input: unknown): { theme: ResolvedTheme | null; issues: string[] } {
  if (typeof input === `string`) {
    if (!BUILTIN_THEME_IDS.includes(input)) return { theme: null, issues: [`unknown built-in theme "${input}"; known: ${BUILTIN_THEME_IDS.join(`|`)}`] }
    return { theme: builtinTheme(input), issues: [] }
  }
  const { theme, issues } = tryLoadTheme(input, { themes: BUILTIN_THEMES })
  return { theme, issues: issues.map((i) => (i.path ? `${i.path}: ${i.message}` : i.message)) }
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
  /** The flat components, mutated in place while nothing holds them. */
  private list: FlatComponent[] = []
  /** `list` was handed out (`components`): the next update copies first. */
  private shared = false
  private positions = new Map<string, number>()
  data: Record<string, unknown> = {}
  /** The server's `createSurface.theme`, resolved (undefined = the
   *  renderer's own theme). */
  theme?: ResolvedTheme
  private reduced: { root: UiNode | null; issues: ReduceIssue[]; templates?: Record<string, UiNode> } | null = null
  private listeners = new Set<Listener>()
  version = 0

  constructor(surfaceId: string, catalogId: string, private extensions: () => readonly ExtensionDef[]) {
    this.surfaceId = surfaceId
    this.catalogId = catalogId
  }

  /** The flat components as a SNAPSHOT: an array handed out here is never
   *  mutated afterwards (the next update copies first), so a renderer that
   *  keeps it sees a stable value; a stream nobody reads stays linear. */
  get components(): readonly FlatComponent[] {
    this.shared = true
    return this.list
  }

  /** The flat component with this id (undefined = none). */
  component(id: string): FlatComponent | undefined {
    const at = this.positions.get(id)
    return at === undefined ? undefined : this.list[at]
  }

  get root(): UiNode | null {
    return this.reduce().root
  }

  get issues(): ReduceIssue[] {
    return this.reduce().issues
  }

  /** Round 2 §4: the reducer's lifted data templates (absent without any). */
  get templates(): Record<string, UiNode> | undefined {
    return this.reduce().templates
  }

  private reduce() {
    if (!this.reduced)
      this.reduced = this.list.length ? reduceSurface(this.list, { catalogId: this.catalogId, extensions: this.extensions() }) : { root: null, issues: [] }
    return this.reduced
  }

  /** A2UI: a later update replaces components BY ID and keeps the rest
   *  (an id → position map: a streamed surface costs linear, VAPP-103). */
  setComponents(components: readonly FlatComponent[]): void {
    if (this.shared) {
      this.list = this.list.slice()
      this.shared = false
    }
    for (const c of components) {
      const at = this.positions.get(c.id)
      if (at === undefined) {
        this.positions.set(c.id, this.list.length)
        this.list.push(c)
      } else this.list[at] = c
    }
    this.reduced = null
    this.notify()
  }

  /** Re-reduce on the next read (the extension set changed). */
  invalidate(): void {
    this.reduced = null
    this.notify()
  }

  /** Write the data model at `pointer` (`undefined` removes); a refused
   *  write (writePointer) changes nothing and returns the reason. */
  setData(pointer: string, value: unknown): string | undefined {
    const written = writePointer(this.data, pointer, value)
    if (written.error) return written.error
    this.data = written.data as Record<string, unknown>
    this.notify()
    return undefined
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

export class ExponentialHost {
  readonly router: HostRouter
  private stores = new Map<string, SurfaceStore>()
  private subscriptions = new Map<string, (() => void)[]>()
  /** Per surface: the FLAT component owning a failed node → that
   *  component as it was when it failed + the node ids already reported. */
  private paintErrors = new Map<string, Map<string, { component: FlatComponent | undefined; ids: Set<string> }>>()
  private listeners = new Set<Listener>()
  private functions: Record<string, HostFunction>
  private sources: SourceResolvers
  private extensions: ExtensionDef[]
  status: TransportStatus = `closed`
  statusDetail?: string
  /** The last UNSUPPORTED_CATALOG id seen (the catalog-update banner). */
  unsupportedCatalog?: string
  /** The latest problems (newest last, capped); a new array per change. */
  issues: readonly HostIssue[] = []

  constructor(private options: HostOptions = {}) {
    this.extensions = [...(options.extensions ?? [])]
    this.router = new HostRouter({ extensionIds: this.extensions.map((e) => e.id) })
    this.functions = { ...(options.functions ?? {}) }
    this.sources = { ...(options.sources ?? {}) }
    for (const pkg of options.packages ?? []) {
      const issues = this.installPackage(pkg)
      if (issues.length) throw new PackageError(typeof pkg?.id === `string` && pkg.id ? pkg.id : `?`, issues)
    }
  }

  /** False for a local-only host (packages, in-memory feeds): no
   *  `host_offline` state to show. */
  get hasTransport(): boolean {
    return this.options.transport !== undefined
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
    for (const store of this.stores.values()) store.invalidate()
  }

  registerFunction(name: string, fn: HostFunction): void {
    this.functions[name] = fn
  }

  registerSource(scheme: string, resolver: SourceResolvers[string]): void {
    this.sources[scheme.toLowerCase()] = resolver
  }

  /** Install a package; its validation issues are returned AND reported
   *  (`issues`, `onIssue`). A package with issues is not installed. */
  installPackage(pkg: VappPackage): PackageIssue[] {
    const issues = this.router.installPackage(pkg)
    const packageId = typeof pkg?.id === `string` ? pkg.id : undefined
    for (const i of issues) this.report({ code: `PACKAGE_INVALID`, message: i.message, path: i.path, ...(packageId ? { packageId } : {}) })
    return issues
  }

  private report(issue: HostIssue): void {
    const next = [...this.issues, issue]
    this.issues = next.length > MAX_ISSUES ? next.slice(next.length - MAX_ISSUES) : next
    this.options.onIssue?.(issue)
    this.notify()
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
    if (`error` in message) {
      const e = message.error
      this.report({ code: e.code, message: e.message, ...(e.surfaceId ? { surfaceId: e.surfaceId } : {}), ...(e.path !== undefined ? { path: e.path } : {}) })
    }
  }

  private perform(op: HostOp): void {
    this.options.onOp?.(op)
    switch (op.op) {
      case `create`: {
        this.unbind(op.surfaceId)
        this.forgetPaintErrors(op.surfaceId)
        const store = new SurfaceStore(op.surfaceId, op.catalogId, () => this.extensions)
        store.sendDataModel = op.sendDataModel === true
        store.packageId = this.router.surface(op.surfaceId)?.packageId
        if (op.theme !== undefined && op.theme !== null) {
          const { theme, issues } = resolveSurfaceTheme(op.theme)
          if (theme) store.theme = theme
          else this.send(errorMessage(`VALIDATION_FAILED`, op.surfaceId, `createSurface.theme is unusable: ${issues.join(`; `)}`, `/createSurface/theme`))
        }
        this.stores.set(op.surfaceId, store)
        this.notify()
        return
      }
      case `components`: {
        // An updated component's failures are forgotten (its props
        // changed: it may paint now, or fail anew); the rest stay reported.
        const failed = this.paintErrors.get(op.surfaceId)
        if (failed) for (const c of op.components) failed.delete(c.id)
        this.stores.get(op.surfaceId)?.setComponents(op.components)
        return
      }
      case `data`: {
        const error = this.stores.get(op.surfaceId)?.setData(op.path, op.value)
        if (error) this.send(errorMessage(`VALIDATION_FAILED`, op.surfaceId, error, op.path || `/`))
        return
      }
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
        this.forgetPaintErrors(op.surfaceId)
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
    let consentError: string | undefined
    if (decision === `ask`) {
      // A throwing/rejecting consent hook is a deny, never a rejected call
      // (the painter's pending control must settle).
      let ok = false
      try {
        ok = (await this.options.policy?.onFunctionCall?.(call)) === true
      } catch (e) {
        consentError = e instanceof Error ? e.message : String(e)
      }
      if (!ok) decision = `deny`
    }
    if (decision === `deny`) {
      this.send(errorMessage(`FUNCTION_DENIED`, call.surfaceId, consentError === undefined ? `${call.name} was not allowed` : `${call.name} was not allowed: the consent hook failed: ${consentError}`))
      return consentError === undefined ? { decision } : { decision, error: consentError }
    }
    const fn = this.functions[call.name]
    if (!fn) return { decision: `allow` }
    try {
      return { decision: `allow`, result: await fn(call.args, call) }
    } catch (e) {
      return { decision: `allow`, error: e instanceof Error ? e.message : String(e) }
    }
  }

  /** The URL policy every href passes (relative urls against the urls' or
   *  the media `baseUrl`). */
  urlPolicy(): UrlPolicy {
    return { baseUrl: this.options.policy?.media?.baseUrl, ...this.options.policy?.urls }
  }

  /** openUrl / Link through the URL policy. True when it opened. */
  openUrl(url: string): boolean {
    const d = decideUrl(this.urlPolicy(), url)
    if (!d.allowed || !d.url) return false
    const open = this.options.policy?.openUrl ?? ((u: string) => globalThis.window?.open(u, `_blank`, `noopener`))
    open(d.url)
    return true
  }

  /** A renderer's painter for `componentId` failed. Reported ONCE per
   *  surface + component id (whatever the message) as an A2UI
   *  `RENDER_FAILED` error (and so a host issue), and not again until that
   *  component's props change (an `updateComponents` naming it, or the
   *  surface re-created). A part or template-instance id counts under the
   *  flat component it belongs to (`card.body` → `card`), so the record is
   *  bounded by the surface's components. */
  paintError(error: { surfaceId: string; componentId: string; message: string }): void {
    const store = this.stores.get(error.surfaceId)
    let owner = error.componentId
    while (store && !store.component(owner) && owner.includes(`.`)) owner = owner.slice(0, owner.lastIndexOf(`.`))
    const current = store?.component(owner)
    let failed = this.paintErrors.get(error.surfaceId)
    if (!failed) this.paintErrors.set(error.surfaceId, (failed = new Map()))
    let entry = failed.get(owner)
    if (!entry || entry.component !== current) failed.set(owner, (entry = { component: current, ids: new Set() }))
    if (entry.ids.has(error.componentId)) return
    entry.ids.add(error.componentId)
    this.send(errorMessage(RENDER_FAILED, error.surfaceId, error.message, `/components/${error.componentId}`))
  }

  private forgetPaintErrors(surfaceId: string): void {
    this.paintErrors.delete(surfaceId)
  }

  /** The media policy every src passes (`policy.media`). */
  mediaOptions(): MediaOptions | undefined {
    return this.options.policy?.media
  }

  /** The image loader's request (absolute url + auth headers). */
  mediaRequest(url: string): MediaRequest | null {
    return mediaRequest(url, this.options.policy?.media)
  }
}
