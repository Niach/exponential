import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import { ChangesView } from "@/components/changes-view"

const file = (filename: string): DiffFile =>
  fromPullFile({
    filename,
    status: `modified`,
    additions: 2,
    deletions: 1,
    patch: [`@@ -1 +1,2 @@`, ` kept`, `+added`].join(`\n`),
  })

// EXP-916: the view is the diff and nothing else — the turn-scope chip is gone
// with the per-turn file cards that produced it, and every surface's actions
// live in its own header.
describe(`ChangesView`, () => {
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
    // The file tree heads itself with the contract's summary label.
    expect(screen.getByText(`2 files +2 −0`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`diff-nav-row-src/b.ts`))
    expect(onSelect).toHaveBeenCalledWith(`src/b.ts`)
  })

  it(`nothing sits above the cards — no chip, no bar`, () => {
    render(<ChangesView files={[file(`src/a.ts`)]} nav="auto" />)
    expect(screen.queryByTestId(`changes-scope-chip`)).toBeNull()
    expect(screen.queryByTestId(`changes-top-bar`)).toBeNull()
  })

  it(`cards start OPEN (EXP-916), the file column is the tree`, () => {
    render(<ChangesView files={[file(`src/a.ts`), file(`src/b.ts`)]} nav="auto" />)
    expect(screen.getByTestId(`file-diff-tree`)).toBeTruthy()
    expect(screen.getAllByText(`@@ -1 +1,2 @@`)).toHaveLength(2)
  })

  it(`nav="none" draws the cards alone — the phone's sheet owns the list`, () => {
    render(
      <ChangesView
        files={[file(`src/a.ts`), file(`src/b.ts`)]}
        nav="none"
        defaultCollapsed
      />
    )
    expect(screen.queryByTestId(`file-diff-tree`)).toBeNull()
    expect(screen.getAllByTestId(`file-diff-card`)).toHaveLength(2)
    // defaultCollapsed: no rows until a header is clicked.
    expect(screen.queryByText(`@@ -1 +1,2 @@`)).toBeNull()
  })
})
