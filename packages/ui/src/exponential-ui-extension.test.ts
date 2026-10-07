// VAPP-85: the Exponential app's extension catalog for Exponential UI is
// valid against the SDK, expands through the core macro table, and names
// only components the app already draws (docs/shared-ui-inventory.md).

import { describe, expect, it } from "vitest"
import {
  catalogPrompt,
  defineExtension,
  expandMacros,
  reduceSurface,
  type ExtensionDef,
} from "@exponential-at/ui"
import extensionJson from "../exponential-ui/extension.json"

const extension = defineExtension(extensionJson as unknown as ExtensionDef)

describe(`Exponential app extension (VAPP-85)`, () => {
  it(`is a valid extension of the core catalog`, () => {
    expect(extension.id).toBe(`https://ui.exponential.at/catalogs/exponential-app/v1`)
    expect(Object.keys(extension.components)).toEqual([
      `IssueRow`,
      `RunRow`,
      `IssueChip`,
      `LabelChip`,
      `StatusGlyph`,
      `LiveDot`,
      `IconDisc`,
      `ComposerHeadline`,
      `DecisionCard`,
      `ChoiceDialog`,
      `ResultTile`,
      `ResultGroup`,
      `Diff`,
    ])
  })

  it(`every example reduces without issues; the macros expand to core natives`, () => {
    for (const [name, def] of Object.entries(extension.components)) {
      const { root, issues } = reduceSurface([{ id: `root`, component: name, ...(def.example ?? {}) }], {
        catalogId: extension.id,
        extensions: [extension],
      })
      expect(issues, name).toEqual([])
      if (def.kind === `macro`) expect(root.component, name).not.toBe(name)
      else expect(root.component).toBe(name)
    }
    const chip = reduceSurface([{ id: `root`, component: `LabelChip`, name: `bug`, color: `#EF4444` }], { catalogId: extension.id, extensions: [extension] }).root
    expect(chip.component).toBe(`Box`)
    expect(chip.recipe).toEqual({ macro: `Pill`, part: `root`, props: { selected: false, tone: `neutral` } })
    expect(chip.children.map((c) => c.id)).toEqual([`root.dot`, `root.label`])
    expect(expandMacros(chip)).toEqual(chip)
  })

  it(`is listed in the catalog prompt after the core`, () => {
    const prompt = catalogPrompt({ extensions: [extension] })
    expect(prompt).toContain(`Extension ${extension.id} (Exponential app):`)
    expect(prompt.indexOf(`\nIssueRow:`)).toBeGreaterThan(prompt.indexOf(`\nTreeGuides:`))
  })
})
