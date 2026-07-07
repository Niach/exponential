import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { FileDiffList, type PullFile } from "@/components/diff-view"

// FileDiffList itself never calls tRPC (only DiffView does), but the module
// imports the client — stub it so the test doesn't drag in the app router.
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {},
}))

const smallFile: PullFile = {
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
}

const binaryFile: PullFile = {
  filename: `assets/logo.png`,
  status: `added`,
  additions: 0,
  deletions: 0,
}

function makeLargeFile(lines: number): PullFile {
  return {
    filename: `big/generated.txt`,
    status: `modified`,
    additions: 0,
    deletions: 0,
    patch: [
      `@@ -1,${lines} +1,${lines} @@`,
      ...Array.from({ length: lines }, (_, i) => ` line ${i + 1}`),
    ].join(`\n`),
  }
}

describe(`FileDiffList`, () => {
  it(`renders a file navigation summary for multi-file diffs`, () => {
    render(<FileDiffList files={[smallFile, binaryFile]} />)

    expect(screen.getByText(`2 files changed`)).toBeTruthy()
    // Nav row + section header both name the file.
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

  it(`collapses large patches by default and caps the reveal with Show more`, () => {
    render(<FileDiffList files={[makeLargeFile(600)]} />)

    // Collapsed: line count hint visible, no diff rows yet.
    expect(screen.getByText(`601 lines`)).toBeTruthy()
    expect(screen.queryByText(`line 42`)).toBeNull()

    fireEvent.click(screen.getByRole(`button`, { name: /generated\.txt/ }))

    // Expanded: first chunk rendered, remainder behind the Show-more button.
    expect(screen.getByText(`line 42`)).toBeTruthy()
    expect(screen.getByText(/101 hidden/)).toBeTruthy()
    expect(screen.queryByText(`line 550`)).toBeNull()

    fireEvent.click(screen.getByRole(`button`, { name: /more lines/ }))
    expect(screen.getByText(`line 550`)).toBeTruthy()
  })

  it(`shows a note instead of rows for files without a textual diff`, () => {
    render(<FileDiffList files={[binaryFile]} />)

    expect(
      screen.getByText(`No textual diff (binary or too large).`)
    ).toBeTruthy()
  })
})
