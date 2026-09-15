import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { DiffCounts, DiffPath, DiffStatusLetter } from "./diff-counts"

// EXP-895: the diff atoms. Two locked decisions live here — the deletion count
// is U+2212 MINUS SIGN (never an ASCII hyphen) and every colour comes from a
// `--diff-*` token utility, so no `emerald-`/`rose-` class may appear.

describe(`DiffCounts`, () => {
  it(`is the mono +N −M with a U+2212 minus and the diff token colours`, () => {
    const { container } = render(<DiffCounts additions={12} deletions={3} />)
    expect(container.textContent).toBe(`+12 −3`)
    expect(container.textContent).not.toContain(`-3`)
    expect(container.querySelector(`.font-mono`)).not.toBeNull()
    expect(container.querySelector(`.text-diff-add-fg`)?.textContent).toBe(`+12`)
    expect(container.querySelector(`.text-diff-del-fg`)?.textContent).toBe(
      `−3`
    )
    expect(container.querySelector(`svg`)).toBeNull()
  })

  it(`carries no raw tailwind palette class`, () => {
    const { container } = render(<DiffCounts additions={1} deletions={1} />)
    expect(container.innerHTML).not.toContain(`emerald`)
    expect(container.innerHTML).not.toContain(`rose`)
  })
})

describe(`DiffStatusLetter`, () => {
  it(`colours A and D only; M, R and C stay muted`, () => {
    const { container: added } = render(<DiffStatusLetter status="added" />)
    expect(added.textContent).toBe(`A`)
    expect(added.querySelector(`.text-diff-add-fg`)).not.toBeNull()

    const { container: removed } = render(<DiffStatusLetter status="removed" />)
    expect(removed.textContent).toBe(`D`)
    expect(removed.querySelector(`.text-diff-del-fg`)).not.toBeNull()

    for (const [status, letter] of [
      [`modified`, `M`],
      [`renamed`, `R`],
      [`copied`, `C`],
    ] as const) {
      const { container } = render(<DiffStatusLetter status={status} />)
      expect(container.textContent).toBe(letter)
      expect(container.querySelector(`.text-muted-foreground`)).not.toBeNull()
      // EXP-895 dropped the invented amber/sky accents.
      expect(container.innerHTML).not.toContain(`amber`)
      expect(container.innerHTML).not.toContain(`sky-`)
    }
  })
})

describe(`DiffPath`, () => {
  it(`dims the directory and leaves the basename at full weight`, () => {
    render(<DiffPath path="apps/web/src/example.ts" />)
    const dir = screen.getByText(`apps/web/src/`)
    expect(dir.className).toContain(`text-muted-foreground`)
    expect(screen.getByText(`example.ts`)).toBeTruthy()
  })

  it(`middle-truncates the directory on a phone, never the filename`, () => {
    const { container } = render(
      <DiffPath
        path="apps/web/src/components/deeply/nested/example.ts"
        isMobile
      />
    )
    expect(container.textContent).toContain(`…`)
    expect(container.textContent?.endsWith(`example.ts`)).toBe(true)
  })

  it(`a bare filename has no dimmed run at all`, () => {
    const { container } = render(<DiffPath path="README.md" />)
    expect(container.querySelector(`.text-muted-foreground`)).toBeNull()
    expect(container.textContent).toBe(`README.md`)
  })
})
