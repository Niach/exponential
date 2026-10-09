// Round 1 (contract §3–4, a11y.json DatePicker): DatePicker, DateRangePicker
// and TimePicker. ONE calendar grid (role=grid, a roving tab stop on the
// focused day): arrows ±1 day / ±1 week, PageUp/PageDown ±1 month (Shift =
// ±1 year), Home/End = the start/end of the week by the LOCALE week start
// (`weekStart(locale)` from catalog/locale.json, never the browser's), Enter
// / Space pick. Month and weekday names are the surface locale's (Intl).
// The range picker selects in two steps (start, then end; an earlier end
// swaps). TimePicker = a listbox of `step`-minute times between min and max
// (shown in the locale's clock), typed `HH:mm` accepted in its filter.

import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react"
import { Popover as PopoverPrimitive } from "radix-ui"
import { OVERLAY_OFFSET, OVERLAY_PADDING, weekStart } from "@exponential-at/ui"
import type { Formatter } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import { FieldErrors, useField } from "../form"
import type { NativeProps } from "../node-view"
import { useBoundState } from "./bound"
import { useListNavigation } from "./listbox"
import { bool, BuiltinIcon, num, str, useParts, TextPart, type PartFn } from "./shared"

export const isoDate = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, `0`)}-${String(d.getDate()).padStart(2, `0`)}`
export const parseIsoDate = (s: string): Date | null => {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(s)
  return m ? new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3])) : null
}
const addDays = (d: Date, n: number) => new Date(d.getFullYear(), d.getMonth(), d.getDate() + n)
const addMonths = (d: Date, n: number) => {
  const last = new Date(d.getFullYear(), d.getMonth() + n + 1, 0).getDate()
  return new Date(d.getFullYear(), d.getMonth() + n, Math.min(d.getDate(), last))
}
const sameDay = (a: Date | null, b: Date | null) => Boolean(a && b && isoDate(a) === isoDate(b))

/** The 6×7 days of `view`'s month, starting on `first` (0 = Sunday). */
export function monthGrid(view: Date, first: number): Date[] {
  const start = new Date(view.getFullYear(), view.getMonth(), 1)
  const lead = (start.getDay() - first + 7) % 7
  return Array.from({ length: 42 }, (_, i) => new Date(start.getFullYear(), start.getMonth(), 1 - lead + i))
}

interface CalendarProps {
  part: PartFn
  dayPart: `day`
  locale: string
  /** The surface Formatter: month and weekday names, day labels (round 2 §3). */
  formatter: Formatter
  firstDay: number
  min: Date | null
  max: Date | null
  /** Days drawn selected. */
  isSelected: (d: Date) => boolean
  /** Days drawn inside a range (DateRangePicker). */
  inRange?: (d: Date) => boolean
  initial: Date
  onPick: (d: Date) => void
  t: (id: string) => string
  /** The SURFACE direction (never the document's): ArrowLeft/Right mirror in
   *  RTL, like the grid. */
  direction?: `ltr` | `rtl`
}

/** The roving tab stop of a month view: the focused day when it is in the
 *  visible month and enabled, else the enabled in-month day nearest to it,
 *  else none (a disabled day never holds the stop). */
export function calendarStop(days: readonly Date[], view: Date, focus: Date, disabled: (d: Date) => boolean): Date | null {
  const inMonth = days.filter((d) => d.getMonth() === view.getMonth() && d.getFullYear() === view.getFullYear() && !disabled(d))
  if (inMonth.some((d) => sameDay(d, focus))) return focus
  let best: Date | null = null
  for (const d of inMonth) if (!best || Math.abs(d.getTime() - focus.getTime()) < Math.abs(best.getTime() - focus.getTime())) best = d
  return best
}

export function CalendarGrid({ part, dayPart, formatter, firstDay, min, max, isSelected, inRange, initial, onPick, t, direction = `ltr` }: CalendarProps) {
  const [focus, setFocus] = useState<Date>(initial)
  const [view, setView] = useState<Date>(new Date(initial.getFullYear(), initial.getMonth(), 1))
  const grid = useRef<HTMLDivElement>(null)
  const moved = useRef(false)
  const days = monthGrid(view, firstDay)
  const weekdays = useMemo(() => {
    // 2023-01-01 was a Sunday; names through the surface Formatter (`EEE`;
    // the one-letter column head is the locale's narrow form, Intl).
    const narrow = new Intl.DateTimeFormat(formatter.locale, { weekday: `narrow` })
    return Array.from({ length: 7 }, (_, i) => {
      const d = new Date(2023, 0, 1 + ((firstDay + i) % 7))
      return { short: formatter.date(isoDate(d), { format: `EEE` }), narrow: narrow.format(d) }
    })
  }, [formatter, firstDay])
  const monthLabel = formatter.date(isoDate(view), { format: `MMMM yyyy` })
  const dayLabel = useMemo(() => new Intl.DateTimeFormat(formatter.locale, { weekday: `long`, year: `numeric`, month: `long`, day: `numeric` }), [formatter.locale])
  const disabled = (d: Date) => (min !== null && d < min) || (max !== null && d > max)
  // The month buttons move the focused day WITH the view (same day number,
  // clamped to the month), so the grid keeps a tab stop.
  const shiftMonth = (n: number) => {
    const next = addMonths(focus, n)
    setFocus(next)
    setView(new Date(next.getFullYear(), next.getMonth(), 1))
  }
  const stop = calendarStop(days, view, focus, disabled)
  const goto = (d: Date) => {
    moved.current = true
    setFocus(d)
    if (d.getMonth() !== view.getMonth() || d.getFullYear() !== view.getFullYear()) setView(new Date(d.getFullYear(), d.getMonth(), 1))
  }
  useEffect(() => {
    if (!moved.current) return
    moved.current = false
    grid.current?.querySelector<HTMLButtonElement>(`[data-date="${isoDate(focus)}"]`)?.focus()
  }, [focus, view])
  const onKey = (e: KeyboardEvent) => {
    const k = e.key
    let next: Date | null = null
    if (k === `ArrowLeft`) next = addDays(focus, direction === `rtl` ? 1 : -1)
    else if (k === `ArrowRight`) next = addDays(focus, direction === `rtl` ? -1 : 1)
    else if (k === `ArrowUp`) next = addDays(focus, -7)
    else if (k === `ArrowDown`) next = addDays(focus, 7)
    else if (k === `PageUp`) next = addMonths(focus, e.shiftKey ? -12 : -1)
    else if (k === `PageDown`) next = addMonths(focus, e.shiftKey ? 12 : 1)
    else if (k === `Home`) next = addDays(focus, -((focus.getDay() - firstDay + 7) % 7))
    else if (k === `End`) next = addDays(focus, 6 - ((focus.getDay() - firstDay + 7) % 7))
    else if (k === `Enter` || k === ` `) {
      e.preventDefault()
      if (!disabled(focus)) onPick(focus)
      return
    }
    if (next) {
      e.preventDefault()
      goto(next)
    }
  }
  const rows: Date[][] = []
  for (let i = 0; i < 6; i++) rows.push(days.slice(i * 7, i * 7 + 7))
  return (
    <>
      <div className="xui-calendar-head">
        <button type="button" className="xui-calendar-nav" aria-label={t(`previousMonth`)} onClick={() => shiftMonth(-1)}>
          <BuiltinIcon slot="DatePicker.previousMonth" />
        </button>
        <span aria-live="polite">{monthLabel}</span>
        <button type="button" className="xui-calendar-nav" aria-label={t(`nextMonth`)} onClick={() => shiftMonth(1)}>
          <BuiltinIcon slot="DatePicker.nextMonth" />
        </button>
      </div>
      <div ref={grid} className="xui-calendar-grid" role="grid" aria-label={monthLabel} onKeyDown={onKey}>
        <div role="row" className="xui-calendar-row">
          {weekdays.map((w, i) => (
            <span key={i} role="columnheader" className="xui-calendar-weekday" aria-label={w.short} title={w.short}>
              {w.narrow}
            </span>
          ))}
        </div>
        {rows.map((week, r) => (
          <div role="row" key={r} className="xui-calendar-row">
            {week.map((d) => {
              const key = isoDate(d)
              const sel = isSelected(d)
              const range = inRange?.(d) ?? false
              const outside = d.getMonth() !== view.getMonth()
              const off = disabled(d)
              const focused = sameDay(d, stop)
              return (
                <span role="gridcell" key={key} aria-selected={sel} className="xui-calendar-cell">
                  <button
                    type="button"
                    {...(part(dayPart, sel && `selected`, off && `disabled`) as Record<string, string>)}
                    data-date={key}
                    data-outside={outside ? `` : undefined}
                    data-range={range ? `` : undefined}
                    data-today={sameDay(d, new Date()) ? `` : undefined}
                    disabled={off}
                    tabIndex={focused ? 0 : -1}
                    onFocus={() => {
                      if (!sameDay(d, focus)) setFocus(d)
                    }}
                    aria-label={dayLabel.format(d)}
                    aria-current={sameDay(d, new Date()) ? `date` : undefined}
                    onClick={() => {
                      setFocus(d)
                      onPick(d)
                    }}
                  >
                    {range && !sel ? <span {...(part(`range`) as Record<string, string>)} aria-hidden="true" /> : null}
                    <span className="xui-calendar-day-label">{d.getDate()}</span>
                  </button>
                </span>
              )
            })}
          </div>
        ))}
      </div>
    </>
  )
}

function useFirstDay(props: Record<string, unknown>, locale: string): number {
  return props.firstDayOfWeek === undefined ? weekStart(locale) : ((Math.round(num(props.firstDayOfWeek)) % 7) + 7) % 7
}

export function DatePickerNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const [value, setValue] = useBoundState(node, scope, `value`, str(props.value))
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const f = useField({ node, domId, props, value, focusRef: triggerRef })
  const [open, setOpen] = useState(false)
  const selected = value ? parseIsoDate(value) : null
  const min = props.min ? parseIsoDate(str(props.min)) : null
  const max = props.max ? parseIsoDate(str(props.max)) : null
  const firstDay = useFirstDay(props, ctx.locale)
  const label = selected ? ctx.formatter.date(isoDate(selected)) : ``
  const disabled = bool(props.disabled) || Boolean(f.form?.disabled)
  const pick = (d: Date) => {
    const next = isoDate(d)
    setValue(next)
    f.change(next)
    f.touch()
    void emit(`change`, { value: next })
    setOpen(false)
  }
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
      <PopoverPrimitive.Root open={open} onOpenChange={setOpen}>
        <PopoverPrimitive.Trigger ref={triggerRef} id={id} {...(part(`trigger`, open && `open`, f.invalid && `invalid`, disabled && `disabled`) as Record<string, string>)} disabled={disabled} data-value={value || undefined} aria-haspopup="dialog" aria-invalid={f.invalid || undefined} aria-describedby={f.describedBy}>
          <span>{label || <span {...(part(`placeholder`) as Record<string, string>)}>{str(props.placeholder)}</span>}</span>
          <BuiltinIcon slot="DatePicker.trigger" />
        </PopoverPrimitive.Trigger>
        <PopoverPrimitive.Portal container={ctx.portal ?? undefined}>
          <PopoverPrimitive.Content {...(part(`calendar`, open && `open`) as Record<string, string>)} dir={ctx.direction} role="dialog" aria-label={str(props.label) || undefined} align="start" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="DatePicker" onOpenAutoFocus={(e) => {
            e.preventDefault()
            const el = (e.currentTarget as HTMLElement | null)?.querySelector<HTMLButtonElement>(`.xui-calendar-grid button[tabindex="0"]`)
            el?.focus()
          }}>
            <CalendarGrid part={part} dayPart="day" locale={ctx.locale} formatter={ctx.formatter} firstDay={firstDay} min={min} max={max} isSelected={(d) => sameDay(d, selected)} initial={selected ?? new Date()} onPick={pick} t={ctx.t} direction={ctx.direction} />
          </PopoverPrimitive.Content>
        </PopoverPrimitive.Portal>
      </PopoverPrimitive.Root>
      <FieldErrors part={part} field={f} />
    </div>
  )
}

export function DateRangePickerNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const [start, setStart] = useBoundState(node, scope, `start`, str(props.start))
  const [end, setEnd] = useBoundState(node, scope, `end`, str(props.end))
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const f = useField({ node, domId, props, value: { start, end }, focusRef: triggerRef })
  const [open, setOpen] = useState(false)
  const [pending, setPending] = useState<Date | null>(null)
  const [hover, setHover] = useState<Date | null>(null)
  const s = start ? parseIsoDate(start) : null
  const e = end ? parseIsoDate(end) : null
  const min = props.min ? parseIsoDate(str(props.min)) : null
  const max = props.max ? parseIsoDate(str(props.max)) : null
  const firstDay = useFirstDay(props, ctx.locale)
  const fmt = { format: (d: Date) => ctx.formatter.date(isoDate(d)) }
  const label = s ? `${fmt.format(s)} – ${e ? fmt.format(e) : ``}` : ``
  useEffect(() => {
    if (!open) {
      setPending(null)
      setHover(null)
    }
  }, [open])
  const pick = (d: Date) => {
    if (!pending) {
      setPending(d)
      return
    }
    const [a, b] = d < pending ? [d, pending] : [pending, d]
    const next = { start: isoDate(a), end: isoDate(b) }
    setStart(next.start)
    setEnd(next.end)
    setPending(null)
    f.change(next)
    f.touch()
    void emit(`change`, next)
    setOpen(false)
  }
  const lo = pending ?? s
  const hi = pending ? hover : e
  const [rangeA, rangeB] = lo && hi ? (hi < lo ? [hi, lo] : [lo, hi]) : [lo, hi]
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
      <PopoverPrimitive.Root open={open} onOpenChange={setOpen}>
        <PopoverPrimitive.Trigger ref={triggerRef} id={id} {...(part(`trigger`, open && `open`, f.invalid && `invalid`) as Record<string, string>)} data-start={start || undefined} data-end={end || undefined} aria-haspopup="dialog" aria-describedby={f.describedBy}>
          <span>{label || <span {...(part(`placeholder`) as Record<string, string>)}>{str(props.placeholder)}</span>}</span>
          <BuiltinIcon slot="DateRangePicker.trigger" />
        </PopoverPrimitive.Trigger>
        <PopoverPrimitive.Portal container={ctx.portal ?? undefined}>
          <PopoverPrimitive.Content {...(part(`calendar`, open && `open`) as Record<string, string>)} dir={ctx.direction} role="dialog" aria-label={str(props.label) || undefined} align="start" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="DateRangePicker" data-pending={pending ? isoDate(pending) : undefined} onOpenAutoFocus={(ev) => {
            ev.preventDefault()
            ;(ev.currentTarget as HTMLElement | null)?.querySelector<HTMLButtonElement>(`.xui-calendar-grid button[tabindex="0"]`)?.focus()
          }} onPointerOver={(ev) => {
            const date = (ev.target as HTMLElement).closest?.(`[data-date]`)?.getAttribute(`data-date`)
            if (pending && date) setHover(parseIsoDate(date))
          }} onFocus={(ev) => {
            const date = (ev.target as HTMLElement).getAttribute?.(`data-date`)
            if (pending && date) setHover(parseIsoDate(date))
          }}>
            <CalendarGrid
              part={part}
              dayPart="day"
              locale={ctx.locale}
              formatter={ctx.formatter}
              firstDay={firstDay}
              min={min}
              max={max}
              isSelected={(d) => sameDay(d, pending) || (!pending && (sameDay(d, s) || sameDay(d, e)))}
              inRange={(d) => Boolean(rangeA && rangeB && d > rangeA && d < rangeB)}
              initial={s ?? new Date()}
              onPick={pick}
              t={ctx.t}
              direction={ctx.direction}
            />
          </PopoverPrimitive.Content>
        </PopoverPrimitive.Portal>
      </PopoverPrimitive.Root>
      <FieldErrors part={part} field={f} />
    </div>
  )
}

const parseTime = (s: string): number | null => {
  const m = /^(\d{1,2}):(\d{2})$/.exec(s.trim())
  if (!m) return null
  const h = Number(m[1])
  const min = Number(m[2])
  return h < 24 && min < 60 ? h * 60 + min : null
}
const formatHHmm = (minutes: number) => `${String(Math.floor(minutes / 60)).padStart(2, `0`)}:${String(minutes % 60).padStart(2, `0`)}`

export function TimePickerNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const part = useParts(node, props)
  const ctx = useSurfaceContext()
  const id = useId()
  const listId = `${id}-list`
  const [value, setValue] = useBoundState(node, scope, `value`, str(props.value))
  const triggerRef = useRef<HTMLButtonElement | null>(null)
  const f = useField({ node, domId, props, value, focusRef: triggerRef })
  const disabled = bool(props.disabled) || Boolean(f.form?.disabled)
  const [open, setOpen] = useState(false)
  const step = Math.max(1, Math.round(num(props.step, 15)))
  const lo = parseTime(str(props.min)) ?? 0
  const hi = parseTime(str(props.max)) ?? 24 * 60 - 1
  const times = useMemo(() => {
    const out: number[] = []
    for (let m = lo; m <= hi; m += step) out.push(m)
    return out
  }, [lo, hi, step])
  const clock = useMemo(() => new Intl.DateTimeFormat(ctx.locale, { hour: `numeric`, minute: `2-digit` }), [ctx.locale])
  const show = (m: number) => clock.format(new Date(2000, 0, 1, Math.floor(m / 60), m % 60))
  const current = parseTime(value)
  const pick = (i: number) => {
    const next = formatHHmm(times[i])
    setValue(next)
    f.change(next)
    f.touch()
    void emit(`change`, { value: next })
    setOpen(false)
    triggerRef.current?.focus()
  }
  const nav = useListNavigation({ count: times.length, labelOf: (i) => show(times[i]), onPick: pick, typeahead: true })
  const listRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return
    const i = current === null ? 0 : times.findIndex((t) => t >= current)
    nav.setActive(i < 0 ? 0 : i)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])
  const activeId = nav.active >= 0 ? `${listId}-${nav.active}` : undefined
  useEffect(() => {
    if (activeId) listRef.current?.querySelector(`[id="${activeId}"]`)?.scrollIntoView?.({ block: `nearest` })
  }, [activeId])
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <TextPart part={part(`label`)} text={props.label} as="label" htmlFor={id} />
      <PopoverPrimitive.Root open={open} onOpenChange={setOpen}>
        <PopoverPrimitive.Trigger ref={triggerRef} id={id} {...(part(`trigger`, open && `open`, disabled && `disabled`, f.invalid && `invalid`) as Record<string, string>)} disabled={disabled} role="combobox" aria-haspopup="listbox" aria-expanded={open} aria-controls={open ? listId : undefined} data-value={value || undefined} aria-describedby={f.describedBy}>
          <span>{current !== null ? show(current) : <span {...(part(`placeholder`) as Record<string, string>)}>{str(props.placeholder)}</span>}</span>
          <BuiltinIcon slot="TimePicker.trigger" />
        </PopoverPrimitive.Trigger>
        <PopoverPrimitive.Portal container={ctx.portal ?? undefined}>
          <PopoverPrimitive.Content {...(part(`list`, open && `open`) as Record<string, string>)} dir={ctx.direction} align="start" sideOffset={OVERLAY_OFFSET} collisionPadding={OVERLAY_PADDING} data-xui-overlay="TimePicker" onOpenAutoFocus={(e) => {
            e.preventDefault()
            listRef.current?.focus()
          }}>
            <div ref={listRef} id={listId} role="listbox" tabIndex={0} aria-activedescendant={activeId} aria-labelledby={id} className="xui-TimePicker-listbox" onKeyDown={nav.onKeyDown}>
              {times.map((m, i) => {
                const sel = m === current
                return (
                  <div key={m} id={`${listId}-${i}`} role="option" aria-selected={sel} data-value={formatHHmm(m)} {...(part(`item`, (sel || i === nav.active) && `selected`) as Record<string, string>)} onPointerMove={() => nav.setActive(i)} onMouseDown={(e) => e.preventDefault()} onClick={() => pick(i)}>
                    {show(m)}
                  </div>
                )
              })}
            </div>
          </PopoverPrimitive.Content>
        </PopoverPrimitive.Portal>
      </PopoverPrimitive.Root>
      <FieldErrors part={part} field={f} />
    </div>
  )
}
