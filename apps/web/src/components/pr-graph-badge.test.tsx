import { fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import type { CodingSession, Issue } from "@/db/schema"

// EXP-897 Part 4: the overlay's SECTIONS per face. The model has its own
// tests (`lib/pr-graph.test.ts`); this proves each face shows its own section
// off the same graph, with the same row primitives.
// SLOP-16 r3: THE "Related work" view — group bands over existing rows.

const openSession = vi.hoisted(() => vi.fn())
const navigate = vi.hoisted(() => vi.fn())
const viewport = vi.hoisted(() => ({ phone: false }))

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({ children, ...rest }: { children: React.ReactNode }) => (
    <a {...rest}>{children}</a>
  ),
  useNavigate: () => navigate,
}))
vi.mock(`@/hooks/use-open-session`, () => ({
  useOpenSession: () => openSession,
}))
// SLOP-16 r3: the dialog is the ONE surface at every size — the phone flag
// only changes the rows' density (the sheet arm is the dialog's own).
vi.mock(`@exp/ui`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useIsMobile: () => viewport.phone }
})
// EXP-930: the chip's LINK half matters — a node that does not open its issue
// is the bug. The stub renders whatever link the view hands it.
// SLOP-15: the stub also reports the SIZE it was asked for — the blocked-by
// mini-graph draws the small chip.
vi.mock(`@/components/issue-chip`, () => ({
  IssueChip: ({
    issue,
    link,
    size,
  }: {
    issue: { identifier: string }
    link?: (props: Record<string, unknown>) => React.ReactElement
    size?: string
  }) => {
    const body = (
      <span data-testid={`chip-${issue.identifier}`} data-size={size ?? `md`}>
        {issue.identifier}
      </span>
    )
    return link ? link({ children: body }) : body
  },
}))
// SLOP-16 r3: the rows are the product's EXISTING ones — the relations card's
// issue row and the session tree's run rows. Stubbed to what they are handed.
vi.mock(`@/components/issue-relations-card`, () => ({
  RowList: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  RelationIssueRow: ({
    issue,
    link,
    depth,
    phone,
  }: {
    issue: { identifier: string }
    link?: (props: {
      className: string
      children: React.ReactNode
    }) => React.ReactElement
    depth?: number
    phone: boolean
  }) => (
    <div
      data-testid={`relation-row-${issue.identifier}`}
      data-depth={depth ?? 0}
      data-phone={String(phone)}
    >
      {link
        ? link({ className: ``, children: issue.identifier })
        : issue.identifier}
    </div>
  ),
}))
vi.mock(`@/components/session-list-rows`, () => ({
  RunningSessionRow: ({
    row,
    depth,
  }: {
    row: { session: { id: string } }
    depth: number
  }) => (
    <div data-testid={`run-row-${row.session.id}`} data-depth={depth}>
      <span data-testid="run-dot" />
    </div>
  ),
  PastSessionRow: ({ sessionId, depth }: { sessionId: string; depth: number }) => (
    <div data-testid={`run-row-${sessionId}`} data-depth={depth} />
  ),
}))
vi.mock(`@/hooks/use-agents-data`, () => ({
  useSessionListRows: (_teamId: string | undefined, sessions: unknown[]) =>
    sessions.map((session) => ({
      session,
      issue: undefined,
      device: { label: null },
      paused: false,
    })),
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamUsers: () => ({ userMap: new Map() }),
}))
vi.mock(`@/components/issue-coding-rows`, () => ({
  PrStateBadge: ({ state }: { state: string | null }) => <span>{state}</span>,
}))
vi.mock(`@/components/agent-session-row`, () => ({
  pastRunRowByline: () => ``,
}))
vi.mock(`@/lib/collections`, () => ({
  boardCollection: {},
  codingSessionCollection: {},
  issueCollection: {},
  issueRelationCollection: {},
}))
// EXP-1079: the badge reads four collections through `useLiveQuery`, each
// query aliasing its source (`s` sessions, `i` issues, `r` relations, `b`
// boards). The stub runs the builder against a probe that records the alias
// and answers with THAT table's rows — empty unless a test fills it.
const liveRows = vi.hoisted(() => ({
  tables: {} as Record<string, unknown[]>,
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    useLiveQuery: (build: (query: unknown) => unknown) => {
      let alias = ``
      const chain = { where: () => chain }
      const probe = {
        from: (source: Record<string, unknown>) => {
          alias = Object.keys(source)[0] ?? ``
          return chain
        },
      }
      const result = build(probe)
      return { data: result === undefined ? [] : (liveRows.tables[alias] ?? []) }
    },
  }
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
      relations={input.relations}
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
    // The face's own band leads.
    const bands = screen
      .getAllByTestId(/^pr-graph-band-/)
      .map((node) => node.getAttribute(`data-testid`))
    expect(bands).toEqual([`pr-graph-band-blocked`, `pr-graph-band-batch`])
    expect(screen.getByText(`Blocked by`)).toBeTruthy()
    expect(screen.getByTestId(`chip-BLOCKER`)).toBeTruthy()
    expect(screen.getByText(`In batch with`)).toBeTruthy()
    // EXP-980: the blocked-by section is the mini-graph — blocker in wave 0,
    // the subject behind it in wave 1.
    expect(
      screen.getByTestId(`issue-graph-node-BLOCKER`).getAttribute(`data-wave`)
    ).toBe(`0`)
    expect(
      screen.getByTestId(`issue-graph-node-BATA`).getAttribute(`data-wave`)
    ).toBe(`1`)
    // The batch band = the relations card's rows, each a link; the subject
    // never lists itself as its own batch partner.
    const partners = screen.getByTestId(`pr-graph-batch-partners`)
    expect(
      within(partners).getByTestId(`relation-row-BATB`).querySelector(`a`)
    ).toBeTruthy()
    expect(within(partners).queryByTestId(`relation-row-BATA`)).toBeNull()
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
    // The session tree's own rows: root first, then its child, nested.
    const runs = screen.getAllByTestId(/^run-row-/)
    expect(runs.map((node) => node.getAttribute(`data-testid`))).toEqual([
      `run-row-root`,
      `run-row-child`,
    ])
    expect(runs.map((node) => node.getAttribute(`data-depth`))).toEqual([
      `0`,
      `1`,
    ])
  })

  // EXP-930: the pill on a batch run says `2 issues`; behind it those two
  // issues, each opening. A batch row links NO issue (`issue_id` is null) —
  // the covered set comes off `batch_issue_ids`.
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
    expect(screen.getByTestId(`relation-row-BATA`).querySelector(`a`)).toBeTruthy()
    expect(screen.getByTestId(`relation-row-BATB`).querySelector(`a`)).toBeTruthy()
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
    // The Reviews queue's own stack row, bottom first.
    const rows = screen.getAllByTestId(/^review-row-/)
    expect(rows.map((node) => node.getAttribute(`data-testid`))).toEqual([
      `review-row-LOWER`,
      `review-row-UPPER`,
    ])
    // `Merge stack` = the BOTTOM row's trailing control, as on Reviews.
    const merge = within(rows[0]).getByTestId(`pr-graph-merge-stack`)
    expect(merge.textContent).toContain(`Merge stack`)
    fireEvent.click(merge)
    expect(onMergeStack).toHaveBeenCalledWith(`upper`)
    // The row opens the entry's review page.
    fireEvent.click(rows[1])
    expect(navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/reviews/$issueIdentifier`,
      params: { teamSlug: `acme`, issueIdentifier: `UPPER` },
    })
  })

  // No duplicated information: the batch band lists the batch, so its PR row
  // folds nothing.
  it(`lists a batch's issues once on the changes face`, () => {
    overlay(`changes`, {
      issue: batchA,
      issues: [batchA, batchB],
      sessions: [],
    })
    expect(screen.getByText(`#9`)).toBeTruthy()
    expect(screen.getAllByTestId(`relation-row-BATB`)).toHaveLength(1)
    expect(
      within(screen.getByTestId(`pr-graph-batch-partners`)).getByTestId(
        `relation-row-BATB`
      )
    ).toBeTruthy()
    // Nothing to merge as a stack: a lone batch PR is one entry.
    expect(screen.queryByTestId(`pr-graph-merge-stack`)).toBeNull()
  })

  // …and with no batch band (the subject sits ABOVE a batch PR), the batch
  // PR's issues fold under its row at depth 1.
  it(`folds a lower batch entry's issues under its pr row`, () => {
    const top = issue(`top`, { prBaseBranch: `exp/batch-1`, prNumber: 10 })
    overlay(`changes`, {
      issue: top,
      issues: [batchA, batchB, top],
      sessions: [],
    })
    expect(screen.queryByTestId(`pr-graph-band-batch`)).toBeNull()
    const stack = screen.getByTestId(`pr-graph-band-stack`)
    const order = within(stack)
      .getAllByTestId(/^(review|relation)-row-/)
      .map((node) => node.getAttribute(`data-testid`))
    expect(order).toEqual([
      `review-row-BATA`,
      `relation-row-BATA`,
      `relation-row-BATB`,
      `review-row-TOP`,
    ])
    expect(
      within(stack).getByTestId(`relation-row-BATB`).getAttribute(`data-depth`)
    ).toBe(`1`)
  })

  it(`says so when nothing else is linked`, () => {
    overlay(`issue`, { issue: issue(`lone`), issues: [], sessions: [] })
    expect(
      screen.getByText(`Nothing else is linked to this issue.`)
    ).toBeTruthy()
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

// EXP-1079: the desktop showed "the runs around this one" on the Run face of
// any run with a family; the web showed nothing there.
describe(`PrGraphBadge on the run face (EXP-1079)`, () => {
  afterEach(() => {
    liveRows.tables = {}
  })

  const family = [session(`child`, `root`), session(`root`)]

  it(`wears the session-tree concept for a run with a family and no pr relation`, () => {
    liveRows.tables = { s: family }
    render(
      <PrGraphBadge teamId="t1" teamSlug="acme" face="run" session={family[0]} />
    )
    const badge = screen.getByTestId(`pr-graph-badge`)
    expect(badge.getAttribute(`aria-label`)).toBe(`The runs around this one`)
    expect(badge.querySelector(`svg.lucide-workflow`)).toBeTruthy()
    // One other run of the family, counted beside the glyph.
    expect(within(badge).getByText(`+1`)).toBeTruthy()
  })

  it(`stays away for a run that is alone`, () => {
    liveRows.tables = { s: [session(`alone`)] }
    render(
      <PrGraphBadge
        teamId="t1"
        teamSlug="acme"
        face="run"
        session={session(`alone`)}
      />
    )
    expect(screen.queryByTestId(`pr-graph-badge`)).toBeNull()
  })

  // EXP-1097: the chip no longer reads the face — a family shows on Changes
  // (and Issue) exactly as on Run.
  it(`draws the same chip on the changes face`, () => {
    liveRows.tables = { s: family }
    render(
      <PrGraphBadge teamId="t1" teamSlug="acme" face="changes" session={family[0]} />
    )
    const badge = screen.getByTestId(`pr-graph-badge`)
    expect(badge.getAttribute(`aria-label`)).toBe(`The runs around this one`)
    expect(within(badge).getByText(`+1`)).toBeTruthy()
  })
})

// SLOP-16: the header badge is a quiet ICON BUTTON — the glyph names the
// shape, a muted `+N` counts the rest; no chip restates the title.
describe(`PrGraphBadge as the header icon button (SLOP-16)`, () => {
  afterEach(() => {
    liveRows.tables = {}
  })

  it(`wears the batch glyph and the count, no chip`, () => {
    liveRows.tables = { i: [batchA, batchB], b: [{ id: `b1`, slug: `web` }] }
    render(
      <PrGraphBadge teamId="t1" teamSlug="acme" face="issue" issue={batchB} />
    )
    const badge = screen.getByTestId(`pr-graph-badge`)
    expect(badge.getAttribute(`aria-label`)).toBe(`Batch pull request`)
    expect(badge.getAttribute(`data-shape`)).toBe(`batch`)
    expect(badge.querySelector(`svg.lucide-boxes`)).toBeTruthy()
    expect(within(badge).getByTestId(`pr-graph-badge-count`).textContent).toBe(`+1`)
    expect(within(badge).queryByTestId(`chip-BATA`)).toBeNull()
    expect(within(badge).queryByTestId(`chip-BATB`)).toBeNull()
  })

  it(`wears the stack glyph for a pull request stack`, () => {
    liveRows.tables = { i: [lower, upper], b: [{ id: `b1`, slug: `web` }] }
    render(
      <PrGraphBadge teamId="t1" teamSlug="acme" face="changes" issue={upper} />
    )
    const badge = screen.getByTestId(`pr-graph-badge`)
    expect(badge.getAttribute(`aria-label`)).toBe(`Pull request stack`)
    expect(badge.querySelector(`svg.lucide-layers`)).toBeTruthy()
    expect(within(badge).getByText(`+1`)).toBeTruthy()
  })

  // SLOP-16 r3: ONE surface at every size — the standard dialog, titled
  // "Related work" (a phone gets its bottom-sheet arm from the dialog itself).
  it.each([
    [`≥md`, false],
    [`a phone`, true],
  ])(`opens the Related work dialog on click (%s)`, (_label, phone) => {
    viewport.phone = phone
    liveRows.tables = { i: [batchA, batchB], b: [{ id: `b1`, slug: `web` }] }
    render(
      <PrGraphBadge teamId="t1" teamSlug="acme" face="issue" issue={batchB} />
    )
    expect(screen.queryByRole(`dialog`)).toBeNull()
    fireEvent.click(screen.getByTestId(`pr-graph-badge`))
    const dialog = screen.getByRole(`dialog`)
    expect(dialog.getAttribute(`data-testid`)).toBe(`pr-graph-overlay`)
    expect(within(dialog).getByText(`Related work`)).toBeTruthy()
    expect(within(dialog).getByText(`In batch with`)).toBeTruthy()
    expect(
      within(dialog)
        .getByTestId(`relation-row-BATA`)
        .getAttribute(`data-phone`)
    ).toBe(String(phone))
    viewport.phone = false
  })
})

// EXP-1097: open blockers alone earn the badge on the Issue face — the
// blocked-by glyph, the other blockers counted.
describe(`PrGraphBadge for a blocked issue (EXP-1097)`, () => {
  afterEach(() => {
    liveRows.tables = {}
  })

  it(`wears the blocked-by glyph and counts the blockers`, () => {
    const me = issue(`me`, { prUrl: null })
    const b1 = issue(`b1`, { prUrl: null })
    const b2 = issue(`b2`, { prUrl: null })
    liveRows.tables = {
      i: [me, b1, b2],
      b: [{ id: `b1`, slug: `web` }],
      r: [
        { type: `blocks`, issueId: `b2`, relatedIssueId: `me` },
        { type: `blocks`, issueId: `b1`, relatedIssueId: `me` },
      ],
    }
    render(<PrGraphBadge teamId="t1" teamSlug="acme" face="issue" issue={me} />)
    const badge = screen.getByTestId(`pr-graph-badge`)
    expect(badge.getAttribute(`aria-label`)).toBe(`Blocked by`)
    expect(badge.getAttribute(`data-shape`)).toBe(`blocked`)
    expect(badge.querySelector(`svg.lucide-circle-slash`)).toBeTruthy()
    expect(within(badge).getByText(`+1`)).toBeTruthy()
  })
})

// SLOP-15 / SLOP-16 r3: Blocked by = the COMPACT mini-graph on every
// platform — narrow boxes, the small chip.
describe(`PrGraphOverlay compactness (SLOP-15)`, () => {
  it(`draws the blocked-by graph compact, with small chips`, () => {
    overlay(`issue`, {
      issue: batchA,
      session: null,
      issues: [batchA, batchB, blocker],
      sessions: [],
      relations: [{ type: `blocks`, issueId: `blocker`, relatedIssueId: `bata` }],
    })
    expect(screen.getByTestId(`issue-graph`).getAttribute(`data-density`)).toBe(`compact`)
    const node = screen.getByTestId(`issue-graph-node-BLOCKER`)
    expect(within(node).getByTestId(`chip-BLOCKER`).getAttribute(`data-size`)).toBe(`sm`)
  })
})
