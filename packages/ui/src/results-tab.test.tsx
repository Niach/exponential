import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { RESULTS_TAB, type Result } from "@exp/domain-contract/results-tab"
import { ResultsTab, resultThumbWidth } from "./results-tab"

const picture = (label: string) => ({ label, attachmentId: label, width: 1600, height: 1000, caption: null })
const result = (id: string, over: Partial<Result> = {}): Result => ({
  id,
  sessionId: `s`,
  sessionName: `S`,
  createdAt: 0,
  age: `2h`,
  title: `Title ${id}`,
  description: null,
  pictures: [],
  surface: false,
  ...over,
})

describe(`ResultsTab`, () => {
  it(`draws the sections in order, title and age, no kind badge`, () => {
    render(<ResultsTab results={[result(`a`), result(`b`, { description: `Why` })]} size="wide" />)
    const sections = screen.getAllByTestId(RESULTS_TAB.testIds.section)
    expect(sections.map((s) => s.getAttribute(`data-result`))).toEqual([`a`, `b`])
    expect(sections[0]!.textContent).toBe(`Title a2h`)
    expect(sections[1]!.textContent).toContain(`Why`)
  })

  it(`a picture opens THE lightbox over that result's pictures at its index`, () => {
    render(<ResultsTab results={[result(`a`, { pictures: [picture(`one`), picture(`two`)] })]} size="wide" />)
    expect(screen.queryByTestId(`preview-zoom`)).toBeNull()
    fireEvent.click(screen.getAllByRole(`button`)[1]!)
    expect(screen.getByRole(`dialog`)).toBeTruthy()
    const shown = screen.getByRole(`dialog`).querySelector(`img`)
    expect(shown?.getAttribute(`src`)).toBe(`/api/attachments/two`)
  })

  it(`none = THE EmptyState, the PR block still above it`, () => {
    render(<ResultsTab results={[]} size="phone" lead={<div data-testid="lead" />} />)
    expect(screen.getByTestId(`lead`)).toBeTruthy()
    expect(screen.getByText(RESULTS_TAB.copy.empty)).toBeTruthy()
  })

  it(`sizes a thumbnail by aspect, capped; an unknown size = 4:3`, () => {
    expect(resultThumbWidth({ width: 1179, height: 2556 }, `wide`)).toBe(111)
    expect(resultThumbWidth({ width: 1600, height: 1000 }, `wide`)).toBe(RESULTS_TAB.tile.thumbMaxWidth.wide)
    expect(resultThumbWidth({ width: null, height: null }, `phone`)).toBe(320)
  })
})
