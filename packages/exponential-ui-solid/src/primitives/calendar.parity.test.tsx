// FEED-131: the Solid Calendar paints React's (shadcn's Calendar on
// react-day-picker 9) whole-body DOM. React runs through `createElement`
// (its JSX stays out of this package's Solid transform), loaded at run time.
// Dates are local midnights; `today` is pinned wherever the marker shows.

import { createElement, type FunctionComponent } from "react"
import { Calendar } from "./calendar"
import type { CalendarProps } from "./calendar"
import { expectElementParity } from "../test/parity"

const REACT_CALENDAR = `@exponential-at/ui-react/primitives/calendar`
type ReactCalendar = { Calendar: FunctionComponent<Record<string, unknown>>; CalendarDayButton: FunctionComponent<Record<string, unknown>> }
let reactCalendar: ReactCalendar | undefined
const loadReact = async () => (reactCalendar ??= (await import(/* @vite-ignore */ REACT_CALENDAR)) as ReactCalendar)

const d = (y: number, m: number, day: number) => new Date(y, m - 1, day)
const TODAY = d(2026, 10, 22)

/** One case: the same props on both sides (a factory: fresh Dates each). */
async function parity(props: () => Record<string, unknown>, wrap?: Record<string, string>, solidProps: () => Record<string, unknown> = props) {
  const { Calendar: ReactCalendarComponent } = await loadReact()
  await expectElementParity({
    react: () => (wrap ? createElement(`div`, wrap, createElement(ReactCalendarComponent, props())) : createElement(ReactCalendarComponent, props())),
    solid: () => (wrap ? <div {...wrap}><Calendar {...(solidProps() as CalendarProps)} /></div> : <Calendar {...(solidProps() as CalendarProps)} />),
  })
}

describe(`Calendar parity (React = the spec)`, () => {
  it(`single, a selected date, today pinned by the prop`, async () => {
    await parity(() => ({ mode: `single`, selected: d(2026, 10, 15), defaultMonth: d(2026, 10, 1), today: TODAY }))
  })

  it(`single, controlled (onSelect), no today in view: the far month has no marker`, async () => {
    await parity(() => ({ mode: `single`, selected: d(2031, 3, 9), onSelect: () => {}, defaultMonth: d(2031, 3, 1) }))
  })

  it(`multiple`, async () => {
    await parity(() => ({ mode: `multiple`, selected: [d(2026, 10, 2), d(2026, 10, 14), d(2026, 10, 30)], defaultMonth: d(2026, 10, 1), today: TODAY }))
  })

  it(`range: start, middle, end`, async () => {
    await parity(() => ({ mode: `range`, selected: { from: d(2026, 10, 5), to: d(2026, 10, 12) }, defaultMonth: d(2026, 10, 1), today: TODAY }))
  })

  it(`range: an open range (from only)`, async () => {
    await parity(() => ({ mode: `range`, selected: { from: d(2026, 10, 5), to: undefined }, defaultMonth: d(2026, 10, 1), today: TODAY }))
  })

  it(`range: nothing selected yet`, async () => {
    await parity(() => ({ mode: `range`, defaultMonth: d(2026, 10, 1), today: TODAY }))
  })

  it(`outside days off`, async () => {
    await parity(() => ({ mode: `single`, showOutsideDays: false, defaultMonth: d(2026, 10, 1), today: TODAY }))
  })

  it(`no mode: plain cells with grid-cell labels`, async () => {
    await parity(() => ({ defaultMonth: d(2026, 10, 1), today: TODAY }))
  })

  it(`disabled matchers: day-of-week, before, a date list and a function`, async () => {
    await parity(() => ({
      mode: `single`,
      selected: d(2026, 10, 13),
      defaultMonth: d(2026, 10, 1),
      today: TODAY,
      disabled: [{ dayOfWeek: [0, 6] }, { before: d(2026, 10, 6) }, [d(2026, 10, 20), d(2026, 10, 21)], (date: Date) => date.getDate() === 28],
    }))
  })

  it(`disabled interval and range matchers`, async () => {
    await parity(() => ({ mode: `single`, defaultMonth: d(2026, 10, 1), today: TODAY, disabled: [{ after: d(2026, 10, 25), before: d(2026, 10, 29) }, { from: d(2026, 10, 8), to: d(2026, 10, 10) }] }))
  })

  it(`two months`, async () => {
    await parity(() => ({ mode: `range`, numberOfMonths: 2, selected: { from: d(2026, 10, 28), to: d(2026, 11, 3) }, defaultMonth: d(2026, 10, 1), today: TODAY }))
  })

  it(`week numbers (and weekStartsOn Monday)`, async () => {
    await parity(() => ({ mode: `single`, showWeekNumber: true, defaultMonth: d(2026, 10, 1), today: TODAY }))
    await parity(() => ({ mode: `single`, showWeekNumber: true, weekStartsOn: 1, defaultMonth: d(2027, 1, 1), today: TODAY }))
    await parity(() => ({ mode: `single`, showWeekNumber: true, ISOWeek: true, defaultMonth: d(2026, 12, 1), today: TODAY }))
  })

  it(`captionLayout dropdown, dropdown-months, dropdown-years`, async () => {
    await parity(() => ({ mode: `single`, captionLayout: `dropdown`, selected: d(2026, 10, 15), defaultMonth: d(2026, 10, 1), today: TODAY }))
    await parity(() => ({ mode: `single`, captionLayout: `dropdown-months`, defaultMonth: d(2026, 10, 1), today: TODAY }))
    await parity(() => ({ mode: `single`, captionLayout: `dropdown-years`, defaultMonth: d(2026, 10, 1), today: TODAY, fromYear: 2020, toYear: 2030 }))
  })

  it(`min/max years: the nav ends disable the buttons and clamp the dropdowns`, async () => {
    await parity(() => ({ mode: `single`, captionLayout: `dropdown`, fromYear: 2024, toYear: 2026, defaultMonth: d(2026, 12, 1), today: TODAY }))
    await parity(() => ({ mode: `single`, startMonth: d(2026, 10, 1), endMonth: d(2026, 12, 1), defaultMonth: d(2026, 9, 1), today: TODAY }))
  })

  it(`RTL: dir on the wrapper, and DayPicker's dir prop`, async () => {
    await parity(() => ({ mode: `single`, selected: d(2026, 10, 15), defaultMonth: d(2026, 10, 1), today: TODAY }), { dir: `rtl` })
    await parity(() => ({ mode: `single`, dir: `rtl`, defaultMonth: d(2026, 10, 1), today: TODAY }))
  })

  it(`buttonVariant, class, classNames, footer, data-* passthrough`, async () => {
    await parity(() => ({ mode: `single`, buttonVariant: `outline`, defaultMonth: d(2026, 10, 1), today: TODAY, required: true, "data-testid": `cal` }))
    await parity(() => ({ mode: `single`, defaultMonth: d(2026, 10, 1), today: TODAY, footer: `Pick a day.` }))
    const base = { mode: `single`, defaultMonth: d(2026, 10, 1), today: TODAY, classNames: { day: `my-day`, caption_label: `my-caption` }, modifiers: { booked: [d(2026, 10, 7), d(2026, 10, 8)] }, modifiersClassNames: { booked: `is-booked` } }
    await parity(() => ({ ...base, className: `w-80 p-1` }), undefined, () => ({ ...base, class: `w-80 p-1` }))
  })
})
