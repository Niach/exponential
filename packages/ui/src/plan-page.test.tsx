import { fireEvent, render, screen, within } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { PLAN_PAGE_SPECIMEN, PLAN_PAGE_TEST_IDS, planOutline } from "@exp/domain-contract/plan-page"
import { PlanPageSpecimen } from "./plan-page-specimen"

describe(`PlanPage`, () => {
  it(`wide: the outline lists the sections (the specimen's active one marked), a group of pictures = THE Mosaic, the rail on question 1`, () => {
    render(<PlanPageSpecimen />)
    const outline = screen.getByTestId(PLAN_PAGE_TEST_IDS.outline)
    const rows = within(outline).getAllByRole(`button`)
    const sections = planOutline(PLAN_PAGE_SPECIMEN.card.plan)
    expect(rows.map((row) => row.dataset.id)).toEqual(sections.map((section) => section.id))
    expect(outline.querySelector(`[data-active]`)!.getAttribute(`data-id`)).toBe(PLAN_PAGE_SPECIMEN.activeSection)
    expect(screen.getAllByTestId(PLAN_PAGE_TEST_IDS.mosaic)).toHaveLength(1)
    expect(screen.queryByTestId(PLAN_PAGE_TEST_IDS.carousel)).toBeNull()
    expect(screen.getByTestId(`plan-rail`).textContent).toContain(PLAN_PAGE_SPECIMEN.card.questions[0]!.prompt)
  })

  it(`phone: the group = THE Carousel, the foot = THE Dock's plan card (no composer), ☰ opens the outline`, () => {
    render(<PlanPageSpecimen size="phone" />)
    expect(screen.getByTestId(PLAN_PAGE_TEST_IDS.carousel)).toBeTruthy()
    expect(screen.queryByTestId(PLAN_PAGE_TEST_IDS.mosaic)).toBeNull()
    expect(screen.getByTestId(`dock-card`)).toBeTruthy()
    expect(document.querySelector(`textarea`)).toBeNull()
    fireEvent.click(screen.getByTestId(PLAN_PAGE_TEST_IDS.outlineToggle))
    expect(screen.getAllByTestId(`outline`).length).toBeGreaterThan(0)
  })
})
