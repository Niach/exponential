import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import type { Board, Issue } from "@/db/schema"
import { IssueChangesFace } from "@/components/issue-changes-face"
import type { ReviewFilesState } from "@/hooks/use-review-files"

// EXP-952: the route fetches the files and hands the state down; the test
// plays the route.
const filesState = {
  value: { kind: `files`, files: [] as DiffFile[] } as ReviewFilesState,
}

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
// EXP-1094: the two workflow shapes the face reads to hide Merge on a node PR.
const workflowState = {
  workflows: [] as { id: string; status: string; teamId: string }[],
  nodes: [] as {
    workflowId: string
    issueId: string
    memberIssueIds: string[]
    teamId: string
  }[],
}
vi.mock(`@/hooks/use-workflows`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/hooks/use-workflows")>()),
  useTeamWorkflows: () => workflowState.workflows,
  useTeamWorkflowNodes: () => workflowState.nodes,
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
  workflowState.workflows = []
  workflowState.nodes = []
  return render(
    <IssueChangesFace
      issue={issue}
      board={board}
      teamSlug="acme"
      teamId="t1"
      readOnly={false}
      filesState={filesState.value}
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

  it(`draws the cards alone — the sheet owns the list — and OPEN (EXP-916)`, () => {
    filesState.value = { kind: `files`, files: [file(`src/a.ts`), file(`src/b.ts`)] }
    renderFace()
    expect(screen.getByTestId(`changes-view`)).toBeTruthy()
    expect(screen.queryByTestId(`file-diff-tree`)).toBeNull()
    expect(screen.getAllByTestId(`file-diff-card`)).toHaveLength(2)
    // EXP-916: cards start OPEN everywhere — only a file past the contract's
    // collapse threshold folds itself.
    expect(screen.getAllByText(`@@ -1 +1,2 @@`)).toHaveLength(2)
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

  // EXP-1094: a workflow NODE PR merges through the workflow (the server
  // refuses the row merge, PR #864), so the bar carries NO Merge capsule
  // while that workflow is running or paused, and offers it again once the
  // workflow is done.
  it(`hides the Merge capsule on a node PR of a live workflow`, () => {
    filesState.value = { kind: `files`, files: [file(`src/a.ts`)] }
    const { rerender, unmount } = renderFace()
    workflowState.workflows = [{ id: `w1`, status: `running`, teamId: `t1` }]
    workflowState.nodes = [
      { workflowId: `w1`, issueId: `other`, memberIssueIds: [`i1`], teamId: `t1` },
    ]
    rerender(
      <IssueChangesFace
        issue={issue}
        board={board}
        teamSlug="acme"
        teamId="t1"
        readOnly={false}
        filesState={filesState.value}
        switcher={<div data-testid="switcher" />}
      />
    )
    expect(
      screen.queryByRole(`button`, { name: `Merge pull request` })
    ).toBeNull()
    unmount()

    workflowState.workflows = [{ id: `w1`, status: `done`, teamId: `t1` }]
    render(
      <IssueChangesFace
        issue={issue}
        board={board}
        teamSlug="acme"
        teamId="t1"
        readOnly={false}
        filesState={filesState.value}
        switcher={<div data-testid="switcher" />}
      />
    )
    expect(
      screen.getAllByRole(`button`, { name: `Merge pull request` })
    ).toHaveLength(1)
  })

  it(`nothing pushed = the empty note, no sheet, no merge`, () => {
    filesState.value = { kind: `none` }
    renderFace()
    expect(
      screen.getByText(`No changes yet — nothing has been pushed for this issue.`)
    ).toBeTruthy()
    expect(screen.queryByTestId(`changes-file-sheet-button`)).toBeNull()
  })
})
