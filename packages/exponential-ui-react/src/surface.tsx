// VAPP-87 + round 1: `<ExponentialSurface>` — one A2UI surface painted with
// real CSS. The root carries the theme's scope class + the variable block
// (per mode), the surface's node sheet, the live regions, and an inner
// `xui` CONTAINER the breakpoints resolve against (a container query cannot
// target its own element, so the root and the container are two elements).
// The OVERLAY and TOAST layers sit INSIDE the container (round 1, audit bug
// C): a Dialog's content still matches `@container xui` breakpoints, and an
// inline-size container does not trap `position: fixed`. Themes,
// extensions, the host plugin and the surface settings (locale, strings,
// mode incl. `system`, density, contrast) are props; nothing global is
// touched, so two surfaces on one page can wear two themes.

import { useCallback, useEffect, useId, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, type CSSProperties, type ReactNode, type Ref } from "react"
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
} from "@exponential-at/ui"
import type { Formatter, ModeName, ResolvedTheme, ScrollAlign, TemplateBudget, ThemeIssue, ThemeSource, UiNode, SurfaceCommand } from "@exponential-at/ui"
import { BASE_CSS } from "@exponential-at/ui/web"
import { compileNodeSheet, nodeSheetBatches, queryToken, share, surfaceClass, type NodeSheetBatch } from "@exponential-at/ui/web"
import { SurfaceContext, type SurfaceContextValue } from "./context"
import { CLIENT_FUNCTIONS, memoFormatter, setPointer, type DataModel } from "@exponential-at/ui/web"
import { extensionCatalogs, extensionMacroNames, registeredExtensions, subscribeExtensions, type ReactExtension } from "./extensions"
import type { HostPlugin } from "./host"
import { NodeView } from "./node-view"
import { useMediaQuery } from "./platform"
import { openAllowed } from "@exponential-at/ui/web"
import { compiledTheme } from "@exponential-at/ui/web"
import type { SurfaceState } from "./use-surface"

export type ThemeInput = ResolvedTheme | ThemeSource | string

/** What a host can ask of a live surface (`SurfaceCommand`, a11y.json). */
export interface SurfaceHandle {
  run: (command: SurfaceCommand) => void
  /** Focus the node (or its first focusable descendant); false if absent. */
  focus: (id: string) => boolean
  announce: (text: string, live?: `polite` | `assertive`) => void
  scrollIntoView: (id: string) => boolean
  /** Round 2: item `index` (data order) of the List/Table `id` into view;
   *  false when no such list is painted. */
  scrollToIndex: (id: string, index: number, align?: ScrollAlign) => boolean
}

export interface ExponentialSurfaceProps {
  /** The state from `useSurface` (tree + data + templates)… */
  surface?: SurfaceState
  /** …or a normalized tree (a fixture) with an optional data model… */
  root?: UiNode | null
  /** …and its LIFTED templates (`ReduceResult.templates`, round 2 §4): a
   *  data template's component lives here, never in the tree. */
  templates?: Readonly<Record<string, UiNode>>
  data?: DataModel
  /** A built-in id (`exponential` | `neutral` | `playful` | `orange`), a theme file
   *  (resolved against the built-ins) or a resolved theme. */
  theme?: ThemeInput
  /** `light | dark | system` (default `system`: the platform's preference,
   *  followed live). */
  mode?: ModeName | `system`
  /** Scales `$control.*` and `$spacing.*` by the theme's `$density.*`. */
  density?: `compact` | `default` | `comfortable`
  /** `system` (default) follows `prefers-contrast: more`; `high` forces the
   *  theme's high-contrast overlays. */
  contrast?: `normal` | `high` | `system`
  /** BCP 47 (default `en-US`): formatting, calendar names, week start, the
   *  default text direction. */
  locale?: string
  /** IANA time zone dates and relative times format in (default the
   *  platform's). */
  timeZone?: string
  /** Built-in string overrides by id (`catalog/strings.json`). */
  strings?: Record<string, string>
  host?: HostPlugin
  extensions?: readonly ReactExtension[]
  /** Forced interaction states on every node (the builder's recipe sheet). */
  states?: readonly string[]
  /** Geometry mode: leaves become fixed boxes of the returned size. */
  measure?: (node: UiNode) => { w: number; h: number } | null
  /** Default: the locale's direction (`textDirection`). */
  direction?: `ltr` | `rtl`
  /** The surface width; default = the host's box. */
  width?: number | string
  /** The height of the VIEWPORT the surface is shown in: what the height and
   *  orientation style conditions test (the Rust core's `set_viewport`
   *  height). Default: an explicit inline `style.height` on the surface,
   *  else UNKNOWN (orientation = landscape, no min/max-height matches;
   *  contract §2) — never the surface's CONTENT height (a height rule must
   *  not feed back into its own measurement). The gpui host passes 0 (=
   *  unknown) today, so the default keeps web and desktop identical. */
  viewportHeight?: number
  /** An explicit id (defaults to React's `useId`); must be unique per page. */
  id?: string
  /** Host → surface commands (focus, announce, scrollIntoView). */
  handleRef?: Ref<SurfaceHandle>
  className?: string
  style?: CSSProperties
  children?: ReactNode
}

/** A theme prop, resolved: the theme to paint with and why the asked one
 *  was refused (empty when it loaded). */
export interface ThemeResolution {
  theme: ResolvedTheme
  issues: ThemeIssue[]
}

const resolvedCache = new Map<string, ThemeResolution>()

/** A theme prop → the resolved theme (built-ins cached by id, theme files
 *  resolved against the built-ins and cached by content). NEVER throws
 *  (round 4, VAPP-103): an unknown built-in id or an invalid theme file
 *  paints with the default theme and carries its issues, which the surface
 *  reports through `HostPlugin.onThemeIssues`. */
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

const EMPTY: readonly ReactExtension[] = []
const NO_TEMPLATES: Readonly<Record<string, UiNode>> = {}
const EMPTY_HOST: HostPlugin = {}

const useIsoLayoutEffect = typeof window === `undefined` ? useEffect : useLayoutEffect

/** What the surface knows of its box: the measured WIDTH and the condition
 *  height (an explicit inline height; see `viewportHeight`). */
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

/** The only layout facts that reach React: the active breakpoint and the
 *  matching JS-evaluated conditions (`data-xq`). A resize that changes
 *  neither re-renders nothing. */
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

const NO_CHUNKS: ReadonlyMap<string, string> = new Map()
/** One `dangerouslySetInnerHTML` object per batch (a new object rewrites the element). */
const batchHtml = new WeakMap<NodeSheetBatch, { __html: string }>()

const FOCUSABLE = `input:not([disabled]),textarea:not([disabled]),select:not([disabled]),button:not([disabled]),a[href],[tabindex]:not([tabindex="-1"]),[contenteditable="true"]`

/** `intlFormatter` built on its first use (VAPP-137): the platform zone
 *  costs an `Intl.DateTimeFormat` (~7 ms cold), and most surfaces format
 *  nothing. */
function lazyFormatter(locale: string, timeZone: string | undefined): Formatter {
  let base: Formatter | undefined
  const get = () => (base ??= intlFormatter(locale, timeZone ?? platformZone()))
  return {
    locale,
    number: (value, o) => get().number(value, o),
    currency: (value, currency, o) => get().currency(value, currency, o),
    percent: (value, o) => get().percent(value, o),
    date: (value, o) => get().date(value, o),
    relativeTime: (value, now) => get().relativeTime(value, now),
    plural: (value) => get().plural(value),
  }
}

const platformZone = (): string => {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || `UTC`
  } catch {
    return `UTC`
  }
}

/** Bundlers replace `process.env.NODE_ENV`; without one (a raw ESM load)
 *  it throws, which counts as production. Declared here so the published
 *  types build without Node's. */
declare const process: { env: { NODE_ENV?: string } }
const IS_DEV = (() => {
  try {
    return process.env.NODE_ENV !== `production`
  } catch {
    return false
  }
})()
const warnedTemplates = new Set<string>()

/** Dev only: a node whose `template.component` is not in `templates` paints
 *  NO items (round 2 §4: the reducer lifts templates out of the tree; pass
 *  the reduce result's `templates` next to `root`). */
function warnMissingTemplates(surfaceId: string, root: UiNode, templates: Record<string, UiNode>): void {
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

/** Whether a tree calls `formatRelativeTime` without a `now`, or has a
 *  Table with a `relativeTime` column (or bound columns, unknown until
 *  data): the surface then re-binds once a minute (round 2 §3). */
const isPlain = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

/** Structural equality, short-circuiting on identity at every level (an
 *  inline `data={{ items, filter }}` with stable values costs one pass over
 *  its top-level keys). */
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

function sameBudget(a: TemplateBudget | undefined, b: TemplateBudget | undefined): boolean {
  if (a === b) return true
  if (!a || !b || a.issue?.id !== b.issue?.id || a.issue?.message !== b.issue?.message || a.allowed.size !== b.allowed.size) return false
  for (const [k, v] of a.allowed) if (b.allowed.get(k) !== v) return false
  return true
}

type LiveMembers = Pick<SurfaceContextValue, `data` | `breakpoint` | `xq` | `mode`>

/** The context with its live members as getters over `live`. */
function withLive(value: Omit<SurfaceContextValue, keyof LiveMembers>, live: { current: LiveMembers }): SurfaceContextValue {
  return Object.defineProperties(value, {
    data: { get: () => live.current.data, enumerable: true, configurable: true },
    breakpoint: { get: () => live.current.breakpoint, enumerable: true, configurable: true },
    xq: { get: () => live.current.xq, enumerable: true, configurable: true },
    mode: { get: () => live.current.mode, enumerable: true, configurable: true },
  }) as SurfaceContextValue
}

export function ExponentialSurface({
  surface,
  root: rootProp,
  templates: templatesProp,
  data: dataProp,
  theme: themeProp,
  mode: modeSetting = `system`,
  density = `default`,
  contrast = `system`,
  locale = DEFAULT_LOCALE,
  timeZone,
  strings: stringOverrides,
  host,
  extensions: extensionsProp = EMPTY,
  states = EMPTY as unknown as readonly string[],
  measure,
  direction: directionProp,
  width,
  viewportHeight,
  id: idProp,
  handleRef,
  className,
  style,
  children,
}: ExponentialSurfaceProps) {
  const reactId = useId()
  const surfaceId = idProp ?? surface?.surfaceId ?? reactId.replace(/[^a-zA-Z0-9]/g, ``)
  const prefersDark = useMediaQuery(`(prefers-color-scheme: dark)`)
  const prefersContrast = useMediaQuery(`(prefers-contrast: more)`)
  const reducedMotion = useMediaQuery(`(prefers-reduced-motion: reduce)`)
  const hover = useMediaQuery(`(hover: hover)`, true)
  const mode: ModeName = modeSetting === `system` ? resolveMode(`system`, prefersDark) : modeSetting
  const resolution = useMemo(() => resolveThemeInput(themeProp), [themeProp])
  const baseTheme = resolution.theme
  // An unusable theme never crashes the host: the surface paints with the
  // default theme and hands the issues over once per resolution.
  const onThemeIssues = host?.onThemeIssues
  useEffect(() => {
    if (resolution.issues.length > 0) onThemeIssues?.(resolution.issues)
  }, [resolution, onThemeIssues])
  const theme = surfaceTheme(baseTheme, density, contrast === `high` || (contrast === `system` && prefersContrast))
  const global = useSyncExternalStore(subscribeExtensions, registeredExtensions, registeredExtensions)
  const extensions = useMemo(() => [...global, ...extensionsProp], [global, extensionsProp])
  const extensionDefs = useMemo(() => extensionCatalogs(extensions), [extensions])
  const macros = useMemo(() => extensionMacroNames(extensions), [extensions])
  const compiled = useMemo(() => compiledTheme(theme, { extensionMacros: macros }), [theme, macros])
  const direction = directionProp ?? textDirection(locale)
  const strings = useMemo(() => stringTable(stringOverrides ?? {}), [stringOverrides])
  const t = useCallback((id: string, params?: Record<string, unknown>) => formatString(strings[id] ?? id, params ?? {}), [strings])

  // Data: the surface state's, or local state over the `data` prop.
  // A `data` prop with new CONTENT replaces the local model (an inline
  // object re-created on every parent render keeps the user's edits).
  // Compared structurally against the last ADOPTED prop (identity first,
  // then per top-level key), never serialised.
  const [localData, setLocalData] = useState<DataModel>(dataProp ?? {})
  const [seenData, setSeenData] = useState(dataProp)
  if (dataProp !== seenData && !sameData(dataProp, seenData)) {
    setSeenData(dataProp)
    setLocalData(dataProp ?? {})
  }
  const data = surface ? surface.data : localData
  // Through a ref: the surface state is a new object on every change, and a
  // new `setData` would be a new context (every node re-rendered).
  const latestSurface = useRef(surface)
  latestSurface.current = surface
  const setData = useCallback((pointer: string, value: unknown) => {
    const current = latestSurface.current
    if (current) current.setData(pointer, value)
    else setLocalData((cur) => setPointer(cur, pointer, value))
  }, [])

  // VAPP-141: a new tree shares every unchanged node with the last one
  // (`share`), so a streamed `updateComponents` re-renders only the nodes
  // it changed.
  const lastTree = useRef<{ root: UiNode | null; templates: Readonly<Record<string, UiNode>> }>({ root: null, templates: NO_TEMPLATES })
  const rawRoot = surface ? surface.root : (rootProp ?? null)
  const root = useMemo(() => (lastTree.current.root = share(lastTree.current.root, rawRoot)), [rawRoot])
  // Round 2 §4: a data template's node comes from the reducer's LIFTED
  // `templates` (it is no longer in the tree, so never painted in place).
  const rawTemplates = (surface ? surface.templates : templatesProp) ?? NO_TEMPLATES
  const templates = useMemo(() => (lastTree.current.templates = share(lastTree.current.templates, rawTemplates)), [rawTemplates])
  const templateNode = useCallback((componentId: string) => templates[componentId], [templates])
  const templateRoots = useMemo(() => Object.values(templates), [templates])
  // VAPP-103: past `maxTemplateItems` items or `maxComponents` nodes per
  // surface the rest is not rendered (ONE issue, the Rust build's rule).
  // Kept while its counts are the same (it is in the context: a new one
  // re-renders every node).
  const lastBudget = useRef<TemplateBudget | undefined>(undefined)
  const budget = useMemo(() => {
    const next = root ? templateBudget(root, data, templateNode) : undefined
    if (sameBudget(lastBudget.current, next)) return lastBudget.current
    return (lastBudget.current = next)
  }, [root, data, templateNode])
  useEffect(() => {
    if (budget?.issue) console.warn(`[exponential-ui] ${surfaceId}: ${budget.issue.id}: ${budget.issue.message}`)
  }, [surfaceId, budget?.issue?.id, budget?.issue?.message])
  useEffect(() => {
    if (IS_DEV && root) warnMissingTemplates(surfaceId, root, templates)
  }, [surfaceId, root, templates])
  const sheet = useMemo(() => (root ? compileNodeSheet([root, ...templateRoots], surfaceId, theme) : { css: ``, queries: [], chunks: NO_CHUNKS }), [root, templateRoots, surfaceId, theme])
  // VAPP-152: the node sheet in `<style>` batches; a streamed node adds
  // one, the unchanged ones keep their element and text (no surface-wide
  // restyle).
  const lastBatches = useRef<NodeSheetBatch[]>([])
  const batches = useMemo(() => (lastBatches.current = nodeSheetBatches(lastBatches.current, sheet.chunks)), [sheet.chunks])

  const [rootEl, setRootEl] = useState<HTMLElement | null>(null)
  const [portal, setPortal] = useState<HTMLElement | null>(null)
  const [toastLayer, setToastLayer] = useState<HTMLElement | null>(null)
  // The box lives in a REF: the ResizeObserver tick updates it and only
  // pokes React when the derived layout (breakpoint + data-xq) changes, so
  // dragging a window edge does not re-render the tree 60 times a second.
  const box = useRef<SurfaceBox>({ width: typeof width === `number` ? width : 0, height: undefined, measured: false })
  const layoutInputs: LayoutInputs = { breakpoints: theme.tokens.breakpoint ?? {}, queries: sheet.queries, viewportHeight, hover, reducedMotion }
  const { breakpoint, xq } = surfaceLayout(box.current, layoutInputs)
  const inputsRef = useRef(layoutInputs)
  inputsRef.current = layoutInputs
  const [, setLayoutKey] = useState(``)
  useIsoLayoutEffect(() => {
    if (!rootEl) return
    const read = () => {
      const r = rootEl.getBoundingClientRect()
      // No layout yet (SSR, jsdom, display:none): keep the initial guess.
      if (!box.current.measured && r.width === 0 && r.height === 0) return
      // Only an EXPLICIT height is a viewport; a content-sized root is not.
      const explicit = rootEl.style.height !== `` && rootEl.style.height !== `auto`
      box.current = { width: r.width, height: explicit ? r.height : undefined, measured: true }
      const next = surfaceLayout(box.current, inputsRef.current)
      setLayoutKey(`${next.breakpoint ?? ``}|${next.xq}`)
    }
    read()
    if (typeof ResizeObserver === `undefined`) return
    const ro = new ResizeObserver(read)
    ro.observe(rootEl)
    return () => ro.disconnect()
  }, [rootEl])

  // Live regions: `announce` swaps the text (cleared first, so the same
  // message twice is spoken twice).
  const politeRef = useRef<HTMLDivElement>(null)
  const assertiveRef = useRef<HTMLDivElement>(null)
  const announce = useCallback((text: string, live: `polite` | `assertive` = `polite`) => {
    const el = live === `assertive` ? assertiveRef.current : politeRef.current
    if (!el) return
    el.textContent = ``
    setTimeout(() => {
      el.textContent = text
    }, 30)
  }, [])

  const functions = useMemo(() => ({ ...CLIENT_FUNCTIONS, ...(host?.functions ?? {}) }), [host?.functions])
  const formatter = useMemo(() => memoFormatter(lazyFormatter(locale, timeZone)), [locale, timeZone])
  // The surface clock: a tree that formats a relative time against "now"
  // re-binds once a minute (a new `now` reaches every node).
  const liveClock = useMemo(() => usesLiveClock(root) || usesLiveClock(templateRoots), [root, templateRoots])
  const [minute, setMinute] = useState(() => Date.now())
  useEffect(() => {
    if (!liveClock) return
    const timer = setInterval(() => setMinute(Date.now()), 60_000)
    return () => clearInterval(timer)
  }, [liveClock])
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const now = useMemo(() => () => Date.now(), [minute])
  const scrollers = useRef(new Map<string, (index: number, align?: ScrollAlign) => void>())
  const registerScroller = useCallback((id: string, scroll: (index: number, align?: ScrollAlign) => void) => {
    scrollers.current.set(id, scroll)
    return () => {
      if (scrollers.current.get(id) === scroll) scrollers.current.delete(id)
    }
  }, [])
  const paintFailures = useRef(new Map<string, string>()).current
  const hostValue = host ?? EMPTY_HOST
  // Every open passes the URL policy (the host's opener or a new tab).
  const openUrl = useCallback((url: string) => openAllowed(hostValue, url), [hostValue])

  // VAPP-141 + VAPP-130: data, breakpoint, xq and mode are LIVE members (getters over this
  // render's values); the context keeps its identity when only they change
  // and the layout effect below tells the subscribed nodes (`live.ts`).
  const live = useRef({ data, breakpoint, xq, mode })
  live.current = { data, breakpoint, xq, mode }
  const listeners = useRef(new Set<() => void>()).current
  const subscribe = useCallback(
    (listener: () => void) => {
      listeners.add(listener)
      return () => void listeners.delete(listener)
    },
    [listeners]
  )
  useIsoLayoutEffect(() => {
    for (const listener of [...listeners]) listener()
  }, [data, breakpoint, xq, mode, listeners])
  const ctx: SurfaceContextValue = useMemo(
    () => withLive({
      surfaceId,
      compiled,
      theme,
      host: hostValue,
      extensions,
      extensionDefs,
      setData,
      templateNode,
      templateBudget: budget,
      states,
      measure,
      portal,
      toastLayer,
      direction,
      functions,
      openUrl,
      locale,
      formatter,
      now,
      registerScroller,
      paintFailures,
      strings,
      t,
      reducedMotion,
      hover,
      announce,
      subscribe,
    }, live),
    [surfaceId, compiled, theme, hostValue, extensions, extensionDefs, setData, templateNode, budget, states, measure, portal, toastLayer, direction, functions, openUrl, locale, formatter, now, registerScroller, paintFailures, strings, t, reducedMotion, hover, announce, subscribe]
  )

  const nodeEl = useCallback((id: string): HTMLElement | null => rootEl?.querySelector<HTMLElement>(`[data-xui-id="${typeof CSS !== `undefined` && CSS.escape ? CSS.escape(id) : id.replace(/"/g, `\\"`)}"]`) ?? null, [rootEl])
  useImperativeHandle(
    handleRef,
    (): SurfaceHandle => {
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
        el.scrollIntoView({ block: `nearest`, behavior: reducedMotion ? `auto` : `smooth` })
        return true
      }
      const scrollToIndex = (id: string, index: number, align?: ScrollAlign) => {
        const scroll = scrollers.current.get(id)
        if (!scroll) return false
        scroll(index, align)
        return true
      }
      return {
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
    },
    [nodeEl, announce, reducedMotion]
  )

  const md = theme.tokens.breakpoint?.md ?? 768
  const scopeSel = `.${surfaceClass(surfaceId)}`
  const layerCss = `@layer xui-base{@container xui (width >= ${md}px){${scopeSel} .xui-toast-layer{left:auto;right:var(--xui-spacing-lg);transform:none;align-items:flex-end}${scopeSel}[dir="rtl"] .xui-toast-layer{right:auto;left:var(--xui-spacing-lg)}}}`

  // VAPP-141: React rewrites a `dangerouslySetInnerHTML` whenever the
  // OBJECT is new, and a rewritten sheet restyles the whole surface: one
  // object per sheet text.
  const themeHtml = useMemo(() => ({ __html: `${BASE_CSS}\n${compiled.css}` }), [compiled.css])
  const nodesHtml = useMemo(() => {
    const list: readonly NodeSheetBatch[] = batches.length ? batches : [{ chunks: NO_CHUNKS, css: sheet.css }]
    return list.map((batch, i) => {
      const css = i === 0 ? `${batch.css}${layerCss}` : batch.css
      const hit = batchHtml.get(batch)
      if (hit && hit.__html === css) return hit
      const html = { __html: css }
      batchHtml.set(batch, html)
      return html
    })
  }, [batches, sheet.css, layerCss])

  return (
    <div
      ref={setRootEl}
      className={[`xui-surface`, compiled.scope, surfaceClass(surfaceId), className].filter(Boolean).join(` `)}
      data-xui-surface={surfaceId}
      data-xui-mode={mode}
      data-xui-theme={theme.id}
      data-xui-density={density === `default` ? undefined : density}
      data-xui-motion={reducedMotion ? `reduce` : undefined}
      data-xq={xq || undefined}
      dir={direction}
      style={width === undefined ? style : { width, ...style }}
    >
      <style data-xui-style="theme" dangerouslySetInnerHTML={themeHtml} />
      {nodesHtml.map((html, i) => (
        <style key={i} data-xui-style="nodes" dangerouslySetInnerHTML={html} />
      ))}
      <SurfaceContext.Provider value={ctx}>
        <div className="xui-container">
          {root ? <NodeView node={root} /> : null}
          {children}
          <div ref={setPortal} className="xui-layer" data-xui-layer="overlay" />
          <div ref={setToastLayer} className="xui-toast-layer" data-xui-layer="toast" />
        </div>
      </SurfaceContext.Provider>
      <div ref={politeRef} className="xui-sr-only" aria-live="polite" aria-atomic="true" data-xui-live="polite" />
      <div ref={assertiveRef} className="xui-sr-only" aria-live="assertive" aria-atomic="true" data-xui-live="assertive" />
    </div>
  )
}
