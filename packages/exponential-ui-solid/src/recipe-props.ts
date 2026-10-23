// VAPP-144: the recipe query of a native node, cached.

import { catalogView, recipeParts } from "@exponential-at/ui"
import type { ExtensionDef } from "@exponential-at/ui"

const NO_EXTENSIONS: readonly ExtensionDef[] = []
const recipeCache = new WeakMap<readonly ExtensionDef[], Map<string, { names: string[]; defaults: Record<string, unknown> } | null>>()

/** `nativeRecipeProps` (the recipe query a NATIVE node answers to: its
 *  `when` props with the catalog defaults filled in) with the catalog view
 *  and the part specs looked up ONCE per extension set and component: the
 *  shared function rebuilds both for every call, i.e. for every node. Same
 *  keys in the same order. */
export function nativeRecipeProps(component: string, props: Record<string, unknown>, extensions: readonly ExtensionDef[] = NO_EXTENSIONS): Record<string, unknown> {
  let byComponent = recipeCache.get(extensions)
  if (!byComponent) {
    byComponent = new Map()
    recipeCache.set(extensions, byComponent)
  }
  let hit = byComponent.get(component)
  if (hit === undefined) {
    const def = catalogView(extensions as ExtensionDef[]).components[component]
    const spec = recipeParts(extensions)[component]
    hit = def && spec ? { names: spec.props, defaults: Object.fromEntries(spec.props.map((n) => [n, def.props[n]?.default])) } : null
    byComponent.set(component, hit)
  }
  const out: Record<string, unknown> = {}
  if (!hit) return out
  for (const name of hit.names) {
    const value = props[name] ?? hit.defaults[name]
    if (value !== undefined) out[name] = value
  }
  return out
}
