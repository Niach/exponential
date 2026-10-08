// VAPP-87: EXTENSIONS. An extension = its catalog (`defineExtension` from
// `@exponential-at/ui`: own id, extends the core, natives + macros) plus one
// React component per NATIVE kind it adds (macros expand in the reducer and
// need no component). `registerExtension` adds one to the module registry
// every surface reads; the `extensions` prop of `ExponentialSurface` adds
// per-surface ones. An extension may also OVERRIDE a core native's painter
// (`components: { Switch: MySwitch }`), the VAPP-92 "painter override".

import { defineExtension } from "@exponential-at/ui"
import type { ExtensionDef } from "@exponential-at/ui"
import type { ComponentType } from "react"
import type { ExtensionComponentProps } from "./host"

export type ExtensionComponent = ComponentType<ExtensionComponentProps>

export interface ReactExtension {
  /** The extension catalog (validated). `null` for a pure painter override. */
  catalog: ExtensionDef | null
  components: Record<string, ExtensionComponent>
}

export interface RegisterExtensionOptions {
  catalog?: ExtensionDef
  components?: Record<string, ExtensionComponent>
}

const registry: ReactExtension[] = []
const listeners = new Set<() => void>()

/** Register an extension for every surface on the page. Returns an
 *  unregister function. */
export function registerExtension(options: RegisterExtensionOptions): () => void {
  const ext: ReactExtension = {
    catalog: options.catalog ? defineExtension(options.catalog) : null,
    components: options.components ?? {},
  }
  registry.push(ext)
  for (const l of listeners) l()
  return () => {
    const i = registry.indexOf(ext)
    if (i >= 0) registry.splice(i, 1)
    for (const l of listeners) l()
  }
}

/** A per-surface extension (the `extensions` prop), validated once. */
export function defineReactExtension(options: RegisterExtensionOptions): ReactExtension {
  return { catalog: options.catalog ? defineExtension(options.catalog) : null, components: options.components ?? {} }
}

export function registeredExtensions(): readonly ReactExtension[] {
  return registry
}

export function subscribeExtensions(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

/** The catalog defs of a list of React extensions (for the reducer). */
export function extensionCatalogs(extensions: readonly ReactExtension[]): ExtensionDef[] {
  return extensions.flatMap((e) => (e.catalog ? [e.catalog] : []))
}

/** The macro names extensions add (their parts live in the `xui-part` layer). */
export function extensionMacroNames(extensions: readonly ReactExtension[]): Set<string> {
  const out = new Set<string>()
  for (const ext of extensions) {
    for (const [name, def] of Object.entries(ext.catalog?.components ?? {})) if (def.kind === `macro`) out.add(name)
  }
  return out
}

export function extensionComponent(extensions: readonly ReactExtension[], component: string): ExtensionComponent | undefined {
  for (let i = extensions.length - 1; i >= 0; i--) {
    const hit = extensions[i].components[component]
    if (hit) return hit
  }
  return undefined
}
