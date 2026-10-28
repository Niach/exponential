import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import home from "@exp/domain-contract/fixtures/composer-home.json"
import { composerAgentRow } from "@exp/domain-contract/composer-agent-row"
import { ComposerAgentRow, composerAgentOptionTestId } from "./composer-agent-row"

// S-3 (composer-home.json `agentRow`): the circles in the reference's order,
// the selected one checked, the label line, a tap reports the id.

describe(`ComposerAgentRow`, () => {
  for (const c of home.agentRow.cases) {
    it(c.name, () => {
      const onPick = vi.fn()
      const state = composerAgentRow({
        agents: c.agents.map((agent) => ({ ...agent, icon: null, color: null })),
        me: c.me,
        picked: c.picked,
      })
      const { unmount } = render(<ComposerAgentRow row={state} onPick={onPick} />)
      const radios = screen.getAllByRole(`radio`)
      expect(radios.map((radio) => radio.getAttribute(`data-testid`))).toEqual(
        c.expected.options.map(composerAgentOptionTestId)
      )
      expect(
        screen.getByTestId(composerAgentOptionTestId(c.expected.selected)).getAttribute(`aria-checked`)
      ).toBe(`true`)
      expect(screen.getByTestId(home.agentRow.testIds.label).textContent).toBe(c.expected.label)
      fireEvent.click(radios[radios.length - 1]!)
      expect(onPick).toHaveBeenCalledWith(home.agentRow.temporaryId)
      unmount()
    })
  }
})
