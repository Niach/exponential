// VAPP-87 + round 1: what every painted node reads from its surface.

import { createContext, useContext } from "react"
import type { ExtensionDef, Formatter, ModeName, ResolvedTheme, ScrollAlign, TemplateBudget, UiNode } from "@exponential-at/ui"
import type { CompiledTheme } from "./theme-css"
import type { HostPlugin } from "./host"
import type { ReactExtension } from "./extensions"
import type { ClientFunction, DataModel, ResolveContext } from "./data"

export interface SurfaceContextValue {
  surfaceId: string
  compiled: CompiledTheme
  /** The theme the surface runs with (density + contrast applied). */
  theme: ResolvedTheme
  /** The RESOLVED mode (`system` already followed). */
  mode: ModeName
  host: HostPlugin
  extensions: readonly ReactExtension[]
  extensionDefs: readonly ExtensionDef[]
  data: DataModel
  setData: (pointer: string, value: unknown) => void
  /** A template component id → its node (round 2: the reducer's LIFTED
   *  `templates`); `undefined` when absent. */
  templateNode: (componentId: string) => UiNode | undefined
  /** VAPP-103: the surface's template items counted against
   *  `maxTemplateItems` (`templateBudget`); absent = every item renders. */
  templateBudget?: TemplateBudget
  /** Interaction states forced on every node (the recipe sheet). */
  states: readonly string[]
  /** Geometry mode: every leaf becomes a fixed box of this size. */
  measure?: (node: UiNode) => { w: number; h: number } | null
  /** Where overlays portal: the overlay LAYER inside the `xui` container
   *  (so container breakpoints still match in a Dialog, audit bug C). */
  portal: HTMLElement | null
  /** The toast layer (above dialogs). */
  toastLayer: HTMLElement | null
  /** The direction of the node being painted (round 2 §2: `direction` on
   *  ANY node, inherited; the surface's at the root). */
  direction: `ltr` | `rtl`
  functions: Record<string, ClientFunction>
  openUrl: (url: string) => void
  /** Round 1 §4: the surface locale (BCP 47). */
  locale: string
  /** Round 2 §3: the surface Formatter (Intl in `locale` + the time zone). */
  formatter: Formatter
  /** The surface clock (epoch ms) `formatRelativeTime` reads without `now`;
   *  ticks once a minute while the tree uses it. */
  now: () => number
  /** Round 2: a windowed List/Table registers its `scrollToIndex` under its
   *  painted id (the host command); returns the unregister. */
  registerScroller: (id: string, scroll: (index: number, align?: ScrollAlign) => void) => () => void
  /** VAPP-103: the painted ids whose painter failed → their props as they
   *  failed (reported once until the props change; an entry leaves when
   *  the node paints or unmounts, so it never outgrows the mounted tree). */
  paintFailures?: Map<string, string>
  /** The built-in string table (defaults + host overrides). */
  strings: Readonly<Record<string, string>>
  /** A built-in string by id with `{name}` placeholders filled. */
  t: (id: string, params?: Record<string, unknown>) => string
  /** `activeBreakpoint(surface width, theme.tokens.breakpoint)`; null =
   *  base. (The raw box is NOT in the context, nor in React state: the
   *  surface keeps it in a ref and re-renders only when the breakpoint or
   *  the matching height/orientation conditions (`data-xq`) change; nodes
   *  are memoized, so a surface re-render does not cascade by itself.) */
  breakpoint: string | null
  /** The surface's matching JS-evaluated conditions (`data-xq` tokens):
   *  a `@media` block's `direction` reads it. */
  xq?: string
  /** The platform asks for reduced motion. */
  reducedMotion: boolean
  /** A hover-capable pointer. */
  hover: boolean
  /** Speak a message through the surface's live regions. */
  announce: (text: string, live?: `polite` | `assertive`) => void
}

export const SurfaceContext = createContext<SurfaceContextValue | null>(null)

export function useSurfaceContext(): SurfaceContextValue {
  const ctx = useContext(SurfaceContext)
  if (!ctx) throw new Error(`exponential-ui: this component must render inside <ExponentialSurface>`)
  return ctx
}

/** The data scope of a template item (relative paths resolve here). */
export const ScopeContext = createContext<string>(``)

/** The INSTANCE suffix of a template item (`.0`, `.alice`): appended to the
 *  ids of the nodes it renders (`data-xui-id`) while their CSS class stays
 *  the template node's own, so every item wears the node sheet's rules. */
export const InstanceContext = createContext<string>(``)

/** The resolve context of one render (bindings + calls + `$string`). */
export function resolveContextOf(ctx: SurfaceContextValue, scope: string): ResolveContext {
  return { data: ctx.data, scope, functions: ctx.functions, openUrl: ctx.openUrl, locale: ctx.locale, strings: ctx.strings, formatter: ctx.formatter, now: ctx.now }
}
