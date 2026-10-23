// VAPP-106: the model-facing text of the catalog (every component and prop
// description, every example, the functions' descriptions) lives OUTSIDE the
// runtime catalog. `generated/core.runtime.json` is what the reducer, the
// validator and every renderer read (and what the Rust core embeds);
// `generated/core.descriptions.json` is the rest, keyed like the catalog.
// `describeCatalog()` puts the two back together IN PLACE on `coreCatalog`
// (once; later calls return at the first check): the `./prompt` and
// `./schema` entries call it before they read a description, so a browser
// that only renders never downloads a word of it. The generator
// (scripts/generate.ts) makes the split from catalog/core.catalog.json, the
// source, with `splitDescriptions`; `catalog.described.test.ts` proves the
// round trip.

import descriptionsJson from "../generated/core.descriptions.json" with { type: "json" }
import { coreCatalog } from "./catalog"
import type { CatalogSource, ComponentDef, DefSchema, PropSchema } from "./types"

/** The text a prop schema carries (recursing into `items`). */
export interface PropDescription {
  description?: string
  items?: PropDescription
}

/** The text of the catalog, keyed like the catalog itself. */
export interface CatalogDescriptions {
  components: Record<string, { description?: string; example?: Record<string, unknown>; props: Record<string, PropDescription> }>
  defs: Record<string, { description?: string; properties: Record<string, PropDescription> }>
  functions: { core: Record<string, string>; basic: Record<string, string> }
}

/** The runtime view of a schema object: its text may be absent (src/types.ts keeps it typed present). */
type Mutable<T> = { -readonly [K in keyof T as K extends `description` | `example` ? never : K]: T[K] } & { description?: string; example?: Record<string, unknown> }

function takeProp(schema: Mutable<PropSchema>): PropDescription {
  const out: PropDescription = {}
  if (schema.description !== undefined) {
    out.description = schema.description
    delete schema.description
  }
  if (schema.items) {
    const items = takeProp(schema.items as Mutable<PropSchema>)
    if (Object.keys(items).length) out.items = items
  }
  return out
}

function giveProp(schema: Mutable<PropSchema>, text: PropDescription | undefined): void {
  if (!text) return
  if (text.description !== undefined) schema.description = text.description
  if (text.items && schema.items) giveProp(schema.items as Mutable<PropSchema>, text.items)
}

/** Every `$comment` dropped, recursively (the source carries them for people). */
function withoutComments<T>(value: T): T {
  if (Array.isArray(value)) return value.map(withoutComments) as T
  if (typeof value === `object` && value !== null)
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .filter(([k]) => k !== `$comment`)
        .map(([k, v]) => [k, withoutComments(v)])
    ) as T
  return value
}

/** The source catalog split into what the runtime needs and the text only
 *  the prompt, the schema and the docs read. Pure: the source stays as it is. */
export function splitDescriptions(source: CatalogSource): { runtime: CatalogSource; descriptions: CatalogDescriptions } {
  const runtime = withoutComments(structuredClone(source)) as Mutable<CatalogSource>
  const descriptions: CatalogDescriptions = { components: {}, defs: {}, functions: { core: {}, basic: {} } }
  for (const [name, def] of Object.entries(runtime.components) as [string, Mutable<ComponentDef>][]) {
    const entry: CatalogDescriptions[`components`][string] = { props: {} }
    if (def.description !== undefined) {
      entry.description = def.description
      delete def.description
    }
    if (def.example !== undefined) {
      entry.example = def.example
      delete def.example
    }
    for (const [prop, schema] of Object.entries(def.props)) {
      const text = takeProp(schema as Mutable<PropSchema>)
      if (Object.keys(text).length) entry.props[prop] = text
    }
    descriptions.components[name] = entry
  }
  for (const [name, def] of Object.entries(runtime.defs) as [string, Mutable<DefSchema>][]) {
    const entry: CatalogDescriptions[`defs`][string] = { properties: {} }
    if (def.description !== undefined) {
      entry.description = def.description
      delete def.description
    }
    for (const [prop, schema] of Object.entries(def.properties)) {
      const text = takeProp(schema as Mutable<PropSchema>)
      if (Object.keys(text).length) entry.properties[prop] = text
    }
    descriptions.defs[name] = entry
  }
  for (const group of [`core`, `basic`] as const)
    for (const [name, fn] of Object.entries(runtime.functions[group]) as [string, { description?: string }][]) {
      if (fn.description === undefined) continue
      descriptions.functions[group][name] = fn.description
      delete fn.description
    }
  return { runtime, descriptions }
}

/** Puts the text back onto a runtime catalog, in place. */
export function applyDescriptions(target: CatalogSource, descriptions: CatalogDescriptions): void {
  for (const [name, text] of Object.entries(descriptions.components)) {
    const def = target.components[name] as Mutable<ComponentDef> | undefined
    if (!def) continue
    if (text.description !== undefined) def.description = text.description
    if (text.example !== undefined) def.example = text.example
    for (const [prop, propText] of Object.entries(text.props)) giveProp(def.props[prop] as Mutable<PropSchema>, propText)
  }
  for (const [name, text] of Object.entries(descriptions.defs)) {
    const def = target.defs[name] as Mutable<DefSchema> | undefined
    if (!def) continue
    if (text.description !== undefined) def.description = text.description
    for (const [prop, propText] of Object.entries(text.properties)) giveProp(def.properties[prop] as Mutable<PropSchema>, propText)
  }
  for (const group of [`core`, `basic`] as const)
    for (const [name, description] of Object.entries(descriptions.functions[group])) {
      const fn = target.functions[group][name] as { description?: string } | undefined
      if (fn) fn.description = description
    }
}

let described = false

/** `coreCatalog` with its descriptions and examples attached (once per
 *  process); what the prompt, the schema emitter and the docs read. */
export function describeCatalog(): CatalogSource {
  if (!described) {
    applyDescriptions(coreCatalog, descriptionsJson as unknown as CatalogDescriptions)
    described = true
  }
  return coreCatalog
}
