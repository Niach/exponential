import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import { ChangesView } from "@/components/changes-view"
import { DIFF_SCOPE_ALL_LABEL } from "@/lib/session-file-cards"

const file = (filename: string): DiffFile =>
  fromPullFile({
    filename,
    status: `modified`,
    additions: 2,
    deletions: 1,
    patch: [`@@ -1 +1,2 @@`, ` kept`, `+added`].join(`\n`),
  })

// EXP-877/EXP-895: the scope-chip behaviours the diff PANE's test used to lock
// (EXP-862), now on the Changes VIEW — the one body the review page, the run's
// Changes face and an issue's Changes face all draw.
describe(`ChangesView`, () => {
  it(`the scope chip names the turn and takes the view back to all changes`, () => {
    const onClearScope = vi.fn()
    render(
      <ChangesView
        files={[file(`src/a.ts`)]}
        nav="auto"
        selected="src/a.ts"
        onSelect={vi.fn()}
        scopeLabel="This turn: 1 file"
        onClearScope={onClearScope}
      />
    )
    expect(screen.getByText(`This turn: 1 file`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`changes-scope-chip`))
    expect(onClearScope).toHaveBeenCalledOnce()
    expect(screen.getByLabelText(DIFF_SCOPE_ALL_LABEL)).toBeTruthy()
  })

  it(`hides the chip when there is nothing to widen back to`, () => {
    render(
      <ChangesView
        files={[file(`src/a.ts`)]}
        nav="auto"
        selected={null}
        onSelect={vi.fn()}
        scopeLabel="This turn: 1 file"
      />
    )
    expect(screen.queryByTestId(`changes-scope-chip`)).toBeNull()
    expect(screen.queryByText(`This turn: 1 file`)).toBeNull()
  })

  it(`lists the files and hands a pick to the caller`, () => {
    const onSelect = vi.fn()
    render(
      <ChangesView
        files={[file(`src/a.ts`), file(`src/b.ts`)]}
        nav="auto"
        selected="src/a.ts"
        onSelect={onSelect}
      />
    )
    // The file column heads itself with the contract's summary label.
    expect(screen.getByText(`2 files +2 −0`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`diff-nav-row-src/b.ts`))
    expect(onSelect).toHaveBeenCalledWith(`src/b.ts`)
  })

  it(`nav="none" draws the cards alone — the phone's sheet owns the list`, () => {
    render(
      <ChangesView
        files={[file(`src/a.ts`), file(`src/b.ts`)]}
        nav="none"
        defaultCollapsed
      />
    )
    expect(screen.queryByTestId(`file-diff-nav`)).toBeNull()
    expect(screen.getAllByTestId(`file-diff-card`)).toHaveLength(2)
    // defaultCollapsed: no rows until a header is clicked.
    expect(screen.queryByText(`@@ -1 +1,2 @@`)).toBeNull()
  })
})
