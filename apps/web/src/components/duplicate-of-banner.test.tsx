import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { Issue } from "@/db/schema"

// EXP-887: the "Duplicate of" banner names an ISSUE, so it draws the ONE issue
// chip (glyph · identifier · title) instead of the mono capsule it used to be
// with the title repeated beside it. The hover preview self-disables on
// phones, so the mobile mock keeps this off the synced collections.

const canonical = {
  id: `i-canonical`,
  identifier: `APP-12`,
  title: `The same crash, filed twice`,
  status: `backlog`,
  statusId: null,
} as unknown as Issue

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => true,
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useLiveQuery: () => ({ data: [canonical] }),
}))

const open = vi.fn()
vi.mock(`@/components/issue-ref-provider`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIssueRefs: () => ({ open, resolve: () => null, resolveById: () => null }),
}))

import { DuplicateOfBanner } from "@/components/issue-detail-view"

describe(`the duplicate-of banner`, () => {
  it(`draws the canonical issue as the shared chip and opens it`, () => {
    render(
      <DuplicateOfBanner
        duplicateOfId={canonical.id}
        onUnmark={vi.fn()}
        readOnly
      />
    )
    const chip = screen.getByTestId(`duplicate-of-chip`)
    // The chip's own box, never a capsule — and the title lives INSIDE it now.
    expect(chip.className).toContain(`issue-chip`)
    expect(chip.className).not.toContain(`rounded-full`)
    expect(chip.textContent).toContain(`APP-12`)
    expect(chip.textContent).toContain(`The same crash, filed twice`)

    fireEvent.click(screen.getByRole(`button`, { name: `Open APP-12` }))
    expect(open).toHaveBeenCalledWith(`APP-12`)
  })

  it(`hides the Unmark action for a read-only viewer`, () => {
    const onUnmark = vi.fn()
    const { rerender } = render(
      <DuplicateOfBanner
        duplicateOfId={canonical.id}
        onUnmark={onUnmark}
        readOnly
      />
    )
    expect(screen.queryByText(`Unmark`)).toBeNull()
    rerender(
      <DuplicateOfBanner
        duplicateOfId={canonical.id}
        onUnmark={onUnmark}
        readOnly={false}
      />
    )
    fireEvent.click(screen.getByText(`Unmark`))
    expect(onUnmark).toHaveBeenCalledTimes(1)
  })
})
