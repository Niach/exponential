import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import type { Board, Issue } from "@/db/schema"
import { IssueChangesFace } from "@/components/issue-changes-face"

const filesState = vi.hoisted(() => ({
  value: { kind: `files`, files: [] as DiffFile[] },
}))

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@/hooks/use-review-files`, () => ({
  useReviewFiles: () => ({ state: filesState.value, reload: vi.fn() }),
}))
// The face reads the steer config off the session view; importing that module
// would drag the whole steering surface into this test.
vi.mock(`@/components/agent-session`, () => ({
  useSteerConfig: () => ({ enabled: true }),
}))
vi.mock(`@/hooks/use-issue-property-handlers`, () => ({
  useIssuePropertyHandlers: () => ({
    handleBoardChange: vi.fn(),
    handleUnmarkDuplicate: vi.fn(),
    duplicatePicker: null,
  }),
}))
// A stub header that only re-renders its ACTION slot — that slot is what
// EXP-895 moved GitHub into, and it is all this test needs from the bar.
vi.mock(`@/components/issue-mobile-header`, () => ({
  IssueMobileHeader: ({ action }: { action?: React.ReactNode }) => (
    <div data-testid="issue-mobile-header">{action}</div>
  ),
  TitleStateDot: () => null,
}))

const file = (filename: string): DiffFile =>
  fromPullFile({
    filename,
    status: `modified`,
    additions: 1,
    deletions: 0,
    patch: [`@@ -1 +1,2 @@`, ` kept`, `+added`].join(`\n`),
  })

const issue = {
  id: `i1`,
  identifier: `MET-12`,
  teamId: `t1`,
  boardId: `b1`,
  title: `Do the thing`,
  prState: `open`,
  prNumber: 7,
  prUrl: `https://github.com/o/r/pull/7`,
  branch: `exp/MET-12`,
  updatedAt: `2026-09-01T10:00:00.000Z`,
  duplicateOfId: null,
} as unknown as Issue
const board = { id: `b1`, slug: `met` } as unknown as Board

function renderFace() {
  return render(
    <IssueChangesFace
      issue={issue}
      board={board}
      teamSlug="acme"
      teamId="t1"
      readOnly={false}
      switcher={<div data-testid="switcher" />}
    />
  )
}

// EXP-895: an issue's Changes face — the FILE SHEET on the bar's leading slot,
// GitHub up in the header's action slot, exactly one Merge (the capsule).
describe(`IssueChangesFace`, () => {
  it(`puts GitHub in the header action slot and the file sheet in the bar`, () => {
    filesState.value = { kind: `files`, files: [file(`src/a.ts`), file(`src/b.ts`)] }
    renderFace()
    expect(
      screen
        .getByTestId(`issue-mobile-header`)
        .querySelector(`[data-testid="changes-github-action"]`)
    ).not.toBeNull()
    // The bar's leading slot is the sheet, not GitHub any more.
    expect(screen.getByTestId(`changes-file-sheet-button`).textContent).toBe(`2`)
    expect(screen.queryByTestId(`changes-github-circle`)).toBeNull()
    expect(screen.getByTestId(`switcher`)).toBeTruthy()
  })

  it(`draws the cards alone — the sheet owns the list — and all of them closed`, () => {
    filesState.value = { kind: `files`, files: [file(`src/a.ts`), file(`src/b.ts`)] }
    renderFace()
    expect(screen.getByTestId(`changes-view`)).toBeTruthy()
    expect(screen.queryByTestId(`file-diff-nav`)).toBeNull()
    expect(screen.getAllByTestId(`file-diff-card`)).toHaveLength(2)
    expect(screen.queryByText(`@@ -1 +1,2 @@`)).toBeNull()
  })

  it(`a pick in the sheet scrolls the cards to that file`, () => {
    filesState.value = { kind: `files`, files: [file(`src/a.ts`), file(`src/b.ts`)] }
    Element.prototype.scrollIntoView = vi.fn()
    const raf = vi
      .spyOn(window, `requestAnimationFrame`)
      .mockImplementation((cb: FrameRequestCallback) => {
        cb(0)
        return 0
      })
    renderFace()
    fireEvent.click(screen.getByTestId(`changes-file-sheet-button`))
    fireEvent.click(screen.getByTestId(`diff-nav-row-src/b.ts`))
    expect(Element.prototype.scrollIntoView).toHaveBeenCalled()
    raf.mockRestore()
  })

  it(`the bar carries EXACTLY ONE merge control while the PR is open`, () => {
    filesState.value = { kind: `files`, files: [file(`src/a.ts`)] }
    renderFace()
    expect(
      screen.getAllByRole(`button`, { name: `Merge pull request` })
    ).toHaveLength(1)
  })

  it(`nothing pushed = the empty note, no sheet, no merge`, () => {
    filesState.value = { kind: `none`, files: [] }
    renderFace()
    expect(
      screen.getByText(`No changes yet — nothing has been pushed for this issue.`)
    ).toBeTruthy()
    expect(screen.queryByTestId(`changes-file-sheet-button`)).toBeNull()
  })
})
