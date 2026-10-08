// VAPP-92: the recipe CONTRACT as code — which parts each component has and
// which props a theme's `when` may key on. Natives are listed by hand in
// catalog/recipes.json; macro parts are derived from their templates in
// macros.json (every `part`, the macro's `recipeProps` + each part's
// `$recipe` keys), so the two files cannot disagree.

import recipesJson from "../catalog/recipes.json" with { type: "json" }
import { catalogView } from "./catalog"
import type { CatalogView } from "./catalog"
import type { ExtensionDef, MacroChild, MacroDef } from "./types"

export const RECIPE_STATES: readonly string[] = recipesJson.states
export const RECIPE_KEYS: readonly string[] = recipesJson.keys

export interface PartSpec {
  parts: string[]
  /** The prop names a `when` may use (besides `state`). */
  props: string[]
}

const NATIVE_PARTS = recipesJson.native as Record<string, PartSpec>

/** Every part of a template (children first-to-last, then the slot parts a
 *  template builds, e.g. AlertDialog's footer); `$children`/`$slot:` splices
 *  carry no part. */
function walkTemplate(tpl: MacroChild, parts: Set<string>, props: Set<string>): void {
  if (typeof tpl === `string`) return
  parts.add(tpl.part)
  for (const key of Object.keys(tpl.$recipe ?? {})) props.add(key)
  for (const child of tpl.children ?? []) walkTemplate(child, parts, props)
  for (const slot of Object.values(tpl.slots ?? {})) if (typeof slot !== `string`) walkTemplate(slot, parts, props)
}

/** A macro's parts and recipe props, read off its template. */
export function macroParts(def: MacroDef): PartSpec {
  const parts = new Set<string>()
  const props = new Set<string>(def.recipeProps)
  walkTemplate(def.root, parts, props)
  return { parts: [...parts], props: [...props] }
}

/** Every component (core + the given extensions) → its parts and `when`
 *  props. An extension's natives declare `recipe: {parts, props}` on their
 *  component def; without it a native gets `root` and its enum/boolean props. */
export function recipeParts(extensions: readonly ExtensionDef[] = []): Record<string, PartSpec> {
  const view: CatalogView = catalogView(extensions)
  const out: Record<string, PartSpec> = {}
  for (const [name, def] of Object.entries(view.components)) {
    if (def.kind === `macro`) {
      const macro = view.macros[name]
      if (macro) out[name] = macroParts(macro)
      continue
    }
    const declared = (def as { recipe?: PartSpec }).recipe
    if (NATIVE_PARTS[name]) out[name] = { parts: [...NATIVE_PARTS[name].parts], props: [...NATIVE_PARTS[name].props] }
    else if (declared) out[name] = { parts: [...declared.parts], props: [...declared.props] }
    else
      out[name] = {
        parts: [`root`],
        props: Object.entries(def.props)
          .filter(([, p]) => p.type === `enum` || p.type === `boolean`)
          .map(([k]) => k),
      }
  }
  return out
}

/** The recipe query a NATIVE node answers to: its `when` props with the
 *  catalog defaults filled in (a macro part carries its own `recipe`). */
export function nativeRecipeProps(
  component: string,
  props: Record<string, unknown>,
  extensions: readonly ExtensionDef[] = []
): Record<string, unknown> {
  const view = catalogView(extensions)
  const def = view.components[component]
  const spec = recipeParts(extensions)[component]
  const out: Record<string, unknown> = {}
  if (!def || !spec) return out
  for (const name of spec.props) {
    const value = props[name] ?? def.props[name]?.default
    if (value !== undefined) out[name] = value
  }
  return out
}
