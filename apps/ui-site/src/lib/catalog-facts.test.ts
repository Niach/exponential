import { describe, expect, test } from "bun:test"
import basicMap from "@exponential-at/ui/catalog/basic-map.json"
import coreCatalog from "@exponential-at/ui/catalog/core.catalog.json"
import { CATALOG_ID, COMPONENT_DOCS, LITE_CATALOG_ID, componentSlug } from "./catalog"
import { BASIC_FUNCTION_COUNT, COMPONENT_COUNT, CORE_CATALOG, FUNCTION_COUNT, CORE_LITE_CATALOG, MACRO_COUNT, NATIVE_COUNT, componentSlugOf } from "./catalog-facts"

describe(`catalog facts`, () => {
  test(`ids and counts match the generated docs`, () => {
    expect(CORE_CATALOG).toBe(CATALOG_ID)
    expect(CORE_LITE_CATALOG).toBe(LITE_CATALOG_ID)
    expect(COMPONENT_COUNT).toBe(COMPONENT_DOCS.length)
    expect(NATIVE_COUNT).toBe(COMPONENT_DOCS.filter((d) => d.kind === `native`).length)
    expect(MACRO_COUNT).toBe(COMPONENT_DOCS.filter((d) => d.kind === `macro`).length)
    expect(FUNCTION_COUNT).toBe(coreCatalog.functions.names.length)
    expect(BASIC_FUNCTION_COUNT).toBe(FUNCTION_COUNT - Object.keys(coreCatalog.functions.core).length)
  })

  test(`every component's page slug is its kebab-case name`, () => {
    for (const doc of COMPONENT_DOCS) expect(componentSlugOf(doc.name)).toBe(componentSlug(doc))
  })

  test(`every A2UI basic mapping target has a component page`, () => {
    const names = new Set(COMPONENT_DOCS.map((d) => d.name))
    for (const rule of Object.values(basicMap.components)) expect(names.has((rule as { to: string }).to)).toBe(true)
  })

  test(`the content pages link only real components`, async () => {
    const names = new Set(COMPONENT_DOCS.map((d) => d.name))
    const source = await Bun.file(new URL(`../pages/Concepts.tsx`, import.meta.url)).text()
    for (const [, name] of source.matchAll(/<C name="([A-Za-z]+)"/g)) expect(names.has(name)).toBe(true)
  })
})
