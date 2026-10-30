// FEED-130: `<exponential-surface>`, the Exponential UI custom element. ONE
// painter, two entry points: the element writes no DOM of its own, it mounts
// the Solid renderer's `<ExponentialSurface>` into itself (light DOM) with a
// props object of getters over signals, so an attribute or property change
// re-binds exactly what reads it. The element is `display: contents` (one
// shared `:where(<tag>)` rule `define()` installs, so any author rule wins):
// the surface's `.xui-surface` root is THE box, where React's div would be.
//
// Mounting: the first element attached in a task mounts at once; the others
// attached in the same task mount together in a microtask, inside ONE Solid
// batch (hundreds of fresh per-surface sheets measured once, not a style
// recalc + layout per element).
//
// Inputs: attributes (strings) and properties (any value), each one signal
// created in the constructor (a property set before the element is attached
// works; the constructor never touches attributes, parser-created elements
// forbid it). Outputs: the host plugin's callbacks AND DOM events (events.ts).
// Sources, in order: the `surface` property (a `createSurface` store), a
// host (the `host` property or one built from `src`, transport.ts), the
// store `apply()` creates, else the fixture `root`/`templates`/`data`.

import { Show, batch, createComponent, createEffect, createMemo, createRoot, createSignal, onCleanup, untrack, type Accessor, type JSX, type Setter } from "solid-js"
import { render } from "solid-js/web"
import type { ExponentialHost, ModeName, ScrollAlign, SurfaceCommand, ThemeIssue, TransportStatus, UiNode } from "@exponential_app/eui"
import { setPointer, type DataModel } from "@exponential_app/eui/web"
import { ExponentialSurface, createHostSurface, createSurface, extensionCatalogs, hostPlugin, px } from "@exponential_app/eui-solid"
import type { A2uiMessage, ExponentialSurfaceProps, HostPlugin, SolidExtension, SurfaceHandle, SurfaceState, ThemeInput } from "@exponential_app/eui-solid"
import { emit, emitWaiting, joinPending } from "./events"
import { createHost, decideTransport, transportKey } from "./transport"

export const TAG = `exponential-surface`

type Density = NonNullable<ExponentialSurfaceProps[`density`]>
type Contrast = NonNullable<ExponentialSurfaceProps[`contrast`]>
type Mode = ModeName | `system`
type Direction = `ltr` | `rtl`

/** attribute → property. */
const ATTRIBUTES = {
  theme: `theme`,
  mode: `mode`,
  density: `density`,
  contrast: `contrast`,
  locale: `locale`,
  "time-zone": `timeZone`,
  direction: `direction`,
  width: `width`,
  height: `height`,
  "surface-id": `surfaceId`,
  src: `src`,
  post: `post`,
  transport: `transport`,
} as const
type AttrName = keyof typeof ATTRIBUTES
type AttrProp = (typeof ATTRIBUTES)[AttrName]
const ATTRIBUTE_OF = Object.fromEntries(Object.entries(ATTRIBUTES).map(([a, p]) => [p, a])) as Record<AttrProp, AttrName>

const pick = <T extends string>(value: unknown, allowed: readonly T[]): T | undefined => (typeof value === `string` && (allowed as readonly string[]).includes(value) ? (value as T) : undefined)

/** A numeric string = px, any other string = a CSS length. */
export function lengthOf(value: number | string | null | undefined): number | string | undefined {
  if (value === null || value === undefined || value === ``) return undefined
  if (typeof value === `number`) return Number.isFinite(value) ? value : undefined
  const t = value.trim()
  return /^-?\d+(\.\d+)?$/.test(t) ? Number(t) : t
}

/** Every settable property (the upgrade pass). */
const PROPERTIES = [...Object.values(ATTRIBUTES), `surface`, `root`, `templates`, `data`, `host`, `plugin`, `extensions`, `strings`, `states`, `measure`, `viewportHeight`] as const

const isConnectable = (h: ExponentialHost) => h.hasTransport && h.status === `closed`

/** The elements attached in the current task after the first one: mounted
 *  together in a microtask (connectedCallback). */
let burst: Set<ExponentialSurfaceElement> | null = null

export class ExponentialSurfaceElement extends HTMLElement {
  static get observedAttributes(): string[] {
    return Object.keys(ATTRIBUTES)
  }

  // One signal per input (attribute-backed or not).
  #theme = createSignal<ThemeInput | undefined>(undefined)
  #mode = createSignal<string | undefined>(undefined)
  #density = createSignal<string | undefined>(undefined)
  #contrast = createSignal<string | undefined>(undefined)
  #locale = createSignal<string | undefined>(undefined)
  #timeZone = createSignal<string | undefined>(undefined)
  #direction = createSignal<string | undefined>(undefined)
  #width = createSignal<number | string | undefined>(undefined)
  #height = createSignal<number | string | undefined>(undefined)
  #surfaceId = createSignal<string | undefined>(undefined)
  #src = createSignal<string | undefined>(undefined)
  #post = createSignal<string | undefined>(undefined)
  #transport = createSignal<string | undefined>(undefined)
  #surface = createSignal<SurfaceState | undefined>(undefined)
  #root = createSignal<UiNode | null | undefined>(undefined)
  #templates = createSignal<Readonly<Record<string, UiNode>> | undefined>(undefined)
  #data = createSignal<DataModel | undefined>(undefined)
  #host = createSignal<ExponentialHost | undefined>(undefined)
  #plugin = createSignal<HostPlugin | undefined>(undefined)
  #extensions = createSignal<readonly SolidExtension[] | undefined>(undefined)
  #strings = createSignal<Record<string, string> | undefined>(undefined)
  #states = createSignal<readonly string[] | undefined>(undefined)
  #measure = createSignal<ExponentialSurfaceProps[`measure`]>(undefined)
  #viewportHeight = createSignal<number | undefined>(undefined)
  /** The store `apply()` made (and its owner's disposer). */
  #store = createSignal<SurfaceState | null>(null)
  #storeDispose?: () => void
  /** The host built from `src` (owned: connected and closed here). */
  #built?: { key: string; host: ExponentialHost }
  #status = createSignal<{ status: TransportStatus; detail?: string } | undefined>(undefined)
  /** VAPP-107: why `src`/`post` built no host (the URL policy denied one). */
  #denied = createSignal<string | undefined>(undefined)
  #liveHost?: ExponentialHost
  #currentSurface: Accessor<SurfaceState | null> = () => null
  #handle: SurfaceHandle | null = null
  #ready = false
  #dispose?: () => void
  /** Set while a property writes its attribute (the callback must not
   *  overwrite a non-string value). */
  #reflecting = false

  // --- attributes ---------------------------------------------------------

  attributeChangedCallback(name: string, _old: string | null, value: string | null): void {
    if (this.#reflecting) return
    const prop = ATTRIBUTES[name as AttrName]
    if (prop) this.#signal(prop)[1](() => value ?? undefined)
  }

  #signal(prop: AttrProp): [Accessor<unknown>, Setter<unknown>] {
    switch (prop) {
      case `theme`:
        return this.#theme as never
      case `mode`:
        return this.#mode as never
      case `density`:
        return this.#density as never
      case `contrast`:
        return this.#contrast as never
      case `locale`:
        return this.#locale as never
      case `timeZone`:
        return this.#timeZone as never
      case `direction`:
        return this.#direction as never
      case `width`:
        return this.#width as never
      case `height`:
        return this.#height as never
      case `surfaceId`:
        return this.#surfaceId as never
      case `src`:
        return this.#src as never
      case `post`:
        return this.#post as never
      case `transport`:
        return this.#transport as never
    }
  }

  /** A property set: a string reflects to its attribute, null/undefined
   *  removes it, any other value (an object theme, a number width) stays a
   *  property only. */
  #setAttrProp(prop: AttrProp, value: unknown): void {
    const attr = ATTRIBUTE_OF[prop]
    if (typeof value === `string` || value === null || value === undefined) {
      this.#reflecting = true
      try {
        if (typeof value === `string`) this.setAttribute(attr, value)
        else this.removeAttribute(attr)
      } finally {
        this.#reflecting = false
      }
    }
    this.#signal(prop)[1](() => value ?? undefined)
  }

  get theme(): ThemeInput | undefined {
    return this.#theme[0]()
  }
  set theme(v: ThemeInput | undefined | null) {
    this.#setAttrProp(`theme`, v)
  }
  get mode(): Mode | undefined {
    return this.#mode[0]() as Mode | undefined
  }
  set mode(v: Mode | undefined | null) {
    this.#setAttrProp(`mode`, v)
  }
  get density(): Density | undefined {
    return this.#density[0]() as Density | undefined
  }
  set density(v: Density | undefined | null) {
    this.#setAttrProp(`density`, v)
  }
  get contrast(): Contrast | undefined {
    return this.#contrast[0]() as Contrast | undefined
  }
  set contrast(v: Contrast | undefined | null) {
    this.#setAttrProp(`contrast`, v)
  }
  get locale(): string | undefined {
    return this.#locale[0]()
  }
  set locale(v: string | undefined | null) {
    this.#setAttrProp(`locale`, v)
  }
  get timeZone(): string | undefined {
    return this.#timeZone[0]()
  }
  set timeZone(v: string | undefined | null) {
    this.#setAttrProp(`timeZone`, v)
  }
  get direction(): Direction | undefined {
    return this.#direction[0]() as Direction | undefined
  }
  set direction(v: Direction | undefined | null) {
    this.#setAttrProp(`direction`, v)
  }
  get width(): number | string | undefined {
    return this.#width[0]()
  }
  set width(v: number | string | undefined | null) {
    this.#setAttrProp(`width`, v)
  }
  get height(): number | string | undefined {
    return this.#height[0]()
  }
  set height(v: number | string | undefined | null) {
    this.#setAttrProp(`height`, v)
  }
  /** The surface id (`surface-id`, default `main`). */
  get surfaceId(): string {
    return this.#surfaceId[0]() || `main`
  }
  set surfaceId(v: string | undefined | null) {
    this.#setAttrProp(`surfaceId`, v)
  }
  get src(): string | undefined {
    return this.#src[0]()
  }
  set src(v: string | undefined | null) {
    this.#setAttrProp(`src`, v)
  }
  get post(): string | undefined {
    return this.#post[0]()
  }
  set post(v: string | undefined | null) {
    this.#setAttrProp(`post`, v)
  }
  get transport(): string | undefined {
    return this.#transport[0]()
  }
  set transport(v: string | undefined | null) {
    this.#setAttrProp(`transport`, v)
  }

  // --- properties ---------------------------------------------------------

  get surface(): SurfaceState | undefined {
    return this.#surface[0]()
  }
  set surface(v: SurfaceState | undefined | null) {
    this.#surface[1](() => v ?? undefined)
  }
  get root(): UiNode | null | undefined {
    return this.#root[0]()
  }
  set root(v: UiNode | null | undefined) {
    this.#root[1](() => v)
  }
  get templates(): Readonly<Record<string, UiNode>> | undefined {
    return this.#templates[0]()
  }
  set templates(v: Readonly<Record<string, UiNode>> | undefined | null) {
    this.#templates[1](() => v ?? undefined)
  }
  get data(): DataModel | undefined {
    return this.#data[0]()
  }
  set data(v: DataModel | undefined | null) {
    this.#data[1](() => v ?? undefined)
  }
  get host(): ExponentialHost | undefined {
    return this.#host[0]()
  }
  set host(v: ExponentialHost | undefined | null) {
    this.#host[1](() => v ?? undefined)
  }
  get plugin(): HostPlugin | undefined {
    return this.#plugin[0]()
  }
  set plugin(v: HostPlugin | undefined | null) {
    this.#plugin[1](() => v ?? undefined)
  }
  get extensions(): readonly SolidExtension[] | undefined {
    return this.#extensions[0]()
  }
  set extensions(v: readonly SolidExtension[] | undefined | null) {
    this.#extensions[1](() => v ?? undefined)
  }
  get strings(): Record<string, string> | undefined {
    return this.#strings[0]()
  }
  set strings(v: Record<string, string> | undefined | null) {
    this.#strings[1](() => v ?? undefined)
  }
  get states(): readonly string[] | undefined {
    return this.#states[0]()
  }
  set states(v: readonly string[] | undefined | null) {
    this.#states[1](() => v ?? undefined)
  }
  get measure(): ExponentialSurfaceProps[`measure`] {
    return this.#measure[0]()
  }
  set measure(v: ExponentialSurfaceProps[`measure`] | null) {
    this.#measure[1](() => v ?? undefined)
  }
  get viewportHeight(): number | undefined {
    return this.#viewportHeight[0]()
  }
  set viewportHeight(v: number | undefined | null) {
    this.#viewportHeight[1](() => v ?? undefined)
  }

  // --- read-only ----------------------------------------------------------

  /** The painted surface's handle; null until the first paint. */
  get handle(): SurfaceHandle | null {
    return this.#handle
  }
  /** The active store's reduce issues. */
  get issues(): SurfaceState[`issues`] {
    return this.surfaceState?.issues ?? []
  }
  /** The host's transport state (a host property or one built from `src`);
   *  `closed` when the URL policy denied `src`/`post` (see `statusDetail`). */
  get status(): TransportStatus | undefined {
    return (this.#liveHost ?? this.#host[0]())?.status ?? this.#status[0]()?.status
  }
  /** The transport's detail (an error, or `src denied by the URL policy (private)`). */
  get statusDetail(): string | undefined {
    return (this.#liveHost ?? this.#host[0]())?.statusDetail ?? this.#status[0]()?.detail
  }
  /** The state painted: the `surface` property, the host's surface, or the
   *  store `apply()` made. */
  get surfaceState(): SurfaceState | null {
    return untrack(() => this.#surface[0]() ?? this.#currentSurface() ?? this.#ownStore())
  }

  // --- sources ------------------------------------------------------------

  #ownStore(): SurfaceState | null {
    const s = this.#store[0]()
    return s && s.surfaceId === this.surfaceId ? s : null
  }

  /** The host in use: the property, else one built from `src` (rebuilt when
   *  src/post/transport change; the old one is closed). VAPP-107: `src` and
   *  `post` pass the page's URL policy (`plugin.urls`) first; a denied one
   *  builds no host (the status says why). */
  #activeHost(): ExponentialHost | undefined {
    const given = this.#host[0]()
    if (given) {
      this.#dropBuilt()
      this.#denied[1](undefined)
      return given
    }
    const src = this.#src[0]()
    if (!src) {
      this.#dropBuilt()
      this.#denied[1](undefined)
      return undefined
    }
    const attrs = { src, post: this.#post[0](), transport: this.#transport[0]() }
    const policy = this.#plugin[0]()?.urls
    const decision = decideTransport(attrs, policy)
    if (!decision.allowed) {
      this.#dropBuilt()
      this.#denied[1](decision.reason)
      return undefined
    }
    this.#denied[1](undefined)
    const key = transportKey(attrs)
    if (this.#built?.key !== key) {
      this.#dropBuilt()
      this.#built = { key, host: createHost(attrs, extensionCatalogs(untrack(() => this.#extensions[0]()) ?? []), policy) }
    }
    return this.#built.host
  }

  #dropBuilt(): void {
    const b = this.#built
    this.#built = undefined
    b?.host.close()
  }

  // --- methods ------------------------------------------------------------

  /** One A2UI server message: to the `surface` property, else the host,
   *  else a store made on first use (surface id = `surfaceId`). */
  apply(message: A2uiMessage): void {
    const given = untrack(() => this.#surface[0]())
    if (given) return given.apply(message)
    const host = untrack(() => this.#activeHost())
    if (host) {
      host.receive(message)
      return
    }
    let store = untrack(() => this.#ownStore())
    if (!store) {
      this.#storeDispose?.()
      const surfaceId = this.surfaceId
      store = createRoot((dispose) => {
        this.#storeDispose = dispose
        return createSurface({ surfaceId })
      })
      this.#store[1](() => store)
    }
    store.apply(message)
  }

  /** Write the data model (the active store's; in fixture mode a new `data`). */
  setData(pointer: string, value: unknown): void {
    const state = this.surfaceState
    if (state) state.setData(pointer, value)
    else this.data = setPointer(untrack(() => this.#data[0]()) ?? {}, pointer, value)
  }

  /** Back to the store's initial state (a host surface ignores it). */
  reset(): void {
    this.surfaceState?.reset()
  }

  run(command: SurfaceCommand): void {
    this.#handle?.run(command)
  }
  focus(id?: string | FocusOptions): boolean {
    if (typeof id !== `string`) {
      super.focus(id)
      return true
    }
    return this.#handle?.focus(id) ?? false
  }
  announce(text: string, live?: `polite` | `assertive`): void {
    this.#handle?.announce(text, live)
  }
  scrollIntoView(id?: string | boolean | ScrollIntoViewOptions): boolean {
    if (typeof id !== `string`) {
      super.scrollIntoView(id)
      return true
    }
    return this.#handle?.scrollIntoView(id) ?? false
  }
  scrollToIndex(id: string, index: number, align?: ScrollAlign): boolean {
    return this.#handle?.scrollToIndex(id, index, align) ?? false
  }

  // --- the plugin ---------------------------------------------------------

  /** The page's plugin with every callback also dispatched as an event. */
  #elementPlugin(base: HostPlugin): HostPlugin {
    return {
      ...base,
      onAction: (e) => {
        const own = base.onAction?.(e)
        return joinPending([own, ...emitWaiting(this, `xui-action`, e)])
      },
      onInput: (e) => {
        const own = base.onInput?.(e)
        return joinPending([own, ...emitWaiting(this, `xui-input`, e)])
      },
      onUpload: (files, target) => {
        const own = base.onUpload?.(files, target)
        return joinPending([own, ...emitWaiting(this, `xui-upload`, { files, target })])
      },
      onFunctionCall: (call) => {
        const own = base.onFunctionCall?.(call)
        let answered = false
        let answer: unknown
        emit(this, `xui-function-call`, {
          call,
          respond: (value) => {
            if (answered) return
            answered = true
            answer = value
          },
        })
        return answered ? answer : own
      },
      openUrl: (url) => {
        if (!emit(this, `xui-open-url`, { url }, true)) return
        if (base.openUrl) base.openUrl(url)
        else if (typeof window !== `undefined`) window.open(url, `_blank`, `noopener,noreferrer`)
      },
      onPaintError: (error) => {
        base.onPaintError?.(error)
        emit(this, `xui-paint-error`, { error })
      },
      onThemeIssues: (issues: ThemeIssue[]) => {
        base.onThemeIssues?.(issues)
        emit(this, `xui-theme-issues`, { issues })
      },
      onUnknown: (node) => {
        base.onUnknown?.(node)
        emit(this, `xui-unknown`, { node })
      },
    }
  }

  /** The plugin for a host surface: `hostPlugin` over the element's, with
   *  the open still announced (`xui-open-url`) before the host opens it. */
  #hostPlugin(host: ExponentialHost, base: HostPlugin): HostPlugin {
    const merged = hostPlugin(host, this.#elementPlugin(base))
    return {
      ...merged,
      openUrl: (url) => {
        if (!emit(this, `xui-open-url`, { url }, true)) return
        if (base.openUrl) base.openUrl(url)
        else host.openUrl(url)
      },
    }
  }

  // --- mounting -----------------------------------------------------------

  /** A property set on the element before it was upgraded is an own data
   *  property shadowing the accessor: move it onto the accessor. */
  #upgradeProperties(): void {
    for (const name of PROPERTIES) {
      if (!Object.prototype.hasOwnProperty.call(this, name)) continue
      const value = (this as unknown as Record<string, unknown>)[name]
      delete (this as unknown as Record<string, unknown>)[name]
      ;(this as unknown as Record<string, unknown>)[name] = value
    }
  }

  connectedCallback(): void {
    this.#upgradeProperties()
    if (this.#dispose) return
    if (burst) {
      burst.add(this)
      return
    }
    burst = new Set()
    queueMicrotask(() => {
      const pending = burst
      burst = null
      if (pending?.size) batch(() => pending.forEach((el) => el.#mount()))
    })
    this.#mount()
  }

  #mount(): void {
    if (this.#dispose || !this.isConnected) return
    this.replaceChildren()
    this.#ready = false
    this.#dispose = render(() => this.#view(), this)
  }

  disconnectedCallback(): void {
    burst?.delete(this)
    const dispose = this.#dispose
    this.#dispose = undefined
    dispose?.()
    this.#handle = null
    this.#currentSurface = () => null
    this.#dropBuilt()
    this.#status[1](undefined)
  }

  #view() {
    const host = createMemo(() => this.#activeHost())
    // The host's state → `status` + `xui-status`; a host with a transport
    // that never connected is connected now (one built here is closed on
    // detach and on rebuild; a host property stays the page's).
    createEffect(() => {
      const h = host()
      this.#liveHost = h
      if (!h) {
        // A denied src/post: closed, with the reason, as a transport error would be.
        const reason = this.#denied[0]()
        const s = reason ? { status: `closed` as const, detail: reason } : undefined
        this.#status[1](s)
        if (s) emit(this, `xui-status`, s)
        return
      }
      let last: { status: TransportStatus; detail?: string } | undefined
      const check = () => {
        if (last && last.status === h.status && last.detail === h.statusDetail) return
        last = { status: h.status, detail: h.statusDetail }
        this.#status[1](last)
        emit(this, `xui-status`, h.statusDetail === undefined ? { status: h.status } : { status: h.status, detail: h.statusDetail })
      }
      const unsubscribe = h.subscribe(check)
      if (isConnectable(h)) h.connect()
      check()
      onCleanup(() => {
        unsubscribe()
        if (this.#liveHost === h) this.#liveHost = undefined
      })
    })
    const hostSurface = createMemo(() => {
      const h = host()
      return h ? createHostSurface(h, this.surfaceId) : undefined
    })
    const current = createMemo((): SurfaceState | null => this.#surface[0]() ?? hostSurface()?.() ?? this.#ownStore())
    this.#currentSurface = current
    const plugin = createMemo(() => {
      const base = this.#plugin[0]() ?? {}
      const h = host()
      return h ? this.#hostPlugin(h, base) : this.#elementPlugin(base)
    })
    const fixture = () => !current()
    const height = () => lengthOf(this.#height[0]())
    const handleRef = (handle: SurfaceHandle) => {
      this.#handle = handle
      if (this.#ready) return
      this.#ready = true
      queueMicrotask(() => {
        if (this.#handle === handle && this.isConnected) emit(this, `xui-ready`, { handle })
      })
    }
    const el = this
    const props: ExponentialSurfaceProps = {
      get surface() {
        return current() ?? undefined
      },
      get root() {
        return fixture() ? el.#root[0]() : undefined
      },
      get templates() {
        return fixture() ? el.#templates[0]() : undefined
      },
      get data() {
        return fixture() ? el.#data[0]() : undefined
      },
      get theme() {
        return current()?.theme ?? el.#theme[0]()
      },
      get mode() {
        return pick(el.#mode[0](), [`light`, `dark`, `system`] as const)
      },
      get density() {
        return pick(el.#density[0](), [`compact`, `default`, `comfortable`] as const)
      },
      get contrast() {
        return pick(el.#contrast[0](), [`normal`, `high`, `system`] as const)
      },
      get locale() {
        return el.#locale[0]() || undefined
      },
      get timeZone() {
        return el.#timeZone[0]() || undefined
      },
      get direction() {
        return pick(el.#direction[0](), [`ltr`, `rtl`] as const)
      },
      get width() {
        return lengthOf(el.#width[0]())
      },
      get style() {
        const h = height()
        return h === undefined ? undefined : { height: px(h) }
      },
      get viewportHeight() {
        return el.#viewportHeight[0]()
      },
      get strings() {
        return el.#strings[0]()
      },
      get extensions() {
        return el.#extensions[0]()
      },
      get states() {
        return el.#states[0]()
      },
      get measure() {
        return el.#measure[0]()
      },
      get host() {
        return plugin()
      },
      get id() {
        return el.surfaceId
      },
      handleRef,
    }
    // A host surface paints only once the host has it; a src/post the URL
    // policy denied (no host) paints nothing either.
    return createComponent(Show as unknown as (p: { when: boolean; children: JSX.Element }) => JSX.Element, {
      get when() {
        return (!host() && !el.#denied[0]()) || el.#surface[0]() !== undefined || current() !== null
      },
      get children() {
        return createComponent(ExponentialSurface, props)
      },
    })
  }
}

