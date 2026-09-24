// EXP-941: "Unassign" used to be a `__unassign__` CommandItem that only
// existed once somebody was assigned. EXP-1021: it is the shared
// `AssigneePicker`'s `allowsNone` row now — always present, reporting `null`,
// with no sentinel string anywhere.
import { fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"
import { AssigneePicker } from "@/components/issue-properties/assignee-picker"
import type { User } from "@/db/schema"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const USERS = [
  { id: `u-1`, name: `Ada Lovelace`, email: `ada@example.com`, image: null },
  { id: `u-2`, name: `Alan Turing`, email: `alan@example.com`, image: null },
] as unknown as User[]

const rows = () =>
  Array.from(document.querySelectorAll(`[data-slot=command-item]`))

describe(`AssigneePicker`, () => {
  const onSelect = vi.fn()
  beforeEach(() => onSelect.mockReset())

  const open = (selectedUserId: string | null) => {
    render(
      <AssigneePicker
        users={USERS}
        selectedUserId={selectedUserId}
        onSelect={onSelect}
      />
    )
    fireEvent.click(screen.getAllByRole(`button`)[0]!)
  }

  it(`offers Unassign with no sentinel value and reports null`, () => {
    open(`u-1`)
    const all = rows()
    expect(all).toHaveLength(3)
    expect(all[0]!.textContent).toContain(`Unassign`)
    for (const row of all) {
      expect(row.outerHTML).not.toContain(`__`)
    }
    fireEvent.click(all[0]!)
    expect(onSelect).toHaveBeenCalledWith(null)
  })

  it(`checks the assigned row and reports the pick`, () => {
    open(`u-2`)
    expect(
      rows()[2]!.querySelector(`[data-selected-glyph=check]`)
    ).toBeTruthy()
    fireEvent.click(rows()[1]!)
    expect(onSelect).toHaveBeenCalledWith(`u-1`)
  })

  it(`searches on the email as well as the name`, () => {
    open(null)
    fireEvent.change(
      document.querySelector(`[data-slot=command-input]`) as HTMLInputElement,
      { target: { value: `alan@example` } }
    )
    // One row left: the avatar (its `AT` initials), the name, and the email as
    // the shared picker's muted second line.
    expect(rows()).toHaveLength(1)
    const row = rows()[0]!
    expect(row.textContent).toBe(`ATAlan Turingalan@example.com`)
    expect(
      row.querySelector(`[data-slot=picker-description]`)?.textContent
    ).toBe(`alan@example.com`)
  })
})
