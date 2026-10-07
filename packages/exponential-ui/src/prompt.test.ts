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
    expect(terse.length).toBeLessThan(lite.length)
  })

  test(`names the catalog, every visible component and prop, never the placeholder`, () => {
    expect(full.startsWith(`Catalog ${CORE_CATALOG_ID}.`)).toBe(true)
    for (const [name, def] of Object.entries(coreCatalog.components)) {
      if (def.hidden) {
        expect(full).not.toContain(`\n${name}:`)
        continue
      }
      expect(full).toContain(`\n${name}: ${def.description}`)
      for (const prop of Object.keys(def.props)) expect(full, `${name}.${prop}`).toMatch(new RegExp(`\\n  ${prop}[*~]*: `))
    }
    expect(full).toContain(`Functions (client-side`)
  })

  test(`the lite prompt drops overlays, media and Chart`, () => {
    const lite = catalogPrompt({ lite: true })
    expect(lite.startsWith(`Catalog ${CORE_LITE_CATALOG_ID}.`)).toBe(true)
    for (const name of [`Dialog`, `Drawer`, `Popover`, `Tooltip`, `DropdownMenu`, `Image`, `Video`, `Chart`]) expect(lite).not.toContain(`\n${name}:`)
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
