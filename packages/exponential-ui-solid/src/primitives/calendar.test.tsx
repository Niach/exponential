// FEED-131: the Solid Calendar's behaviour (DayPicker 9's): selection per
// mode, month navigation, the roving focus and its keys, `onSelect`'s
// `(selected, triggerDate, modifiers, event)` payload.

import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library"
import { createSignal } from "solid-js"
import { Calendar, type DateRange } from "./calendar"

const d = (y: number, m: number, day: number) => new Date(y, m - 1, day)
const TODAY = d(2026, 10, 22)
const caption = (container: HTMLElement) => container.querySelector(`.rdp-caption_label`)?.textContent
const dayButton = (container: HTMLElement, iso: string) => container.querySelector<HTMLButtonElement>(`td[data-day="${iso}"] button`)!

afterEach(cleanup)

describe(`Calendar (Solid)`, () => {
  it(`single: uncontrolled selection toggles, and onSelect gets DayPicker's payload`, () => {
    const calls: unknown[][] = []
    const { container } = render(() => <Calendar mode="single" defaultMonth={d(2026, 10, 1)} today={TODAY} />)
    fireEvent.click(dayButton(container, `2026-10-15`))
    expect(container.querySelector(`td[data-day="2026-10-15"]`)!.getAttribute(`data-selected`)).toBe(`true`)
    expect(dayButton(container, `2026-10-15`).getAttribute(`aria-label`)).toBe(`Thursday, October 15th, 2026, selected`)
    fireEvent.click(dayButton(container, `2026-10-15`))
    expect(container.querySelector(`td[data-day="2026-10-15"]`)!.hasAttribute(`data-selected`)).toBe(false)
    cleanup()

    const [selected, setSelected] = createSignal<Date | undefined>(d(2026, 10, 3))
    const { container: c2 } = render(() => (
      <Calendar
        mode="single"
        selected={selected()}
        onSelect={(next, trigger, modifiers, e) => {
          calls.push([next, trigger, modifiers, e])
          setSelected(next)
        }}
        defaultMonth={d(2026, 10, 1)}
        today={TODAY}
      />
    ))
    fireEvent.click(dayButton(c2, `2026-10-22`))
    expect(calls).toHaveLength(1)
    const [next, trigger, modifiers, e] = calls[0]!
    expect(next).toEqual(d(2026, 10, 22))
    expect(trigger).toEqual(d(2026, 10, 22))
    expect(modifiers).toMatchObject({ today: true, outside: false, disabled: false, hidden: false, focused: true })
    expect(e).toBeInstanceOf(MouseEvent)
    expect(c2.querySelector(`td[data-day="2026-10-22"]`)!.getAttribute(`data-selected`)).toBe(`true`)
    expect(c2.querySelector(`td[data-day="2026-10-03"]`)!.hasAttribute(`data-selected`)).toBe(false)
  })

  it(`multiple: adds and removes dates; max replaces`, () => {
    const seen: (Date[] | undefined)[] = []
    const { container } = render(() => <Calendar mode="multiple" defaultMonth={d(2026, 10, 1)} today={TODAY} max={2} onSelect={(v) => seen.push(v)} selected={[d(2026, 10, 1)]} />)
    fireEvent.click(dayButton(container, `2026-10-02`))
    expect(seen.at(-1)).toEqual([d(2026, 10, 1), d(2026, 10, 2)])
    fireEvent.click(dayButton(container, `2026-10-01`))
    expect(seen.at(-1)).toEqual([])
  })

  it(`range: from, then to, then a new start`, () => {
    let range: DateRange | undefined
    const { container } = render(() => <Calendar mode="range" defaultMonth={d(2026, 10, 1)} today={TODAY} />)
    const td = (iso: string) => container.querySelector(`td[data-day="${iso}"]`)!
    fireEvent.click(dayButton(container, `2026-10-05`))
    expect(td(`2026-10-05`).getAttribute(`data-selected`)).toBe(`true`)
    fireEvent.click(dayButton(container, `2026-10-09`))
    expect(td(`2026-10-05`).className).toContain(`rdp-range_start`)
    expect(td(`2026-10-07`).className).toContain(`rdp-range_middle`)
    expect(td(`2026-10-09`).className).toContain(`rdp-range_end`)
    expect(dayButton(container, `2026-10-07`).getAttribute(`data-range-middle`)).toBe(`true`)
    cleanup()

    const { container: c2 } = render(() => <Calendar mode="range" selected={{ from: d(2026, 10, 5), to: d(2026, 10, 9) }} onSelect={(r) => (range = r)} defaultMonth={d(2026, 10, 1)} today={TODAY} />)
    fireEvent.click(dayButton(c2, `2026-10-12`))
    expect(range).toEqual({ from: d(2026, 10, 5), to: d(2026, 10, 12) })
    fireEvent.click(dayButton(c2, `2026-10-02`))
    expect(range).toEqual({ from: d(2026, 10, 2), to: d(2026, 10, 9) })
  })

  it(`disabled days are not selectable`, () => {
    const onSelect = vi.fn()
    const { container } = render(() => <Calendar mode="single" disabled={{ dayOfWeek: [0, 6] }} onSelect={onSelect} defaultMonth={d(2026, 10, 1)} today={TODAY} />)
    const sat = dayButton(container, `2026-10-03`)
    expect(sat.disabled).toBe(true)
    fireEvent.click(sat)
    expect(onSelect).not.toHaveBeenCalled()
  })

  it(`month navigation: previous / next update the caption and onMonthChange`, () => {
    const months: Date[] = []
    const { container } = render(() => <Calendar mode="single" defaultMonth={d(2026, 10, 1)} today={TODAY} onMonthChange={(m) => months.push(m)} />)
    expect(caption(container)).toBe(`October 2026`)
    fireEvent.click(screen.getByLabelText(`Go to the Next Month`))
    expect(caption(container)).toBe(`November 2026`)
    fireEvent.click(screen.getByLabelText(`Go to the Previous Month`))
    fireEvent.click(screen.getByLabelText(`Go to the Previous Month`))
    expect(caption(container)).toBe(`September 2026`)
    expect(months).toEqual([d(2026, 11, 1), d(2026, 10, 1), d(2026, 9, 1)])
  })

  it(`navigation stops at endMonth (aria-disabled, no move)`, () => {
    const { container } = render(() => <Calendar mode="single" endMonth={d(2026, 10, 1)} defaultMonth={d(2026, 10, 1)} today={TODAY} />)
    const next = screen.getByLabelText(`Go to the Next Month`)
    expect(next.getAttribute(`aria-disabled`)).toBe(`true`)
    fireEvent.click(next)
    expect(caption(container)).toBe(`October 2026`)
  })

  it(`the dropdown caption changes the month and the year`, () => {
    const { container } = render(() => <Calendar mode="single" captionLayout="dropdown" fromYear={2024} toYear={2027} defaultMonth={d(2026, 10, 1)} today={TODAY} />)
    const [monthSelect, yearSelect] = Array.from(container.querySelectorAll(`select`))
    expect(monthSelect!.value).toBe(`9`)
    expect(yearSelect!.value).toBe(`2026`)
    fireEvent.change(monthSelect!, { target: { value: `2` } })
    expect(container.querySelector(`[role="status"]`)!.textContent).toBe(`March 2026`)
    const [, years] = Array.from(container.querySelectorAll(`select`))
    fireEvent.change(years!, { target: { value: `2024` } })
    expect(container.querySelector(`[role="status"]`)!.textContent).toBe(`March 2024`)
  })

  it(`roving focus: one tabbable day (selected > today), arrows / PageUp / Home / End move it`, () => {
    const { container } = render(() => <Calendar mode="single" selected={d(2026, 10, 15)} defaultMonth={d(2026, 10, 1)} today={TODAY} />)
    const tabbable = () => Array.from(container.querySelectorAll(`button[tabindex="0"]`)).map((b) => b.closest(`td`)!.getAttribute(`data-day`))
    expect(tabbable()).toEqual([`2026-10-15`])

    const start = dayButton(container, `2026-10-15`)
    start.focus()
    expect(container.querySelector(`td[data-day="2026-10-15"]`)!.getAttribute(`data-focused`)).toBe(`true`)
    const press = (key: string, shiftKey = false) => fireEvent.keyDown(document.activeElement!, { key, shiftKey })
    const focusedDay = () => (document.activeElement as HTMLElement).closest(`td`)!.getAttribute(`data-day`)

    press(`ArrowRight`)
    expect(focusedDay()).toBe(`2026-10-16`)
    press(`ArrowDown`)
    expect(focusedDay()).toBe(`2026-10-23`)
    press(`ArrowLeft`)
    expect(focusedDay()).toBe(`2026-10-22`)
    press(`ArrowUp`)
    expect(focusedDay()).toBe(`2026-10-15`)
    press(`Home`)
    expect(focusedDay()).toBe(`2026-10-11`)
    press(`End`)
    expect(focusedDay()).toBe(`2026-10-17`)
    press(`PageDown`)
    expect(caption(container)).toBe(`November 2026`)
    expect(focusedDay()).toBe(`2026-11-17`)
    press(`ArrowRight`, true)
    expect(caption(container)).toBe(`December 2026`)
    press(`PageUp`, true)
    expect(caption(container)).toBe(`December 2025`)
    expect(focusedDay()).toBe(`2025-12-17`)
  })

  it(`Enter / Space select the focused day (native button activation)`, () => {
    const onSelect = vi.fn()
    const { container } = render(() => <Calendar mode="single" onSelect={onSelect} defaultMonth={d(2026, 10, 1)} today={TODAY} />)
    const today = dayButton(container, `2026-10-22`)
    expect(today.getAttribute(`tabindex`)).toBe(`0`)
    expect(today.type).toBe(`button`)
    today.focus()
    // A <button type="button"> turns Enter / Space into a click (the browser does; jsdom does not, so the click stands in).
    fireEvent.click(today)
    expect(onSelect.mock.calls[0]![0]).toEqual(d(2026, 10, 22))
  })

  it(`RTL: DayPicker's dir prop flips the horizontal arrows`, () => {
    const { container } = render(() => <Calendar mode="single" dir="rtl" selected={d(2026, 10, 15)} defaultMonth={d(2026, 10, 1)} today={TODAY} />)
    dayButton(container, `2026-10-15`).focus()
    fireEvent.keyDown(document.activeElement!, { key: `ArrowLeft` })
    expect((document.activeElement as HTMLElement).closest(`td`)!.getAttribute(`data-day`)).toBe(`2026-10-16`)
  })
})
