// #EXP-1836 (chats.json `shell.boardChips`): the board's tabs = THE session
// screen's SegmentedControl, a muted count after My issues and Drafts,
// Drafts only while the viewer has drafts.
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { BoardChips, boardChipCount, boardChips } from "@/components/board-chips"

describe(`BoardChips`, () => {
  it(`drafts only while there are drafts`, () => {
    expect(boardChips(0)).toEqual([`all`, `mine`])
    expect(boardChips(2)).toEqual([`all`, `mine`, `drafts`])
  })

  it(`counts: none for All issues or zero`, () => {
    expect(boardChipCount(`all`, { mine: 4, drafts: 1 })).toBeNull()
    expect(boardChipCount(`mine`, { mine: 0, drafts: 1 })).toBeNull()
    expect(boardChipCount(`mine`, { mine: 4, drafts: 1 })).toBe(4)
  })

  it(`renders a tab strip with muted counts and switches`, () => {
    const onChange = vi.fn()
    render(<BoardChips value="all" counts={{ mine: 2, drafts: 1 }} onChange={onChange} />)
    const tabs = screen.getAllByRole(`tab`)
    expect(tabs.map((tab) => tab.textContent)).toEqual([`All issues`, `My issues2`, `Drafts1`])
    expect(tabs[0]!.getAttribute(`aria-selected`)).toBe(`true`)
    fireEvent.mouseDown(tabs[1]!)
    fireEvent.click(tabs[1]!)
    expect(onChange).toHaveBeenCalledWith(`mine`)
  })
})
