// Tests for the Reviews queue's stack merge: every member of the chain the
// call walks spins, and nothing keeps a spinner the call did not land. Lives
// under a `-` prefix so the route generator ignores it.
import { act, fireEvent, render, screen, within } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { stackMergeChoice } from "@/lib/pr-stack"
import type { ReviewEntry } from "@/hooks/use-reviews-data"

const mocks = vi.hoisted(() => ({
  mergePr: vi.fn(),
  reviews: { current: null as unknown },
}))

vi.mock(`@tanstack/react-router`, () => ({
  createFileRoute: () => (options: Record<string, unknown>) => ({
    ...options,
    useParams: () => ({ teamSlug: `acme` }),
  }),
  redirect: vi.fn(),
  useNavigate: () => vi.fn(),
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { issues: { mergePr: { mutate: mocks.mergePr } } },
}))
vi.mock(`@/hooks/use-reviews-data`, () => ({
  useReviewsData: () => mocks.reviews.current,
}))
vi.mock(`@/hooks/use-team-data`, () => ({
  useTeamBySlug: () => ({ id: `t1`, slug: `acme` }),
}))
vi.mock(`@/hooks/use-team-permissions`, () => ({
  useTeamPermissions: () => ({ isMember: true }),
}))
vi.mock(`@/hooks/use-open-composer`, () => ({
  useOpenComposer: () => vi.fn(),
}))
vi.mock(`@/components/agent-session`, () => ({
  useSteerConfig: () => ({ enabled: true }),
}))
vi.mock(`@/components/pr-graph-badge`, () => ({ PrGraphBadge: () => null }))

import { Route, stackMergeEntries } from "@/routes/t/$teamSlug/reviews/index"

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

// EXP-1 (bottom) ← EXP-2 ← EXP-3 (top), and EXP-9 beside the stack.
const bottom = issue(`b`, `EXP-1`, `master`)
const middle = issue(`m`, `EXP-2`, `exp/EXP-1`)
const top = issue(`t`, `EXP-3`, `exp/EXP-2`)
const lone = issue(`l`, `EXP-9`, `master`)
const openIssues = [top, bottom, middle, lone]

function entryOf(...issues: ReturnType<typeof issue>[]): ReviewEntry {
  return {
    key: issues[0]!.prUrl,
    issue: issues[0]!,
    issues,
  } as unknown as ReviewEntry
}
const entries = [top, middle, bottom, lone].map((row) => entryOf(row))

beforeEach(() => {
  mocks.mergePr.mockReset()
  mocks.reviews.current = {
    groups: [{ board: { id: `b1`, name: `App` }, entries }],
    sessionEntries: [],
    externalGroups: [],
    count: entries.length,
    isLoading: false,
    externalLoading: false,
    removeExternalPull: vi.fn(),
    openIssues,
  }
})

/** The trailing pill of a row: `Merge` at rest, `Merging…` in flight. */
function pill(identifier: string): HTMLElement {
  return within(screen.getByTestId(`review-row-${identifier}`)).getByRole(
    `button`
  )
}

/** A merge the test settles by hand. */
function pendingMerge() {
  let resolve!: (value: unknown) => void
  let reject!: (error: unknown) => void
  mocks.mergePr.mockReturnValue(
    new Promise((res, rej) => {
      resolve = res
      reject = rej
    })
  )
  return { resolve, reject }
}

function mount() {
  const Page = (Route as unknown as { component: () => React.ReactElement })
    .component
  render(<Page />)
}

function pressInDialog(name: string) {
  fireEvent.click(
    within(screen.getByTestId(`stack-merge-choice-dialog`)).getByRole(
      `button`,
      { name }
    )
  )
}

describe(`stackMergeEntries`, () => {
  const choice = stackMergeChoice(middle, openIssues)!

  it(`Merge stack walks the whole chain, never a row beside it`, () => {
    const walked = stackMergeEntries(
      choice,
      { issueId: choice.topIssueId, mergeStack: true },
      entries
    )
    expect(walked.map((entry) => entry.issue.identifier).sort()).toEqual([
      `EXP-1`,
      `EXP-2`,
      `EXP-3`,
    ])
  })

  it(`Merge this on a middle member walks it and what lies below`, () => {
    const walked = stackMergeEntries(
      choice,
      { issueId: middle.id, mergeStack: true },
      entries
    )
    expect(walked.map((entry) => entry.issue.identifier).sort()).toEqual([
      `EXP-1`,
      `EXP-2`,
    ])
  })

  it(`a plain merge walks nothing`, () => {
    expect(stackMergeEntries(choice, { issueId: bottom.id }, entries)).toEqual(
      []
    )
  })

  it(`finds a batch member by any of its issues`, () => {
    // EXP-2 shares its pull request with EXP-5; the row's newest issue (its
    // representative) is EXP-5, the chain names the PR `EXP-2 +1`.
    const sibling = issue(`s`, `EXP-5`, `exp/EXP-1`, middle.prUrl)
    const batch = entryOf(sibling, middle)
    const batchChoice = stackMergeChoice(top, [...openIssues, sibling])!
    expect(batchChoice.members).toEqual([`EXP-1`, `EXP-2 +1`, `EXP-3`])
    const walked = stackMergeEntries(
      batchChoice,
      { issueId: batchChoice.topIssueId, mergeStack: true },
      [entryOf(top), batch, entryOf(bottom), entryOf(lone)]
    )
    expect(walked).toContain(batch)
    expect(walked).toHaveLength(3)
  })
})

describe(`Reviews stack merge`, () => {
  it(`spins every chain member and releases them all on a refusal`, async () => {
    const merge = pendingMerge()
    mount()
    fireEvent.click(pill(`EXP-2`))
    pressInDialog(`Merge stack`)

    expect(mocks.mergePr).toHaveBeenCalledWith(
      { issueId: `t`, mergeStack: true },
      expect.anything()
    )
    for (const identifier of [`EXP-1`, `EXP-2`, `EXP-3`]) {
      expect(pill(identifier).textContent).toBe(`Merging…`)
      expect(pill(identifier)).toHaveProperty(`disabled`, true)
    }
    expect(pill(`EXP-9`).textContent).toBe(`Merge`)

    await act(async () => {
      merge.reject(new Error(`boom`))
    })
    for (const identifier of [`EXP-1`, `EXP-2`, `EXP-3`]) {
      expect(pill(identifier).textContent).toBe(`Merge`)
    }
    // The refusal captions the pressed row alone (a bare Error carries no
    // server message, so the stack fallback shows).
    const caption = `The stack could not be merged`
    expect(
      within(screen.getByTestId(`review-row-EXP-2`)).getByText(caption)
    ).toBeTruthy()
    expect(screen.getAllByText(caption)).toHaveLength(1)
  })

  it(`Merge this on a middle member leaves the member above it live`, () => {
    pendingMerge()
    mount()
    fireEvent.click(pill(`EXP-2`))
    pressInDialog(`Merge this pull request`)

    expect(mocks.mergePr).toHaveBeenCalledWith(
      { issueId: `m`, mergeStack: true },
      expect.anything()
    )
    expect(pill(`EXP-1`).textContent).toBe(`Merging…`)
    expect(pill(`EXP-2`).textContent).toBe(`Merging…`)
    expect(pill(`EXP-3`).textContent).toBe(`Merge`)
  })

  it(`a queued stack releases what it did not land`, async () => {
    const merge = pendingMerge()
    mount()
    fireEvent.click(pill(`EXP-3`))
    pressInDialog(`Merge stack`)

    await act(async () => {
      merge.resolve({
        merged: false,
        queued: true,
        stack: [{ identifier: `EXP-1`, prNumber: 1 }],
      })
    })
    // Landed: spins until the echo removes the row. The rest come back.
    expect(pill(`EXP-1`).textContent).toBe(`Merging…`)
    expect(pill(`EXP-2`).textContent).toBe(`Merge`)
    expect(pill(`EXP-3`).textContent).toBe(`Merge`)
  })

  it(`a landed stack keeps every spinner until the echo`, async () => {
    const merge = pendingMerge()
    mount()
    fireEvent.click(pill(`EXP-1`))
    pressInDialog(`Merge stack`)

    await act(async () => {
      merge.resolve({
        merged: true,
        stack: [`EXP-1`, `EXP-2`, `EXP-3`].map((identifier, at) => ({
          identifier,
          prNumber: at + 1,
        })),
      })
    })
    for (const identifier of [`EXP-1`, `EXP-2`, `EXP-3`]) {
      expect(pill(identifier).textContent).toBe(`Merging…`)
    }
  })
})
