// EXP-958: the search sheet is ONE body — the shared `ComboboxList` fed by the
// shared engine (EXP-892) — inside two shells. This pins the desktop shell:
// ONE search field, one row per ranked result, a pick that NAVIGATES, and no
// selection marker anywhere (nothing here is ever "the picked row").
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const mockState = vi.hoisted(() => {
  const boards = [
    { id: `b-1`, slug: `core`, name: `Core`, icon: null, color: null },
  ]
  const issues = [
    {
      id: `i-1`,
      identifier: `EXP-1`,
      title: `Alpha`,
      boardId: `b-1`,
      status: `backlog`,
      statusId: null,
    },
    {
      id: `i-2`,
      identifier: `EXP-2`,
      title: `Beta`,
      boardId: `b-1`,
      status: `backlog`,
      statusId: null,
    },
  ]
  return {
    boards,
    issues,
    results: [...issues] as unknown[],
    navigate: vi.fn(),
  }
})

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => mockState.navigate,
}))
vi.mock(`@tanstack/react-db`, () => ({
  useLiveQuery: () => ({ data: mockState.issues }),
  inArray: () => true,
}))
vi.mock(`@/lib/collections`, () => ({ issueCollection: {} }))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBoards: () => mockState.boards,
}))
// The engine has its own fixtures; here it only has to rank, so the stub hands
// back whatever the test put in front of it, in order.
vi.mock(`@/hooks/use-issue-search-results`, () => ({
  useIssueSearchResults: () => ({ results: mockState.results }),
}))
vi.mock(`@/components/issue-properties/status-dropdown`, () => ({
  IssueStatusIcon: () => <span data-testid="status-icon" />,
}))

const { IssueSearchSheet } = await import(`@/components/issue-search-sheet`)

const rows = () =>
  Array.from(document.querySelectorAll(`[data-slot=command-item]`))

function open() {
  render(
    <IssueSearchSheet
      open
      onOpenChange={vi.fn()}
      teamId="team-1"
      teamSlug="acme"
    />
  )
}

describe(`IssueSearchSheet`, () => {
  beforeEach(() => {
    mockState.navigate.mockReset()
    mockState.results = [...mockState.issues]
  })

  it(`renders one shared search field over the engine's rows`, () => {
    open()

    expect(document.querySelectorAll(`[data-slot=command-input]`).length).toBe(1)
    expect(rows().map((row) => row.textContent)).toEqual([
      `AlphaCore · EXP-1`,
      `BetaCore · EXP-2`,
    ])
    // Picking navigates, so no row is ever the picked one.
    expect(document.querySelector(`[data-selected-glyph]`)).toBeNull()
  })

  it(`a pick navigates to the issue on its board`, () => {
    open()

    fireEvent.click(rows()[1]!)

    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: {
        teamSlug: `acme`,
        boardSlug: `core`,
        issueIdentifier: `EXP-2`,
      },
    })
  })

  it(`says what to do while nothing matches`, () => {
    mockState.results = []
    open()

    expect(rows()).toHaveLength(0)
    expect(screen.getByText(`Type to search issues`)).toBeTruthy()
  })
})
