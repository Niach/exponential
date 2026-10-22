// FEED-128: EXTENSIONS on Solid (React's extensions.ts). An extension = its
// catalog (`defineExtension` from `@exponential-at/ui`) plus one Solid
// component per NATIVE kind it adds. `registerExtension` adds one to the
// module registry every surface reads (a signal: mounted surfaces follow);
// the `extensions` prop adds per-surface ones. An extension may also
// OVERRIDE a core native's painter (`components: { Switch: MySwitch }`).

import { createSignal, type Component } from "solid-js"
import { defineExtension } from "@exponential-at/ui"
import type { ExtensionDef } from "@exponential-at/ui"
import type { ExtensionComponentProps } from "./host"

export type ExtensionComponent = Component<ExtensionComponentProps>

export interface SolidExtension {
  /** The extension catalog (validated). `null` for a pure painter override. */
  catalog: ExtensionDef | null
  components: Record<string, ExtensionComponent>
}

export interface RegisterExtensionOptions {
  catalog?: ExtensionDef
  components?: Record<string, ExtensionComponent>
}

const [registry, setRegistry] = createSignal<readonly SolidExtension[]>([])
const listeners = new Set<() => void>()

/** Register an extension for every surface on the page (last wins).
 *  Returns an unregister function. */
export function registerExtension(options: RegisterExtensionOptions): () => void {
  const ext = defineSolidExtension(options)
  setRegistry((cur) => [...cur, ext])
  for (const l of listeners) l()
  return () => {
    setRegistry((cur) => cur.filter((e) => e !== ext))
    for (const l of listeners) l()
  }
}

/** A per-surface extension (the `extensions` prop), validated once. */
export function defineSolidExtension(options: RegisterExtensionOptions): SolidExtension {
  return { catalog: options.catalog ? defineExtension(options.catalog) : null, components: options.components ?? {} }
}

/** The registered extensions (a reactive read). */
export function registeredExtensions(): readonly SolidExtension[] {
  return registry()
}

export function subscribeExtensions(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** The catalog defs of a list of extensions (for the reducer). */
export function extensionCatalogs(extensions: readonly SolidExtension[]): ExtensionDef[] {
  return extensions.flatMap((e) => (e.catalog ? [e.catalog] : []))
}

/** The macro names extensions add (their parts live in the `xui-part` layer). */
export function extensionMacroNames(extensions: readonly SolidExtension[]): Set<string> {
  const out = new Set<string>()
  for (const ext of extensions) {
    for (const [name, def] of Object.entries(ext.catalog?.components ?? {})) if (def.kind === `macro`) out.add(name)
  }
  return out
}

export function extensionComponent(extensions: readonly SolidExtension[], component: string): ExtensionComponent | undefined {
  for (let i = extensions.length - 1; i >= 0; i--) {
    const hit = extensions[i].components[component]
    if (hit) return hit
  }
  return undefined
}
