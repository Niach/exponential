import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import spec from "@exp/domain-contract/fixtures/getting-started.json"

import { GettingStartedList, gettingStartedRowTestId } from "./getting-started-list"

// EXP-1270: the block's face draws the model's entries as they come, in the
// fixture's words: the band (title, progress, Docs), one row per entry in
// order — done rows checked, the ONE next row expanded with its why and cta,
// later rows plain with a chevron.

const CASE = spec.cases[1]!
const INVITE = spec.steps.find((step) => step.key === `invite`)!

function renderList(props: Partial<Parameters<typeof GettingStartedList>[0]> = {}) {
  return render(
    <GettingStartedList
      entries={CASE.expected.entries}
      done={CASE.expected.done}
      total={CASE.expected.total}
      progress={CASE.expected.progress}
      {...props}
    />
  )
}

describe(`GettingStartedList`, () => {
  it(`draws the band and one row per entry, in order`, () => {
    renderList()
    const block = screen.getByTestId(spec.testIds.block)
    expect(block.style.maxWidth).toBe(`${spec.maxWidth}px`)
    expect(screen.getByText(spec.title)).toBeTruthy()
    expect(screen.getByText(CASE.expected.progress)).toBeTruthy()
    const docs = screen.getByTestId(spec.testIds.docs)
    expect(docs.getAttribute(`href`)).toBe(spec.docs.url)
    expect(docs.textContent).toBe(spec.docs.label)
    const rows = document.querySelectorAll(`[data-slot=getting-started-row]`)
    expect([...rows].map((row) => row.getAttribute(`data-testid`))).toEqual(
      CASE.expected.entries.map((entry) => gettingStartedRowTestId(entry.key))
    )
  })

  it(`checks done rows, expands the ONE next row, chevrons the rest`, () => {
    renderList()
    const board = screen.getByTestId(gettingStartedRowTestId(`board`))
    expect(board.getAttribute(`data-done`)).toBe(`true`)
    expect(board.querySelector(`[aria-label=Done]`)).toBeTruthy()

    const invite = screen.getByTestId(gettingStartedRowTestId(`invite`))
    expect(invite.getAttribute(`data-next`)).toBe(`true`)
    expect(invite.textContent).toContain(INVITE.title)
    expect(invite.textContent).toContain(spec.nextLabel)
    expect(invite.textContent).toContain(INVITE.why)
    expect(screen.getByTestId(spec.testIds.cta).textContent).toBe(INVITE.cta)
    expect(invite.style.paddingTop).toBe(`${spec.row.expandedPadding}px`)
    expect(document.querySelectorAll(`[data-next=true]`)).toHaveLength(1)
    expect(document.querySelectorAll(`[data-testid=${spec.testIds.cta}]`)).toHaveLength(1)

    const widget = screen.getByTestId(gettingStartedRowTestId(`widget`))
    expect(widget.textContent).toBe(`Feedback widget`)
    expect(widget.style.minHeight).toBe(`${spec.row.height}px`)
    // No numbers, no descriptions on a later row.
    expect(widget.textContent).not.toMatch(/\d/)
  })

  it(`a click hands the row's key back: done rows, the next row and its button`, () => {
    const onSelect = vi.fn()
    renderList({ onSelect })
    fireEvent.click(screen.getByTestId(gettingStartedRowTestId(`github`)))
    fireEvent.click(screen.getByTestId(gettingStartedRowTestId(`widget`)))
    fireEvent.click(screen.getByTestId(gettingStartedRowTestId(`invite`)))
    fireEvent.click(screen.getByTestId(spec.testIds.cta))
    expect(onSelect.mock.calls).toEqual([[`github`], [`widget`], [`invite`], [`invite`]])
  })

  it(`phones: taller rows, full width, Docs as an icon button`, () => {
    const phone = spec.cases[4]!
    renderList({
      entries: phone.expected.entries,
      done: phone.expected.done,
      total: phone.expected.total,
      progress: phone.expected.progress,
      phone: true,
    })
    expect(screen.getByTestId(spec.testIds.block).style.maxWidth).toBe(``)
    const docs = screen.getByTestId(spec.testIds.docs)
    expect(docs.getAttribute(`aria-label`)).toBe(spec.docs.label)
    expect(docs.textContent).toBe(``)
    expect(
      screen.getByTestId(gettingStartedRowTestId(`board`)).style.minHeight
    ).toBe(`${spec.row.heightPhone}px`)
  })
})
