import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/needs-you.json"
import { NEEDS_YOU_COPY, NEEDS_YOU_TEST_IDS, type NeedsYouRow } from "@exp/domain-contract/needs-you"
import { NeedsYouEntryRow, NeedsYouList } from "./needs-you"

// A PR row carries a meta; a card's has none.
const rows = fixture.prCases[0]!.expected as unknown as NeedsYouRow[]

describe(`NeedsYouList`, () => {
  it(`paints the title + age, then the chat · meta; a press names the row`, () => {
    const onOpen = vi.fn()
    render(<NeedsYouList rows={rows} onOpen={onOpen} />)
    const painted = screen.getAllByTestId(NEEDS_YOU_TEST_IDS.row)
    expect(painted[0]!.textContent).toContain(`${rows[0]!.title}${rows[0]!.age}`)
    expect(painted[0]!.textContent).toContain(`${rows[0]!.chat} · ${rows[0]!.meta}`)
    // No meta = the chat alone.
    expect(painted[1]!.textContent).not.toContain(` · `)
    expect(painted.slice(0, 2).map((row) => row.getAttribute(`data-kind`))).toEqual([`pr`, `question`])
    fireEvent.click(painted[1]!)
    expect(onOpen).toHaveBeenCalledWith(rows[1])
  })

  it(`no row = THE EmptyState`, () => {
    render(<NeedsYouList rows={[]} />)
    expect(screen.getByText(NEEDS_YOU_COPY.empty)).toBeTruthy()
    expect(screen.queryByTestId(NEEDS_YOU_TEST_IDS.list)).toBeNull()
  })
})

describe(`NeedsYouEntryRow`, () => {
  it(`names the entry and its count`, () => {
    const onOpen = vi.fn()
    render(<NeedsYouEntryRow count={4} onOpen={onOpen} />)
    const entry = screen.getByTestId(NEEDS_YOU_TEST_IDS.entry)
    expect(entry.textContent).toContain(NEEDS_YOU_COPY.entry)
    expect(entry.textContent).toContain(`4`)
    fireEvent.click(entry)
    expect(onOpen).toHaveBeenCalled()
  })
})
