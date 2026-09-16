import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import { DIFF_FILTER_PLACEHOLDER, FileDiffTree } from "./file-diff-tree"

// EXP-916: the file column is a TREE now. The ported `file-diff-nav` cases (the
// summary header, a pick reporting the whole path, the filter, the selection
// highlight, the empty label) plus the tree's own: folders fold,
// a lone-child chain compacts, a query flattens.

/** The parsed counts are the patch's own — `fromPullFile` only falls back to
 *  GitHub's numbers when there are no hunks at all. */
const file = (filename: string, adds = 1): DiffFile =>
  fromPullFile({
    filename,
    status: `modified`,
    additions: adds,
    deletions: 0,
    patch: [
      `@@ -1 +1,${adds + 1} @@`,
      ` kept`,
      ...Array.from({ length: adds }, (_, i) => `+added ${i}`),
    ].join(`\n`),
  })

const FILES = [file(`src/alpha.ts`), file(`apps/web/src/beta.tsx`, 5)]

describe(`FileDiffTree`, () => {
  it(`heads the list with the contract's summary label`, () => {
    render(<FileDiffTree files={FILES} onSelect={vi.fn()} />)
    // `summaryLabel(2, 6, 0)` — U+2212 on the deletions.
    expect(screen.getByText(`2 files +6 −0`)).toBeTruthy()
  })

  it(`a pick reports the path, not the basename`, () => {
    const onSelect = vi.fn()
    render(<FileDiffTree files={FILES} onSelect={onSelect} />)
    fireEvent.click(screen.getByTestId(`diff-nav-row-apps/web/src/beta.tsx`))
    expect(onSelect).toHaveBeenCalledWith(`apps/web/src/beta.tsx`)
  })

  it(`a file row is letter · name · counts, the DIR is its folder`, () => {
    render(<FileDiffTree files={FILES} onSelect={vi.fn()} />)
    const row = screen.getByTestId(`diff-nav-row-apps/web/src/beta.tsx`)
    expect(row.textContent).toBe(`Mbeta.tsx+5 −0`)
  })

  it(`compacts a lone-child directory chain into one folder row`, () => {
    render(<FileDiffTree files={FILES} onSelect={vi.fn()} />)
    // `apps` → `web` → `src` has one child each and no files of its own.
    const dir = screen.getByTestId(`diff-nav-dir-apps/web/src`)
    expect(dir.textContent).toContain(`apps/web/src`)
    expect(dir.textContent).toContain(`+5 −0`)
    expect(screen.queryByTestId(`diff-nav-dir-apps`)).toBeNull()
  })

  it(`folders open by default and a click folds one away`, () => {
    render(<FileDiffTree files={FILES} onSelect={vi.fn()} />)
    const dir = screen.getByTestId(`diff-nav-dir-apps/web/src`)
    expect(dir.getAttribute(`aria-expanded`)).toBe(`true`)
    expect(screen.getByTestId(`diff-nav-row-apps/web/src/beta.tsx`)).toBeTruthy()

    fireEvent.click(dir)
    expect(
      screen.getByTestId(`diff-nav-dir-apps/web/src`).getAttribute(`aria-expanded`)
    ).toBe(`false`)
    expect(screen.queryByTestId(`diff-nav-row-apps/web/src/beta.tsx`)).toBeNull()
    // The sibling directory is untouched — the fold is keyed by path.
    expect(screen.getByTestId(`diff-nav-row-src/alpha.ts`)).toBeTruthy()

    fireEvent.click(screen.getByTestId(`diff-nav-dir-apps/web/src`))
    expect(screen.getByTestId(`diff-nav-row-apps/web/src/beta.tsx`)).toBeTruthy()
  })

  it(`a query flattens the tree to the matching file rows`, () => {
    render(<FileDiffTree files={FILES} onSelect={vi.fn()} />)
    fireEvent.change(screen.getByLabelText(DIFF_FILTER_PLACEHOLDER), {
      target: { value: `BETA` },
    })
    expect(screen.queryByTestId(`diff-nav-row-src/alpha.ts`)).toBeNull()
    expect(screen.getByTestId(`diff-nav-row-apps/web/src/beta.tsx`)).toBeTruthy()
    // No folders at all while a query stands.
    expect(screen.queryByTestId(`diff-nav-dir-apps/web/src`)).toBeNull()
    // The header still counts the WHOLE diff, not the filtered slice.
    expect(screen.getByText(`2 files +6 −0`)).toBeTruthy()

    // A directory segment filters too.
    fireEvent.change(screen.getByLabelText(DIFF_FILTER_PLACEHOLDER), {
      target: { value: `apps/` },
    })
    expect(screen.queryByTestId(`diff-nav-row-src/alpha.ts`)).toBeNull()
    expect(screen.getByTestId(`diff-nav-row-apps/web/src/beta.tsx`)).toBeTruthy()
  })

  it(`always carries its filter field`, () => {
    render(<FileDiffTree files={FILES} onSelect={vi.fn()} />)
    expect(screen.getByTestId(`diff-nav-filter`)).toBeTruthy()
  })

  it(`highlights the selected row`, () => {
    render(
      <FileDiffTree files={FILES} selected="src/alpha.ts" onSelect={vi.fn()} />
    )
    expect(
      screen.getByTestId(`diff-nav-row-src/alpha.ts`).className
    ).toContain(`bg-glass-active`)
  })

  it(`an empty diff says so through the summary label`, () => {
    render(<FileDiffTree files={[]} onSelect={vi.fn()} />)
    expect(screen.getByText(`No changes`)).toBeTruthy()
  })
})
