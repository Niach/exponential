import { fireEvent, render, screen } from "@testing-library/react"
import { useState, type ReactNode } from "react"
import { describe, expect, it, vi } from "vitest"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import {
  MOBILE_MERGE_FLOAT_CLEARANCE,
  MOBILE_WORK_BAR_CLEARANCE,
} from "@exp/ui"
import type { Board, Issue } from "@/db/schema"
import {
  IssueChangesBody,
  IssueChangesFace,
  MergeCapsule,
} from "@/components/issue-changes-face"
import {
  MobileMergeFloat,
  mobileFaceClearance,
} from "@/components/mobile-merge-float"
import type { ReviewFilesState } from "@/hooks/use-review-files"

// EXP-952: the route fetches the files and hands the state down; the test
// plays the route.
const filesState = {
  value: { kind: `files`, files: [] as DiffFile[] } as ReviewFilesState,
}

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
// The face reads the steer config off the session view; importing that module
// would drag the whole steering surface into this test.
vi.mock(`@/components/agent-session`, () => ({
  useSteerConfig: () => ({ enabled: true }),
}))
// The merge control's own behaviour (confirm, stack choice, Fix conflicts) is
// `SessionMergePill`'s; the capsule only dresses it.
vi.mock(`@/components/session-merge-button`, () => ({
  SessionMergePill: ({
    label,
    className,
    prState,
  }: {
    label?: string
    className?: string
    prState: string | null
  }) =>
    prState === `open` ? (
      <button type="button" className={className} data-testid="merge-pill">
        {label}
      </button>
    ) : null,
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
  IssueMobileHeader: ({
    action,
    tabs,
  }: {
    action?: React.ReactNode
    tabs?: React.ReactNode
  }) => (
    <>
      <div data-testid="issue-mobile-header">{action}</div>
      {tabs}
    </>
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

const MERGE = {
  issueId: `i1`,
  prState: `open`,
  prNumber: 7,
  branch: `exp/MET-12`,
  updatedAt: `2026-09-01T10:00:00.000Z`,
  steerEnabled: true,
}

function Face({ merge }: { merge?: ReactNode }) {
  // EXP-1154: the route owns the selection (a Results file row seeds it).
  const [selected, setSelected] = useState<string | null>(null)
  return (
    <IssueChangesFace
      issue={issue}
      board={board}
      teamSlug="acme"
      teamId="t1"
      readOnly={false}
      filesState={filesState.value}
      selected={selected}
      onSelect={setSelected}
      merge={merge}
      tabs={<div data-testid="tabs" />}
    />
  )
}

function renderFace(merge?: ReactNode) {
  return render(<Face merge={merge} />)
}

// EXP-895: an issue's Changes face — the FILE SHEET on the bar's leading slot,
// GitHub up in the header's action slot, the white Merge beside the sheet
// (EXP-1154).
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
    expect(screen.getByTestId(`tabs`)).toBeTruthy()
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

  // EXP-1154: the Merge left the header band — the bar's centred cluster is
  // the file sheet + the solid white Merge capsule again.
  it(`carries the white Merge capsule in the bar beside the sheet`, () => {
    filesState.value = { kind: `files`, files: [file(`src/a.ts`)] }
    renderFace(<MergeCapsule {...MERGE} />)
    const bar = screen.getByTestId(`mobile-work-bar`)
    expect(bar.getAttribute(`data-layout`)).toBe(`cluster`)
    const merge = bar.querySelector(`[data-testid="merge-pill"]`)!
    expect(merge.textContent).toBe(`Merge PR`)
    expect(merge.className).toContain(`rounded-full`)
    expect(merge.className).toContain(`h-[52px]`)
    expect(
      bar.querySelector(`[data-testid="changes-file-sheet-button"]`)
    ).not.toBeNull()
  })

  it(`keeps the bar for Merge alone while nothing loaded yet`, () => {
    filesState.value = { kind: `loading` }
    renderFace(<MergeCapsule {...MERGE} />)
    expect(screen.getByTestId(`merge-pill`)).toBeTruthy()
    expect(screen.queryByTestId(`changes-file-sheet-button`)).toBeNull()
    expect(screen.getByText(`Loading changes…`)).toBeTruthy()
  })

  it(`nothing pushed = the empty note, no sheet, no merge`, () => {
    filesState.value = { kind: `none` }
    renderFace()
    expect(
      screen.getByText(`No changes yet. Nothing has been pushed for this issue.`)
    ).toBeTruthy()
    expect(screen.queryByTestId(`changes-file-sheet-button`)).toBeNull()
  })
})

// EXP-1154: the md+ body and the floating capsule of the composer faces.
describe(`IssueChangesBody`, () => {
  it(`offers Retry on a failed load`, () => {
    const onRetry = vi.fn()
    render(
      <IssueChangesBody
        state={{ kind: `error`, message: `GitHub is down` }}
        selected={null}
        onSelect={() => {}}
        onRetry={onRetry}
      />
    )
    expect(screen.getByText(`Couldn’t load changes: GitHub is down`)).toBeTruthy()
    fireEvent.click(screen.getByText(`Retry`))
    expect(onRetry).toHaveBeenCalled()
  })
})

describe(`MobileMergeFloat`, () => {
  it(`floats the capsule above the bar while the PR is open`, () => {
    const { rerender } = render(<MobileMergeFloat {...MERGE} />)
    const float = screen.getByTestId(`mobile-merge-float`)
    expect(float.style.bottom).toContain(`safe-area-inset-bottom`)
    expect(float.querySelector(`[data-testid="merge-pill"]`)).not.toBeNull()
    rerender(<MobileMergeFloat {...MERGE} hidden />)
    expect(screen.queryByTestId(`mobile-merge-float`)).toBeNull()
    rerender(<MobileMergeFloat {...MERGE} prState="merged" />)
    expect(screen.queryByTestId(`mobile-merge-float`)).toBeNull()
  })
})

// EXP-1154: a face scroller under the bar reserves the float's 52px + 10px
// gap on top of the bar's clearance while the float is mounted.
describe(`mobileFaceClearance`, () => {
  it(`switches to the float clearance only while the float shows`, () => {
    expect(mobileFaceClearance(false)).toBe(MOBILE_WORK_BAR_CLEARANCE)
    expect(mobileFaceClearance(true)).toBe(MOBILE_MERGE_FLOAT_CLEARANCE)
    expect(MOBILE_MERGE_FLOAT_CLEARANCE).toContain(`62px`)
  })
})
