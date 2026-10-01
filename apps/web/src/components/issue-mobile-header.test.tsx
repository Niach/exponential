import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue, Board } from "@/db/schema"

// The PHONE header of a Work face: the identifier centred, the face's own
// action and — on the Issue face alone — the issue's `…` and pin.

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => vi.fn(),
  Link: ({ children, ...rest }: { children: React.ReactNode }) => (
    <a {...rest}>{children}</a>
  ),
}))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/components/issue-actions-menu`, () => ({
  issueUrlFor: () => `https://exp.test/issue`,
}))
vi.mock(`@/components/issue-detail-mobile-menu`, () => ({
  IssueDetailMobileMenu: () => <button type="button">More</button>,
}))
vi.mock(`@/components/pin-toggle-button`, () => ({
  PinToggleButton: () => <button type="button">Pin</button>,
}))
vi.mock(`@/lib/collections`, () => ({
  codingSessionCollection: {},
  issueCollection: {},
  issueRelationCollection: {},
}))

import { IssueMobileHeader } from "@/components/issue-mobile-header"

const issue = {
  id: `lower`,
  identifier: `LOWER`,
  title: `Issue lower`,
  status: `in_progress`,
  boardId: `b1`,
  branch: `exp/LOWER`,
  prUrl: `https://github.com/acme/app/pull/1`,
  prNumber: 1,
  prState: `open`,
  duplicateOfId: null,
} as unknown as Issue

const board = { id: `b1`, slug: `met` } as unknown as Board

function renderHeader(face?: `issue` | `run` | `changes`) {
  return render(
    <IssueMobileHeader
      issue={issue}
      board={board}
      teamSlug="acme"
      teamId="t1"
      readOnly={false}
      face={face}
      handlers={{
        handleBoardChange: vi.fn(),
        handleUnmarkDuplicate: vi.fn(),
      }}
    />
  )
}

describe(`IssueMobileHeader`, () => {
  it(`centres the identifier`, () => {
    renderHeader()
    expect(screen.getByText(`LOWER`)).toBeTruthy()
  })

  // EXP-934: Share / Move to board / Delete act on the ISSUE, so they belong
  // to the Issue face alone. Every other face keeps the run's own verb.
  it(`carries the context menu on the issue face alone`, () => {
    const issueFace = renderHeader(`issue`)
    expect(screen.getByText(`More`)).toBeTruthy()
    expect(screen.getByText(`Pin`)).toBeTruthy()
    issueFace.unmount()

    for (const face of [`run`, `changes`] as const) {
      const other = renderHeader(face)
      expect(screen.queryByText(`More`), face).toBeNull()
      expect(screen.queryByText(`Pin`), face).toBeNull()
      other.unmount()
    }
  })
})
