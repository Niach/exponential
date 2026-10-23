// FEED-128: what an EMBEDDING APP provides (the host plugin) and what the
// Solid renderer sends it. The framework-free half (events, paint errors,
// source options, the plugin shape) is `@exponential_app/ui/web`
// (host-types.ts); Solid pins its component types. VAPP-91: the host API
// plugs in through `hostPlugin(host)` / `<HostSurface>` (host-surface.tsx).

import type { Component, JSX } from "solid-js"
import type { LucideProps } from "lucide-solid"
import type { UiNode, ResolvedTheme, ModeName } from "@exponential_app/ui"
import type { HostPlugin as WebHostPlugin, IconMap as WebIconMap } from "@exponential_app/ui/web"

export type { SurfaceActionEvent, SurfaceInputEvent, PaintError, SourceOption } from "@exponential_app/ui/web"

/** An icon component the host maps a registry name to (`lucide-solid`'s
 *  icon shape). */
export type IconComponent = (props: LucideProps) => JSX.Element
export type IconMap = WebIconMap<IconComponent>

/** The host plugin on Solid: icons are Solid components, `Markdown` a Solid
 *  component taking `{text, class}`. */
export type HostPlugin = WebHostPlugin<IconComponent, Component<{ text: string; class?: string }>>

/** What an extension component receives (React's `ExtensionComponentProps`
 *  as a Solid props object: every field is a live getter, never destructure
 *  it): the node, its props resolved, the theme and mode, the rendered
 *  children, and an emitter for its events. */
export interface ExtensionComponentProps {
  readonly node: UiNode
  readonly props: Record<string, unknown>
  readonly theme: ResolvedTheme
  readonly mode: ModeName
  readonly tokens: ResolvedTheme[`tokens`]
  /** The node's children: read ONCE (each read paints them again). */
  readonly children?: JSX.Element
  /** Rendered slots by name: read each ONCE. */
  readonly slots: Record<string, JSX.Element>
  /** Fire one of the node's `on` handlers. `own` = what the component just
   *  WROTE, by prop (round 4 own write). */
  readonly emit: (event: string, payload?: Record<string, unknown>, own?: Record<string, unknown>) => Promise<void> | void
  /** The element attributes the renderer wants on the root (`class`,
   *  `data-xui-id`, `data-xui-c`, recipe `data-r-*`/`data-m-*`, `data-xs`,
   *  aria, `dir`, `style` in Solid's kebab-case form). */
  readonly rootProps: Record<string, unknown>
}
