// REV2-85: the option tables are DISPLAY-ordered, so an unknown/forward-compat
// value must NOT resolve to `options[0]` ("Urgent") — the trigger falls back to
// the lifecycle start of the vocabulary, exactly like `getIssuePriorityConfig`.
// EXP-958 moved that guarantee here: the picker is the shared `Combobox` now,
// whose matched selection is EMPTY for a value the table does not carry, so the
// trigger renders from the caller's resolved config instead.
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { PriorityDropdown } from "@/components/issue-properties/priority-dropdown"
import type { IssuePriority } from "@/lib/domain"

const mockState = vi.hoisted(() => ({ update: vi.fn() }))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { issues: { update: { mutate: mockState.update } } },
}))

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const rows = () =>
  Array.from(document.querySelectorAll(`[data-slot=command-item]`))

describe(`PriorityDropdown`, () => {
  beforeEach(() => mockState.update.mockReset().mockResolvedValue({}))

  it(`names the fallback, not the first display-ordered option`, () => {
    render(
      <PriorityDropdown
        issueId="issue-1"
        priority={`blocker` as IssuePriority}
        disabled
      />
    )

    expect(screen.getByLabelText(`Change priority (current: No priority)`))
      .toBeTruthy()
    expect(
      screen.queryByLabelText(`Change priority (current: Urgent)`)
    ).toBeNull()
  })

  it(`checks the picked row and reports the pick`, () => {
    render(<PriorityDropdown issueId="issue-1" priority="high" />)
    fireEvent.click(screen.getByLabelText(`Change priority (current: High)`))

    const all = rows()
    expect(all).toHaveLength(5)
    expect(all[1]!.querySelector(`[data-selected-glyph=check]`)).toBeTruthy()

    fireEvent.click(all[0]!)
    expect(mockState.update).toHaveBeenCalledWith({
      id: `issue-1`,
      priority: `urgent`,
    })
  })
})
