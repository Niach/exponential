// EXP-941: the move-to-board picker rides the shared `Combobox` now, so the
// one rule that must survive the swap is the one the primitive knows nothing
// about — a row NEVER moves the issue. It stages the confirmation dialog
// (EXP-426: the server renumbers the issue in the target board), and only
// "Move" reports the pick.
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Board } from "@/db/schema"

// Radix positions its content with ResizeObserver and cmdk scrolls the active
// row into view; jsdom has neither.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const BOARDS = [
  { id: `board-1`, teamId: `team-1`, name: `Design`, color: `#ef4444` },
  { id: `board-2`, teamId: `team-1`, name: `Platform`, color: `#22c55e` },
] as unknown as Board[]

vi.mock(`@/lib/collections`, () => ({ boardCollection: {} }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: BOARDS }) }
})

const { BoardPicker } = await import(
  `@/components/issue-properties/board-picker`
)

const rows = () =>
  Array.from(document.querySelectorAll(`[data-slot=command-item]`))

describe(`BoardPicker`, () => {
  const onSelect = vi.fn()
  beforeEach(() => onSelect.mockReset())

  const open = () => {
    render(
      <BoardPicker
        teamId="team-1"
        selectedBoardId="board-1"
        issueIdentifier="EXP-42"
        onSelect={onSelect}
      />
    )
    fireEvent.click(screen.getAllByRole(`button`)[0]!)
  }

  it(`stages the confirmation instead of moving the issue`, () => {
    open()
    expect(rows()).toHaveLength(2)
    fireEvent.click(rows()[1]!)
    // The list closed and the confirmation opened — nothing moved yet.
    expect(onSelect).not.toHaveBeenCalled()
    expect(screen.getByTestId(`issue-move-board-confirm`)).toBeTruthy()

    fireEvent.click(screen.getByText(`Move`))
    expect(onSelect).toHaveBeenCalledWith(`board-2`)
  })

  it(`picking the board the issue is already on is a no-op`, () => {
    open()
    fireEvent.click(rows()[0]!)
    expect(onSelect).not.toHaveBeenCalled()
    expect(screen.queryByTestId(`issue-move-board-confirm`)).toBeNull()
  })
})
