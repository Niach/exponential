import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/composer-menu.json"
import searchList from "@exp/domain-contract/fixtures/search-list.json"
import icons from "@exp/icons/icons.json"
import { contract } from "@exp/domain-contract"
import {
  COMPOSER_MENU_TEST_IDS,
  composerMenuLayout,
  composerMenuRowTestId,
  implementButtonLabel,
  type ComposerMenuConditions,
  type ComposerMenuSurface,
} from "@/components/launch-dialog/composer-menu"

// EXP-1249: the "+" menu layout replays composer-menu.json — the same case
// names the desktop, iOS and Android suites run.

describe(`composerMenuLayout`, () => {
  for (const testCase of fixture.cases) {
    it(testCase.name, () => {
      const layout = composerMenuLayout(
        testCase.surface as ComposerMenuSurface,
        testCase.conditions as ComposerMenuConditions
      )
      expect(
        layout.map((entry) => (entry.kind === `separator` ? `-` : entry.id))
      ).toEqual(testCase.expected)
    })
  }

  it(`names every row's glyph by an existing concept`, () => {
    const concepts = new Set(Object.keys(icons.semantic))
    const all = composerMenuLayout(`launch`, {
      temporary: true,
      device: true,
      account: true,
      model: true,
      repositories: true,
      subagentModel: true,
      ultracode: true,
      mcp: true,
      computerUse: true,
    })
    expect(all.filter((e) => e.kind !== `separator`)).toHaveLength(
      fixture.rows.filter((r) => r.kind !== `separator`).length
    )
    for (const entry of all) {
      if (entry.kind === `separator`) continue
      expect(concepts.has(entry.icon), entry.icon).toBe(true)
    }
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

describe(`the submenus are the search list`, () => {
  it(`one row height, eight rows visible: the search list's own geometry`, () => {
    expect(fixture.submenuRowHeight).toBe(searchList.row.height)
    expect(fixture.submenuMaxRows).toBe(searchList.maxVisibleRows)
  })

  it(`names the submenu search field's test id`, () => {
    expect(COMPOSER_MENU_TEST_IDS.submenuSearch).toBe(`agent-composer-submenu-search`)
  })

  it(`carries no message-agent row any more (S-3)`, () => {
    expect(fixture.rows.some((r) => `id` in r && r.id === `message-agent`)).toBe(
      false
    )
  })

  it(`carries no suggestion rows and no brand mark any more (EXP-1270)`, () => {
    expect(`suggestions` in fixture).toBe(false)
    expect(`brandMark` in fixture.testIds).toBe(false)
  })
})

describe(`implementButtonLabel`, () => {
  for (const testCase of fixture.implementLabels) {
    it(`${testCase.count} → ${testCase.expected}`, () => {
      expect(implementButtonLabel(testCase.count)).toBe(testCase.expected)
    })
  }
})
