import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import { FileDiffList, truncatedLinesNote } from "./file-diff-list"

// EXP-895: the ported `diff-view.test.tsx` — the same behaviours (nav summary,
// size-based collapse, the capped reveal, the review layout, focus-to-scroll)
// over `DiffFile[]` instead of GitHub's `PullFile`. EXP-916: the column is the
// file TREE and it no longer folds.

const smallFile: DiffFile = fromPullFile({
  filename: `src/example.ts`,
  status: `modified`,
  additions: 2,
  deletions: 1,
  patch: [
    `@@ -1,2 +1,3 @@`,
    ` const kept = 1`,
    `-const removed = 2`,
    `+const addedOne = 3`,
    `+const addedTwo = 4`,
  ].join(`\n`),
})

const binaryFile: DiffFile = fromPullFile({
  filename: `assets/logo.png`,
  status: `added`,
  additions: 0,
  deletions: 0,
})

function makeLargeFile(lines: number): DiffFile {
  return fromPullFile({
    filename: `big/generated.txt`,
    status: `modified`,
    additions: 0,
    deletions: 0,
    patch: [
      `@@ -1,${lines} +1,${lines} @@`,
      ...Array.from({ length: lines }, (_, i) => ` line ${i + 1}`),
    ].join(`\n`),
  })
}

describe(`FileDiffList`, () => {
  it(`renders the file column for a multi-file diff`, () => {
    render(<FileDiffList files={[smallFile, binaryFile]} />)

    expect(screen.getByTestId(`file-diff-tree`)).toBeTruthy()
    expect(screen.getByText(`2 files +2 −1`)).toBeTruthy()
    // Nav row + card header both name the file.
    expect(screen.getAllByText(`example.ts`).length).toBeGreaterThanOrEqual(2)
  })

  it(`expands small files by default with line numbers and highlighting`, () => {
    const { container } = render(<FileDiffList files={[smallFile]} />)

    expect(screen.getByText(`@@ -1,2 +1,3 @@`)).toBeTruthy()
    // Highlighted lines are split into hljs token spans — assert on text
    // content + the presence of token markup.
    expect(container.textContent).toContain(`removed = 2`)
    expect(container.textContent).toContain(`addedTwo = 4`)
    expect(container.querySelector(`.diff-code .hljs-keyword`)).toBeTruthy()
    // Old gutter for the deletion (line 2) and new gutter for the last add
    // (line 3) both render.
    const gutterSelector = `span[class*="tabular-nums"]`
    expect(
      screen.getAllByText(`2`, { selector: gutterSelector }).length
    ).toBeGreaterThanOrEqual(1)
    expect(
      screen.getAllByText(`3`, { selector: gutterSelector }).length
    ).toBeGreaterThanOrEqual(1)
  })

  it(`collapses large files by default and caps the reveal with Show more`, () => {
    render(<FileDiffList files={[makeLargeFile(600)]} nav="none" />)

    expect(screen.queryByText(`line 42`)).toBeNull()

    fireEvent.click(screen.getByRole(`button`, { name: /generated\.txt/ }))

    // Expanded: first chunk rendered, remainder behind the Show-more button.
    expect(screen.getByText(`line 42`)).toBeTruthy()
    expect(screen.getByText(/100 hidden/)).toBeTruthy()
    expect(screen.queryByText(`line 550`)).toBeNull()

    fireEvent.click(screen.getByRole(`button`, { name: /more lines/ }))
    expect(screen.getByText(`line 550`)).toBeTruthy()
  })

  it(`review layout: nav="none" + defaultCollapsed (EXP-248/EXP-706)`, () => {
    render(
      <FileDiffList files={[smallFile, binaryFile]} nav="none" defaultCollapsed />
    )

    // Nothing above the files: the file column is gone, and so is the summary —
    // the Changes top bar carries both.
    expect(screen.queryByTestId(`file-diff-tree`)).toBeNull()
    expect(screen.getAllByText(`example.ts`).length).toBe(1)

    // Even the small file starts collapsed…
    expect(screen.queryByText(`@@ -1,2 +1,3 @@`)).toBeNull()

    // …and expands on demand.
    fireEvent.click(screen.getByRole(`button`, { name: /example\.ts/ }))
    expect(screen.getByText(`@@ -1,2 +1,3 @@`)).toBeTruthy()
  })

  it(`shows a note instead of rows for a file with no hunks`, () => {
    render(<FileDiffList files={[binaryFile]} />)
    // `fromPullFile` of an empty-patch ADD: not binary, just empty.
    expect(screen.getByText(`Empty file added`)).toBeTruthy()
  })

  it(`a single file needs no file column`, () => {
    render(<FileDiffList files={[smallFile]} />)
    expect(screen.queryByTestId(`file-diff-tree`)).toBeNull()
  })

  it(`the publisher's dropped-line count rides under the cards`, () => {
    render(<FileDiffList files={[smallFile]} truncatedLines={120} />)
    expect(screen.getByTestId(`diff-truncation-note`).textContent).toBe(
      `120 more lines truncated`
    )
    expect(truncatedLinesNote(1)).toBe(`1 more line truncated`)
    expect(truncatedLinesNote(120)).toBe(`120 more lines truncated`)
  })

  it(`an empty file set says only what the caller told it to`, () => {
    const { rerender } = render(<FileDiffList files={[]} />)
    expect(screen.queryByTestId(`file-diff-card`)).toBeNull()
    rerender(<FileDiffList files={[]} emptyLabel="No changes in this PR." />)
    expect(screen.getByText(`No changes in this PR.`)).toBeTruthy()
  })

  it(`EXP-916: the file column does not fold — no control takes it away`, () => {
    render(<FileDiffList files={[smallFile, binaryFile]} />)
    expect(screen.getByTestId(`file-diff-tree`)).toBeTruthy()
    expect(screen.queryByTestId(`diff-nav-fold`)).toBeNull()
  })
})

// The caller drives the list from outside — a nav row, a turn's file card, a
// bottom sheet — and the named file has to EXPAND even when its size collapsed
// it.
describe(`focusPath`, () => {
  it(`expands and scrolls to the named file`, () => {
    const scrollIntoView = vi.fn()
    Element.prototype.scrollIntoView = scrollIntoView
    const raf = vi
      .spyOn(window, `requestAnimationFrame`)
      .mockImplementation((cb: FrameRequestCallback) => {
        cb(0)
        return 0
      })
    render(
      <FileDiffList
        files={[makeLargeFile(600)]}
        nav="none"
        focusPath="big/generated.txt"
      />
    )
    expect(screen.getByText(`line 42`)).toBeTruthy()
    expect(scrollIntoView).toHaveBeenCalled()
    raf.mockRestore()
  })

  it(`no focus leaves the size-based default standing`, () => {
    render(<FileDiffList files={[makeLargeFile(600)]} nav="none" />)
    expect(screen.queryByText(`line 42`)).toBeNull()
  })

  it(`picking a file in the column expands it`, () => {
    const raf = vi
      .spyOn(window, `requestAnimationFrame`)
      .mockImplementation((cb: FrameRequestCallback) => {
        cb(0)
        return 0
      })
    Element.prototype.scrollIntoView = vi.fn()
    render(<FileDiffList files={[makeLargeFile(600), smallFile]} />)
    expect(screen.queryByText(`line 42`)).toBeNull()
    fireEvent.click(screen.getByTestId(`diff-nav-row-big/generated.txt`))
    expect(screen.getByText(`line 42`)).toBeTruthy()
    raf.mockRestore()
  })
})
