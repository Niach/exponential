import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { DiffFile } from "@exp/domain-contract/diff"
import {
  GuideSectionDiff,
  guideSectionSummary,
} from "@/components/guide-section-diff"

// EXP-1251: a section's diff as a page under the Guide — the back row names
// the Guide and the section, the md+ tree sits in the column, only the
// section's files show.

function diffFile(path: string, additions: number, deletions: number): DiffFile {
  return { path, status: `modified`, additions, deletions, hunks: [] } as unknown as DiffFile
}

const page = {
  section: 2 as const,
  caption: `02 / 06`,
  title: `Paint, motion, RTL`,
  files: [diffFile(`Paint/Painter.swift`, 402, 118), diffFile(`Paint/Overlays.swift`, 210, 61)],
  additions: 612,
  deletions: 179,
}

describe(`GuideSectionDiff`, () => {
  it(`sums the section in the back row`, () => {
    expect(guideSectionSummary(page)).toBe(`+612 −179 · 2 files`)
  })

  it(`returns to the Guide and names the section`, () => {
    const onBack = vi.fn()
    render(
      <GuideSectionDiff page={page} selected={null} onSelect={vi.fn()} onBack={onBack} isMobile={false} />
    )
    const row = screen.getByTestId(`guide-section-back-row`)
    expect(row.textContent).toContain(`Guide`)
    expect(screen.getByTestId(`guide-section-caption`).textContent).toBe(`02 / 06`)
    expect(row.textContent).toContain(`Paint, motion, RTL`)
    fireEvent.click(screen.getByTestId(`guide-section-back`))
    expect(onBack).toHaveBeenCalled()
  })

  it(`md+ draws the tree in the column; a phone leaves it to the sheet`, () => {
    const { rerender } = render(
      <GuideSectionDiff page={page} selected={null} onSelect={vi.fn()} onBack={vi.fn()} isMobile={false} />
    )
    expect(screen.getByTestId(`guide-section-tree`)).toBeTruthy()
    rerender(
      <GuideSectionDiff page={page} selected={null} onSelect={vi.fn()} onBack={vi.fn()} isMobile />
    )
    expect(screen.queryByTestId(`guide-section-tree`)).toBeNull()
    expect(screen.getByTestId(`file-diff-list`)).toBeTruthy()
  })
})
