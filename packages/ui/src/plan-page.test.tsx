import { fireEvent, render, screen, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { PLAN_PAGE_SPECIMEN, PLAN_PAGE_TEST_IDS, planOutline } from "@exp/domain-contract/plan-page"
import { chatAskRow } from "@exp/domain-contract/session-tree"
import { PlanPage } from "./plan-page"
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

  it(`phone: the group = THE Carousel, the questions float (no Dock, no composer), ☰ opens the outline`, () => {
    render(<PlanPageSpecimen size="phone" />)
    expect(screen.getByTestId(PLAN_PAGE_TEST_IDS.carousel)).toBeTruthy()
    expect(screen.queryByTestId(PLAN_PAGE_TEST_IDS.mosaic)).toBeNull()
    expect(document.querySelector(`[data-slot=plan-questions-panel]`)).toBeTruthy()
    expect(screen.queryByTestId(`dock-card`)).toBeNull()
    expect(document.querySelector(`textarea`)).toBeNull()
    fireEvent.click(screen.getByTestId(PLAN_PAGE_TEST_IDS.outlineToggle))
    expect(screen.getAllByTestId(`outline`).length).toBeGreaterThan(0)
  })

  it(`a picture inside the prose (mid-sentence) opens THE lightbox, never the renderer's own preview`, () => {
    const plan = `## A\n\nThe bar ![bar](/api/attachments/a) counts.\n\n![one](/api/attachments/b)\n\n![two](/api/attachments/c)`
    const row = chatAskRow({
      id: `m`,
      sessionId: `s`,
      authorKind: `agent`,
      authorId: null,
      body: ``,
      card: { kind: `plan`, plan, questions: [] },
      answeredAt: null,
      consumedAt: null,
      createdAt: new Date(0).toISOString(),
      authorSessionId: `s`,
    }) as Extract<ReturnType<typeof chatAskRow>, { type: `plan` }>
    const ownPreview = vi.fn()
    // The renderer's own image preview (FeedText's) sits on the picture.
    const renderText = (text: string) => (
      <p>
        {text.split(/(!\[[^\]]*\]\([^)]*\))/).map((part, at) => {
          const picture = /^!\[([^\]]*)\]\(([^)]*)\)$/.exec(part)
          return picture ? <img key={at} alt={picture[1]} src={picture[2]} onClick={ownPreview} /> : part
        })}
      </p>
    )
    render(<PlanPage row={row} mobile={false} renderText={renderText} busy={false} />)
    fireEvent.click(screen.getByAltText(`bar`))
    expect(ownPreview).not.toHaveBeenCalled()
    expect(within(screen.getByRole(`dialog`)).getByAltText(`bar`)).toBeTruthy()
  })
})
