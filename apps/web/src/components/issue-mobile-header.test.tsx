import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue, Board } from "@/db/schema"

// EXP-897: the PHONE header of a Work face carries the same stack / batch
// pill the md+ work header wears — tapping it opens the overlay as a sheet.
// The pill is absent whenever the issue is part of nothing, so a lone issue's
// header keeps the EXP-893 layout exactly as it was.

const rows = vi.hoisted(() => ({ value: [] as unknown[] }))

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
// The badge's own dependencies — the model is what this test exercises, not
// the rows it draws inside the overlay.
vi.mock(`@/hooks/use-open-session`, () => ({ useOpenSession: () => vi.fn() }))
vi.mock(`@/components/issue-chip`, () => ({ IssueChip: () => null }))
vi.mock(`@/components/issue-coding-rows`, () => ({ PrStateBadge: () => null }))
vi.mock(`@/components/agent-session-row`, () => ({
  RunningIndicator: () => null,
}))
vi.mock(`@/lib/collections`, () => ({
  codingSessionCollection: {},
  issueCollection: {},
  issueRelationCollection: {},
}))
// One store behind all three of the badge's queries: only the ISSUE rows
// decide a stack, and neither a session nor a `blocks` relation matches an
// issue row, so the other two come out empty on their own.
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: rows.value }) }
})

import { IssueMobileHeader } from "@/components/issue-mobile-header"
import { PrGraphBadge } from "@/components/pr-graph-badge"

const issue = (id: string, over: Partial<Issue> = {}): Issue =>
  ({
    id,
    identifier: id.toUpperCase(),
    title: `Issue ${id}`,
    status: `in_progress`,
    teamId: `t1`,
    boardId: `b1`,
    branch: `exp/${id.toUpperCase()}`,
    prBaseBranch: null,
    prUrl: `https://github.com/acme/app/pull/${id}`,
    prNumber: 1,
    prState: `open`,
    duplicateOfId: null,
    ...over,
  }) as unknown as Issue

const board = { id: `b1`, slug: `met` } as unknown as Board
const lower = issue(`lower`)
const upper = issue(`upper`, { prBaseBranch: `exp/LOWER`, prNumber: 2 })

function renderHeader(subject: Issue) {
  return render(
    <IssueMobileHeader
      issue={subject}
      board={board}
      teamSlug="acme"
      teamId="t1"
      readOnly={false}
      handlers={{
        handleBoardChange: vi.fn(),
        handleUnmarkDuplicate: vi.fn(),
      }}
      graphBadge={
        <PrGraphBadge
          teamId="t1"
          teamSlug="acme"
          face="issue"
          issue={subject}
        />
      }
    />
  )
}

describe(`IssueMobileHeader`, () => {
  it(`wears the stack pill when the issue's pull request is stacked`, () => {
    rows.value = [lower, upper]
    renderHeader(upper)
    expect(screen.getByTestId(`pr-graph-badge`)).toBeTruthy()
    expect(screen.getByText(`2 of 2`)).toBeTruthy()
  })

  it(`wears no pill when the issue is part of nothing`, () => {
    rows.value = [lower]
    renderHeader(lower)
    expect(screen.queryByTestId(`pr-graph-badge`)).toBeNull()
    // The header itself is untouched — identifier centred, `…` on the right.
    expect(screen.getByText(`LOWER`)).toBeTruthy()
  })
})
