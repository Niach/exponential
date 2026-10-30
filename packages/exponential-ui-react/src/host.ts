// VAPP-87: what an EMBEDDING APP provides (the host plugin) and what the
// renderer sends it. The plugin knows no transport: actions and input edits
// are plain objects the host forwards wherever it likes. VAPP-91: the host
// API (`@exponential_app/eui` `ExponentialHost`: transport, functions,
// bindings, negotiation, policy) plugs in through `hostPlugin(host)` /
// `<HostSurface>` (host-surface.tsx).

import type { ComponentType, SVGProps } from "react"
import type { UiNode, ResolvedTheme, ModeName } from "@exponential_app/eui"
import type { HostPlugin as WebHostPlugin, IconMap as WebIconMap } from "@exponential_app/eui/web"

// FEED-128: the framework-free half (events, paint errors, source options,
// the plugin shape) lives in `@exponential_app/eui/web` (host-types.ts); React
// pins its component types.
export type { SurfaceActionEvent, SurfaceInputEvent, PaintError, SourceOption } from "@exponential_app/eui/web"

/** An icon component the host maps a registry name to (`lucide-react`'s
 *  `LucideIcon` shape fits). */
export type IconComponent = ComponentType<SVGProps<SVGSVGElement> & { size?: number | string; className?: string }>
export type IconMap = WebIconMap<IconComponent>

/** The host plugin on React: icons are React components, `Markdown` a
 *  React component taking `{text, className}`. */
export type HostPlugin = WebHostPlugin<IconComponent, ComponentType<{ text: string; className?: string }>>

/** What an extension component receives: the node, its props resolved, the
 *  theme and mode, the rendered children, and an emitter for its events. */
export interface ExtensionComponentProps {
  node: UiNode
  props: Record<string, unknown>
  theme: ResolvedTheme
  mode: ModeName
  tokens: ResolvedTheme[`tokens`]
  /** The node's children, already rendered. */
  children?: React.ReactNode
  /** Rendered slots by name. */
  slots: Record<string, React.ReactNode>
  /** Fire one of the node's `on` handlers. `own` = what the component
   *  just WROTE, by prop (`{value: "ship"}`): the action resolves against
   *  the data with it applied (round 4 own write), never inferred from the
   *  payload. */
  emit: (event: string, payload?: Record<string, unknown>, own?: Record<string, unknown>) => void
  /** The element attributes the renderer wants on the root (classes, data). */
  rootProps: Record<string, unknown>
}
