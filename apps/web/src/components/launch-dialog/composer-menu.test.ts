import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/composer-menu.json"
import icons from "@exp/icons/icons.json"
import { contract } from "@exp/domain-contract"
import {
  composerMenuLayout,
  composerMenuRowTestId,
  implementButtonLabel,
  type ComposerMenuConditions,
} from "@/components/launch-dialog/composer-menu"

// EXP-1249: the "+" menu layout replays composer-menu.json — the same case
// names the desktop, iOS and Android suites run.

describe(`composerMenuLayout`, () => {
  for (const testCase of fixture.cases) {
    it(testCase.name, () => {
      const layout = composerMenuLayout(
        testCase.conditions as ComposerMenuConditions
      )
      expect(
        layout.map((entry) => (entry.kind === `separator` ? `-` : entry.id))
      ).toEqual(testCase.expected)
    })
  }

  it(`names every row's glyph by an existing concept`, () => {
    const concepts = new Set(Object.keys(icons.semantic))
    const all = composerMenuLayout({
      subagentModel: true,
      ultracode: true,
      mcp: true,
      computerUse: true,
    })
    for (const entry of all) {
      if (entry.kind === `separator`) continue
      expect(concepts.has(entry.icon), entry.icon).toBe(true)
    }
    expect(concepts.has(fixture.suggestions.icon)).toBe(true)
  })

  it(`names the contract's computer-use payload key and cap`, () => {
    expect(contract.codingSession.launchKeys).toContain(
      fixture.computerUse.payloadKey
    )
    expect(fixture.computerUse.cap).toBe(contract.codingSession.computerUseCap)
  })

  it(`spells the row test ids from the fixture template`, () => {
    expect(composerMenuRowTestId(`computer-use`)).toBe(
      `agent-composer-menu-computer-use`
    )
  })
})

describe(`implementButtonLabel`, () => {
  for (const testCase of fixture.implementLabels) {
    it(`${testCase.count} → ${testCase.expected}`, () => {
      expect(implementButtonLabel(testCase.count)).toBe(testCase.expected)
    })
  }
})
