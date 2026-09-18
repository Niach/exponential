import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import { ChangesFileSheet, CHANGED_FILES_TITLE } from "./changes-file-sheet"

const file = (filename: string): DiffFile =>
  fromPullFile({
    filename,
    status: `modified`,
    additions: 1,
    deletions: 0,
    patch: [`@@ -1 +1,2 @@`, ` kept`, `+added`].join(`\n`),
  })

const FILES = [file(`src/a.ts`), file(`apps/web/src/b.tsx`)]

// EXP-895: the phone's file list. `FileDiffList`'s md+ aside is gone on a phone,
// so the list is a bottom sheet off the work bar's LEADING slot. EXP-916: the
// sheet holds the same file TREE the md+ column does.
describe(`ChangesFileSheet`, () => {
  it(`the bar button wears the files glyph and the file count`, () => {
    render(<ChangesFileSheet files={FILES} onSelect={vi.fn()} />)
    const button = screen.getByTestId(`changes-file-sheet-button`)
    expect(button.textContent).toBe(`2`)
    expect(button.querySelector(`svg`)).not.toBeNull()
    // The 52px work-bar circle, not an ad-hoc button.
    expect(button.className).toContain(`size-[52px]`)
    // Closed until tapped.
    expect(screen.queryByTestId(`changes-file-sheet`)).toBeNull()
  })

  it(`opens the sheet with the shared file TREE`, () => {
    render(<ChangesFileSheet files={FILES} onSelect={vi.fn()} />)
    fireEvent.click(screen.getByTestId(`changes-file-sheet-button`))
    // The title is the contract's.
    expect(CHANGED_FILES_TITLE).toBe(`Changed files`)
    expect(screen.getByText(CHANGED_FILES_TITLE)).toBeTruthy()
    expect(screen.getByTestId(`file-diff-tree`)).toBeTruthy()
    expect(screen.getByTestId(`diff-nav-row-src/a.ts`)).toBeTruthy()
    // …and its folders, compacted the contract's way.
    expect(screen.getByTestId(`diff-nav-dir-apps/web/src`)).toBeTruthy()
  })

  it(`a pick closes the sheet and reports the path`, () => {
    const onSelect = vi.fn()
    render(<ChangesFileSheet files={FILES} onSelect={onSelect} />)
    fireEvent.click(screen.getByTestId(`changes-file-sheet-button`))
    fireEvent.click(screen.getByTestId(`diff-nav-row-apps/web/src/b.tsx`))
    expect(onSelect).toHaveBeenCalledWith(`apps/web/src/b.tsx`)
    expect(screen.queryByTestId(`diff-nav-row-src/a.ts`)).toBeNull()
  })
})
