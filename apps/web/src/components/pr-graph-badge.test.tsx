import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { CodingSession, Issue } from "@/db/schema"

// EXP-897 Part 4: the overlay's SECTIONS per face. The model has its own
// tests (`lib/pr-graph.test.ts`); this proves each face shows its own section
// off the same graph, with the same row primitives.

const openSession = vi.hoisted(() => vi.fn())

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({ children, ...rest }: { children: React.ReactNode }) => (
    <a {...rest}>{children}</a>
  ),
}))
vi.mock(`@/hooks/use-open-session`, () => ({
  useOpenSession: () => openSession,
}))
// EXP-930: the chip's LINK half matters now — a batch row that does not open
// its issue is the whole bug. The stub renders whatever link the overlay hands
// it, so the assertions can see an anchor.
vi.mock(`@/components/issue-chip`, () => ({
  IssueChip: ({
    issue,
    link,
  }: {
    issue: { identifier: string }
    link?: (props: Record<string, unknown>) => React.ReactElement
  }) => {
    const body = (
      <span data-testid={`chip-${issue.identifier}`}>{issue.identifier}</span>
    )
    return link ? link({ children: body }) : body
  },
}))
vi.mock(`@/components/issue-coding-rows`, () => ({
  PrStateBadge: ({ state }: { state: string | null }) => <span>{state}</span>,
}))
vi.mock(`@/components/agent-session-row`, () => ({
  RunningIndicator: () => <span data-testid="run-dot" />,
}))
vi.mock(`@/lib/collections`, () => ({
  codingSessionCollection: {},
  issueCollection: {},
  issueRelationCollection: {},
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: [] }) }
})

import { PrGraphBadge, PrGraphOverlay } from "@/components/pr-graph-badge"
import { prGraph } from "@/lib/pr-graph"

const issue = (
  id: string,
  over: Partial<Issue> = {}
): Issue =>
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
    ...over,
  }) as unknown as Issue

const session = (
  id: string,
  parentSessionId: string | null = null,
  issueId: string | null = null
): CodingSession =>
  ({
    id,
    parentSessionId,
    issueId,
    status: `running`,
    startedAt: `2026-09-10T10:00:00Z`,
    prState: null,
  }) as unknown as CodingSession

const lower = issue(`lower`)
const upper = issue(`upper`, { prBaseBranch: `exp/LOWER`, prNumber: 2 })
const batchUrl = `https://github.com/acme/app/pull/9`
const batchA = issue(`bata`, { prUrl: batchUrl, branch: `exp/batch-1`, prNumber: 9 })
const batchB = issue(`batb`, { prUrl: batchUrl, branch: `exp/batch-1`, prNumber: 9 })
const blocker = issue(`blocker`)

function overlay(
  face: `issue` | `run` | `changes`,
  input: Parameters<typeof prGraph<Issue, CodingSession>>[0],
  onMergeStack?: (id: string) => void
) {
  const graph = prGraph(input)
  return render(
    <PrGraphOverlay
      face={face}
      graph={graph}
      issues={input.issues}
      boardSlugById={new Map([[`b1`, `web`]])}
      subjectIssue={input.issue ?? null}
      teamSlug="acme"
      onMergeStack={onMergeStack}
      onClose={vi.fn()}
    />
  )
}

describe(`PrGraphOverlay`, () => {
  it(`shows Blocked by and In batch with on the issue face`, () => {
    overlay(`issue`, {
      issue: batchA,
      issues: [batchA, batchB, blocker],
      sessions: [],
      relations: [
        { type: `blocks`, issueId: `blocker`, relatedIssueId: `bata` },
      ],
    })
    expect(screen.getByText(`Blocked by`)).toBeTruthy()
    expect(screen.getByTestId(`chip-BLOCKER`)).toBeTruthy()
    expect(screen.getByText(`In batch with`)).toBeTruthy()
    expect(screen.getByTestId(`chip-BATB`)).toBeTruthy()
    // The subject never lists itself as its own batch partner.
    expect(screen.queryByTestId(`chip-BATA`)).toBeNull()
  })

  it(`shows the session tree on the run face`, () => {
    const rows = [session(`child`, `root`, `upper`), session(`root`, null, `lower`)]
    overlay(`run`, {
      session: rows[0],
      issue: upper,
      issues: [lower, upper],
      sessions: rows,
    })
    expect(screen.getByText(`Runs`)).toBeTruthy()
    // Root first, then its child — the nested order.
    const names = screen.getAllByText(/LOWER|UPPER/).map((node) => node.textContent)
    expect(names).toEqual([`LOWER`, `UPPER`])
    expect(screen.getAllByTestId(`run-dot`)).toHaveLength(2)
  })

  // EXP-930: the pill on a batch run says `2 issues`; behind it those two
  // issues, each opening. A batch row links NO issue (`issue_id` is null) —
  // the covered set comes off `batch_issue_ids`, so keying the overlay off
  // `issue_id` was what left it with nothing to show.
  it(`lists the batch's issues on a batch run's run face`, () => {
    const batchRun = {
      ...session(`batch`),
      issueId: null,
      actionName: null,
      batchIssueIds: [`bata`, `batb`],
      branch: `exp/batch-1`,
    } as unknown as CodingSession
    overlay(`run`, {
      session: batchRun,
      issues: [batchA, batchB],
      sessions: [batchRun],
    })
    expect(screen.getByText(`Issues`)).toBeTruthy()
    expect(screen.getByTestId(`chip-BATA`).closest(`a`)).toBeTruthy()
    expect(screen.getByTestId(`chip-BATB`).closest(`a`)).toBeTruthy()
    // The run tree is still there, under the issues it covers.
    expect(screen.getByText(`Runs`)).toBeTruthy()
  })

  it(`shows the stack bottom-up with Merge stack on the changes face`, () => {
    const onMergeStack = vi.fn()
    overlay(
      `changes`,
      { issue: upper, issues: [lower, upper], sessions: [] },
      onMergeStack
    )
    expect(screen.getByText(`Pull requests`)).toBeTruthy()
    const numbers = screen
      .getAllByText(/^#\d+$/)
      .map((node) => node.textContent)
    expect(numbers).toEqual([`#1`, `#2`])
    expect(screen.getByTestId(`pr-graph-merge-stack`)).toBeTruthy()
  })

  it(`folds a batch entry's issues under it on the changes face`, () => {
    overlay(`changes`, {
      issue: batchA,
      issues: [batchA, batchB],
      sessions: [],
    })
    expect(screen.getByText(`#9`)).toBeTruthy()
    expect(screen.getByTestId(`chip-BATA`)).toBeTruthy()
    expect(screen.getByTestId(`chip-BATB`)).toBeTruthy()
    // Nothing to merge as a stack: a lone batch PR is one entry.
    expect(screen.queryByTestId(`pr-graph-merge-stack`)).toBeNull()
  })
})

// EXP-916: a `glyph` badge sits in a FIXED lead cell of a Reviews grid row.
// When the graph has no badge to draw (the sibling issues have not synced yet)
// it must still fill that cell, or the whole row shifts one column left.
describe(`PrGraphBadge fallback (EXP-916)`, () => {
  it(`renders its fallback when the graph carries no badge`, () => {
    render(
      <PrGraphBadge
        teamId="t1"
        teamSlug="acme"
        face="changes"
        issue={issue(`lone`)}
        variant="glyph"
        fallback={<span data-testid="badge-fallback" />}
      />
    )
    // No batch, no stack — nothing for the badge itself to say.
    expect(screen.queryByTestId(`pr-graph-badge`)).toBeNull()
    expect(screen.getByTestId(`badge-fallback`)).toBeTruthy()
  })

  it(`no fallback keeps the old behaviour — nothing at all`, () => {
    const { container } = render(
      <PrGraphBadge
        teamId="t1"
        teamSlug="acme"
        face="changes"
        issue={issue(`lone`)}
        variant="glyph"
      />
    )
    expect(container.innerHTML).toBe(``)
  })
})
