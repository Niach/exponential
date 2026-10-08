// VAPP-87: `<ExponentialSurface>` — one A2UI surface painted with real CSS.
// The root carries the theme's scope class + the variable block (per mode),
// the surface's node sheet, and an inner `xui` CONTAINER the breakpoints
// resolve against (a container query cannot target its own element, so the
// root and the container are two elements; overlays portal to the ROOT,
// outside the containment the container creates). Themes, extensions and
// the host plugin are props; nothing global is touched, so two surfaces on
// one page can wear two themes.

import { useCallback, useId, useMemo, useState, useSyncExternalStore, type CSSProperties, type ReactNode } from "react"
import { builtinTheme, loadTheme, BUILTIN_THEMES, DEFAULT_THEME_ID, CORE_CATALOG_ID } from "@exponential-at/ui"
import type { ModeName, ResolvedTheme, ThemeSource, UiNode, FlatComponent } from "@exponential-at/ui"
import { BASE_CSS } from "./base-css"
import { nodeSheet, surfaceClass } from "./box-css"
import { SurfaceContext, type SurfaceContextValue } from "./context"
import { CLIENT_FUNCTIONS, setPointer, type DataModel } from "./data"
import { extensionCatalogs, extensionMacroNames, registeredExtensions, subscribeExtensions, type ReactExtension } from "./extensions"
import type { HostPlugin } from "./host"
import { NodeView } from "./node-view"
import { compiledTheme } from "./theme-css"
import { templateNodeFrom, type SurfaceState } from "./use-surface"

export type ThemeInput = ResolvedTheme | ThemeSource | string

export interface ExponentialSurfaceProps {
  /** The state from `useSurface` (tree + data + templates)… */
  surface?: SurfaceState
  /** …or a normalized tree (a fixture) with an optional data model. */
  root?: UiNode | null
  data?: DataModel
  /** A built-in id (`exponential` | `neutral` | `playful`), a theme file
   *  (resolved against the built-ins) or a resolved theme. */
  theme?: ThemeInput
  mode?: ModeName
  host?: HostPlugin
  extensions?: readonly ReactExtension[]
  /** Forced interaction states on every node (the builder's recipe sheet). */
  states?: readonly string[]
  /** Geometry mode: leaves become fixed boxes of the returned size. */
  measure?: (node: UiNode) => { w: number; h: number } | null
  direction?: `ltr` | `rtl`
  /** The surface width; default = the host's box. */
  width?: number | string
  /** An explicit id (defaults to React's `useId`); must be unique per page. */
  id?: string
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

const EMPTY: readonly ReactExtension[] = []
const noComponents: readonly FlatComponent[] = []

function defaultOpenUrl(url: string) {
  if (typeof window !== `undefined`) window.open(url, `_blank`, `noopener,noreferrer`)
}

export function ExponentialSurface({
  surface,
  root: rootProp,
  data: dataProp,
  theme: themeProp,
  mode = `dark`,
  host,
  extensions: extensionsProp = EMPTY,
  states = EMPTY as unknown as readonly string[],
  measure,
  direction = `ltr`,
  width,
  id: idProp,
  className,
  style,
  children,
}: ExponentialSurfaceProps) {
  const reactId = useId()
  const surfaceId = idProp ?? surface?.surfaceId ?? reactId.replace(/[^a-zA-Z0-9]/g, ``)
  const theme = useMemo(() => resolveThemeInput(themeProp), [themeProp])
  const global = useSyncExternalStore(subscribeExtensions, registeredExtensions, registeredExtensions)
  const extensions = useMemo(() => [...global, ...extensionsProp], [global, extensionsProp])
  const extensionDefs = useMemo(() => extensionCatalogs(extensions), [extensions])
  const macros = useMemo(() => extensionMacroNames(extensions), [extensions])
  const compiled = useMemo(() => compiledTheme(theme, { extensionMacros: macros }), [theme, macros])

  // Data: the surface state's, or local state over the `data` prop.
  const [localData, setLocalData] = useState<DataModel>(dataProp ?? {})
  const data = surface ? surface.data : dataProp !== undefined && localData === dataProp ? dataProp : localData
  const setData = useCallback(
    (pointer: string, value: unknown) => {
      if (surface) surface.setData(pointer, value)
      else setLocalData((cur) => setPointer(cur, pointer, value))
    },
    [surface]
  )

  const root = surface ? surface.root : (rootProp ?? null)
  const sheet = useMemo(() => (root ? nodeSheet(root, surfaceId, theme.fonts) : ``), [root, surfaceId, theme.fonts])
  const [portal, setPortal] = useState<HTMLElement | null>(null)
  const functions = useMemo(() => ({ ...CLIENT_FUNCTIONS, ...(host?.functions ?? {}) }), [host?.functions])
  const openUrl = host?.openUrl ?? defaultOpenUrl
  const hostValue = host ?? {}
  const components = surface?.components ?? noComponents
  const catalogId = surface?.catalogId ?? CORE_CATALOG_ID
  const templateNode = useCallback((componentId: string) => templateNodeFrom(components, catalogId, extensionDefs, componentId), [components, catalogId, extensionDefs])

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
      direction,
      functions,
      openUrl,
    }),
    [surfaceId, compiled, theme, mode, hostValue, extensions, extensionDefs, data, setData, templateNode, states, measure, portal, direction, functions, openUrl]
  )

  return (
    <div
      ref={setPortal}
      className={[`xui-surface`, compiled.scope, surfaceClass(surfaceId), className].filter(Boolean).join(` `)}
      data-xui-surface={surfaceId}
      data-xui-mode={mode}
      data-xui-theme={theme.id}
      dir={direction}
      style={width === undefined ? style : { width, ...style }}
    >
      <style data-xui-style="theme" dangerouslySetInnerHTML={{ __html: `${BASE_CSS}\n${compiled.css}` }} />
      <style data-xui-style="nodes" dangerouslySetInnerHTML={{ __html: sheet }} />
      <SurfaceContext.Provider value={ctx}>
        <div className="xui-container">
          {root ? <NodeView node={root} /> : null}
          {children}
        </div>
      </SurfaceContext.Provider>
    </div>
  )
}
