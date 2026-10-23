// FEED-128: `<ExponentialSurface>` on Solid — one A2UI surface painted with
// the SAME DOM and the SAME compiled stylesheet as the React renderer
// (surface.tsx there): the root carries the theme's scope class + the
// variable block (per mode), the surface's node sheet, the live regions,
// and an inner `xui` CONTAINER the breakpoints resolve against; the OVERLAY
// and TOAST layers sit INSIDE the container. Everything is derived: signals
// for the inputs (props, platform preferences, the measured box), memos for
// the theme, the sheets, the layout facts; the context is an object of
// getters over them, so a node re-binds only on what it reads.

import { For, Show, createComputed, createEffect, createMemo, createSignal, createUniqueId, mergeProps, on, onCleanup, onMount, untrack, type JSX } from "solid-js"
import {
  activeBreakpoint,
  applyContrast,
  applyDensity,
  builtinTheme,
  formatString,
  intlFormatter,
  themeOrDefault,
  mediaMatches,
  parseMediaCondition,
  resolveMode,
  stringTable,
  templateBudget,
  textDirection,
  DEFAULT_LOCALE,
  DEFAULT_THEME_ID,
} from "@exponential_app/ui"
import type { ModeName, ResolvedTheme, ScrollAlign, ThemeIssue, ThemeSource, UiNode, SurfaceCommand } from "@exponential_app/ui"
import { BASE_CSS, CLIENT_FUNCTIONS, compileNodeSheet, compiledTheme, memoFormatter, nodeSheetBatches, openAllowed, queryToken, setPointer, share, surfaceClass, walkNodes, type DataModel, type NodeSheetBatch } from "@exponential_app/ui/web"
import { SurfaceContext, type SurfaceContextValue } from "./context"
import { extensionCatalogs, extensionMacroNames, registeredExtensions, type SolidExtension } from "./extensions"
import type { HostPlugin } from "./host"
import { NodeView, latest } from "./node-view"
import { createMediaQuery } from "./platform"
import { px, type SolidStyle } from "./style"
import type { SurfaceState } from "./create-surface"

export type ThemeInput = ResolvedTheme | ThemeSource | string

/** What a host can ask of a live surface (`SurfaceCommand`, a11y.json). */
export interface SurfaceHandle {
  run: (command: SurfaceCommand) => void
  /** Focus the node (or its first focusable descendant); false if absent. */
  focus: (id: string) => boolean
  announce: (text: string, live?: `polite` | `assertive`) => void
  scrollIntoView: (id: string) => boolean
  /** Item `index` (data order) of the List/Table `id` into view. */
  scrollToIndex: (id: string, index: number, align?: ScrollAlign) => boolean
}

export interface ExponentialSurfaceProps {
  /** The state from `createSurface` (tree + data + templates)… */
  surface?: SurfaceState
  /** …or a normalized tree (a fixture) with an optional data model… */
  root?: UiNode | null
  /** …and its LIFTED templates (`ReduceResult.templates`). */
  templates?: Readonly<Record<string, UiNode>>
  data?: DataModel
  /** A built-in id, a theme file (resolved against the built-ins) or a
   *  resolved theme. */
  theme?: ThemeInput
  /** `light | dark | system` (default `system`, followed live). */
  mode?: ModeName | `system`
  density?: `compact` | `default` | `comfortable`
  contrast?: `normal` | `high` | `system`
  /** BCP 47 (default `en-US`). */
  locale?: string
  /** IANA time zone (default the platform's). */
  timeZone?: string
  /** Built-in string overrides by id (`catalog/strings.json`). */
  strings?: Record<string, string>
  host?: HostPlugin
  extensions?: readonly SolidExtension[]
  /** Forced interaction states on every node (the builder's recipe sheet). */
  states?: readonly string[]
  /** Geometry mode: leaves become fixed boxes of the returned size. */
  measure?: (node: UiNode) => { w: number; h: number } | null
  /** Default: the locale's direction (`textDirection`). */
  direction?: `ltr` | `rtl`
  /** The surface width; default = the host's box. */
  width?: number | string
  /** The height of the VIEWPORT the surface is shown in (the height and
   *  orientation conditions); default an explicit inline `style.height`,
   *  else unknown. */
  viewportHeight?: number
  /** An explicit id (defaults to `createUniqueId`); unique per page. */
  id?: string
  /** Host → surface commands: called once with the surface's handle (React's
   *  `handleRef`, as a Solid ref callback). */
  handleRef?: (handle: SurfaceHandle) => void
  class?: string
  /** Solid's style form (kebab-case keys). */
  style?: JSX.CSSProperties
  children?: JSX.Element
}

/** A theme prop, resolved: the theme to paint with and why the asked one
 *  was refused (empty when it loaded). */
export interface ThemeResolution {
  theme: ResolvedTheme
  issues: ThemeIssue[]
}

const resolvedCache = new Map<string, ThemeResolution>()

/** A theme prop → the resolved theme. NEVER throws (round 4). */
export function resolveThemeInput(input: ThemeInput | undefined): ThemeResolution {
  if (!input) return { theme: builtinTheme(DEFAULT_THEME_ID), issues: [] }
  if (typeof input === `string`) return themeOrDefault(input)
  if (`chain` in input && Array.isArray((input as ResolvedTheme).chain)) return { theme: input as ResolvedTheme, issues: [] }
  let key: string
  try {
    key = JSON.stringify(input)
  } catch {
    return { theme: builtinTheme(DEFAULT_THEME_ID), issues: [{ path: `theme`, message: `expected a theme object {id, name, modes, tokens, recipes}` }] }
  }
  let hit = resolvedCache.get(key)
  if (!hit) {
    hit = themeOrDefault(input)
    resolvedCache.set(key, hit)
  }
  return hit
}

const derivedCache = new WeakMap<ResolvedTheme, Map<string, ResolvedTheme>>()

/** The theme a surface runs with: density applied, then the contrast
 *  overlays when asked (memoized per theme, so the compiled sheet is too). */
export function surfaceTheme(theme: ResolvedTheme, density: `compact` | `default` | `comfortable`, highContrast: boolean): ResolvedTheme {
  if (density === `default` && !highContrast) return theme
  let byKey = derivedCache.get(theme)
  if (!byKey) {
    byKey = new Map()
    derivedCache.set(theme, byKey)
  }
  const key = `${density}/${highContrast}`
  let hit = byKey.get(key)
  if (!hit) {
    hit = applyDensity(theme, density)
    if (highContrast) hit = applyContrast(hit)
    byKey.set(key, hit)
  }
  return hit
}

const EMPTY: readonly never[] = []
const NO_TEMPLATES: Readonly<Record<string, UiNode>> = {}
const EMPTY_HOST: HostPlugin = {}

/** What the surface knows of its box. */
export interface SurfaceBox {
  width: number
  /** Undefined = unknown. */
  height: number | undefined
  measured: boolean
}

export interface LayoutInputs {
  breakpoints: Record<string, number>
  queries: readonly string[]
  viewportHeight: number | undefined
  hover: boolean
  reducedMotion: boolean
}

/** The only layout facts that reach the tree: the active breakpoint and the
 *  matching JS-evaluated conditions (`data-xq`). */
export function surfaceLayout(box: SurfaceBox, inputs: LayoutInputs): { breakpoint: string | null; xq: string } {
  const breakpoint = activeBreakpoint(box.width, inputs.breakpoints)
  const height = inputs.viewportHeight ?? box.height
  const ctx = { width: box.width, height, hover: inputs.hover, reducedMotion: inputs.reducedMotion }
  const xq = inputs.queries
    .filter((key) => {
      const c = parseMediaCondition(key)
      return c ? mediaMatches(c, ctx) : false
    })
    .map(queryToken)
    .join(` `)
  return { breakpoint, xq }
}

const FOCUSABLE = `input:not([disabled]),textarea:not([disabled]),select:not([disabled]),button:not([disabled]),a[href],[tabindex]:not([tabindex="-1"]),[contenteditable="true"]`

const platformZone = (): string => {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || `UTC`
  } catch {
    return `UTC`
  }
}

declare const process: { env: { NODE_ENV?: string } }
const IS_DEV = (() => {
  try {
    return process.env.NODE_ENV !== `production`
  } catch {
    return false
  }
})()
const warnedTemplates = new Set<string>()

/** Dev only: a node whose `template.component` is not in `templates`. */
function warnMissingTemplates(surfaceId: string, root: UiNode, templates: Readonly<Record<string, UiNode>>): void {
  const visit = (n: UiNode) => {
    const id = (n.template as { component?: unknown } | undefined)?.component
    if (typeof id === `string` && !templates[id]) {
      const key = `${surfaceId}\u0000${id}`
      if (!warnedTemplates.has(key)) {
        warnedTemplates.add(key)
        console.warn(`[exponential-ui] surface "${surfaceId}": "${n.id}" templates "${id}", which is not in \`templates\` (pass the reducer's \`templates\` with \`root\`); it renders no items.`)
      }
    }
    for (const c of n.children) visit(c)
    for (const sl of Object.values(n.slots ?? {})) visit(sl)
  }
  visit(root)
  for (const t of Object.values(templates)) visit(t)
}

const isPlain = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

/** Structural equality, short-circuiting on identity at every level. */
export function sameData(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true
  if (typeof a !== `object` || typeof b !== `object` || a === null || b === null) return false
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false
    for (let i = 0; i < a.length; i++) if (!sameData(a[i], b[i])) return false
    return true
  }
  if (!isPlain(b) || Array.isArray(b)) return false
  const pa = Object.getPrototypeOf(a)
  if (pa !== Object.prototype && pa !== null) return false
  const ka = Object.keys(a as object)
  if (ka.length !== Object.keys(b).length) return false
  for (const k of ka) if (!Object.prototype.hasOwnProperty.call(b, k) || !sameData((a as Record<string, unknown>)[k], b[k])) return false
  return true
}

function usesLiveClock(value: unknown): boolean {
  if (Array.isArray(value)) return value.some(usesLiveClock)
  if (typeof value !== `object` || value === null) return false
  const v = value as Record<string, unknown>
  if (v.call === `formatRelativeTime` && (typeof v.args !== `object` || v.args === null || (v.args as Record<string, unknown>).now === undefined)) return true
  if (v.component === `Table`) {
    const columns = (v.props as Record<string, unknown> | undefined)?.columns
    if (Array.isArray(columns) ? columns.some((c) => (c as Record<string, unknown> | null)?.type === `relativeTime`) : typeof columns === `object` && columns !== null) return true
  }
  return Object.values(v).some(usesLiveClock)
}

export function ExponentialSurface(input: ExponentialSurfaceProps): JSX.Element {
  const props = mergeProps({ mode: `system` as const, density: `default` as const, contrast: `system` as const, locale: DEFAULT_LOCALE }, input)
  const uid = createUniqueId()
  const surfaceId = createMemo(() => props.id ?? props.surface?.surfaceId ?? uid.replace(/[^a-zA-Z0-9]/g, ``))
  const prefersDark = createMediaQuery(`(prefers-color-scheme: dark)`)
  const prefersContrast = createMediaQuery(`(prefers-contrast: more)`)
  const reducedMotion = createMediaQuery(`(prefers-reduced-motion: reduce)`)
  const hover = createMediaQuery(`(hover: hover)`, true)
  const mode = createMemo((): ModeName => (props.mode === `system` ? resolveMode(`system`, prefersDark()) : props.mode))
  const resolution = createMemo(() => resolveThemeInput(props.theme))
  // An unusable theme never crashes the host: the surface paints with the
  // default theme and hands the issues over once per resolution.
  createEffect(() => {
    const r = resolution()
    const report = props.host?.onThemeIssues
    if (r.issues.length > 0) untrack(() => report?.(r.issues))
  })
  const theme = createMemo(() => surfaceTheme(resolution().theme, props.density, props.contrast === `high` || (props.contrast === `system` && prefersContrast())))
  const extensions = createMemo(() => [...registeredExtensions(), ...(props.extensions ?? EMPTY)])
  const extensionDefs = createMemo(() => extensionCatalogs(extensions()))
  const macros = createMemo(() => extensionMacroNames(extensions()))
  const compiled = createMemo(() => compiledTheme(theme(), { extensionMacros: macros() }))
  const direction = createMemo(() => props.direction ?? textDirection(props.locale))
  const strings = createMemo(() => stringTable(props.strings ?? {}))
  const t = (id: string, params?: Record<string, unknown>) => formatString(strings()[id] ?? id, params ?? {})

  // Data: the surface state's, or a local signal over the `data` prop. A
  // `data` prop with new CONTENT replaces the local model (an equal one
  // keeps the user's edits), compared structurally against the last ADOPTED.
  const [localData, setLocalData] = createSignal<DataModel>(untrack(() => props.data) ?? {})
  let seenData = untrack(() => props.data)
  createEffect(
    on(
      () => props.data,
      (next) => {
        if (next !== seenData && !sameData(next, seenData)) {
          seenData = next
          setLocalData(next ?? {})
        }
      },
      { defer: true }
    )
  )
  const data = () => (props.surface ? props.surface.data : localData())
  const setData = (pointer: string, value: unknown) => {
    if (props.surface) props.surface.setData(pointer, value)
    else setLocalData((cur) => setPointer(cur, pointer, value))
  }

  // VAPP-152: a new tree shares every unchanged node with the last one
  // (`share`), so a streamed `updateComponents` re-binds only the nodes it
  // changed.
  const root = createMemo<UiNode | null>((prev) => share(prev, props.surface ? props.surface.root : (props.root ?? null)))
  const latestRoot = latest(root)
  const templates = createMemo<Readonly<Record<string, UiNode>>>((prev) => share(prev, (props.surface ? props.surface.templates : props.templates) ?? NO_TEMPLATES))
  const templateNode = (componentId: string) => templates()[componentId]
  const templateRoots = createMemo(() => Object.values(templates()))
  // VAPP-152: one signal per node id, written only when that id's node
  // OBJECT changed (the shared tree keeps the others). A listed child reads
  // its own cell, not its parent's node, so a streamed card notifies the
  // nodes it changed instead of marking the whole tree.
  let byId = new Map<string, UiNode>()
  const cells = new Map<string, { get: () => UiNode | undefined; set: (n: UiNode | undefined) => void }>()
  createComputed(() => {
    const next = new Map<string, UiNode>()
    const r = root()
    for (const top of r ? [r, ...templateRoots()] : templateRoots()) for (const n of walkNodes(top)) if (!next.has(n.id)) next.set(n.id, n)
    byId = next
    for (const [id, cell] of cells) {
      const n = next.get(id)
      if (n === undefined) cells.delete(id)
      else cell.set(n)
    }
  })
  const nodeCell = (id: string): (() => UiNode | undefined) | undefined => {
    let cell = cells.get(id)
    if (!cell) {
      const n = byId.get(id)
      if (n === undefined) return undefined
      const [get, set] = createSignal<UiNode | undefined>(n)
      cell = { get, set: (v) => set(() => v) }
      cells.set(id, cell)
    }
    return cell.get
  }
  // VAPP-103: past `maxTemplateItems` items or `maxComponents` nodes.
  const budget = createMemo(() => {
    const r = root()
    return r ? templateBudget(r, data(), templateNode) : undefined
  })
  createEffect(
    on(
      () => [surfaceId(), budget()?.issue?.id, budget()?.issue?.message] as const,
      ([id, issueId, message]) => {
        if (issueId) console.warn(`[exponential-ui] ${id}: ${issueId}: ${message}`)
      }
    )
  )
  createEffect(() => {
    const r = root()
    if (IS_DEV && r) warnMissingTemplates(surfaceId(), r, templates())
  })
  const sheet = createMemo(() => {
    const r = root()
    return r ? compileNodeSheet([r, ...templateRoots()], surfaceId(), theme()) : { css: ``, queries: [] as string[], chunks: new Map<string, string>() }
  })
  // VAPP-152: the node sheet in `<style>` batches; a streamed node adds
  // one, the unchanged ones keep their element (no surface-wide restyle).
  const batches = createMemo<NodeSheetBatch[]>((prev) => nodeSheetBatches(prev ?? [], sheet().chunks))
  const styleBatches = createMemo((): readonly NodeSheetBatch[] => (batches().length ? batches() : [{ chunks: new Map(), css: sheet().css }]))

  const [portal, setPortal] = createSignal<HTMLElement | null>(null)
  const [toastLayer, setToastLayer] = createSignal<HTMLElement | null>(null)
  let rootEl: HTMLDivElement | undefined
  // The box is a signal only the layout memo reads: a resize that changes
  // neither the breakpoint nor `data-xq` re-binds nothing.
  const [box, setBox] = createSignal<SurfaceBox>({ width: typeof untrack(() => props.width) === `number` ? (untrack(() => props.width) as number) : 0, height: undefined, measured: false })
  const layout = createMemo(
    () => surfaceLayout(box(), { breakpoints: theme().tokens.breakpoint ?? {}, queries: sheet().queries, viewportHeight: props.viewportHeight, hover: hover(), reducedMotion: reducedMotion() }),
    undefined,
    { equals: (a, b) => a.breakpoint === b.breakpoint && a.xq === b.xq }
  )
  const breakpoint = () => layout().breakpoint
  const xq = () => layout().xq
  onMount(() => {
    const el = rootEl
    if (!el) return
    const read = () => {
      const r = el.getBoundingClientRect()
      // No layout yet (jsdom, display:none): keep the initial guess.
      if (!untrack(box).measured && r.width === 0 && r.height === 0) return
      // Only an EXPLICIT height is a viewport; a content-sized root is not.
      const explicit = el.style.height !== `` && el.style.height !== `auto`
      setBox({ width: r.width, height: explicit ? r.height : undefined, measured: true })
    }
    read()
    if (typeof ResizeObserver === `undefined`) return
    const ro = new ResizeObserver(read)
    ro.observe(el)
    onCleanup(() => ro.disconnect())
  })

  // Live regions: `announce` swaps the text (cleared first, so the same
  // message twice is spoken twice).
  let politeEl: HTMLDivElement | undefined
  let assertiveEl: HTMLDivElement | undefined
  const announce = (text: string, live: `polite` | `assertive` = `polite`) => {
    const el = live === `assertive` ? assertiveEl : politeEl
    if (!el) return
    el.textContent = ``
    setTimeout(() => {
      el.textContent = text
    }, 30)
  }

  const functions = createMemo(() => ({ ...CLIENT_FUNCTIONS, ...(props.host?.functions ?? {}) }))
  const formatter = createMemo(() => memoFormatter(intlFormatter(props.locale, props.timeZone ?? platformZone())))
  // The surface clock: a tree that formats a relative time against "now"
  // re-binds once a minute.
  const liveClock = createMemo(() => usesLiveClock(root()) || usesLiveClock(templateRoots()))
  const [minute, setMinute] = createSignal(Date.now())
  createEffect(() => {
    if (!liveClock()) return
    const timer = setInterval(() => setMinute(Date.now()), 60_000)
    onCleanup(() => clearInterval(timer))
  })
  const now = createMemo(() => {
    minute()
    return () => Date.now()
  })
  const scrollers = new Map<string, (index: number, align?: ScrollAlign) => void>()
  const registerScroller = (id: string, scroll: (index: number, align?: ScrollAlign) => void) => {
    scrollers.set(id, scroll)
    return () => {
      if (scrollers.get(id) === scroll) scrollers.delete(id)
    }
  }
  const paintFailures = new Map<string, string>()
  const host = createMemo(() => props.host ?? EMPTY_HOST)
  // Every open passes the URL policy (the host's opener or a new tab).
  const openUrl = (url: string) => openAllowed(untrack(host), url)

  const ctx: SurfaceContextValue = {
    get surfaceId() {
      return surfaceId()
    },
    get compiled() {
      return compiled()
    },
    get theme() {
      return theme()
    },
    get mode() {
      return mode()
    },
    get host() {
      return host()
    },
    get extensions() {
      return extensions()
    },
    get extensionDefs() {
      return extensionDefs()
    },
    get data() {
      return data()
    },
    setData,
    templateNode,
    nodeCell,
    get templateBudget() {
      return budget()
    },
    get states() {
      return props.states ?? EMPTY
    },
    get measure() {
      return props.measure
    },
    get portal() {
      return portal()
    },
    get toastLayer() {
      return toastLayer()
    },
    get direction() {
      return direction()
    },
    get functions() {
      return functions()
    },
    openUrl,
    get locale() {
      return props.locale
    },
    get formatter() {
      return formatter()
    },
    get now() {
      return now()
    },
    registerScroller,
    paintFailures,
    get strings() {
      return strings()
    },
    t,
    get breakpoint() {
      return breakpoint()
    },
    get xq() {
      return xq()
    },
    get reducedMotion() {
      return reducedMotion()
    },
    get hover() {
      return hover()
    },
    announce,
  }

  const nodeEl = (id: string): HTMLElement | null => rootEl?.querySelector<HTMLElement>(`[data-xui-id="${typeof CSS !== `undefined` && CSS.escape ? CSS.escape(id) : id.replace(/"/g, `\\"`)}"]`) ?? null
  const focus = (id: string) => {
    const el = nodeEl(id)
    if (!el) return false
    const target = el.matches(FOCUSABLE) ? el : el.querySelector<HTMLElement>(FOCUSABLE)
    ;(target ?? el).focus()
    return true
  }
  const scrollIntoView = (id: string) => {
    const el = nodeEl(id)
    if (!el) return false
    el.scrollIntoView({ block: `nearest`, behavior: untrack(reducedMotion) ? `auto` : `smooth` })
    return true
  }
  const scrollToIndex = (id: string, index: number, align?: ScrollAlign) => {
    const scroll = scrollers.get(id)
    if (!scroll) return false
    scroll(index, align)
    return true
  }
  const handle: SurfaceHandle = {
    focus,
    scrollIntoView,
    scrollToIndex,
    announce,
    run: (command) => {
      if (`focus` in command) focus(command.focus.id)
      else if (`announce` in command) announce(command.announce.text, command.announce.live)
      else if (`scrollIntoView` in command) scrollIntoView(command.scrollIntoView.id)
      else if (`scrollToIndex` in command) scrollToIndex(command.scrollToIndex.id, command.scrollToIndex.index, command.scrollToIndex.align)
    },
  }
  untrack(() => props.handleRef?.(handle))

  const layerCss = createMemo(() => {
    const md = theme().tokens.breakpoint?.md ?? 768
    const scopeSel = `.${surfaceClass(surfaceId())}`
    return `@layer xui-base{@container xui (width >= ${md}px){${scopeSel} .xui-toast-layer{left:auto;right:var(--xui-spacing-lg);transform:none;align-items:flex-end}${scopeSel}[dir="rtl"] .xui-toast-layer{right:auto;left:var(--xui-spacing-lg)}}}`
  })
  const rootStyle = (): JSX.CSSProperties | undefined => (props.width === undefined ? props.style : ({ width: px(props.width), ...(props.style ?? {}) } as SolidStyle))

  return (
    <div
      ref={rootEl}
      class={[`xui-surface`, compiled().scope, surfaceClass(surfaceId()), props.class].filter(Boolean).join(` `)}
      data-xui-surface={surfaceId()}
      data-xui-mode={mode()}
      data-xui-theme={theme().id}
      data-xui-density={props.density === `default` ? undefined : props.density}
      data-xui-motion={reducedMotion() ? `reduce` : undefined}
      data-xq={xq() || undefined}
      dir={direction()}
      style={rootStyle()}
    >
      <style data-xui-style="theme" textContent={`${BASE_CSS}\n${compiled().css}`} />
      <For each={styleBatches()}>{(batch, i) => <style data-xui-style="nodes" textContent={i() === 0 ? `${batch.css}${layerCss()}` : batch.css} />}</For>
      <SurfaceContext.Provider value={ctx}>
        <div class="xui-container">
          <Show when={root()}>
            <NodeView node={latestRoot()} />
          </Show>
          {props.children}
          <div ref={setPortal} class="xui-layer" data-xui-layer="overlay" />
          <div ref={setToastLayer} class="xui-toast-layer" data-xui-layer="toast" />
        </div>
      </SurfaceContext.Provider>
      <div ref={politeEl} class="xui-sr-only" aria-live="polite" aria-atomic="true" data-xui-live="polite" />
      <div ref={assertiveEl} class="xui-sr-only" aria-live="assertive" aria-atomic="true" data-xui-live="assertive" />
    </div>
  )
}
