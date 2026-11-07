// Round 9 (docs/round-9-contract.md §11): every catalog file we emit is an
// A2UI 1.0 catalog definition; the structural checker locks the rules.

import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import extensionFixture from "../fixtures/catalog-extension.json" with { type: "json" }
import basic10 from "../vendor/a2ui/v1_0/catalogs/basic/v1/catalog.json" with { type: "json" }
import { coreCatalog, CORE_CATALOG_ID, CORE_LITE_CATALOG_ID } from "./catalog"
import { checkCatalogDefinition, coreLiteSchema, coreSchema, extensionSchema } from "./schema"
import type { ComponentDef, ExtensionDef } from "./types"

type Doc = Record<string, any>
const pkgRoot = join(import.meta.dir, `..`)
const read = (rel: string): Doc => JSON.parse(readFileSync(join(pkgRoot, rel), `utf8`)) as Doc
const example = extensionFixture.extension as unknown as ExtensionDef
const core = coreSchema() as Doc
const lite = coreLiteSchema() as Doc
const ext = extensionSchema(example) as Doc

describe(`catalog definitions (round 9 §11)`, () => {
  test.each([
    [`core`, core],
    [`core-lite`, lite],
    [`the example extension`, ext],
    [`catalog/core.schema.json`, read(`catalog/core.schema.json`)],
    [`catalog/core-lite.schema.json`, read(`catalog/core-lite.schema.json`)],
  ])(`%s passes the structural checker`, (_, doc) => {
    expect(checkCatalogDefinition(doc)).toEqual([])
  })

  test(`the vendored 1.0 basic catalog passes every rule but flatness (upstream nests its props in allOf)`, () => {
    expect(checkCatalogDefinition(basic10, { flat: false })).toEqual([])
    const issues = checkCatalogDefinition(basic10)
    expect(issues.length).toBeGreaterThan(0)
    for (const issue of issues) expect(issue).toMatch(/allOf|properties: required/)
  })

  test(`the root: ids, version, lite id, instructions`, () => {
    expect(Object.keys(core)).toEqual([`$schema`, `$id`, `catalogId`, `protocolVersion`, `title`, `description`, `instructions`, `metadata`, `components`, `functions`, `$defs`])
    expect(core.$id).toBe(CORE_CATALOG_ID)
    expect(core.catalogId).toBe(CORE_CATALOG_ID)
    expect(core.protocolVersion).toBe(`1.0`)
    expect(core.metadata).toEqual({ extensions: { at_exponential_lite_catalog_id: CORE_LITE_CATALOG_ID } })
    expect(core.instructions).toContain(`"@path"`)
    expect(lite.$id).toBe(CORE_LITE_CATALOG_ID)
    expect(lite.metadata).toBeUndefined()
    expect(ext.$id).toBe(example.id)
  })

  test(`core-lite = the lite components, the core's functions`, () => {
    const liteNames = Object.entries(coreCatalog.components).filter(([, d]) => d.lite).map(([n]) => n)
    expect(Object.keys(lite.components)).toEqual(liteNames)
    expect(Object.keys(lite.functions)).toEqual(Object.keys(core.functions))
    expect(lite.$defs.anyComponent.oneOf.length).toBe(liteNames.length)
  })

  test(`an extension = the core's components then its own`, () => {
    const names = Object.keys(ext.components)
    expect(names.slice(-2)).toEqual([`TrendLine`, `StatCard`])
    expect(names.length).toBe(Object.keys(coreCatalog.components).length + 2)
    expect(ext.components.TrendLine.properties.tone.enum).toContain(`primary`)
  })

  test(`components are flat with the declared props (§5.3, §5.6)`, () => {
    const button = core.components.Button
    expect(button.properties.component).toEqual({ const: `Button` })
    expect(button.properties.children).toEqual({ $ref: `#/$defs/ChildList` })
    expect(button.properties.on).toEqual({ $ref: `#/$defs/On_Button` })
    expect(button.properties.style).toEqual({ $ref: `#/$defs/Style` })
    expect(button.properties.visible).toEqual({ $ref: `common_types.json#/$defs/DynamicBoolean` })
    expect(button.allowedChildren).toBeUndefined()
    expect(button.metadata.extensions.at_exponential_component).toEqual({ kind: `native`, group: `action`, lite: true, children: `one`, events: [`press`, `holdStart`, `holdEnd`, `holdCancel`] })
    expect(core.components.Text.allowedChildren).toEqual([])
    expect(core.components.Chip.allowedChildren).toBeUndefined()
    expect(core.components.Input.allOf).toEqual([{ $ref: `common_types.json#/$defs/Checkable` }])
    expect(core.components.Input.properties.checks).toBeUndefined()
    expect(core.components.Table.properties.slots).toEqual({ $ref: `#/$defs/Slots_Table` })
    expect(core.$defs.Slots_Table.additionalProperties).toEqual({ $ref: `common_types.json#/$defs/ComponentId` })
    expect(core.$defs.On_Button.properties.press).toEqual({ $ref: `#/$defs/EuiAction` })
    expect(core.$defs.ChildList.oneOf[1].properties.key).toEqual({ type: `string` })
    for (const [name, prop] of [[`Label`, `for`], [`Popover`, `anchor`], [`Menu`, `anchor`]])
      expect(core.components[name].properties[prop].$ref).toBe(`common_types.json#/$defs/ComponentId`)
    // Unknown names the unmapped component in props the envelope owns.
    expect(Object.keys(core.components.Unknown.properties)).toEqual([`component`, `style`, `visible`])
    for (const def of Object.values(core.components) as Doc[]) expect(def.unevaluatedProperties).toBeUndefined()
  })

  test(`prop schemas map onto the common Dynamic* types`, () => {
    expect(core.components.Input.properties.value.$ref).toBe(`common_types.json#/$defs/DynamicString`)
    expect(core.components.Markdown.properties.profile).toMatchObject({ enum: [`document`, `inline`], default: `document` })
    expect(core.components.Markdown.properties.variant).toMatchObject({ enum: [`body`, `caption`], default: `body` })
    expect(JSON.stringify(core.components.Stack.properties.direction)).toContain(`"required":["base"]`)
    expect(core.$defs.check.properties.condition.oneOf).toEqual([{ $ref: `common_types.json#/$defs/DataBinding` }, { $ref: `common_types.json#/$defs/FunctionCall` }])
    expect(core.$defs.check.required).toEqual([`condition`])
  })

  test(`functions follow §6.1`, () => {
    expect(Object.keys(core.functions)).toEqual([...coreCatalog.functions.names])
    for (const name of [`required`, `regex`, `length`, `numeric`, `email`]) expect(core.functions[name].returnType).toBe(`validationResult`)
    expect(core.functions.openUrl).toMatchObject({ returnType: `void`, allowedCallers: `rendererOnly`, requiresUserActivation: true })
    expect(core.functions.set).toMatchObject({ allowedCallers: `rendererOnly`, requiresUserActivation: false })
    expect(core.functions.bind.allowedCallers).toBe(`rendererOnly`)
    expect(core.functions.percent).toMatchObject({ returnType: `string`, allowedCallers: `rendererOrAgent`, requiresUserActivation: false })
    expect(core.functions.set.properties.args.properties.path).toEqual({ type: `string` })
    expect(core.functions.formatString.properties[`@call`]).toEqual({ const: `formatString` })
    expect(core.functions.formatString.required).toEqual([`@call`, `args`])
    expect(core.functions.add.required).toEqual([`@call`])
    for (const def of Object.values(core.functions) as Doc[]) expect(def.unevaluatedProperties).toBeUndefined()
  })

  test(`generation refuses names and refs outside the rules`, () => {
    const comp = (props: ComponentDef[`props`] = {}): ComponentDef => ({ kind: `native`, group: `data`, lite: true, children: `none`, description: `x`, props })
    const withComponents = (components: Record<string, ComponentDef>): ExtensionDef => ({ id: `https://example.com/catalogs/t/v1`, name: `t`, extends: CORE_CATALOG_ID, components })
    expect(() => extensionSchema(withComponents({ Surface: comp() }))).toThrow(/Surface/)
    expect(() => extensionSchema(withComponents({ "my-thing": comp() }))).toThrow(/identifier/)
    expect(() => extensionSchema(withComponents({ Thing: comp({ "a-b": { type: `string`, description: `x` } }) }))).toThrow(/identifier/)
    expect(() => extensionSchema(withComponents({ Thing: comp({ v: { type: `object`, shape: `nope`, description: `x` } }) }))).toThrow(/names nothing/)
    expect(() => extensionSchema(withComponents({ Thing: { ...comp(), parents: [`Nope`] } }))).toThrow(/parents/)
  })

  test(`the checker reports each rule`, () => {
    const bad = structuredClone(core) as Doc
    bad.liteCatalogId = `x`
    bad.protocolVersion = `0.9`
    bad.components.Button.oneOf = []
    bad.components.Text.properties.component = { const: `Txt` }
    bad.functions.app_toast = { ...bad.functions.percent, properties: { "@call": { const: `app_toast` } } }
    bad.functions.a__b = { ...bad.functions.percent, properties: { "@call": { const: `a__b` } } }
    bad.functions.percent = { ...bad.functions.percent, unevaluatedProperties: false }
    bad.$defs.Style = { $ref: `https://example.com/x.json` }
    const issues = checkCatalogDefinition(bad)
    for (const needle of [`root key liteCatalogId`, `/protocolVersion`, `no root oneOf/anyOf`, `/components/Text/properties/component`, `reserved host prefix`, `contains __`, `key unevaluatedProperties is not allowed`, `is not an allowed target`, `anyFunction: oneOf must reference`])
      expect(issues.some((i) => i.includes(needle)), needle).toBe(true)
  })
})
