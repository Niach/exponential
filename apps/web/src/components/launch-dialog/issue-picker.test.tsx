// EXP-941/EXP-892: the composer's issue picker rides the shared `Combobox`
// with `shouldFilter={false}` — the CALLER ranks (checked rows pinned first,
// then the shared engine's hits) and the primitive renders that order
// verbatim. This pins both: the order it hands over, and that a pick reports
// ONE issue id while the list stays open.
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Issue } from "@/db/schema"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const issue = (id: string, identifier: string, title: string) =>
  ({ id, identifier, title, priority: `none` }) as unknown as Issue

const ELIGIBLE = [
  issue(`i-1`, `EXP-1`, `Alpha`),
  issue(`i-2`, `EXP-2`, `Beta`),
  issue(`i-3`, `EXP-3`, `Gamma`),
]

const searchCalls = vi.hoisted(() => ({ query: `` }))

// The shared engine is exercised by its own fixtures; here it only has to
// rank, so the stub reverses the pool to prove the order survives untouched.
vi.mock(`@/hooks/use-issue-search-results`, () => ({
  useIssueSearchResults: ({ query, rows }: { query: string; rows: Issue[] }) => {
    searchCalls.query = query
    return { results: [...rows].reverse() }
  },
}))
vi.mock(`@/components/issue-properties/status-dropdown`, () => ({
  IssueStatusIcon: () => <span data-testid="status-icon" />,
}))
vi.mock(`@/components/issue-properties/priority-dropdown`, () => ({
  PriorityIcon: () => <span data-testid="priority-icon" />,
}))

const { IssuePicker } = await import(`@/components/launch-dialog/issue-picker`)

const rows = () =>
  Array.from(document.querySelectorAll(`[data-slot=command-item]`))

describe(`IssuePicker`, () => {
  const onToggle = vi.fn()
  beforeEach(() => onToggle.mockReset())

  const open = (checked: Issue[] = []) => {
    render(
      <IssuePicker
        teamId="team-1"
        eligible={ELIGIBLE}
        checked={checked}
        onToggle={onToggle}
      >
        <button type="button">Issues</button>
      </IssuePicker>
    )
    fireEvent.click(screen.getByText(`Issues`))
  }

  it(`renders the caller's order verbatim: checked first, then the ranking`, () => {
    open([ELIGIBLE[0]!])
    expect(rows().map((row) => row.textContent)).toEqual([
      `EXP-1Alpha`,
      `EXP-3Gamma`,
      `EXP-2Beta`,
    ])
    expect(rows()[0]!.getAttribute(`aria-pressed`)).toBe(`true`)
    expect(rows()[1]!.getAttribute(`aria-pressed`)).toBe(`false`)
  })

  it(`reports one issue per pick and stays open`, () => {
    open()
    fireEvent.click(rows()[0]!)
    expect(onToggle).toHaveBeenCalledWith(`i-3`)
    expect(rows()).toHaveLength(3)
  })

  it(`unchecks a pinned row`, () => {
    open([ELIGIBLE[1]!])
    fireEvent.click(rows()[0]!)
    expect(onToggle).toHaveBeenCalledWith(`i-2`)
  })

  it(`hands the search box's text to the engine, not to cmdk`, () => {
    open()
    const input = document.querySelector(
      `[data-slot=command-input]`
    ) as HTMLInputElement
    fireEvent.change(input, { target: { value: `gam` } })
    expect(searchCalls.query).toBe(`gam`)
    // cmdk never filtered: all three rows are still on screen.
    expect(rows()).toHaveLength(3)
  })
})
