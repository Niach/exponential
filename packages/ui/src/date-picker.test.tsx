// A negative-offset zone on purpose: `new Date("2026-03-08")` is parsed as
// UTC midnight, which west of Greenwich renders as March 7th. Every one of the
// three hand-rolled pickers this replaces had that bug latent in it, so the
// suite runs where it would bite.
process.env.TZ = `America/Los_Angeles`

import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { DatePicker, formatDateLabel, parseDateValue } from "./date-picker"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never
Element.prototype.scrollIntoView ??= function scrollIntoView() {}

const openPicker = () => {
  fireEvent.click(screen.getAllByRole(`button`)[0]!)
}

describe(`parseDateValue`, () => {
  it(`reads the wire format as a LOCAL date, never as UTC`, () => {
    const date = parseDateValue(`2026-03-08`)!
    expect(date.getFullYear()).toBe(2026)
    expect(date.getMonth()).toBe(2)
    expect(date.getDate()).toBe(8)
    expect(formatDateLabel(date)).toBe(`Mar 8`)
  })

  it(`treats null, empty and rubbish as no date`, () => {
    expect(parseDateValue(null)).toBeNull()
    expect(parseDateValue(``)).toBeNull()
    expect(parseDateValue(`not-a-date`)).toBeNull()
  })
})

describe(`DatePicker`, () => {
  it(`shows the short date on the trigger`, () => {
    render(<DatePicker value="2026-03-08" onChange={vi.fn()} />)
    expect(screen.getAllByRole(`button`)[0]!.textContent).toContain(`Mar 8`)
  })

  it(`shows the placeholder while there is no date`, () => {
    render(<DatePicker value={null} onChange={vi.fn()} />)
    expect(screen.getAllByRole(`button`)[0]!.textContent).toContain(`Due date`)
    render(
      <DatePicker value={null} onChange={vi.fn()} placeholder="Pick a day" />
    )
    expect(screen.getAllByRole(`button`).at(-1)!.textContent).not.toBe(``)
    expect(document.body.textContent).toContain(`Pick a day`)
  })

  it(`reports the picked day in the wire format and closes`, () => {
    const onChange = vi.fn()
    render(<DatePicker value="2026-03-08" onChange={onChange} />)
    openPicker()
    const day = document.querySelector(`[data-day="3/17/2026"]`)!
    expect(day).toBeTruthy()
    fireEvent.click(day)
    expect(onChange).toHaveBeenCalledWith(`2026-03-17`)
    expect(document.querySelector(`[data-slot=popover-content]`)).toBeNull()
  })

  it(`clears to null`, () => {
    const onChange = vi.fn()
    render(<DatePicker value="2026-03-08" onChange={onChange} />)
    openPicker()
    fireEvent.click(screen.getByRole(`button`, { name: `Clear` }))
    expect(onChange).toHaveBeenCalledWith(null)
    expect(document.querySelector(`[data-slot=popover-content]`)).toBeNull()
  })

  it(`drops the clear row when clearable is false`, () => {
    render(<DatePicker value="2026-03-08" onChange={vi.fn()} clearable={false} />)
    openPicker()
    expect(screen.queryByRole(`button`, { name: `Clear` })).toBeNull()
  })

  it(`takes a bespoke trigger and never opens while disabled`, () => {
    const { rerender } = render(
      <DatePicker
        value="2026-03-08"
        onChange={vi.fn()}
        renderTrigger={({ label, value }) => (
          <button type="button">{`${label} · ${value}`}</button>
        )}
      />
    )
    expect(screen.getByRole(`button`).textContent).toBe(`Mar 8 · 2026-03-08`)

    rerender(<DatePicker value="2026-03-08" onChange={vi.fn()} disabled />)
    openPicker()
    expect(document.querySelector(`[data-slot=popover-content]`)).toBeNull()
  })
})
