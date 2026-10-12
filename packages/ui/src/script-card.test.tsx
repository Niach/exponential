import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/script-card.json"
import { scriptCard, type ScriptCardInput } from "@exp/domain-contract/script-card"

import { ScriptCard } from "./script-card"

// EXP-1279: the Script card paints the contract's model — title, meta, one row
// per tool group (count, or the rose failure) and the overflow line; the
// script itself only behind the disclosure.

const cases = fixture.cases as { name: string; input: ScriptCardInput }[]

describe(`ScriptCard`, () => {
  for (const c of cases) {
    it(`paints: ${c.name}`, () => {
      const card = scriptCard(c.input)
      render(<ScriptCard card={card} running={c.input.running} />)
      const root = screen.getByTestId(fixture.testIds.card)
      expect(root.textContent).toContain(card.title)
      if (card.meta) expect(root.textContent).toContain(card.meta)
      const groups = screen.queryAllByTestId(fixture.testIds.group)
      expect(groups).toHaveLength(card.groups.length)
      card.groups.forEach((group, index) => {
        expect(groups[index].textContent).toContain(group.label)
        if (group.failed) expect(groups[index].textContent).toContain(group.failed)
        else if (group.count) expect(groups[index].textContent).toContain(group.count)
      })
      if (card.overflow) expect(root.textContent).toContain(card.overflow)
    })
  }

  it(`the script hides behind its disclosure`, () => {
    const card = scriptCard({ running: false, calls: [] })
    render(<ScriptCard card={card} source={`await tools.exponential.x()`} />)
    expect(screen.queryByTestId(fixture.testIds.source)).toBeNull()
    fireEvent.click(screen.getByRole(`button`, { name: new RegExp(fixture.copy.script) }))
    expect(screen.getByTestId(fixture.testIds.source).textContent).toBe(
      `await tools.exponential.x()`
    )
  })

  it(`no source, no disclosure`, () => {
    render(<ScriptCard card={scriptCard({ running: false })} />)
    expect(screen.queryAllByRole(`button`)).toHaveLength(0)
  })
})
