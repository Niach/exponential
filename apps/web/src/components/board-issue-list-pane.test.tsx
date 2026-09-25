import * as React from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Issue } from "@/db/schema"
import type { IssueGroup } from "@/lib/board-view"
import type { StatusRowOption } from "@/lib/team-statuses"

// EXP-996/EXP-1048: the sidebar pane's MULTISELECT — the gestures the IDE's
// list column has had since EXP-863. A plain click must still open the issue;
// everything else is selection.

const navigate = vi.hoisted(() => vi.fn())

// The real Link needs a router. This stand-in forwards the modifier click and
// "navigates" only when the row did NOT take the click for the selection.
vi.mock(`@tanstack/react-router`, () => ({
  Link: ({
    params,
    to: _to,
    search: _search,
    onClick,
    children,
    ...rest
  }: {
    params: { issueIdentifier: string }
    to?: string
    search?: unknown
    onClick?: (event: React.MouseEvent<HTMLAnchorElement>) => void
    children?: React.ReactNode
  }) => (
    <a
      {...rest}
      data-testid={`pane-row-${params.issueIdentifier}`}
      onClick={(event) => {
        onClick?.(event)
        if (!event.defaultPrevented) navigate(params.issueIdentifier)
      }}
    >
      {children}
    </a>
  ),
}))
// The status glyph is scenery here; the group band reaches for the tint.
vi.mock(`@/components/issue-properties/status-dropdown`, () => ({
  IssueStatusIcon: () => null,
  statusColorClass: () => ``,
}))

import { BoardIssueListPane } from "@/components/board-issue-list-pane"

const status: StatusRowOption = {
  id: `builtin:backlog`,
  name: `Backlog`,
  colorHex: `#888888`,
  category: `backlog`,
  builtinKey: `backlog`,
  sortOrder: 0,
  icon: `circle-dashed`,
}

const makeIssue = (n: number): Issue =>
  ({
    id: `i${n}`,
    identifier: `EXP-${n}`,
    teamId: `t1`,
    boardId: `b1`,
    title: `Row ${n}`,
    status: `backlog`,
    statusId: null,
  }) as unknown as Issue

const groups: IssueGroup[] = [
  { issues: [1, 2, 3, 4].map(makeIssue), status },
]

/** Renders the pane with the host's selection state, as `list-nav.tsx` owns
 *  it, and reports the live selection back. */
function renderPane({ bulk = true }: { bulk?: boolean } = {}) {
  const live = { selected: new Set<string>() }
  function Host() {
    const [selected, setSelected] = React.useState<Set<string>>(new Set())
    live.selected = selected
    return (
      <BoardIssueListPane
        groups={groups}
        teamSlug="acme"
        boardSlug="core"
        activeIssueId=""
        bulkTeamId={bulk ? `t1` : undefined}
        canModerate
        selectedIds={selected}
        onSelectedIdsChange={setSelected}
      />
    )
  }
  render(<Host />)
  return live
}

const ids = (live: { selected: Set<string> }) => [...live.selected].sort()

beforeEach(() => {
  navigate.mockReset()
})

describe(`BoardIssueListPane selection`, () => {
  it(`toggles a row from its checkbox, without opening it`, () => {
    const live = renderPane()
    fireEvent.click(screen.getByLabelText(`Select EXP-2`))
    expect(ids(live)).toEqual([`i2`])
    fireEvent.click(screen.getByLabelText(`Select EXP-2`))
    expect(ids(live)).toEqual([])
    expect(navigate).not.toHaveBeenCalled()
  })

  it(`toggles on Cmd/Ctrl-click instead of navigating`, () => {
    const live = renderPane()
    fireEvent.click(screen.getByTestId(`pane-row-EXP-1`), { metaKey: true })
    fireEvent.click(screen.getByTestId(`pane-row-EXP-3`), { ctrlKey: true })
    expect(ids(live)).toEqual([`i1`, `i3`])
    expect(navigate).not.toHaveBeenCalled()
  })

  it(`extends the range on Shift-click, anchored on the last toggle`, () => {
    const live = renderPane()
    fireEvent.click(screen.getByLabelText(`Select EXP-2`))
    fireEvent.click(screen.getByTestId(`pane-row-EXP-4`), { shiftKey: true })
    expect(ids(live)).toEqual([`i2`, `i3`, `i4`])
    expect(navigate).not.toHaveBeenCalled()
  })

  it(`selects every visible row on Cmd/Ctrl+A and clears on Escape`, () => {
    const live = renderPane()
    fireEvent.keyDown(window, { key: `a`, metaKey: true })
    expect(ids(live)).toEqual([`i1`, `i2`, `i3`, `i4`])
    fireEvent.keyDown(window, { key: `Escape` })
    expect(ids(live)).toEqual([])
  })

  it(`opens the issue on a plain click`, () => {
    const live = renderPane()
    fireEvent.click(screen.getByTestId(`pane-row-EXP-1`))
    expect(navigate).toHaveBeenCalledWith(`EXP-1`)
    expect(ids(live)).toEqual([])
  })

  it(`stays a plain list without a bulk team`, () => {
    const live = renderPane({ bulk: false })
    expect(screen.queryByLabelText(`Select EXP-1`)).toBeNull()
    fireEvent.click(screen.getByTestId(`pane-row-EXP-1`), { metaKey: true })
    expect(navigate).toHaveBeenCalledWith(`EXP-1`)
    expect(ids(live)).toEqual([])
  })
})
