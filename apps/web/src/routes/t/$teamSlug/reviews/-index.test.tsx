// Tests for the Reviews page (EXP-1248): one PrRow per PR, trees nest, a
// stack hangs off ONE rail over its base branch, bands carry no count, rows
// only open (no Merge anywhere). Lives under a `-` prefix so the route
// generator ignores it.
import { fireEvent, render, screen, within } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { ReviewEntry, ReviewItem } from "@/hooks/use-reviews-data"

const mocks = vi.hoisted(() => ({
  navigate: vi.fn(),
  reviews: { current: null as unknown },
}))

vi.mock(`@tanstack/react-router`, () => ({
  createFileRoute: () => (options: Record<string, unknown>) => ({
    ...options,
    useParams: () => ({ teamSlug: `acme` }),
  }),
  redirect: vi.fn(),
  useNavigate: () => mocks.navigate,
}))
vi.mock(`@/hooks/use-reviews-data`, () => ({
  useReviewsData: () => mocks.reviews.current,
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBySlug: () => ({ id: `t1`, slug: `acme` }),
}))
vi.mock(`@/hooks/use-cross-team-scope`, () => ({
  useCrossTeamScope: (team: { id: string }) => ({
    teams: [team],
    teamIds: [team.id],
    grouped: false,
  }),
}))

import {
  Route,
  reviewBlocks,
  reviewRowLabel,
} from "@/routes/t/$teamSlug/reviews/index"

/** An open-PR issue row; `base` = the branch its pull request targets. */
function issue(id: string, identifier: string, base: string, prUrl?: string) {
  return {
    id,
    identifier,
    title: `Title ${identifier}`,
    boardId: `b1`,
    branch: `exp/${identifier}`,
    prBaseBranch: base,
    prState: `open`,
    prNumber: Number(identifier.split(`-`)[1]),
    prUrl: prUrl ?? `https://github.com/o/r/pull/${id}`,
    updatedAt: `2026-10-01T00:00:00Z`,
  }
}

function entryOf(...issues: ReturnType<typeof issue>[]): ReviewEntry {
  return {
    key: issues[0]!.prUrl,
    issue: issues[0]!,
    issues,
  } as unknown as ReviewEntry
}

// A tree: EXP-10 with two children. A stack: EXP-1 ← EXP-2 ← EXP-3 (top).
// EXP-9 alone.
const root = entryOf(issue(`r`, `EXP-10`, `master`))
const childA = entryOf(issue(`a`, `EXP-11`, `exp/EXP-10`))
const childB = entryOf(issue(`c`, `EXP-12`, `exp/EXP-10`))
const bottom = entryOf(issue(`b`, `EXP-1`, `master`))
const middle = entryOf(issue(`m`, `EXP-2`, `exp/EXP-1`))
const top = entryOf(issue(`t`, `EXP-3`, `exp/EXP-2`))
const lone = entryOf(issue(`l`, `EXP-9`, `master`))

const items: ReviewItem[] = [
  { kind: `pr`, entry: root, depth: 0 },
  { kind: `pr`, entry: childA, depth: 1 },
  { kind: `pr`, entry: childB, depth: 1 },
  { kind: `stack`, entries: [top, middle, bottom], baseBranch: `master` },
  { kind: `pr`, entry: lone, depth: 0 },
]

beforeEach(() => {
  mocks.navigate.mockReset()
  mocks.reviews.current = {
    groups: [
      {
        board: { id: `b1`, name: `App`, slug: `app`, teamId: `t1` },
        team: undefined,
        entries: [root, childA, childB, top, middle, bottom, lone],
        items,
      },
    ],
    sessionEntries: [],
    sessionGroups: [
      {
        team: undefined,
        entries: [
          {
            key: `session:s1`,
            session: { id: `s1`, prNumber: 77, actionName: `Release`, issueId: null },
          },
        ],
      },
    ],
    externalGroups: [
      {
        teamId: `t1`,
        repositoryId: `repo1`,
        fullName: `o/r`,
        pulls: [
          { number: 1013, title: `Dockerfiles`, url: `https://github.com/o/r/pull/1013`, draft: false },
          { number: 1014, title: `WIP`, url: `https://github.com/o/r/pull/1014`, draft: true },
        ],
      },
    ],
    count: 10,
    isLoading: false,
    externalLoading: false,
    removeExternalPull: vi.fn(),
    openIssues: [],
  }
})

function mount() {
  const Page = (Route as unknown as { component: () => React.ReactElement })
    .component
  return render(<Page />)
}

/** The PrRow drawing `text` (an identifier or a title). */
function row(text: string): HTMLElement {
  return screen.getByText(text).closest(`[data-pr-row]`) as HTMLElement
}

describe(`reviewRowLabel`, () => {
  it(`names a single-issue PR by its issue`, () => {
    expect(reviewRowLabel(lone)).toEqual({ identifier: `EXP-9`, title: `Title EXP-9` })
  })

  it(`names a batch PR by its first issue plus the rest`, () => {
    const batch = entryOf(issue(`x`, `EXP-5`, `master`), issue(`y`, `EXP-4`, `master`))
    expect(reviewRowLabel(batch)).toEqual({ identifier: `EXP-5 +1`, title: `Title EXP-5` })
  })
})

describe(`reviewBlocks`, () => {
  it(`keeps a tree in ONE list and gives each stack its own rail`, () => {
    const blocks = reviewBlocks(items)
    expect(blocks.map((block) => block.kind)).toEqual([`list`, `stack`, `list`])
    expect(
      blocks[0]!.kind === `list` && blocks[0]!.rows.map((r) => r.depth)
    ).toEqual([0, 1, 1])
  })
})

describe(`Reviews page`, () => {
  it(`draws one PrRow per PR: a tree nests, a stack rails over its base`, () => {
    mount()
    expect(row(`EXP-11`).style.paddingLeft).toBe(row(`EXP-12`).style.paddingLeft)
    expect(row(`EXP-11`).style.paddingLeft).not.toBe(row(`EXP-10`).style.paddingLeft)

    const rail = document.querySelector(`[data-slot="stack-rail"]`) as HTMLElement
    const railRows = [...rail.querySelectorAll(`[data-pr-row]`)]
    expect(railRows.map((el) => el.textContent)).toEqual([
      `EXP-3Title EXP-3stack`,
      `EXP-2Title EXP-2`,
      `EXP-1Title EXP-1`,
      `master`,
    ])
    expect(railRows[3]!.getAttribute(`data-pr-row`)).toBe(`base`)
  })

  it(`carries no Merge, no counts and no dialogs`, () => {
    mount()
    expect(screen.queryByText(/^Merge/)).toBeNull()
    expect(screen.queryByRole(`button`, { name: /merge/i })).toBeNull()
    const header = within(screen.getByTestId(`review-band-b1`)).getByText(`App`)
      .parentElement as HTMLElement
    expect(header.textContent).toBe(`App`)
    expect(screen.queryByRole(`dialog`)).toBeNull()
  })

  it(`a tree row opens the issue's Guide with reviews as the origin`, () => {
    mount()
    fireEvent.click(row(`EXP-11`))
    expect(mocks.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: { teamSlug: `acme`, boardSlug: `app`, issueIdentifier: `EXP-11` },
      search: { from: `reviews`, view: `guide` },
    })
  })

  it(`a stack member opens its own issue's Guide`, () => {
    mount()
    fireEvent.click(row(`EXP-2`))
    expect(mocks.navigate).toHaveBeenCalledWith(
      expect.objectContaining({
        params: expect.objectContaining({ issueIdentifier: `EXP-2` }),
        search: { from: `reviews`, view: `guide` },
      })
    )
  })

  it(`a run PR opens the run's Guide`, () => {
    mount()
    fireEvent.click(row(`#77`))
    expect(mocks.navigate).toHaveBeenCalledWith({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: `acme`, sessionId: `s1` },
      search: { from: `reviews`, view: `guide` },
    })
  })

  it(`an unlinked PR opens GitHub and wears the external-link glyph`, () => {
    const open = vi.spyOn(window, `open`).mockReturnValue(null)
    mount()
    const unlinked = row(`#1013`)
    expect(unlinked.querySelector(`svg[aria-hidden]`)).toBeTruthy()
    fireEvent.click(unlinked)
    expect(open).toHaveBeenCalledWith(
      `https://github.com/o/r/pull/1013`,
      `_blank`,
      `noopener,noreferrer`
    )
    expect(mocks.navigate).not.toHaveBeenCalled()
    expect(row(`#1014`).textContent).toContain(`draft`)
    open.mockRestore()
  })
})
