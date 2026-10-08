// VAPP-87: what every painted node reads from its surface.

import { createContext, useContext } from "react"
import type { ExtensionDef, ModeName, ResolvedTheme, UiNode } from "@exponential-at/ui"
import type { CompiledTheme } from "./theme-css"
import type { HostPlugin } from "./host"
import type { ReactExtension } from "./extensions"
import type { ClientFunction, DataModel } from "./data"

export interface SurfaceContextValue {
  surfaceId: string
  compiled: CompiledTheme
  theme: ResolvedTheme
  mode: ModeName
  host: HostPlugin
  extensions: readonly ReactExtension[]
  extensionDefs: readonly ExtensionDef[]
  data: DataModel
  setData: (pointer: string, value: unknown) => void
  /** Resolves a template component id to its node (`useSurface` keeps the
   *  flat list for this); `undefined` when the tree is nested-only. */
  templateNode: (componentId: string) => UiNode | undefined
  /** Interaction states forced on every node (the recipe sheet). */
  states: readonly string[]
  /** Geometry mode: every leaf becomes a fixed box of this size. */
  measure?: (node: UiNode) => { w: number; h: number } | null
  /** Where overlays portal: the surface root once mounted. */
  portal: HTMLElement | null
  direction: `ltr` | `rtl`
  functions: Record<string, ClientFunction>
  openUrl: (url: string) => void
}

export const SurfaceContext = createContext<SurfaceContextValue | null>(null)

export function useSurfaceContext(): SurfaceContextValue {
  const ctx = useContext(SurfaceContext)
  if (!ctx) throw new Error(`exponential-ui: this component must render inside <ExponentialSurface>`)
  return ctx
}

/** The data scope of a template item (relative paths resolve here). */
export const ScopeContext = createContext<string>(``)
