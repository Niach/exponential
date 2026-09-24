// EXP-958: the search sheet is ONE body — the shared `ComboboxList` fed by the
// shared engine (EXP-892) — inside two shells. This pins the desktop shell:
// ONE search field, one row per ranked result, a pick that NAVIGATES, and no
// selection marker anywhere (nothing here is ever "the picked row").
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import {
  ISSUE_SEARCH_EMPTY_DETAIL,
  ISSUE_SEARCH_EMPTY_HINT,
} from "@/lib/issue-search"

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
    mobile: false,
  }
})

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@exp/ui")>()),
  useIsMobile: () => mockState.mobile,
}))
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
  // EXP-1021: the rows carry the issue's resolved status glyph as the
  // picker's own leading slot too; the body this sheet draws is unchanged.
  toStatusPickerStatus: () => ({
    id: `s1`,
    name: `Backlog`,
    category: `backlog`,
  }),
}))
vi.mock(`@/hooks/use-team-statuses`, () => ({
  useTeamStatusesContext: () => ({ resolve: () => ({ id: `s1` }) }),
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
    mockState.mobile = false
  })

  // EXP-971: the phone arm's back arrow rides INSIDE the primitive's field
  // row, so the one field is still cmdk's and Enter opens the top result.
  it(`the phone arm keeps the keyboard: Enter opens the top result`, () => {
    mockState.mobile = true
    open()

    const input = document.querySelector(`[data-slot=command-input]`)!
    expect(document.querySelectorAll(`[data-slot=command-input]`).length).toBe(1)
    const wrapper = input.closest(`[data-slot=command-input-wrapper]`)!
    expect(wrapper.querySelector(`[aria-label=Back]`)).toBeTruthy()
    // The arrow and the field are one row, and the field is the SearchField
    // look drawn around cmdk's own input — not a second field beside it.
    expect(document.querySelectorAll(`[data-slot=search-field]`).length).toBe(1)
    expect(input.closest(`[data-slot=search-field]`)).toBeTruthy()
    // The pill radius rides the root's `**:` variant, like the desktop arm's
    // wrapper height does.
    expect(input.closest(`[data-slot=combobox-list]`)!.className).toContain(
      `data-[slot=command-input]:rounded-full`
    )
    expect(screen.queryByLabelText(`Clear search`)).toBeNull()

    fireEvent.keyDown(input, { key: `Enter` })

    expect(mockState.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: {
        teamSlug: `acme`,
        boardSlug: `core`,
        issueIdentifier: `EXP-1`,
      },
    })
  })

  it(`the phone arm's field clears like SearchField: empty, caret back`, () => {
    mockState.mobile = true
    open()

    const input = document.querySelector<HTMLInputElement>(
      `[data-slot=command-input]`
    )!
    fireEvent.change(input, { target: { value: `alp` } })
    const clear = screen.getByLabelText(`Clear search`)
    expect(input.className).toContain(`pr-8`)
    fireEvent.click(clear)
    expect(input.value).toBe(``)
    expect(screen.queryByLabelText(`Clear search`)).toBeNull()
    expect(document.activeElement).toBe(input)
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
    // EXP-922: the ×4 copy set (issue-search-surfaces.test.ts locks that the
    // other three surfaces spell the same words).
    expect(screen.getByText(ISSUE_SEARCH_EMPTY_HINT)).toBeTruthy()
    expect(screen.getByText(ISSUE_SEARCH_EMPTY_DETAIL)).toBeTruthy()
  })
})
