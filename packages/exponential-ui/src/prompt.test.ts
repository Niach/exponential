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
