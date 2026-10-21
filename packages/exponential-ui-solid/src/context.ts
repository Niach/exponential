// FEED-128: what every painted node reads from its surface (the React
// renderer's context.tsx, on Solid). The surface value is an object of
// GETTERS over the surface's signals and memos: read a field inside a
// tracking scope (JSX, `createMemo`, `createEffect`) and the reader follows
// it. Never spread it (`{...ctx}` snapshots every getter and loses the
// tracking); derive a changed copy with `deriveContext`.

import { createContext, useContext, type Accessor } from "solid-js"
import type { ExtensionDef, Formatter, ModeName, ResolvedTheme, ScrollAlign, TemplateBudget, UiNode } from "@exponential-at/ui"
import type { ClientFunction, CompiledTheme, DataModel, ResolveContext } from "@exponential-at/ui/web"
import type { HostPlugin } from "./host"
import type { SolidExtension } from "./extensions"

export interface SurfaceContextValue {
  readonly surfaceId: string
  readonly compiled: CompiledTheme
  /** The theme the surface runs with (density + contrast applied). */
  readonly theme: ResolvedTheme
  /** The RESOLVED mode (`system` already followed). */
  readonly mode: ModeName
  readonly host: HostPlugin
  readonly extensions: readonly SolidExtension[]
  readonly extensionDefs: readonly ExtensionDef[]
  readonly data: DataModel
  readonly setData: (pointer: string, value: unknown) => void
  /** A template component id → its node (the reducer's LIFTED `templates`). */
  readonly templateNode: (componentId: string) => UiNode | undefined
  /** VAPP-103: the template items counted against `maxTemplateItems`. */
  readonly templateBudget?: TemplateBudget
  /** Interaction states forced on every node (the recipe sheet). */
  readonly states: readonly string[]
  /** Geometry mode: every leaf becomes a fixed box of this size. */
  readonly measure?: (node: UiNode) => { w: number; h: number } | null
  /** Where overlays portal: the overlay LAYER inside the `xui` container. */
  readonly portal: HTMLElement | null
  /** The toast layer (above dialogs). */
  readonly toastLayer: HTMLElement | null
  /** The direction of the node being painted (round 2 §2). */
  readonly direction: `ltr` | `rtl`
  readonly functions: Record<string, ClientFunction>
  readonly openUrl: (url: string) => void
  readonly locale: string
  readonly formatter: Formatter
  /** The surface clock; ticks once a minute while the tree uses it. */
  readonly now: () => number
  readonly registerScroller: (id: string, scroll: (index: number, align?: ScrollAlign) => void) => () => void
  readonly paintFailures?: Map<string, string>
  readonly strings: Readonly<Record<string, string>>
  readonly t: (id: string, params?: Record<string, unknown>) => string
  /** `activeBreakpoint(surface width, theme.tokens.breakpoint)`; null = base. */
  readonly breakpoint: string | null
  /** The matching JS-evaluated conditions (`data-xq` tokens). */
  readonly xq?: string
  readonly reducedMotion: boolean
  readonly hover: boolean
  readonly announce: (text: string, live?: `polite` | `assertive`) => void
}

export const SurfaceContext = createContext<SurfaceContextValue>()

export function useSurfaceContext(): SurfaceContextValue {
  const ctx = useContext(SurfaceContext)
  if (!ctx) throw new Error(`exponential-ui: this component must render inside <ExponentialSurface>`)
  return ctx
}

/** A copy of `ctx` with some fields re-pointed (each override = a getter
 *  body), every other field still following the original. */
export function deriveContext(ctx: SurfaceContextValue, overrides: { [K in keyof SurfaceContextValue]?: () => SurfaceContextValue[K] }): SurfaceContextValue {
  const out = {} as SurfaceContextValue
  for (const key of Object.keys(Object.getOwnPropertyDescriptors(ctx))) {
    Object.defineProperty(out, key, { enumerable: true, configurable: true, get: () => (ctx as unknown as Record<string, unknown>)[key] })
  }
  for (const [key, get] of Object.entries(overrides)) if (get) Object.defineProperty(out, key, { enumerable: true, configurable: true, get: get as () => unknown })
  return out
}

/** The data scope of a template item (relative paths resolve here): an
 *  ACCESSOR, since a keyed item's index (and so its path) moves. */
export const ScopeContext = createContext<Accessor<string>>(() => ``)

/** The INSTANCE suffix of a template item (`.0`, `.alice`): appended to the
 *  ids of the nodes it renders (`data-xui-id`) while their CSS class stays
 *  the template node's own. */
export const InstanceContext = createContext<Accessor<string>>(() => ``)

export const useScope = (): Accessor<string> => useContext(ScopeContext)
export const useInstance = (): Accessor<string> => useContext(InstanceContext)

/** The resolve context of one evaluation (bindings + calls + `$string`).
 *  Reads the surface's getters: call it inside a memo to track them. */
export function resolveContextOf(ctx: SurfaceContextValue, scope: string): ResolveContext {
  return { data: ctx.data, scope, functions: ctx.functions, openUrl: ctx.openUrl, locale: ctx.locale, strings: ctx.strings, formatter: ctx.formatter, now: ctx.now }
}
