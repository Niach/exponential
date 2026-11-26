import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import home from "@exp/domain-contract/fixtures/composer-home.json"
import { composerAgentRow } from "@exp/domain-contract/composer-agent-row"
import { ComposerAgentRow, composerAgentOptionTestId } from "./composer-agent-row"

// S-3 (composer-home.json `agentRow`): the circles in the reference's order,
// the selected one checked, the label line, a tap reports the id.

describe(`ComposerAgentRow`, () => {
  it(`#EXP-2136: same-named agents of two teams wear their team's mark`, () => {
    const agent = (id: string, name: string, teamId: string) => ({ id, name, teamId, icon: null, color: null, sortOrder: 1, role: `agent`, ownerId: `u` })
    const agents = [agent(`g1`, `Triage`, `t1`), agent(`g2`, `Triage`, `t2`), agent(`g3`, `Designer`, `t2`)]
    const row = { agents, options: [`g1`, `g2`, `g3`, `temporary`], selected: `g1`, agent: agents[0]!, label: `To Triage` }
    const { container } = render(
      <ComposerAgentRow row={row} onPick={() => {}} teamOf={(id) => ({ name: id === `t1` ? `Acme` : `Northwind`, iconUrl: null })} />
    )
    expect(container.querySelectorAll(`[data-slot="team-corner"]`)).toHaveLength(2)
    expect(screen.getByRole(`radio`, { name: `Triage, Acme` })).toBeTruthy()
    expect(screen.getByRole(`radio`, { name: `Designer` })).toBeTruthy()
  })

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
