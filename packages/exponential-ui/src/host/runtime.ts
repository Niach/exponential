// VAPP-91: the framework-agnostic host runtime for TypeScript renderers.
// It owns the transport, the router, one SurfaceStore per surface, the
// source subscriptions, the function registry and the policy hooks; a
// renderer (React: `@exponential_app/eui-react` `<HostSurface>`) paints a
// store and hands its interactions back through `dispatch`.

import { evaluateCall, writePointer } from "../dynamic"
import type { Failure, RouteAnswer, RoutedCall } from "../dynamic"
import { declaredFunction } from "../functions"
import { newSurfaceBridge, surfaceUpdates } from "../surface-updates"
import type { SurfaceBridge, SurfaceRefusal } from "../surface-updates"
import { reduceSurface } from "../reducer"
import { tryLoadTheme } from "../theme"
import type { ResolvedTheme } from "../theme-types"
import { BUILTIN_THEME_IDS, BUILTIN_THEMES, builtinTheme } from "../themes"
import type { ExtensionDef, FlatComponent, ReduceIssue, UiNode } from "../types"
import { RENDER_FAILED, actionMessage, callAgentFunctionMessage, errorMessage, rendererFunctionResponse } from "./contract"
import type { ClientMessage, HostOp } from "./contract"
import { availableExtensions, rendererCapabilities } from "./package"
import type { PackageIssue, AppPackage } from "./package"
import { BUILTIN_FUNCTIONS, combineDecisions, decideFunction, decideRoute, decideUrl, mediaRequest, packagePolicy, routedCallKey } from "./policy"
import type { FunctionDecision, FunctionPolicy, MediaOptions, MediaRequest, UrlPolicy } from "./policy"
import { recordFormTargets } from "./record-macros"
import { HostRouter } from "./router"
import type { RouteOptions } from "./router"
import { hostParams, withHostParams } from "./host-params"
import { parseSource, sourceWithParams } from "./sources"
import { A2UI_VERSION } from "../catalog"
import { AGENT_FUNCTION_TIMEOUT_MS } from "../limits"
import { RECORDS_BATCH_FUNCTION, actionCallName, runAction } from "./actions"
import { NAVIGATE_FUNCTION, navigateTarget } from "./screens"
import type { ActionPlan } from "./actions"
import type { SourceResolvers } from "./sources"

export type TransportStatus = `connecting` | `open` | `closed` | `error`

/** Round 9 §1.3: what travels beside a message about a `sendDataModel`
 *  surface: `{a2uiRendererDataModel: {version, surfaces: {<id>: <data>}}}`. */
export type MessageMetadata = Record<string, unknown>

/** Messages in, renderer messages out. Adapters: `transports.ts`. */
export interface Transport {
  /** Start delivering messages; report the connection state. */
  start(receive: (message: unknown) => void, status: (status: TransportStatus, detail?: string) => void): void
  send(message: ClientMessage, metadata?: MessageMetadata): void | Promise<void>
  close(): void
}

export interface FunctionCallInfo {
  surfaceId: string
  componentId: string
  name: string
  args: Record<string, unknown>
  /** Round 9 §5.5: the call's own catalog (else the surface default). */
  catalogId?: string
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
  /** #EXP-1674: a `hostLinks` Text's segments (catalog/host.json urls); null = none.
   *  #VAPP-258: a segment's `chip` paints it as an inline Chip in a Markdown. */
  textLinks?: (text: string) => HostTextLink[] | null
  /** #VAPP-258 (catalog/host.json `markdown`): a Markdown block image's size. */
  imageSize?: (src: string) => { width: number; height: number } | null
  /** #VAPP-258: a press on a Markdown block image. */
  onImagePress?: (src: string) => void
  /** #VAPP-258: a Markdown one-link paragraph as a video, audio or file block. */
  attachment?: (href: string) => { kind: `video` | `audio` | `file`; name: string } | null
  /** #VAPP-258 step 5 (catalog/host.json `markdown.options`): how this
   *  host's Markdowns render (unset = the defaults). */
  markdownOptions?: { breaks?: `soft` | `hard`; density?: `document` | `chat`; selectable?: boolean; streaming?: boolean; imageMaxHeight?: number | null }
  media?: MediaOptions
}

/** One piece of a host's `textLinks` answer (catalog/host.json urls + `markdown`). */
export interface HostTextLink {
  text: string
  href?: string
  chip?: { kind: `issue` | `mention` | `entity`; label: string; icon?: string }
}

export interface HostOptions {
  transport?: Transport
  /** Round 9 §6.5: the transport's far end reads renderer → agent messages
   *  (the MCP carrier, WebSocket, SSE + POST): an untrusted surface has an
   *  agent, and an unregistered name routes to it. */
  agent?: boolean
  functions?: Record<string, HostFunction>
  sources?: SourceResolvers
  extensions?: readonly ExtensionDef[]
  packages?: readonly AppPackage[]
  policy?: HostPolicy
  /** Every client message that leaves (after the transport got it). */
  onSend?: (message: ClientMessage) => void
  /** Ops the host performs (tests, logging). */
  onOp?: (op: HostOp) => void
  /** VAPP-194: the member a package action runs as (its `me`). */
  me?: string | null
  /** VAPP-194: the clock (epoch ms) and the team zone (minutes east of UTC
   *  at an instant) a package action reads; default the wall clock in UTC. */
  now?: () => number
  zone?: (utcMs: number) => number
  /** VAPP-194: a package action that landed nothing (a false require's
   *  message, a refused write): a failed action, so the host's toast. */
  onActionFailed?: (failure: { surfaceId: string; action: string; message: string }) => void
  /** VAPP-218 (U1): open one of the surface's package screens (`navigate`,
   *  a press or an action's call step, checked by `navigateTarget`). The
   *  host's shell opens it (a deep link's same pair); unset = no shell, so
   *  a navigate is refused. True when it navigated. */
  navigate?: (target: { surfaceId: string; packageId: string; screen: string; params: Record<string, string> }) => boolean | void
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

/** Round 9 §8 / §5.2: an issue past this many per message is summed up. */
export const MAX_SENT_ISSUES = 32

/** A surface's theme (`metadata.extensions.at_exponential_theme`: a built-in id or a theme JSON) → the resolved
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

/** Round 9 §6.5: a routed REACTIVE call: in flight, or settled for the
 *  surface's lifetime. */
type ReactiveRoute = { state: `pending`; previous: unknown } | { state: `value`; value: unknown } | { state: `failure`; failure: Failure }

type Listener = () => void

/** One surface's state: the flat components, the data model, the reduced
 *  tree. Mutations notify subscribers; `root` is reduced lazily. */
export class SurfaceStore {
  readonly surfaceId: string
  /** The surface default (round 9 §5.5); undefined = none. */
  catalogId: string | undefined
  packageId?: string
  sendDataModel = false
  /** The flat components, mutated in place while nothing holds them. */
  private list: FlatComponent[] = []
  /** `list` was handed out (`components`): the next update copies first. */
  private shared = false
  private positions = new Map<string, number>()
  data: Record<string, unknown> = {}
  /** The surface's `metadata.extensions.at_exponential_theme`, resolved
   *  (undefined = the renderer's own theme). */
  theme?: ResolvedTheme
  /** Round 9 §5.1: `createSurface.metadata`. */
  metadata?: { extensions?: Record<string, unknown> }
  /** Round 9 §1.3: `<surfaceId>:<n>`. */
  private callCounter = 0
  /** §1.5: the bare path's stream state. */
  private bridge: SurfaceBridge
  /** Round 9 §6.5: the routed reactive calls, by key. */
  routes = new Map<string, ReactiveRoute>()
  /** Round 9 §5.2: what was sent already: (code, path, message). */
  sentIssues = new Set<string>()
  private reduced: { root: UiNode | null; issues: ReduceIssue[]; templates?: Record<string, UiNode> } | null = null
  private listeners = new Set<Listener>()
  version = 0
  /** #VAPP-288: bumped by every PROGRAMMATIC write (`assign`: an action's
   *  `set` step); a focused field adopts a value that came with a move. */
  writeEpoch = 0

  constructor(surfaceId: string, catalogId: string | undefined, private extensions: () => readonly ExtensionDef[]) {
    this.surfaceId = surfaceId
    this.catalogId = catalogId
    this.bridge = newSurfaceBridge(catalogId)
  }

  /** The next `callAgentFunction` id. */
  nextCallId(): string {
    this.callCounter += 1
    return `${this.surfaceId}:${this.callCounter}`
  }

  /** Round 9 §1.5: a message straight into this surface (no router): the
   *  version check with the 0.9 bridge, the entry check per component.
   *  Answers what was refused. */
  apply(message: unknown): SurfaceRefusal[] {
    const { updates, refused } = surfaceUpdates(message, this.bridge, { extensions: this.extensions() })
    for (const u of updates) {
      if (u.surfaceId !== this.surfaceId) continue
      if (u.kind === `create`) {
        this.catalogId = u.catalogId
        this.list = []
        this.shared = false
        this.positions = new Map()
        this.data = u.dataModel ?? {}
        this.metadata = u.metadata
        this.routes = new Map()
        this.sentIssues = new Set()
        this.reduced = null
        if (u.components.length) this.setComponents(u.components)
        else this.notify()
      } else if (u.kind === `components`) {
        if (u.components.length) this.setComponents(u.components)
      } else if (u.kind === `data`) {
        const error = this.setData(u.path, u.value)
        if (error) refused.push({ message: error, path: u.path || `/` })
      } else {
        this.list = []
        this.positions = new Map()
        this.reduced = null
        this.notify()
      }
    }
    return refused
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

  /** #VAPP-288: a PROGRAMMATIC write (an action's `set` step): `setData`
   *  that also bumps `writeEpoch` (one notify), so a focused field bound to
   *  `pointer` adopts it. Host data writes (updateDataModel, a bind, a
   *  record form) go through `setData` and stay under the echo rule. */
  assign(pointer: string, value: unknown): string | undefined {
    const written = writePointer(this.data, pointer, value)
    if (written.error) return written.error
    this.data = written.data as Record<string, unknown>
    this.writeEpoch += 1
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
  /** Round 9 §6.5: `callAgentFunction`s waiting for their response. */
  private pendingCalls = new Map<string, { settle: (answer: RouteAnswer) => void; timer: ReturnType<typeof setTimeout> }>()
  /** Per surface, one entry per bound path (VAPP-122: a re-bind of a path
   *  cancels the earlier subscription at that path). */
  private subscriptions = new Map<string, { path: string; cancel: () => void }[]>()
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
    this.router = new HostRouter({ extensionIds: this.extensions.map((e) => e.id), extensions: availableExtensions(this.extensions) })
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

  /** Round 9 §10: `inline` = the registered extensions' 1.0 catalog files. */
  rendererCapabilities(inline?: readonly unknown[]) {
    return rendererCapabilities(this.extensions.map((e) => e.id), inline ? { inline } : {})
  }

  get extensionDefs(): readonly ExtensionDef[] {
    return this.extensions
  }

  registerExtension(ext: ExtensionDef): void {
    if (this.extensions.some((e) => e.id === ext.id)) return
    this.extensions.push(ext)
    this.router.registerExtension(ext.id, availableExtensions([ext])[0])
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
  installPackage(pkg: AppPackage): PackageIssue[] {
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

  /** One agent message (a transport calls this; tests and in-memory hosts
   *  may call it directly). UNTRUSTED unless `options.trusted`: only host
   *  code passes it (round 9 §2; `applyTemplate()`, `bind()`). */
  receive(message: unknown, options: RouteOptions = {}): HostOp[] {
    // VAPP-206: `${today}`, `${now}`, `${month}` at this bind, in the team zone.
    const ops = this.router.route(withHostParams(message, hostParams(this.options.now?.(), this.options.zone)), options)
    for (const op of ops) this.perform(op)
    return ops
  }

  /** Round 9 §2: a package template onto a (new) surface: host code, routed
   *  trusted. `surfaceId` must be fresh (`mintSurfaceId` to re-open one). */
  applyTemplate(input: { surfaceId: string; templateId: string; packageId?: string; data?: unknown }): HostOp[] {
    const { surfaceId, templateId, packageId, data } = input
    return this.receive({ version: A2UI_VERSION, applyTemplate: { surfaceId, templateId, ...(packageId === undefined ? {} : { packageId }), ...(data === undefined ? {} : { data }) } }, { trusted: true })
  }

  /** Round 9 §1.4: a fresh id to re-open a surface under. */
  mintSurfaceId(base: string): string {
    return this.router.mintSurfaceId(base)
  }

  /** §1.4's navigation: a surface id that is free, or a minted one; the
   *  caller creates it, then deletes the old surface. */
  freshSurfaceId(base: string): string {
    return this.router.wasCreated(base) ? this.router.mintSurfaceId(base) : base
  }

  send(message: ClientMessage): void {
    const metadata = this.dataModelMetadata(message)
    void this.options.transport?.send(message, metadata)
    this.options.onSend?.(message)
    if (`error` in message) {
      const e = message.error
      this.report({ code: e.code, message: e.message, ...(e.surfaceId ? { surfaceId: e.surfaceId } : {}), ...(e.path !== undefined ? { path: e.path } : {}) })
    }
  }

  /** Round 9 §1.3: the metadata beside a message about a `sendDataModel` surface. */
  private dataModelMetadata(message: ClientMessage): MessageMetadata | undefined {
    const about = `action` in message ? message.action.surfaceId : `callAgentFunction` in message ? message.callAgentFunction.surfaceId : `error` in message ? message.error.surfaceId : undefined
    const store = about === undefined ? undefined : this.stores.get(about)
    if (!store?.sendDataModel) return undefined
    return { a2uiRendererDataModel: { version: A2UI_VERSION, surfaces: { [store.surfaceId]: store.data } } }
  }

  /** Round 9 §5.2 + §18.3: the surface's coded reducer issues, each once for
   *  its lifetime, at most `MAX_SENT_ISSUES` per message. */
  private sendIssues(store: SurfaceStore): void {
    let sent = 0
    let dropped = 0
    for (const issue of store.issues) {
      if (!issue.code) continue
      const key = `${issue.code}\u0000${issue.path}\u0000${issue.message}`
      if (store.sentIssues.has(key)) continue
      if (sent >= MAX_SENT_ISSUES) {
        dropped += 1
        continue
      }
      store.sentIssues.add(key)
      sent += 1
      this.send(errorMessage(issue.code as never, store.surfaceId, issue.message, issue.path))
    }
    if (dropped > 0) this.send(errorMessage(`VALIDATION_FAILED`, store.surfaceId, `${dropped} more issues were dropped`, `/`))
  }

  /** Round 9 §6.8: a reactive evaluation failed; sent once (like a reducer issue). */
  reportFailure(surfaceId: string, failure: Failure, path?: string): void {
    const store = this.stores.get(surfaceId)
    if (!store) return
    const key = `${failure.code}\u0000${failure.functionCallId ?? path ?? ``}\u0000${failure.message}`
    if (store.sentIssues.has(key)) return
    store.sentIssues.add(key)
    this.send(errorMessage(failure.code, failure.functionCallId === undefined ? { surfaceId } : { functionCallId: failure.functionCallId }, failure.message, failure.functionCallId === undefined ? path : undefined))
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
        if (op.metadata) store.metadata = op.metadata
        if (op.theme !== undefined && op.theme !== null) {
          const { theme, issues } = resolveSurfaceTheme(op.theme)
          if (theme) store.theme = theme
          else this.send(errorMessage(`VALIDATION_FAILED`, op.surfaceId, `at_exponential_theme is unusable: ${issues.join(`; `)}`, `/createSurface/metadata/extensions/at_exponential_theme`))
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
        const store = this.stores.get(op.surfaceId)
        store?.setComponents(op.components)
        if (store && (this.options.transport || this.options.agent)) this.sendIssues(store)
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
        const earlier = this.subscriptions.get(op.surfaceId) ?? []
        for (const sub of earlier) if (sub.path === op.path) sub.cancel()
        const kept = earlier.filter((sub) => sub.path !== op.path)
        const cancel = resolver(source, (value) => this.stores.get(op.surfaceId)?.setData(op.path, value))
        this.subscriptions.set(op.surfaceId, cancel ? [...kept, { path: op.path, cancel }] : kept)
        return
      }
      case `delete`:
        this.unbind(op.surfaceId)
        this.forgetPaintErrors(op.surfaceId)
        this.stores.delete(op.surfaceId)
        this.notify()
        return
      case `call`:
        this.answerCall(op)
        return
      case `respond`:
        this.settleCall(op)
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
    for (const sub of this.subscriptions.get(surfaceId) ?? []) sub.cancel()
    this.subscriptions.delete(surfaceId)
  }

  // --- interactions out ---------------------------------------------------

  /** A component's event: the A2UI 1.0 `action` message (`componentId` =
   *  the painted id; `userMessage` = the Action's resolved one). */
  action(event: { surfaceId: string; componentId: string; name: string; context?: Record<string, unknown>; payload?: Record<string, unknown>; userMessage?: string; timestamp?: string }): ClientMessage {
    const message = actionMessage({
      name: event.name,
      surfaceId: event.surfaceId,
      sourceComponentId: event.componentId,
      timestamp: event.timestamp ?? new Date().toISOString(),
      context: event.context ?? {},
      payload: event.payload,
      ...(event.userMessage === undefined ? {} : { userMessage: event.userMessage }),
    })
    this.send(message)
    return message
  }

  /** Round 9 §6.9: run a callRendererFunction that passed the router's
   *  checks; a throw (or a failing evaluation) answers EXECUTION_ERROR. */
  private answerCall(op: Extract<HostOp, { op: `call` }>): void {
    const data = op.surfaceId === undefined ? {} : (this.stores.get(op.surfaceId)?.data ?? {})
    const { value, failure } = evaluateCall({ "@call": op.name, catalogId: op.catalogId, args: op.args }, data, { catalogId: op.catalogId, available: this.router.availableCatalogIds })
    this.send(rendererFunctionResponse(op.functionCallId, failure ? { error: { code: `EXECUTION_ERROR`, message: failure.message } } : { value }))
  }

  /** Round 9 §10: an agentFunctionResponse settles its pending call; an
   *  unknown or settled id is a host issue, never sent. */
  private settleCall(op: Extract<HostOp, { op: `respond` }>): void {
    const pending = this.pendingCalls.get(op.functionCallId)
    if (!pending) {
      this.report({ code: `INVALID_MESSAGE`, message: `no pending call ${op.functionCallId}` })
      return
    }
    this.pendingCalls.delete(op.functionCallId)
    clearTimeout(pending.timer)
    pending.settle(`error` in op ? { failure: { code: `EXPRESSION_ERROR`, message: op.error.message, functionCallId: op.functionCallId } } : { value: op.value })
  }

  /** Round 9 §6.5: does the surface have an agent? */
  hasAgent(surfaceId: string): boolean {
    return this.options.agent === true && this.router.surface(surfaceId)?.agent === true
  }

  /** Round 9 §1.3 + §6.5: one `callAgentFunction`, its answer or the timeout. */
  private callAgent(store: SurfaceStore, name: string, catalogId: string, args: Record<string, unknown>): Promise<RouteAnswer> {
    const functionCallId = store.nextCallId()
    const defaults: Record<string, unknown> = {}
    for (const [k, schema] of Object.entries(declaredFunction(catalogId, name)?.args ?? {}))
      if (typeof schema === `object` && schema !== null && `default` in schema && !(k in args)) defaults[k] = (schema as { default: unknown }).default
    return new Promise((settle) => {
      const timer = setTimeout(() => {
        this.pendingCalls.delete(functionCallId)
        settle({ failure: { code: `EXPRESSION_ERROR`, message: `${name} timed out`, functionCallId } })
      }, AGENT_FUNCTION_TIMEOUT_MS)
      this.pendingCalls.set(functionCallId, { settle, timer })
      this.send(callAgentFunctionMessage(store.surfaceId, functionCallId, name, catalogId, { ...defaults, ...args }))
    })
  }

  /** Round 9 §6.5: `ResolveOptions.route` for one surface's REACTIVE
   *  positions (`decideRoute`). Identical calls share one in-flight call and
   *  a settled answer stays for the surface's lifetime; while pending the
   *  position reads its previous value, else null. */
  reactiveRoute(surfaceId: string): (call: RoutedCall) => RouteAnswer {
    return (call) => {
      const store = this.stores.get(surfaceId)
      if (!store) return { failure: { code: `UNKNOWN_FUNCTION`, message: `no function ${call.name}` } }
      const key = routedCallKey(call.catalogId, call.name, call.args)
      const known = store.routes.get(key)
      if (known) return known.state === `value` ? { value: known.value } : known.state === `failure` ? { failure: known.failure } : { value: known.previous }
      const registered = Object.prototype.hasOwnProperty.call(this.functions, call.name)
      const route = decideRoute({ name: call.name, catalogId: call.catalogId, available: this.router.availableCatalogIds, position: `reactive`, policy: this.options.policy?.functions, packageFunctions: this.packageOf(store)?.functions ?? (store.packageId ? [] : undefined), registered, agent: this.hasAgent(surfaceId) })
      if (route.route === `fail`) return { failure: { code: route.code, message: route.message } }
      store.routes.set(key, { state: `pending`, previous: null })
      void this.settleReactive(store, key, call, route.route === `ask`, registered)
      return { value: null }
    }
  }

  private packageOf(store: SurfaceStore): AppPackage | undefined {
    return store.packageId ? this.router.package(store.packageId) : undefined
  }

  /** The pending reactive call: consent, then the local function or the agent. */
  private async settleReactive(store: SurfaceStore, key: string, call: RoutedCall, ask: boolean, registered: boolean): Promise<void> {
    const info: FunctionCallInfo = { surfaceId: store.surfaceId, componentId: ``, name: call.name, args: call.args, catalogId: call.catalogId }
    let answer: RouteAnswer
    try {
      const allowed = !ask || (await this.options.policy?.onFunctionCall?.(info)) === true
      if (!allowed) answer = { failure: { code: `INVALID_FUNCTION_CALL`, message: `${call.name} was not allowed` } }
      else if (registered) answer = { value: (await this.functions[call.name]!(call.args, info)) ?? null }
      else answer = await this.callAgent(store, call.name, call.catalogId, call.args)
    } catch (e) {
      answer = { failure: { code: `EXPRESSION_ERROR`, message: e instanceof Error ? e.message : String(e) } }
    }
    store.routes.set(key, `failure` in answer ? { state: `failure`, failure: answer.failure } : { state: `value`, value: answer.value })
    if (`failure` in answer) this.reportFailure(store.surfaceId, answer.failure)
    store.notify()
  }

  /** The policy decision for a call, before any consent hook: `not_found`
   *  when nothing can answer it. */
  decide(surfaceId: string, name: string): FunctionDecision {
    // Own names only: `constructor` is not a registered function (VAPP-107).
    const registered = Object.prototype.hasOwnProperty.call(this.functions, name)
    const decision = this.decidePolicy(surfaceId, name)
    if (decision !== `allow` && decision !== `ask`) return decision
    return registered || BUILTIN_FUNCTIONS.includes(name) || this.hasAgent(surfaceId) ? decision : `not_found`
  }

  /** Round 9 §6.5: the policy IN FRONT: the host's and the package's
   *  decision whether or not the name is registered. */
  private decidePolicy(surfaceId: string, name: string): FunctionDecision {
    let decision = decideFunction(this.options.policy?.functions, name, true)
    const pkgId = this.stores.get(surfaceId)?.packageId
    const pkg = pkgId ? this.router.package(pkgId) : undefined
    if (pkg) decision = combineDecisions(decision, decideFunction(packagePolicy(pkg.functions), name, true))
    return decision
  }

  /** An action `functionCall` to a host function: gate, consent, run. A
   *  built-in other than openUrl and bind has no effect as an action. */
  async callFunction(call: FunctionCallInfo): Promise<FunctionOutcome> {
    const builtIn = this.runBuiltIn(call)
    if (builtIn) return builtIn
    if (call.name === NAVIGATE_FUNCTION) return this.navigate(call)
    const action = actionCallName(call.name)
    if (action !== null) return this.runPackageAction(call, action)
    const gate = await this.gate(call)
    if (gate.decision !== `allow`) return gate
    if (this.answersLocally(call.name)) {
      const outcome = await this.run(call)
      // Round 9 §6.8: a local function that threw stops the action.
      if (outcome.error !== undefined) this.send(errorMessage(`EXECUTION_ERROR`, call.surfaceId, outcome.error))
      return outcome
    }
    // Allowed, nothing local: the surface's agent, else UNKNOWN_FUNCTION.
    const store = this.stores.get(call.surfaceId)
    const catalogId = call.catalogId ?? store?.catalogId
    if (!store || catalogId === undefined || !this.hasAgent(call.surfaceId)) {
      this.send(errorMessage(`UNKNOWN_FUNCTION`, call.surfaceId, `no function ${call.name}`))
      return { decision: `not_found` }
    }
    // An ACTION call is never shared or cached: a new id every time.
    const answer = await this.callAgent(store, call.name, catalogId, call.args)
    if (`failure` in answer) {
      this.send(errorMessage(answer.failure.code, { functionCallId: answer.failure.functionCallId! }, answer.failure.message))
      return { decision: `allow`, error: answer.failure.message }
    }
    return { decision: `allow`, result: answer.value }
  }

  /** A registered host function, or a built-in catalog name (no effect as an action). */
  private answersLocally(name: string): boolean {
    return Object.prototype.hasOwnProperty.call(this.functions, name) || BUILTIN_FUNCTIONS.includes(name)
  }

  /** The policy gate and the consent hook: `allow`, or the refusal (sent
   *  as INVALID_FUNCTION_CALL). */
  private async gate(call: FunctionCallInfo): Promise<FunctionOutcome> {
    // VAPP-218: navigate is always allowed (checked when it runs).
    if (call.name === NAVIGATE_FUNCTION) return { decision: `allow` }
    let decision = this.decidePolicy(call.surfaceId, call.name)
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
      this.send(errorMessage(`INVALID_FUNCTION_CALL`, call.surfaceId, consentError === undefined ? `${call.name} was not allowed` : `${call.name} was not allowed: the consent hook failed: ${consentError}`))
      return consentError === undefined ? { decision } : { decision, error: consentError }
    }
    return { decision: `allow` }
  }

  /** VAPP-205: `openUrl` and `bind`, for a press and an action's `call`
   *  step alike. Null for any other name. */
  private runBuiltIn(call: FunctionCallInfo): FunctionOutcome | null {
    if (call.name === `openUrl`) {
      const url = typeof call.args.url === `string` ? call.args.url : ``
      return { decision: this.openUrl(url) ? `allow` : `deny` }
    }
    if (call.name === `bind`) return { decision: this.bind(call.surfaceId, call.args) ? `allow` : `deny` }
    return null
  }

  private async run(call: FunctionCallInfo): Promise<FunctionOutcome> {
    const builtIn = this.runBuiltIn(call)
    if (builtIn) return builtIn
    if (call.name === NAVIGATE_FUNCTION) return this.navigate(call)
    const fn = Object.prototype.hasOwnProperty.call(this.functions, call.name) ? this.functions[call.name] : undefined
    if (!fn) return { decision: `allow` }
    // VAPP-245: a record form's write: landed = its error clears and its
    // editor closes; refused inline = the message under the form.
    const form = recordFormTargets(call.args)
    const store = this.stores.get(call.surfaceId)
    try {
      const result = await fn(call.args, call)
      if (form.error) store?.setData(form.error, null)
      if (form.close) store?.setData(form.close, false)
      return { decision: `allow`, result }
    } catch (e) {
      // By name: an app may load the host entry apart from this module.
      if (e instanceof Error && e.name === `RecordRefusal` && form.error) store?.setData(form.error, e.message)
      return { decision: `allow`, error: e instanceof Error ? e.message : String(e) }
    }
  }

  /** VAPP-194: `actions_<name>` = the surface's package action, all or
   *  nothing: the plan over one snapshot (the args = params, the surface
   *  data, `me`, the clock), every host call's gate and consent, then the
   *  record writes as ONE `records_batch`, then the sets, then the calls.
   *  A failure before the commit lands nothing: EXECUTION_ERROR +
   *  `onActionFailed` (the toast), the outcome's `error`. */
  private async runPackageAction(call: FunctionCallInfo, action: string): Promise<FunctionOutcome> {
    const store = this.stores.get(call.surfaceId)
    const pkg = store?.packageId ? this.router.package(store.packageId) : undefined
    const failed = (message: string): FunctionOutcome => {
      this.send(errorMessage(`EXECUTION_ERROR`, call.surfaceId, message))
      this.options.onActionFailed?.({ surfaceId: call.surfaceId, action, message })
      return { decision: `allow`, error: message }
    }
    if (!store || !pkg) return failed(`no action ${action}`)
    // VAPP-271: a call step with a `result` runs when the plan reaches it
    // (before the commit); the action runs again over the SAME snapshot with
    // its answer as `results.<name>`.
    const ctx = { data: store.data, me: this.options.me ?? null, now: this.options.now?.(), offset: this.options.zone, results: {} as Record<string, unknown> }
    let result = runAction(pkg as unknown as Record<string, unknown>, action, call.args, ctx)
    while (`pending` in result) {
      const pending = { surfaceId: call.surfaceId, componentId: call.componentId, name: result.pending.name, args: result.pending.args }
      const gate = await this.gate(pending)
      if (gate.decision !== `allow`) return failed(gate.error ?? `${pending.name} was not allowed`)
      const outcome = await this.run(pending)
      if (outcome.error !== undefined) return failed(outcome.error)
      ctx.results = { ...ctx.results, [result.pending.result]: outcome.result ?? null }
      result = runAction(pkg as unknown as Record<string, unknown>, action, call.args, ctx)
    }
    if (`error` in result) return failed(result.error)
    const plan: ActionPlan = result.plan
    const calls = plan.calls.map((c) => ({ surfaceId: call.surfaceId, componentId: call.componentId, name: c.name, args: c.args }))
    for (const c of calls) {
      const gate = await this.gate(c)
      if (gate.decision !== `allow`) return failed(gate.error ?? `${c.name} was not allowed`)
    }
    let committed: unknown = { ids: [] }
    if (plan.writes.length) {
      const fn = Object.prototype.hasOwnProperty.call(this.functions, RECORDS_BATCH_FUNCTION) ? this.functions[RECORDS_BATCH_FUNCTION] : undefined
      if (!fn) return failed(`this host keeps no records`)
      try {
        committed = await fn({ writes: plan.writes }, { ...call, name: RECORDS_BATCH_FUNCTION, args: { writes: plan.writes } })
      } catch (e) {
        return failed(e instanceof Error ? e.message : String(e))
      }
    }
    for (const s of plan.sets) {
      const error = store.assign(s.path, s.value)
      if (error) this.send(errorMessage(`VALIDATION_FAILED`, call.surfaceId, error, s.path || `/`))
    }
    for (const c of calls) {
      const outcome = await this.run(c)
      const error = outcome.error ?? (outcome.decision === `allow` ? undefined : `${c.name} was not allowed`)
      if (error !== undefined) return { decision: `allow`, result: committed, error }
    }
    return { decision: `allow`, result: committed }
  }

  /** VAPP-218: `navigate {screen, params?}` → the host's shell, when the
   *  surface's package has that screen. A refusal = INVALID_FUNCTION_CALL. */
  navigate(call: FunctionCallInfo): FunctionOutcome {
    const store = this.stores.get(call.surfaceId)
    const pkg = store?.packageId ? this.router.package(store.packageId) : undefined
    const target = pkg ? navigateTarget(pkg, call.args) : { error: `navigate: the surface has no package` }
    const refuse = (message: string): FunctionOutcome => {
      this.send(errorMessage(`INVALID_FUNCTION_CALL`, call.surfaceId, message))
      return { decision: `deny`, error: message }
    }
    if (`error` in target) return refuse(target.error)
    if (!this.options.navigate) return refuse(`navigate: this host has no screens`)
    const ok = this.options.navigate({ surfaceId: call.surfaceId, packageId: pkg!.id, screen: target.screen, params: target.params })
    return ok === false ? refuse(`navigate: ${target.screen} was not opened`) : { decision: `allow` }
  }

  /** VAPP-122: the `bind` action: re-follow `path` from `source` with
   *  `params` appended (`sourceWithParams`), through the router like a
   *  server's bindDataModel (the surface's package allowlist applies; a
   *  refusal is the usual SOURCE_DENIED / VALIDATION_FAILED error). True
   *  when the bind op ran. */
  bind(surfaceId: string, args: Record<string, unknown>): boolean {
    const path = typeof args.path === `string` ? args.path : ``
    const source = typeof args.source === `string` ? args.source : ``
    if (!source || !this.stores.has(surfaceId)) return false
    const params = args.params && typeof args.params === `object` && !Array.isArray(args.params) ? (args.params as Record<string, unknown>) : undefined
    const ops = this.receive({ version: A2UI_VERSION, bindDataModel: { surfaceId, path, source: sourceWithParams(source, params) } }, { trusted: true })
    return ops.some((op) => op.op === `bind`)
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

  /** #EXP-1674: the policy's `textLinks` (the renderer polices each href). */
  textLinks(text: string): HostTextLink[] | null {
    return this.options.policy?.textLinks?.(text) ?? null
  }

  /** #VAPP-258: the policy's Markdown hooks (catalog/host.json `markdown`);
   *  undefined = the host set none. */
  markdownHooks(): Pick<HostPolicy, `imageSize` | `onImagePress` | `attachment` | `markdownOptions`> {
    const p = this.options.policy
    return { imageSize: p?.imageSize, onImagePress: p?.onImagePress, attachment: p?.attachment, markdownOptions: p?.markdownOptions }
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
