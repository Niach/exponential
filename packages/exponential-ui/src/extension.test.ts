// VAPP-85: the extension contract.

import { describe, expect, test } from "bun:test"
import extensionFixture from "../fixtures/catalog-extension.json" with { type: "json" }
import { CORE_CATALOG_ID } from "./catalog"
import { defineExtension, validateExtension } from "./extension"
import { extensionSchema } from "./schema"
import type { ExtensionDef } from "./types"

const example = extensionFixture.extension as unknown as ExtensionDef

describe(`defineExtension`, () => {
  test(`accepts the example extension`, () => {
    expect(validateExtension(example)).toEqual([])
    expect(defineExtension(example)).toBe(example)
  })

  test(`refuses a core name, a missing macro template, a wrong base and a bad id`, () => {
    const shadow: ExtensionDef = { ...example, components: { ...example.components, Button: example.components.Sparkline } }
    expect(validateExtension(shadow)).toContain(`Button: shadows a core component`)
    const noTemplate: ExtensionDef = { ...example, macros: {} }
    expect(validateExtension(noTemplate)).toContain(`StatCard: a macro needs a template in macros`)
    const wrongBase: ExtensionDef = { ...example, extends: `https://example.com/other` }
    expect(validateExtension(wrongBase)).toContain(`extends must be ${CORE_CATALOG_ID}`)
    const badId: ExtensionDef = { ...example, id: `example` }
    expect(validateExtension(badId)).toContain(`id must be a URL-shaped catalog id`)
    expect(() => defineExtension(badId)).toThrow(/catalog id/)
  })

  test(`its schema extends the core and lists its components`, () => {
    const schema = extensionSchema(example, [`ui-check`, `ui-chevron-up`, `ui-chevron-down`, `ui-minus`]) as { $id: string; extends: string; components: Record<string, unknown>; $defs: { anyComponent: { oneOf: { $ref: string }[] } } }
    expect(schema.$id).toBe(example.id)
    expect(schema.extends).toBe(CORE_CATALOG_ID)
    expect(Object.keys(schema.components)).toEqual([`Sparkline`, `StatCard`])
    expect(schema.$defs.anyComponent.oneOf[0].$ref).toBe(`${CORE_CATALOG_ID}#/$defs/anyComponent`)
  })
})
