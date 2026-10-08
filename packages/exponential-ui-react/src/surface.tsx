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
  loadTheme,
  mediaMatches,
  parseMediaCondition,
  resolveMode,
  stringTable,
  textDirection,
  BUILTIN_THEMES,
  CORE_CATALOG_ID,
  DEFAULT_LOCALE,
  DEFAULT_THEME_ID,
} from "@exponential-at/ui"
import type { ModeName, ResolvedTheme, ThemeSource, UiNode, FlatComponent, SurfaceCommand } from "@exponential-at/ui"
import { BASE_CSS } from "./base-css"
import { compileNodeSheet, queryToken, surfaceClass, walkNodes } from "./box-css"
import { SurfaceContext, type SurfaceContextValue } from "./context"
import { CLIENT_FUNCTIONS, setPointer, type DataModel } from "./data"
import { extensionCatalogs, extensionMacroNames, registeredExtensions, subscribeExtensions, type ReactExtension } from "./extensions"
import type { HostPlugin } from "./host"
import { NodeView } from "./node-view"
import { useMediaQuery } from "./platform"
import { compiledTheme } from "./theme-css"
import { templateNodeFrom, type SurfaceState } from "./use-surface"

export type ThemeInput = ResolvedTheme | ThemeSource | string

/** What a host can ask of a live surface (`SurfaceCommand`, a11y.json). */
export interface SurfaceHandle {
  run: (command: SurfaceCommand) => void
  /** Focus the node (or its first focusable descendant); false if absent. */
  focus: (id: string) => boolean
  announce: (text: string, live?: `polite` | `assertive`) => void
  scrollIntoView: (id: string) => boolean
}

export interface ExponentialSurfaceProps {
  /** The state from `useSurface` (tree + data + templates)… */
  surface?: SurfaceState
  /** …or a normalized tree (a fixture) with an optional data model. */
  root?: UiNode | null
  data?: DataModel
  /** A built-in id (`exponential` | `neutral` | `playful`), a theme file
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

const resolvedCache = new Map<string, ResolvedTheme>()

/** A theme prop → the resolved theme (built-ins cached by id, theme files
 *  resolved against the built-ins and cached by content). */
export function resolveThemeInput(input: ThemeInput | undefined): ResolvedTheme {
  if (!input) return builtinTheme(DEFAULT_THEME_ID)
  if (typeof input === `string`) return builtinTheme(input)
  if (`chain` in input && Array.isArray((input as ResolvedTheme).chain)) return input as ResolvedTheme
  const key = JSON.stringify(input)
  let hit = resolvedCache.get(key)
  if (!hit) {
    hit = loadTheme(input, { themes: BUILTIN_THEMES })
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
const noComponents: readonly FlatComponent[] = []
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

function defaultOpenUrl(url: string) {
  if (typeof window !== `undefined`) window.open(url, `_blank`, `noopener,noreferrer`)
}

const FOCUSABLE = `input:not([disabled]),textarea:not([disabled]),select:not([disabled]),button:not([disabled]),a[href],[tabindex]:not([tabindex="-1"]),[contenteditable="true"]`

function findNode(root: UiNode, id: string): UiNode | undefined {
  for (const n of walkNodes(root)) if (n.id === id) return n
  return undefined
}

export function ExponentialSurface({
  surface,
  root: rootProp,
  data: dataProp,
  theme: themeProp,
  mode: modeSetting = `system`,
  density = `default`,
  contrast = `system`,
  locale = DEFAULT_LOCALE,
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
  const baseTheme = useMemo(() => resolveThemeInput(themeProp), [themeProp])
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
  const [localData, setLocalData] = useState<DataModel>(dataProp ?? {})
  const dataKey = useMemo(() => (dataProp === undefined ? `` : JSON.stringify(dataProp)), [dataProp])
  const [seenDataKey, setSeenDataKey] = useState(dataKey)
  if (dataKey !== seenDataKey) {
    setSeenDataKey(dataKey)
    setLocalData(dataProp ?? {})
  }
  const data = surface ? surface.data : localData
  const setData = useCallback(
    (pointer: string, value: unknown) => {
      if (surface) surface.setData(pointer, value)
      else setLocalData((cur) => setPointer(cur, pointer, value))
    },
    [surface]
  )

  const root = surface ? surface.root : (rootProp ?? null)
  const components = surface?.components ?? noComponents
  const catalogId = surface?.catalogId ?? CORE_CATALOG_ID
  // Template components, reduced once per component list (the reducer drops
  // unreferenced flat components; a nested tree names a node of its own).
  const templateNode = useMemo(() => {
    const cache = new Map<string, UiNode | undefined>()
    return (componentId: string) => {
      if (!cache.has(componentId)) cache.set(componentId, templateNodeFrom(components, catalogId, extensionDefs, componentId) ?? (root ? findNode(root, componentId) : undefined))
      return cache.get(componentId)
    }
  }, [components, catalogId, extensionDefs, root])
  const templateRoots = useMemo(() => {
    const out: UiNode[] = []
    if (!root) return out
    const seen = new Set<string>()
    const visit = (n: UiNode) => {
      for (const node of walkNodes(n)) {
        const c = node.template?.component
        if (!c || seen.has(c)) continue
        seen.add(c)
        const tpl = templateNode(c)
        if (tpl) {
          out.push(tpl)
          visit(tpl)
        }
      }
    }
    visit(root)
    return out
  }, [root, templateNode])
  const sheet = useMemo(() => (root ? compileNodeSheet([root, ...templateRoots], surfaceId, theme) : { css: ``, queries: [] }), [root, templateRoots, surfaceId, theme])

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
  const openUrl = host?.openUrl ?? defaultOpenUrl
  const hostValue = host ?? EMPTY_HOST

  const ctx: SurfaceContextValue = useMemo(
    () => ({
      surfaceId,
      compiled,
      theme,
      mode,
      host: hostValue,
      extensions,
      extensionDefs,
      data,
      setData,
      templateNode,
      states,
      measure,
      portal,
      toastLayer,
      direction,
      functions,
      openUrl,
      locale,
      strings,
      t,
      breakpoint,
      reducedMotion,
      hover,
      announce,
    }),
    [surfaceId, compiled, theme, mode, hostValue, extensions, extensionDefs, data, setData, templateNode, states, measure, portal, toastLayer, direction, functions, openUrl, locale, strings, t, breakpoint, reducedMotion, hover, announce]
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
      return {
        focus,
        scrollIntoView,
        announce,
        run: (command) => {
          if (`focus` in command) focus(command.focus.id)
          else if (`announce` in command) announce(command.announce.text, command.announce.live)
          else if (`scrollIntoView` in command) scrollIntoView(command.scrollIntoView.id)
        },
      }
    },
    [nodeEl, announce, reducedMotion]
  )

  const md = theme.tokens.breakpoint?.md ?? 768
  const scopeSel = `.${surfaceClass(surfaceId)}`
  const layerCss = `@layer xui-base{@container xui (width >= ${md}px){${scopeSel} .xui-toast-layer{left:auto;right:var(--xui-spacing-lg);transform:none;align-items:flex-end}${scopeSel}[dir="rtl"] .xui-toast-layer{right:auto;left:var(--xui-spacing-lg)}}}`

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
      <style data-xui-style="theme" dangerouslySetInnerHTML={{ __html: `${BASE_CSS}\n${compiled.css}` }} />
      <style data-xui-style="nodes" dangerouslySetInnerHTML={{ __html: `${sheet.css}${layerCss}` }} />
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
