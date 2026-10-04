import { fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import type { CodingSession, Issue } from "@/db/schema"

// EXP-897 Part 4 / SLOP-16 r5: THE "Related work" view: the relations card's
// foldable bands (Blocked by · Same pull request · Pull request stack) over
// its rows, nothing else. The model has its own tests (`lib/pr-graph.test.ts`).

const viewport = vi.hoisted(() => ({ phone: false }))

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({
    children,
    to,
    params,
    search,
    className,
    onClick,
  }: {
    children: React.ReactNode
    to: string
    params: Record<string, string>
    search?: Record<string, string>
    className?: string
    onClick?: () => void
  }) => (
    <a
      data-to={to}
      data-issue={params.issueIdentifier}
      data-view={search?.view}
      className={className}
      onClick={onClick}
    >
      {children}
    </a>
  ),
}))
// The dialog is the ONE surface at every size: the phone flag only changes
// the rows' density (the sheet arm is the dialog's own).
vi.mock(`@exp/ui`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useIsMobile: () => viewport.phone }
})
// The band is the relations card's REAL one; its row is stubbed to what it
// is handed (the real one reads the team's statuses and the hover preview).
vi.mock(`@/components/issue-relations-card`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return {
    ...actual,
    RelationIssueRow: ({
      issue,
      link,
      phone,
      code,
      glyph,
      trailing,
      noAssignee,
    }: {
      issue: { identifier: string; title: string }
      link?: (props: {
        className: string
        children: React.ReactNode
      }) => React.ReactElement
      phone: boolean
      code?: string
      glyph?: React.ReactNode
      trailing?: React.ReactNode
      noAssignee?: boolean
    }) => {
      const body = (
        <>
          {glyph ? <span data-testid="row-glyph">{glyph}</span> : null}
          <span data-testid="row-code">{code ?? issue.identifier}</span>
          <span>{issue.title}</span>
        </>
      )
      return (
        <div
          data-testid={`relation-row-${issue.identifier}`}
          data-phone={String(phone)}
          data-assignee={String(!noAssignee)}
        >
          {link ? link({ className: ``, children: body }) : body}
          {trailing}
        </div>
      )
    },
  }
})
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamUsers: () => ({ userMap: new Map() }),
}))
vi.mock(`@/components/issue-coding-rows`, () => ({
  PrStateBadge: ({ state }: { state: string | null }) => (
    <span data-testid="pr-state">{state}</span>
  ),
}))
vi.mock(`@/lib/collections`, () => ({
  boardCollection: {},
  issueCollection: {},
  issueRelationCollection: {},
}))
vi.mock(`@/hooks/use-team-issue-graph`, () => ({
  useTeamBoardIds: () => [`b1`],
}))
// The badge reads three collections through `useLiveQuery`, each query
// aliasing its source (`i` issues, `r` relations, `b` boards). The stub runs
// the builder against a probe that records the alias and answers with THAT
// table's rows: empty unless a test fills it.
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

const lower = issue(`lower`, { prNumber: 1, prState: `merged` })
const upper = issue(`upper`, { prBaseBranch: `exp/LOWER`, prNumber: 2 })
const batchUrl = `https://github.com/acme/app/pull/9`
const batchA = issue(`bata`, { prUrl: batchUrl, branch: `exp/batch-1`, prNumber: 9 })
const batchB = issue(`batb`, { prUrl: batchUrl, branch: `exp/batch-1`, prNumber: 9 })
const blocker = issue(`blocker`)
const boards = new Map([[`b1`, `web`]])

function overlay(
  input: Parameters<typeof prGraph<Issue, CodingSession>>[0],
  onClose = () => {}
) {
  return render(
    <PrGraphOverlay
      graph={prGraph(input)}
      boardSlugById={boards}
      teamId="t1"
      teamSlug="acme"
      onClose={onClose}
    />
  )
}

const bandTitles = () =>
  screen
    .getAllByTestId(/^pr-graph-band-/)
    .map((band) => band.getAttribute(`data-testid`))

describe(`PrGraphOverlay`, () => {
  afterEach(() => {
    viewport.phone = false
  })

  it(`draws the three bands, in order, with their counts`, () => {
    const me = issue(`me`, {
      prUrl: batchUrl,
      branch: `exp/batch-1`,
      prBaseBranch: `exp/LOWER`,
    })
    const partner = issue(`partner`, {
      prUrl: batchUrl,
      branch: `exp/batch-1`,
      prBaseBranch: `exp/LOWER`,
    })
    overlay({
      issue: me,
      issues: [lower, me, partner, blocker],
      relations: [{ type: `blocks`, issueId: `blocker`, relatedIssueId: `me` }],
    })
    expect(bandTitles()).toEqual([
      `pr-graph-band-blocked`,
      `pr-graph-band-batch`,
      `pr-graph-band-stack`,
    ])
    const blocked = screen.getByTestId(`pr-graph-band-blocked`)
    expect(within(blocked).getByText(`Blocked by`)).toBeTruthy()
    expect(within(blocked).getByTestId(`relation-row-BLOCKER`)).toBeTruthy()
    const batch = screen.getByTestId(`pr-graph-band-batch`)
    expect(within(batch).getByText(`Same pull request`)).toBeTruthy()
    // The partners only: the subject itself is not listed.
    expect(within(batch).getByTestId(`relation-row-PARTNER`)).toBeTruthy()
    expect(within(batch).queryByTestId(`relation-row-ME`)).toBeNull()
    const stack = screen.getByTestId(`pr-graph-band-stack`)
    expect(within(stack).getByText(`Pull request stack`)).toBeTruthy()
    // The OTHER pull requests only: the subject's own is left out.
    expect(within(stack).getByTestId(`relation-row-LOWER`)).toBeTruthy()
    expect(within(stack).queryByTestId(`relation-row-ME`)).toBeNull()
  })

  it(`opens an issue row as a link and closes behind it`, () => {
    const onClose = vi.fn()
    overlay({ issue: batchB, issues: [batchA, batchB] }, onClose)
    const link = within(screen.getByTestId(`relation-row-BATA`)).getByText(
      `Issue bata`
    ).parentElement!
    expect(link.getAttribute(`data-to`)).toBe(
      `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`
    )
    fireEvent.click(link)
    expect(onClose).toHaveBeenCalled()
  })

  // EXP-1154: a PR row opens the issue on its Changes face.
  it(`draws a stack row as #n, the pr glyph and its state pill, opening the changes face`, () => {
    const onClose = vi.fn()
    overlay({ issue: upper, issues: [lower, upper] }, onClose)
    const row = screen.getByTestId(`relation-row-LOWER`)
    expect(within(row).getByTestId(`row-code`).textContent).toBe(`#1`)
    expect(within(row).getByTestId(`row-glyph`)).toBeTruthy()
    expect(within(row).getByTestId(`pr-state`).textContent).toBe(`merged`)
    expect(row.getAttribute(`data-assignee`)).toBe(`false`)
    const link = within(row).getByText(`Issue lower`).parentElement!
    expect(link.getAttribute(`data-to`)).toBe(
      `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`
    )
    expect(link.getAttribute(`data-issue`)).toBe(`LOWER`)
    expect(link.getAttribute(`data-view`)).toBe(`diff`)
    fireEvent.click(link)
    expect(onClose).toHaveBeenCalled()
  })

  it(`caps a band at three rows behind Show N more, and folds it`, () => {
    const me = issue(`me`, { prUrl: null })
    const blockers = [`b1`, `b2`, `b3`, `b4`, `b5`].map((id) =>
      issue(id, { prUrl: null })
    )
    overlay({
      issue: me,
      issues: [me, ...blockers],
      relations: blockers.map((row) => ({
        type: `blocks`,
        issueId: row.id,
        relatedIssueId: `me`,
      })),
    })
    const band = screen.getByTestId(`pr-graph-band-blocked`)
    expect(within(band).getAllByTestId(/^relation-row-/)).toHaveLength(3)
    fireEvent.click(within(band).getByText(`Show 2 more`))
    expect(within(band).getAllByTestId(/^relation-row-/)).toHaveLength(5)
    fireEvent.click(within(band).getByText(`Show less`))
    expect(within(band).getAllByTestId(/^relation-row-/)).toHaveLength(3)
    // The header folds the band: its count stays, the rows go.
    fireEvent.click(within(band).getByRole(`button`, { expanded: true }))
    expect(within(band).queryAllByTestId(/^relation-row-/)).toHaveLength(0)
    expect(within(band).getByText(`5`)).toBeTruthy()
  })

  it(`lists a batch run's covered issues and nothing about its runs`, () => {
    const one = issue(`one`, { prUrl: null })
    const two = issue(`two`, { prUrl: null })
    const run = {
      ...session(`run`),
      batchIssueIds: [`one`, `two`],
    } as unknown as CodingSession
    overlay({ session: run, issues: [one, two] })
    expect(bandTitles()).toEqual([`pr-graph-band-batch`])
    expect(screen.getByTestId(`relation-row-ONE`)).toBeTruthy()
    expect(screen.getByTestId(`relation-row-TWO`)).toBeTruthy()
    expect(screen.queryByText(`Runs`)).toBeNull()
    expect(screen.queryByText(`Merge stack`)).toBeNull()
  })

  it(`says so when nothing else is linked`, () => {
    overlay({ issue: issue(`lone`), issues: [] })
    expect(screen.getByText(`Nothing else is linked to this issue.`)).toBeTruthy()
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
        issue={issue(`lone`)}
        variant="glyph"
        fallback={<span data-testid="badge-fallback" />}
      />
    )
    expect(screen.queryByTestId(`pr-graph-badge`)).toBeNull()
    expect(screen.getByTestId(`badge-fallback`)).toBeTruthy()
  })

  it(`no fallback keeps the old behaviour: nothing at all`, () => {
    const { container } = render(
      <PrGraphBadge teamId="t1" teamSlug="acme" issue={issue(`lone`)} variant="glyph" />
    )
    expect(container.innerHTML).toBe(``)
  })
})

// SLOP-16 r5: a run family alone earns no badge.
describe(`PrGraphBadge for a run`, () => {
  afterEach(() => {
    liveRows.tables = {}
  })

  it(`stays away for a run with a family and no relation`, () => {
    const family = [session(`child`, `root`), session(`root`)]
    render(<PrGraphBadge teamId="t1" teamSlug="acme" session={family[0]} />)
    expect(screen.queryByTestId(`pr-graph-badge`)).toBeNull()
  })
})

// SLOP-16: the header badge is a quiet ICON BUTTON: the glyph names the
// shape, a muted `+N` counts the rest.
describe(`PrGraphBadge as the header icon button (SLOP-16)`, () => {
  afterEach(() => {
    liveRows.tables = {}
    viewport.phone = false
  })

  it(`wears the batch glyph and the count`, () => {
    liveRows.tables = { i: [batchA, batchB], b: [{ id: `b1`, slug: `web` }] }
    render(<PrGraphBadge teamId="t1" teamSlug="acme" issue={batchB} />)
    const badge = screen.getByTestId(`pr-graph-badge`)
    expect(badge.getAttribute(`aria-label`)).toBe(`Batch pull request`)
    expect(badge.getAttribute(`data-shape`)).toBe(`batch`)
    expect(badge.querySelector(`svg.lucide-boxes`)).toBeTruthy()
    expect(within(badge).getByTestId(`pr-graph-badge-count`).textContent).toBe(`+1`)
  })

  it(`wears the stack glyph for a pull request stack`, () => {
    liveRows.tables = { i: [lower, upper], b: [{ id: `b1`, slug: `web` }] }
    render(<PrGraphBadge teamId="t1" teamSlug="acme" issue={upper} />)
    const badge = screen.getByTestId(`pr-graph-badge`)
    expect(badge.getAttribute(`aria-label`)).toBe(`Pull request stack`)
    expect(badge.querySelector(`svg.lucide-layers`)).toBeTruthy()
    expect(within(badge).getByText(`+1`)).toBeTruthy()
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
    render(<PrGraphBadge teamId="t1" teamSlug="acme" issue={me} />)
    const badge = screen.getByTestId(`pr-graph-badge`)
    expect(badge.getAttribute(`aria-label`)).toBe(`Blocked by`)
    expect(badge.getAttribute(`data-shape`)).toBe(`blocked`)
    expect(badge.querySelector(`svg.lucide-circle-slash`)).toBeTruthy()
    expect(within(badge).getByText(`+1`)).toBeTruthy()
  })

  // ONE surface at every size: the standard dialog, titled "Related work"
  // (a phone gets its bottom-sheet arm from the dialog itself).
  it.each([
    [`≥md`, false],
    [`a phone`, true],
  ])(`opens the Related work dialog on click (%s)`, (_label, phone) => {
    viewport.phone = phone
    liveRows.tables = { i: [batchA, batchB], b: [{ id: `b1`, slug: `web` }] }
    render(<PrGraphBadge teamId="t1" teamSlug="acme" issue={batchB} />)
    expect(screen.queryByRole(`dialog`)).toBeNull()
    fireEvent.click(screen.getByTestId(`pr-graph-badge`))
    const dialog = screen.getByRole(`dialog`)
    expect(dialog.getAttribute(`data-testid`)).toBe(`pr-graph-overlay`)
    expect(within(dialog).getByText(`Related work`)).toBeTruthy()
    expect(within(dialog).getByText(`Same pull request`)).toBeTruthy()
    expect(
      within(dialog).getByTestId(`relation-row-BATA`).getAttribute(`data-phone`)
    ).toBe(String(phone))
  })
})
