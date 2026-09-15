import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { fromPullFile, type DiffFile } from "@exp/domain-contract/diff"
import { DIFF_FILTER_PLACEHOLDER, FileDiffNav } from "./file-diff-nav"

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

describe(`FileDiffNav`, () => {
  it(`heads the list with the contract's summary label`, () => {
    render(<FileDiffNav files={FILES} onSelect={vi.fn()} />)
    // `summaryLabel(2, 6, 0)` — U+2212 on the deletions.
    expect(screen.getByText(`2 files +6 −0`)).toBeTruthy()
  })

  it(`a pick reports the path, not the basename`, () => {
    const onSelect = vi.fn()
    render(<FileDiffNav files={FILES} onSelect={onSelect} />)
    fireEvent.click(screen.getByTestId(`diff-nav-row-apps/web/src/beta.tsx`))
    expect(onSelect).toHaveBeenCalledWith(`apps/web/src/beta.tsx`)
  })

  it(`rows are letter · name · dimmed dir · counts`, () => {
    render(<FileDiffNav files={FILES} onSelect={vi.fn()} />)
    const row = screen.getByTestId(`diff-nav-row-apps/web/src/beta.tsx`)
    expect(row.textContent).toBe(`Mbeta.tsxapps/web/src+5 −0`)
  })

  it(`the filter matches a path substring, case-insensitively`, () => {
    render(<FileDiffNav files={FILES} onSelect={vi.fn()} />)
    fireEvent.change(screen.getByLabelText(DIFF_FILTER_PLACEHOLDER), {
      target: { value: `BETA` },
    })
    expect(screen.queryByTestId(`diff-nav-row-src/alpha.ts`)).toBeNull()
    expect(screen.getByTestId(`diff-nav-row-apps/web/src/beta.tsx`)).toBeTruthy()
    // The header still counts the WHOLE diff, not the filtered slice.
    expect(screen.getByText(`2 files +6 −0`)).toBeTruthy()

    // A directory segment filters too.
    fireEvent.change(screen.getByLabelText(DIFF_FILTER_PLACEHOLDER), {
      target: { value: `apps/` },
    })
    expect(screen.queryByTestId(`diff-nav-row-src/alpha.ts`)).toBeNull()
  })

  it(`filterable=false drops the field entirely`, () => {
    render(<FileDiffNav files={FILES} onSelect={vi.fn()} filterable={false} />)
    expect(screen.queryByTestId(`diff-nav-filter`)).toBeNull()
  })

  it(`highlights the selected row`, () => {
    render(
      <FileDiffNav files={FILES} selected="src/alpha.ts" onSelect={vi.fn()} />
    )
    expect(
      screen.getByTestId(`diff-nav-row-src/alpha.ts`).className
    ).toContain(`bg-glass-active`)
  })

  it(`an empty diff says so through the summary label`, () => {
    render(<FileDiffNav files={[]} onSelect={vi.fn()} />)
    expect(screen.getByText(`No changes`)).toBeTruthy()
  })
})
