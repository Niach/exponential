import { fireEvent, render, screen, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/work-card.json"
import {
  workCard,
  type WorkCard as WorkCardModel,
  type WorkCardInput,
} from "@exp/domain-contract/work-card"

import { WorkCard } from "./work-card"

// EXP-1240: the card only PAINTS the contract's model — every fixture case's
// header, segments and rows land on screen, and a lane names its phase
// position and lane position back to the host.

const cases = fixture.cases as { name: string; input: WorkCardInput }[]
const caseNamed = (name: string): WorkCardModel => {
  const found = cases.find((c) => c.name === name)
  if (!found) throw new Error(name)
  return workCard(found.input)
}

describe(`WorkCard`, () => {
  for (const c of cases) {
    it(`paints: ${c.name}`, () => {
      const card = workCard(c.input)
      render(<WorkCard card={card} onOpenLane={() => {}} onOpenSubagent={() => {}} />)
      const root = screen.getByTestId(fixture.testIds.card)
      expect(root.getAttribute(`data-kind`)).toBe(card.kind)
      expect(root.textContent).toContain(card.name)
      if (card.progress) expect(root.textContent).toContain(card.progress)
      if (card.elapsed) expect(root.textContent).toContain(card.elapsed)
      const segments = root.querySelectorAll(`[data-slot="work-card-segments"] > [data-state]`)
      expect([...segments].map((node) => node.getAttribute(`data-state`))).toEqual(
        card.segments.map((segment) => segment.state)
      )
      const lanes = card.rows.flatMap((row) => (row.type === `phase` ? row.lanes : []))
      expect(screen.queryAllByTestId(fixture.testIds.lane)).toHaveLength(lanes.length)
      expect(screen.queryAllByTestId(fixture.testIds.folded)).toHaveLength(
        card.rows.filter((row) => row.type === `folded`).length
      )
      // No report text, no token counts.
      expect(root.textContent).not.toMatch(/tokens/)
    })
  }

  it(`a running phase's lane opens with its phase POSITION after the fold`, () => {
    const card = caseNamed(
      `a workflow mid-review folds its finished phases and lists the running phase's lanes`
    )
    const onOpenLane = vi.fn()
    render(<WorkCard card={card} onOpenLane={onOpenLane} />)
    const lanes = screen.getAllByTestId(fixture.testIds.lane)
    fireEvent.click(lanes[3])
    // Contract · Implement · Conformance fold into one row: Review is phase 3.
    expect(onOpenLane).toHaveBeenCalledWith({ phase: 3, lane: 3 })
  })

  it(`the folded row opens the phases the host spelled out, in place`, () => {
    const card = caseNamed(`a completed workflow folds into one row`)
    const onOpenLane = vi.fn()
    render(
      <WorkCard
        card={card}
        onOpenLane={onOpenLane}
        foldedPhases={[
          {
            title: `Alpha`,
            duration: `1h`,
            lanes: [{ label: `a`, model: null, state: `done`, duration: `1h 0m` }],
          },
          {
            title: `Beta`,
            duration: `59m`,
            lanes: [{ label: `b`, model: null, state: `done`, duration: `59m 0s` }],
          },
        ]}
      />
    )
    const folded = screen.getByTestId(fixture.testIds.folded)
    expect(within(folded).queryAllByTestId(fixture.testIds.lane)).toHaveLength(0)
    fireEvent.click(within(folded).getByRole(`button`))
    const lanes = within(folded).getAllByTestId(fixture.testIds.lane)
    expect(lanes.map((lane) => lane.textContent)).toEqual([`a1h 0m`, `b59m 0s`])
    fireEvent.click(lanes[1])
    expect(onOpenLane).toHaveBeenCalledWith({ phase: 1, lane: 0 })
  })

  it(`a subagent card opens its transcript; a task row does nothing`, () => {
    const onOpenSubagent = vi.fn()
    const { unmount } = render(
      <WorkCard card={caseNamed(`a finished subagent`)} onOpenSubagent={onOpenSubagent} />
    )
    fireEvent.click(screen.getByRole(`button`))
    expect(onOpenSubagent).toHaveBeenCalledTimes(1)
    unmount()
    render(<WorkCard card={caseNamed(`a task list shows the task in progress`)} />)
    expect(screen.queryAllByRole(`button`)).toHaveLength(0)
  })

  it(`the host's rows land under the card's own`, () => {
    render(
      <WorkCard card={caseNamed(`a finished subagent`)}>
        <div data-testid="extra">warning</div>
      </WorkCard>
    )
    expect(
      within(screen.getByTestId(fixture.testIds.card)).getByTestId(`extra`)
    ).toBeTruthy()
  })
})
