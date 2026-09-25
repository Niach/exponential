// EXP-941/EXP-1021: the label picker is the shared `LabelPicker` with two
// slots — the "Create label" row is its `footer` and the create form its
// `panel`, which REPLACES the search field and the list. All the create state
// stayed here, so this pins the two edges the swap could have dropped.
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import type { Label } from "@/db/schema"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const LABELS = [
  { id: `label-1`, teamId: `team-1`, name: `bug`, color: `#ef4444` },
  { id: `label-2`, teamId: `team-1`, name: `chore`, color: `#22c55e` },
] as unknown as Label[]

vi.mock(`@/lib/collections`, () => ({ labelCollection: {} }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))
vi.mock(`@tanstack/react-db`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  return { ...actual, useLiveQuery: () => ({ data: LABELS }) }
})

const { LabelPicker } = await import(
  `@/components/issue-properties/label-picker`
)

const rows = () =>
  Array.from(document.querySelectorAll(`[data-slot=command-item]`))

describe(`LabelPicker`, () => {
  const onToggle = vi.fn()
  beforeEach(() => onToggle.mockReset())

  const open = (selected: string[] = []) => {
    render(
      <LabelPicker
        teamId="team-1"
        selectedLabelIds={selected}
        onToggle={onToggle}
      />
    )
    fireEvent.click(screen.getAllByRole(`button`)[0]!)
  }

  it(`toggles ONE label per pick and stays open`, () => {
    open([`label-1`])
    expect(rows()).toHaveLength(2)
    // EXP-1021: a multi pick reads as the ROW's own highlight — no leading
    // circle pair, no checkbox, nothing in the gutter.
    expect(document.querySelectorAll(`[data-picked="true"]`)).toHaveLength(1)
    expect(document.querySelectorAll(`[data-selected-glyph]`)).toHaveLength(0)

    fireEvent.click(rows()[1]!)
    expect(onToggle).toHaveBeenCalledWith(`label-2`)
    // A batch is several picks: the list survives one.
    expect(rows()).toHaveLength(2)

    fireEvent.click(rows()[0]!)
    expect(onToggle).toHaveBeenLastCalledWith(`label-1`)
  })

  it(`the footer row swaps the list for the create panel`, () => {
    open()
    expect(document.querySelector(`[data-slot=command-input]`)).toBeTruthy()

    fireEvent.click(screen.getByText(`Create label`))
    // The panel replaces both the search field and the rows.
    expect(document.querySelector(`[data-slot=command-input]`)).toBeNull()
    expect(rows()).toHaveLength(0)
    expect(screen.getByPlaceholderText(`Label name`)).toBeTruthy()
  })

  it(`the create form refuses a duplicate name`, () => {
    open()
    fireEvent.click(screen.getByText(`Create label`))
    fireEvent.change(screen.getByPlaceholderText(`Label name`), {
      target: { value: `Bug` },
    })
    expect(
      screen.getByText(`A label with this name already exists.`)
    ).toBeTruthy()
  })
})
