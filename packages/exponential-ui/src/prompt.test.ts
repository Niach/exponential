// VAPP-85: the model-facing prompt and its budget on record.

import { describe, expect, test } from "bun:test"
import budget from "../fixtures/prompt-budget.json" with { type: "json" }
import extensionFixture from "../fixtures/catalog-extension.json" with { type: "json" }
import { CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, componentNames, coreCatalog } from "./catalog"
import { defineExtension } from "./extension"
import { catalogPrompt, estimateTokens } from "./prompt"
import type { ExtensionDef } from "./types"

describe(`catalog prompt`, () => {
  const full = catalogPrompt()

  test(`records the full-catalog token count and stays under budget`, () => {
    expect(budget.full).toEqual({ components: componentNames().length, chars: full.length, tokens: estimateTokens(full) })
    expect(budget.full.tokens).toBeLessThanOrEqual(budget.budgetTokens)
    const lite = catalogPrompt({ lite: true })
    expect(budget.lite).toEqual({ components: componentNames({ lite: true }).length, chars: lite.length, tokens: estimateTokens(lite) })
    const terse = catalogPrompt({ terse: true })
    expect(budget.terse).toEqual({ components: componentNames().length, chars: terse.length, tokens: estimateTokens(terse) })
    expect(terse.length).toBeLessThan(full.length)
  })

  test(`names the catalog, every visible component and prop, never the placeholder`, () => {
    expect(full.startsWith(`Catalog ${CORE_CATALOG_ID}.`)).toBe(true)
    for (const [name, def] of Object.entries(coreCatalog.components)) {
      if (def.hidden) {
        expect(full).not.toContain(`\n${name}:`)
        continue
      }
      expect(full).toContain(`\n${name}: ${def.description}`)
      for (const prop of Object.keys(def.props)) expect(full, `${name}.${prop}`).toMatch(new RegExp(`\\n  ${prop}[*~^]*: `))
    }
    // Round 4: every function with its arguments, the action shapes, updateDataModel, filter, token-only colours.
    for (const needle of [`Functions ({call, args}`, `regex(value:str,pattern:str)→bool`, `filter(items:array,query:str,fields:array,where:obj)→array`, `set(path:str,value:any)→void`, `{event: {name, context?}}`, `{functionCall: {call, args}}`, `updateDataModel{surfaceId, path, value}`, `never #hex`, `closes the Dialog or Drawer`, `app.toast`])
      expect(full, needle).toContain(needle)
    for (const name of coreCatalog.functions.names) expect(full, name).toMatch(new RegExp(`[ (]${name}\\(`))
  })

  test(`the lite prompt drops overlays, media, Chart, data tables and the rarer controls`, () => {
    const lite = catalogPrompt({ lite: true })
    expect(lite.startsWith(`Catalog ${CORE_LITE_CATALOG_ID}.`)).toBe(true)
    for (const name of [`Dialog`, `Drawer`, `Popover`, `Tooltip`, `Menu`, `Image`, `Video`, `Chart`, `Table`, `Slider`, `Pagination`]) expect(lite).not.toContain(`\n${name}:`)
    expect(lite).toContain(`\nStack:`)
  })

  test(`registered extensions are appended in the same shape`, () => {
    const ext = defineExtension(extensionFixture.extension as unknown as ExtensionDef)
    const prompt = catalogPrompt({ extensions: [ext] })
    expect(prompt).toContain(`with extensions ${ext.id}`)
    expect(prompt).toContain(`Extension ${ext.id} (${ext.name}):`)
    expect(prompt).toContain(`\nStatCard: `)
    expect(prompt).toContain(`trend: up|down|flat`)
  })
})

describe(`round 1: the prompt teaches responsive authoring`, () => {
  const full = catalogPrompt()

  test(`media conditions, states, breakpoint tokens and responsive props are taught`, () => {
    for (const needle of [`@media (min-width: $breakpoint.md)`, `max-width`, `orientation: portrait|landscape`, `hover: hover|none`, `prefers-reduced-motion: reduce`, `":hover"`, `":focus-visible"`, `":pressed"`, `display: "none"`, `{base, sm?, md?, lg?, xl?}`, `visible`])
      expect(full, needle).toContain(needle)
    expect(full).toMatch(/\n  direction\^: /)
    expect(full).toMatch(/\n  columns\^: /)
  })

  test(`common props are described once, other props keep their description`, () => {
    expect(full).toContain(`Common props (no description below): name (form key)`)
    expect(full).toMatch(/\n  name\*: string\n/)
    expect(full).toContain(`  language: plain|json|`)
    expect(full).toContain(` — The syntax colouring.`)
  })
})

describe(`VAPP-108: sections, the index and components on demand`, () => {
  const { catalogIndex, componentPrompt, DEFAULT_SECTIONS, PROMPT_SECTIONS } = require(`./prompt`) as typeof import("./prompt")

  test(`the default sections print the same prompt as before`, () => {
    expect(catalogPrompt({ sections: DEFAULT_SECTIONS })).toBe(catalogPrompt())
    expect(PROMPT_SECTIONS).toContain(`index`)
  })

  test(`rules alone, the index alone, and the index prompt are each far smaller than the full one; the budget records them`, () => {
    const full = catalogPrompt()
    const rules = catalogPrompt({ sections: [`rules`] })
    const index = catalogPrompt({ sections: [`rules`, `index`, `functions`, `style`] })
    expect(rules).toContain(`Rules:`)
    expect(rules).not.toContain(`\nStack:`)
    expect(index).toContain(`Index (every component`)
    expect(index).toContain(`Stack (children): `)
    expect(index).not.toContain(`\n  gap^: `)
    expect(index.length).toBeLessThan(full.length * 0.45)
    expect(budget.index).toEqual({ components: componentNames().length, chars: index.length, tokens: estimateTokens(index) })
    expect(budget.rules).toEqual({ chars: rules.length, tokens: estimateTokens(rules) })
    // Every offered component is in the index, grouped.
    const lines = catalogIndex()
    for (const name of componentNames()) expect(lines.some((l) => l.includes(`${name}: `) || l.includes(`${name} (`)), name).toBe(true)
  })

  test(`componentPrompt gives one block with the shapes it reaches; only narrows the components section`, () => {
    const section = componentPrompt(`Section`)
    expect(section.startsWith(`Section: `)).toBe(true)
    expect(section).toContain(`\n  count~: number`)
    expect(section).not.toContain(`Shapes:`)
    const table = componentPrompt(`Table`)
    expect(table).toContain(`Shapes: column = {`)
    expect(table).toContain(`sort = {`)
    expect(componentPrompt(`Unknown`)).toBe(``)
    expect(componentPrompt(`Nope`)).toBe(``)
    const only = catalogPrompt({ only: [`Stack`, `Button`] })
    expect(only).toContain(`\nStack: `)
    expect(only).toContain(`\nButton: `)
    expect(only).not.toContain(`\nTable: `)
    expect(only).toContain(`ask for any other component by name`)
    expect(only.length).toBeLessThan(catalogPrompt().length / 3)
    const ext = defineExtension(extensionFixture.extension as unknown as ExtensionDef)
    expect(componentPrompt(`StatCard`, { extensions: [ext] })).toContain(`trend: up|down|flat`)
    expect(catalogIndex({ extensions: [ext] }).join(`\n`)).toContain(`StatCard`)
  })
})
