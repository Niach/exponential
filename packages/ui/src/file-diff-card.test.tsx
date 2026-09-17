import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { parseDiff, type DiffFile } from "@exp/domain-contract/diff"
import { FileDiffCard, noHunksNote } from "./file-diff-card"

// EXP-895: ONE file's card. The divider rows, the rename crumb, the no-hunks
// note and the two densities are the things a caller cannot see through the
// list, so they are locked here.

function parseOne(text: string): DiffFile {
  const { files } = parseDiff(text)
  expect(files).toHaveLength(1)
  return files[0]
}

const TWO_HUNKS = parseOne(
  [
    `diff --git a/src/example.ts b/src/example.ts`,
    `--- a/src/example.ts`,
    `+++ b/src/example.ts`,
    `@@ -10,3 +10,3 @@`,
    ` const kept = 1`,
    `-const removed = 2`,
    `+const added = 3`,
    `@@ -40,2 +40,3 @@`,
    ` tail`,
    `+more`,
    ` end`,
  ].join(`\n`)
)

describe(`FileDiffCard`, () => {
  it(`draws a plain unchanged-lines divider, never a button`, () => {
    render(<FileDiffCard file={TWO_HUNKS} />)
    // `unchangedBefore` of the first hunk (newStart 10) = 9 lines above it…
    const before = screen.getByText(`9 unchanged lines`)
    expect(before.tagName).toBe(`DIV`)
    expect(before.closest(`button`)).toBeNull()
    // …and `unchangedBetween` the two hunks: 40 - (10 + 3) = 27.
    const between = screen.getByText(`27 unchanged lines`)
    expect(between.closest(`button`)).toBeNull()
    // Both hunk headers render verbatim.
    expect(screen.getByText(`@@ -10,3 +10,3 @@`)).toBeTruthy()
    expect(screen.getByText(`@@ -40,2 +40,3 @@`)).toBeTruthy()
  })

  it(`the hunk header row wears the hunk tokens`, () => {
    const { container } = render(<FileDiffCard file={TWO_HUNKS} />)
    const hunk = container.querySelector(`[data-diff-row="hunk"]`)
    expect(hunk?.className).toContain(`bg-diff-hunk-bg`)
    expect(hunk?.className).toContain(`text-diff-hunk-fg`)
    const add = container.querySelector(`[data-diff-row="add"]`)
    expect(add?.className).toContain(`bg-diff-add-bg`)
    const del = container.querySelector(`[data-diff-row="del"]`)
    expect(del?.className).toContain(`bg-diff-del-bg`)
  })

  it(`a rename names where the file came from`, () => {
    const renamed = parseOne(
      [
        `diff --git a/src/old-name.ts b/src/new-name.ts`,
        `rename from src/old-name.ts`,
        `rename to src/new-name.ts`,
        `--- a/src/old-name.ts`,
        `+++ b/src/new-name.ts`,
        `@@ -1 +1 @@`,
        `-old`,
        `+new`,
      ].join(`\n`)
    )
    render(<FileDiffCard file={renamed} />)
    expect(screen.getByText(`new-name.ts`)).toBeTruthy()
    expect(screen.getByText(`← src/old-name.ts`)).toBeTruthy()
    // R, not A/D — and muted (no invented sky accent).
    const letter = screen.getByText(`R`)
    expect(letter.className).toContain(`text-muted-foreground`)
  })

  it(`a file with no hunks gets a note that says WHY`, () => {
    const binary = parseOne(
      [
        `diff --git a/public/logo.png b/public/logo.png`,
        `Binary files a/public/logo.png and b/public/logo.png differ`,
      ].join(`\n`)
    )
    render(<FileDiffCard file={binary} />)
    expect(screen.getByText(`Binary file`)).toBeTruthy()
  })

  it(`the note distinguishes an empty add, a removal and a pure rename`, () => {
    const base = { additions: 0, deletions: 0, binary: false, hunks: [] }
    expect(noHunksNote({ ...base, path: `a`, status: `added` })).toBe(
      `Empty file added`
    )
    expect(noHunksNote({ ...base, path: `a`, status: `removed` })).toBe(
      `File removed`
    )
    expect(noHunksNote({ ...base, path: `a`, status: `renamed` })).toBe(
      `Renamed without content changes`
    )
    // The binary flag beats the status.
    expect(
      noHunksNote({ ...base, path: `a`, status: `added`, binary: true })
    ).toBe(`Binary file`)
  })

  it(`compact density narrows the two gutters and the sign column`, () => {
    const { container } = render(
      <FileDiffCard file={TWO_HUNKS} density="compact" />
    )
    const row = container.querySelector(`[data-diff-row="add"]`)
    expect(row?.className).toContain(`grid-cols-[2.5rem_2.5rem_0.75rem_1fr]`)

    const { container: wide } = render(<FileDiffCard file={TWO_HUNKS} />)
    expect(
      wide.querySelector(`[data-diff-row="add"]`)?.className
    ).toContain(`grid-cols-[3rem_3rem_1rem_1fr]`)
  })

  it(`a meta line is italic and carries no line numbers`, () => {
    const noNewline = parseOne(
      [
        `--- a/x.txt`,
        `+++ b/x.txt`,
        `@@ -1 +1 @@`,
        `-a`,
        `+b`,
        `\\ No newline at end of file`,
      ].join(`\n`)
    )
    const { container } = render(<FileDiffCard file={noNewline} />)
    const meta = container.querySelector(`[data-diff-row="meta"]`)
    expect(meta?.className).toContain(`italic`)
    expect(meta?.textContent).toBe(`No newline at end of file`)
  })

  it(`mounts its body straight away where there is no IntersectionObserver`, () => {
    // EXP-916: an open card waits until it is NEAR the viewport before it
    // builds its rows. jsdom has no observer, and neither does a server
    // render — both must draw the whole body rather than an empty reservation.
    expect(typeof IntersectionObserver).toBe(`undefined`)
    render(<FileDiffCard file={TWO_HUNKS} />)
    expect(screen.getByText(`@@ -10,3 +10,3 @@`)).toBeTruthy()
    expect(screen.queryByTestId(`file-diff-placeholder`)).toBeNull()
  })

  it(`defaultOpen=false hides the body until the header is clicked`, () => {
    render(<FileDiffCard file={TWO_HUNKS} defaultOpen={false} />)
    expect(screen.queryByText(`@@ -10,3 +10,3 @@`)).toBeNull()
  })
})
