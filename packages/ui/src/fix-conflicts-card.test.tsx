import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import {
  branchLine,
  FixConflictsCard,
  FixConflictsNote,
  FixConflictsPrRow,
} from "./fix-conflicts-card"

// EXP-1233: the Fix merge conflicts card's pieces.
describe(`branchLine`, () => {
  it(`joins branch and base with an arrow, and degrades gracefully`, () => {
    expect(branchLine(`exp/APP-14`, `master`)).toBe(`exp/APP-14 → master`)
    expect(branchLine(`exp/APP-14`, null)).toBe(`exp/APP-14`)
    expect(branchLine(null, `master`)).toBe(``)
  })
})

describe(`FixConflictsPrRow`, () => {
  it(`names the pull request, its branches and opens on click`, () => {
    const onClick = vi.fn()
    render(
      <FixConflictsCard>
        <FixConflictsPrRow
          pr={{ prNumber: 2117, branch: `exp/APP-14`, baseBranch: `master` }}
          placeholder="Select a pull request…"
          onClick={onClick}
        />
        <FixConflictsNote>Merge refused: the branch has conflicts.</FixConflictsNote>
      </FixConflictsCard>
    )
    const row = screen.getByRole(`button`, { name: `Pull request #2117` })
    expect(row.textContent).toContain(`#2117`)
    expect(row.textContent).toContain(`exp/APP-14 → master`)
    expect(row.getAttribute(`data-state`)).toBe(`closed`)
    fireEvent.click(row)
    expect(onClick).toHaveBeenCalledOnce()
    const note = screen.getByText(`Merge refused: the branch has conflicts.`)
    expect(note.closest(`[data-slot="fix-conflicts-note"]`)?.className).toContain(
      `text-destructive`
    )
  })

  it(`shows the placeholder alone while nothing is picked`, () => {
    render(<FixConflictsPrRow pr={null} placeholder="Select a pull request…" />)
    const row = screen.getByRole(`button`, { name: `Select a pull request…` })
    expect(row.textContent).toBe(`Select a pull request…`)
  })
})
