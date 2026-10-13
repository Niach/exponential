import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue, Board } from "@/db/schema"

// EXP-893 / EXP-1293: the PHONE header of an issue: back, the identifier
// centred, then the related-work badge (when there is one), the pin and the
// `…`. One face: the issue's own controls always show.

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => vi.fn(),
}))
vi.mock(`@/components/issue-actions-menu`, () => ({
  IssueActionsMenu: () => <button type="button">More</button>,
}))
vi.mock(`@/components/pin-toggle-button`, () => ({
  PinToggleButton: () => <button type="button">Pin</button>,
}))

import { IssueMobileHeader } from "@/components/issue-mobile-header"

const issue = {
  id: `lower`,
  identifier: `LOWER`,
  title: `Issue lower`,
  boardId: `b1`,
  duplicateOfId: null,
} as unknown as Issue
const board = { id: `b1`, slug: `met` } as unknown as Board

function renderHeader(graphBadge?: React.ReactNode) {
  return render(
    <IssueMobileHeader
      issue={issue}
      board={board}
      teamSlug="acme"
      teamId="t1"
      readOnly={false}
      handlers={{
        handleBoardChange: vi.fn(),
        handleUnmarkDuplicate: vi.fn(),
      }}
      graphBadge={graphBadge}
    />
  )
}

describe(`IssueMobileHeader`, () => {
  it(`leads the trailing cluster with the related-work badge`, () => {
    renderHeader(<span data-testid="pr-graph-badge" />)
    const badge = screen.getByTestId(`pr-graph-badge`)
    expect(
      badge.compareDocumentPosition(screen.getByText(`More`)) &
        Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy()
  })

  it(`always carries the pin and the context menu`, () => {
    renderHeader()
    expect(screen.getByText(`LOWER`)).toBeTruthy()
    expect(screen.getByText(`Pin`)).toBeTruthy()
    expect(screen.getByText(`More`)).toBeTruthy()
    expect(screen.queryByTestId(`pr-graph-badge`)).toBeNull()
  })
})
