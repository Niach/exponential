// VAPP-106: the runtime catalog ships without the model-facing text; the
// split and its reverse are exact.

import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { applyDescriptions, describeCatalog, splitDescriptions } from "./catalog.described"
import type { CatalogDescriptions } from "./catalog.described"
import { coreCatalog } from "./catalog"
import type { CatalogSource } from "./types"

// Read from disk, not imported: the test preload (`describeCatalog()`) has
// already attached the text to the imported runtime JSON object, which IS
// `coreCatalog`.
const read = <T,>(rel: string): T => JSON.parse(readFileSync(join(import.meta.dir, `..`, rel), `utf8`)) as T
const source = read<CatalogSource>(`catalog/core.catalog.json`)
const runtimeJson = read<CatalogSource>(`generated/core.runtime.json`)
const descriptionsJson = read<CatalogDescriptions>(`generated/core.descriptions.json`)

/** Every string under a `description` or `example` key at a doc position. */
function docStrings(value: unknown, path: string[] = []): string[] {
  if (Array.isArray(value)) return value.flatMap((v) => docStrings(v, path))
  if (typeof value !== `object` || value === null) return []
  return Object.entries(value).flatMap(([k, v]) => {
    // A prop NAMED description (Card.description, option.description) is a schema object, not text.
    if ((k === `description` && typeof v === `string`) || k === `example` || k === `$comment`) return [`${path.join(`/`)}/${k}`]
    return docStrings(v, [...path, k])
  })
}

describe(`the runtime catalog (VAPP-106)`, () => {
  test(`carries no description, example or $comment`, () => {
    expect(docStrings(runtimeJson)).toEqual([])
  })

  test(`has every component, prop, def, enum and function of the source`, () => {
    const runtime = runtimeJson
    expect(Object.keys(runtime.components)).toEqual(Object.keys(source.components))
    for (const [name, def] of Object.entries(source.components)) expect(Object.keys(runtime.components[name].props), name).toEqual(Object.keys(def.props))
    expect(Object.keys(runtime.defs)).toEqual(Object.keys(source.defs))
    expect(runtime.enums).toEqual(source.enums)
    expect(runtime.functions.names).toEqual(source.functions.names)
    expect(runtime.builtinIcons).toEqual(Object.fromEntries(Object.entries(source.builtinIcons).filter(([k]) => k !== `$comment`)))
  })

  test(`split + apply is the source minus its comments`, () => {
    const { runtime, descriptions } = splitDescriptions(source)
    expect(runtime).toEqual(runtimeJson)
    expect(descriptions).toEqual(descriptionsJson)
    applyDescriptions(runtime, descriptions)
    const expected = JSON.parse(JSON.stringify(source, (k, v) => (k === `$comment` ? undefined : v))) as CatalogSource
    expect(runtime).toEqual(expected)
  })

  test(`the descriptions hold every component's text and example`, () => {
    for (const [name, def] of Object.entries(source.components)) {
      const text = descriptionsJson.components[name]
      expect(text.description, name).toBe(def.description)
      expect(text.example, name).toEqual(def.example)
      for (const [prop, schema] of Object.entries(def.props)) expect(text.props[prop]?.description, `${name}.${prop}`).toBe(schema.description)
    }
  })

  test(`describeCatalog attaches the text to the shared catalog, once`, () => {
    const described = describeCatalog()
    expect(described).toBe(coreCatalog)
    expect(described.components.Button.description).toBe(source.components.Button.description)
    expect(described.functions.core.percent.description).toBe(source.functions.core.percent.description)
    expect(describeCatalog()).toBe(coreCatalog)
  })
})
