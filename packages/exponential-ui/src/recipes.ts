// VAPP-92: the recipe CONTRACT as code — which parts each component has and
// which props a theme's `when` may key on. Natives are listed by hand in
// catalog/recipes.json; macro parts are derived from their templates in
// macros.json (every `part`, the macro's `recipeProps` + each part's
// `$recipe` keys), so the two files cannot disagree.

import recipesJson from "../catalog/recipes.json" with { type: "json" }
import { catalogView } from "./catalog"
import type { CatalogView } from "./catalog"
import type { RecipeQuery } from "./theme-types"
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
  if (typeof tpl === `string` || `$slot` in tpl) return
  parts.add(tpl.part)
  for (const key of Object.keys(tpl.$recipe ?? {})) props.add(key)
  for (const child of tpl.children ?? []) walkTemplate(child, parts, props)
  for (const slot of Object.values(tpl.slots ?? {})) if (typeof slot !== `string`) walkTemplate(slot, parts, props)
  // eApps U3: a data template's item is a part too.
  if (tpl.$template) walkTemplate(tpl.$template.item, parts, props)
}

/** A macro's parts and recipe props, read off its template. */
export function macroParts(def: MacroDef): PartSpec {
  const parts = new Set<string>()
  const props = new Set<string>(def.recipeProps)
  walkTemplate(def.root, parts, props)
  for (const alternative of def.alternatives ?? []) walkTemplate(alternative.root, parts, props)
  for (const part of def.rootParts ?? []) parts.add(part)
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

interface RecipeTables {
  view: CatalogView
  parts: Record<string, PartSpec>
}

let coreTables: RecipeTables | undefined
const tablesByExtensions = new WeakMap<readonly ExtensionDef[], RecipeTables>()

/** VAPP-147: the catalog view and the part table `nativeRecipeProps` reads,
 *  built once per extension list (every painted node asks, on every
 *  render: rebuilding both per call was most of a web renderer's bind). An
 *  empty list is the core's. Read-only: callers get copies from
 *  `recipeParts`. */
function recipeTables(extensions: readonly ExtensionDef[]): RecipeTables {
  if (extensions.length === 0) return (coreTables ??= { view: catalogView(), parts: recipeParts() })
  let hit = tablesByExtensions.get(extensions)
  if (!hit) tablesByExtensions.set(extensions, (hit = { view: catalogView(extensions), parts: recipeParts(extensions) }))
  return hit
}

/** The recipe query a NATIVE node answers to: its `when` props with the
 *  catalog defaults filled in (a macro part carries its own `recipe`). */
export function nativeRecipeProps(
  component: string,
  props: Record<string, unknown>,
  extensions: readonly ExtensionDef[] = []
): Record<string, unknown> {
  const { view, parts } = recipeTables(extensions)
  const def = view.components[component]
  const spec = parts[component]
  const out: Record<string, unknown> = {}
  if (!def || !spec) return out
  for (const name of spec.props) {
    const value = props[name] ?? def.props[name]?.default
    if (value !== undefined) out[name] = value
  }
  return out
}

/** Round 9 §9.1 (fixtures/markdown-inline.json): the recipes an INLINE-profile
 *  Markdown part paints with, first to last (later rules over earlier): a
 *  paragraph IS a Text of its `variant`, a heading a Heading of its level
 *  (the `Heading` macro: Text `title` + `Heading/root`; h5, h6 paint h4). */
export function markdownInlineRecipes(part: `paragraph` | `heading`, options: { variant?: string; level?: number } = {}): RecipeQuery[] {
  if (part === `paragraph`) return [{ component: `Text`, part: `root`, props: { variant: options.variant ?? `body` } }]
  return [
    { component: `Text`, part: `root`, props: { variant: `title` } },
    { component: `Heading`, part: `root`, props: { level: `h${Math.min(Math.max(options.level ?? 2, 1), 4)}` } },
  ]
}

/** #EXP-1760: the macro whose ROOT a native node is, when that macro styles
 *  the native's `part` too (`rootParts`: an IconPicker's Popover
 *  `content`), else null. Web painters add `<Macro>/<part>`'s class beside
 *  the native's; the core layers the same recipe (`macro_query`). */
export function rootPartMacro(node: { component: string; recipe?: { macro?: string; part?: string } }, part: string, macros: Readonly<Record<string, MacroDef>> = catalogView().macros): string | null {
  const macro = node.recipe?.part === `root` ? node.recipe.macro : undefined
  if (!macro || macro === node.component) return null
  return macros[macro]?.rootParts?.includes(part) ? macro : null
}
