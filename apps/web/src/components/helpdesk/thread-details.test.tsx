import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

// EXP-887: the escalated issue in the ticket's details rail is THE issue chip
// — glyph, mono identifier and title in one box — not a second hand-rolled
// identifier link with the title repeated underneath.

const issue = {
  id: `i-1`,
  identifier: `SUP-4`,
  title: `Widget submits twice on Safari`,
  boardId: `b-1`,
  status: `backlog`,
  statusId: null,
}
const board = { id: `b-1`, slug: `support`, teamId: `t-1`, deletedAt: null }

const navigate = vi.fn()
vi.mock(`@tanstack/react-router`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useNavigate: () => navigate,
  // A router-less `<a>`: the chip's link arm must render a REAL anchor so
  // ⌘-click / copy-address keep working — that is what the test locks.
  Link: ({
    to,
    params,
    children,
    ...rest
  }: {
    to: string
    params: Record<string, string>
    children?: React.ReactNode
  }) => (
    <a href={to} data-params={JSON.stringify(params)} {...rest}>
      {children}
    </a>
  ),
}))
vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => true,
}))
vi.mock(`@/lib/collections`, () => ({
  issueCollection: {},
  boardCollection: {},
}))
// Three live queries in order: the linked issue, its board, the team's boards.
let call = 0
vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useLiveQuery: () => {
    const data = [[issue], [board], [board]][call % 3]
    call += 1
    return { data }
  },
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    helpdesk: { escalate: { mutate: vi.fn() } },
    widgets: { submissionForThread: { query: async () => null } },
  },
}))

import { ThreadDetails } from "@/components/helpdesk/support-inbox"

const thread = {
  id: `th-1`,
  teamId: `t-1`,
  subject: `Double submit`,
  status: `open`,
  unread: false,
  linkedIssueId: issue.id,
  reporterEmail: `a@b.c`,
  reporterName: null,
  createdAt: new Date().toISOString(),
  updatedAt: new Date().toISOString(),
  lastMessage: null,
  submissionId: null,
} as unknown as Parameters<typeof ThreadDetails>[0][`thread`]

describe(`the ticket's linked issue`, () => {
  it(`draws the shared chip as a real link to the issue`, () => {
    call = 0
    render(
      <ThreadDetails
        thread={thread}
        teamId="t-1"
        teamSlug="acme"
        onEscalated={async () => {}}
      />
    )
    const chip = screen.getByTestId(`support-linked-issue`)
    expect(chip.className).toContain(`issue-chip`)
    expect(chip.textContent).toContain(`SUP-4`)
    expect(chip.textContent).toContain(`Widget submits twice on Safari`)

    const link = screen.getByRole(`link`, { name: `Open SUP-4` })
    expect(link.getAttribute(`href`)).toBe(
      `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`
    )
    expect(JSON.parse(link.getAttribute(`data-params`) ?? `{}`)).toEqual({
      teamSlug: `acme`,
      boardSlug: `support`,
      issueIdentifier: `SUP-4`,
    })
    expect(screen.queryByRole(`button`, { name: `Open SUP-4` })).toBeNull()
    expect(navigate).not.toHaveBeenCalled()
  })
})
