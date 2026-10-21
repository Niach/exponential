// FEED-128 (React's natives/dates.tsx; round 1 contract §3–4, a11y.json
// DatePicker): DatePicker, DateRangePicker and TimePicker. ONE calendar
// grid (role=grid, a roving tab stop on the focused day): arrows ±1 day /
// ±1 week, PageUp/PageDown ±1 month (Shift = ±1 year), Home/End = the
// start/end of the week by the LOCALE week start (`weekStart(locale)`), Enter
// / Space pick. Month and weekday names are the surface locale's (Intl). The
// range picker selects in two steps (start, then end; an earlier end swaps).
// TimePicker = a listbox of `step`-minute times between min and max (shown
// in the locale's clock).
//
// The popover is Radix Popover's DOM painted by hand (`PickerPopover`):
// the trigger's `aria-expanded`/`aria-controls`/`data-state`, the portalled
// `data-radix-popper-content-wrapper` positioned by @floating-ui/dom with
// Radix's middleware and inline style, the content's `data-side`/
// `data-align`/`tabindex=-1`/`--radix-popover-*` variables, non-modal
// dismissal (outside pointer/focus, Escape) and the focus return. Kobalte's
// Popover paints its own positioner and attributes.

import { For, Index, Show, batch, createComputed, createEffect, createMemo, createSignal, createUniqueId, on, onCleanup, onMount, untrack, type Accessor, type Component, type JSX } from "solid-js"
import { Portal } from "solid-js/web"
import { autoUpdate, computePosition, flip, limitShift, offset, shift, size, type Middleware, type Placement } from "@floating-ui/dom"
import { OVERLAY_OFFSET, OVERLAY_PADDING, weekStart } from "@exponential-at/ui"
import type { Formatter } from "@exponential-at/ui"
import { addDays, addMonths, calendarStop, isoDate, monthGrid, nextEnabledIndex, parseIsoDate, sameDay, typeaheadStep } from "@exponential-at/ui/web"
import { useSurfaceContext } from "../context"
import { FieldErrors, createField } from "../form"
import type { NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { bool, BuiltinIcon, num, parts, str, TextPart, type Attrs, type PartFn } from "./shared"

export { calendarStop, isoDate, monthGrid, parseIsoDate }

// ── the listbox keyboard model (React's natives/listbox.ts) ──────────────

export interface ListNavOptions {
  count: Accessor<number>
  isDisabled?: (index: number) => boolean
  labelOf?: (index: number) => string
  onPick: (index: number) => void
  /** Type-ahead (off when a search field owns the keys). */
  typeahead?: boolean
  initial?: number
}

/** React's `useListNavigation`: an active option (aria-activedescendant,
 *  focus stays where it is), ArrowUp/ArrowDown move it (wrapping, skipping
 *  disabled), Home/End jump, Enter picks, printable keys type-ahead. */
export function createListNavigation(o: ListNavOptions): { active: Accessor<number>; setActive: (i: number) => void; onKeyDown: (e: KeyboardEvent) => void } {
  const isDisabled = o.isDisabled ?? (() => false)
  const labelOf = o.labelOf ?? (() => ``)
  const [active, setActive] = createSignal(o.initial ?? -1)
  let buffer = { text: ``, at: 0 }
  createComputed(() => {
    const c = o.count()
    if (untrack(active) >= c) setActive(c - 1)
  })
  const move = (from: number, delta: number) => nextEnabledIndex(o.count(), from, delta, isDisabled)
  const onKeyDown = (e: KeyboardEvent) => {
    const count = o.count()
    const a = active()
    if (e.key === `ArrowDown`) {
      e.preventDefault()
      setActive(move(a < 0 ? -1 : a, 1))
    } else if (e.key === `ArrowUp`) {
      e.preventDefault()
      setActive(move(a < 0 ? 0 : a, -1))
    } else if (e.key === `Home`) {
      e.preventDefault()
      setActive(move(-1, 1))
    } else if (e.key === `End`) {
      e.preventDefault()
      setActive(move(0, -1))
    } else if (e.key === `Enter` || (e.key === ` ` && o.typeahead && buffer.text === ``)) {
      if (a >= 0 && a < count && !isDisabled(a)) {
        e.preventDefault()
        o.onPick(a)
      }
    } else if (o.typeahead && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const step = typeaheadStep(buffer, e.key, Date.now(), a, count, isDisabled, labelOf)
      buffer = step.buffer
      if (step.index >= 0) setActive(step.index)
    }
  }
  return { active, setActive: (i) => setActive(i), onKeyDown }
}

// ── the popover (Radix Popover's DOM) ────────────────────────────────────

const TABBABLE = `button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),a[href],[tabindex]`

function tabbables(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(TABBABLE)).filter((el) => el.tabIndex >= 0 && !el.closest(`[hidden],[inert]`))
}

/** Radix Popper's `transformOrigin` middleware without an arrow. */
const transformOrigin: Middleware = {
  name: `transformOrigin`,
  fn({ placement, rects }) {
    const [side, align = `center`] = placement.split(`-`)
    const noArrowAlign = ({ start: `0%`, center: `50%`, end: `100%` } as Record<string, string>)[align]
    let x = ``
    let y = ``
    if (side === `bottom`) {
      x = noArrowAlign
      y = `0px`
    } else if (side === `top`) {
      x = noArrowAlign
      y = `${rects.floating.height}px`
    } else if (side === `right`) {
      x = `0px`
      y = noArrowAlign
    } else if (side === `left`) {
      x = `${rects.floating.width}px`
      y = noArrowAlign
    }
    return { data: { x, y } }
  },
}

interface PopoverProps {
  open: boolean
  setOpen: (open: boolean) => void
  trigger: () => HTMLElement | undefined
  contentId: string
  /** The content's own attributes (the part, role, aria-label, overlay…). */
  attrs: Attrs
  /** Radix's `onOpenAutoFocus` (the default prevented): where focus goes. */
  onOpenAutoFocus: (content: HTMLElement) => void
  onPointerOver?: (e: PointerEvent) => void
  onFocusIn?: (e: FocusEvent) => void
  children: JSX.Element
}

/** PopoverPrimitive.Portal + Content (align start, `OVERLAY_OFFSET`,
 *  `OVERLAY_PADDING`), non-modal, inside Radix's `Presence`: a close keeps
 *  the content (`data-state="closed"`) mounted while its exit animation
 *  runs, then unmounts it. */
export function PickerPopover(props: PopoverProps): JSX.Element {
  const [present, setPresent] = createSignal(untrack(() => props.open))
  let node: HTMLElement | undefined
  let prevAnimation = `none`
  const animationName = () => (node ? getComputedStyle(node).animationName || `none` : `none`)
  createEffect(
    on(
      () => props.open,
      (open, was) => {
        if (open) {
          setPresent(true)
          return
        }
        if (!node || !was) {
          setPresent(false)
          return
        }
        const current = animationName()
        if (current === `none` || getComputedStyle(node).display === `none` || prevAnimation === current) setPresent(false)
      },
      { defer: true }
    )
  )
  const presence: PresenceHooks = {
    mount: (el) => {
      node = el
      prevAnimation = animationName()
    },
    unmount: () => {
      node = undefined
      prevAnimation = `none`
    },
    start: (e) => {
      if (e.target === node) prevAnimation = animationName()
    },
    end: (e) => {
      if (!node || e.target !== node || !animationName().includes(CSS.escape(e.animationName)) || untrack(() => props.open)) return
      const fill = node.style.animationFillMode
      const el = node
      el.style.animationFillMode = `forwards`
      setTimeout(() => {
        if (el.style.animationFillMode === `forwards`) el.style.animationFillMode = fill
      })
      setPresent(false)
    },
  }
  return (
    <Show when={present()}>
      <PopoverContent {...props} presence={presence} />
    </Show>
  )
}

interface PresenceHooks {
  mount: (el: HTMLElement) => void
  unmount: () => void
  start: (e: AnimationEvent) => void
  end: (e: AnimationEvent) => void
}

function PopoverContent(props: PopoverProps & { presence: PresenceHooks }): JSX.Element {
  const ctx = useSurfaceContext()
  const [placement, setPlacement] = createSignal<Placement>(`bottom-start`)
  const [positioned, setPositioned] = createSignal(false)
  const [wrapper, setWrapper] = createSignal<HTMLDivElement>()
  let content: HTMLDivElement | undefined
  let interactedOutside = false
  let pointerDownOutside = false
  const side = () => placement().split(`-`)[0]
  const align = () => placement().split(`-`)[1] ?? `center`
  const mountWrapper = (el: HTMLDivElement) => {
    // PopperContent's wrapper: `floatingStyles` before the first placement
    // (`translate(0, -200%)` keeps it off the page while measuring).
    el.setAttribute(`data-radix-popper-content-wrapper`, ``)
    el.style.position = `fixed`
    el.style.left = `0px`
    el.style.top = `0px`
    el.style.transform = `translate(0, -200%)`
    el.style.minWidth = `max-content`
    setWrapper(el)
  }
  createEffect(() => wrapper()?.setAttribute(`dir`, ctx.direction))
  // Radix Popover's non-modal `onInteractOutside`.
  const interactOutside = (target: EventTarget | null, kind: `pointerdown` | `focusin`): boolean => {
    interactedOutside = true
    if (kind === `pointerdown`) pointerDownOutside = true
    const t = props.trigger()
    if (t && target instanceof Node && t.contains(target)) return false
    if (kind === `focusin` && pointerDownOutside) return false
    return true
  }
  const inside = (target: EventTarget | null) => target instanceof Node && Boolean(content?.contains(target))
  onMount(() => {
    const floating = wrapper()
    const anchor = props.trigger()
    if (!floating || !anchor || !content) return
    const z = getComputedStyle(content).zIndex
    if (z) floating.style.zIndex = z
    let live = true
    const detect = { padding: OVERLAY_PADDING, boundary: [] as Element[], altBoundary: false }
    const update = () =>
      void computePosition(anchor, floating, {
        strategy: `fixed`,
        placement: `bottom-start`,
        middleware: [
          offset({ mainAxis: OVERLAY_OFFSET, alignmentAxis: 0 }),
          shift({ mainAxis: true, crossAxis: false, limiter: limitShift(), ...detect }),
          flip({ ...detect }),
          size({
            ...detect,
            apply: ({ elements, rects, availableWidth, availableHeight }) => {
              const s = elements.floating.style
              s.setProperty(`--radix-popper-available-width`, `${availableWidth}px`)
              s.setProperty(`--radix-popper-available-height`, `${availableHeight}px`)
              s.setProperty(`--radix-popper-anchor-width`, `${rects.reference.width}px`)
              s.setProperty(`--radix-popper-anchor-height`, `${rects.reference.height}px`)
            },
          }),
          transformOrigin,
        ],
      }).then(({ x, y, placement: placed, middlewareData }) => {
        if (!live) return
        const dpr = window.devicePixelRatio || 1
        const round = (v: number) => Math.round(v * dpr) / dpr
        floating.style.transform = `translate(${round(x)}px, ${round(y)}px)`
        if (dpr >= 1.5) floating.style.willChange = `transform`
        const origin = middlewareData.transformOrigin as { x: string; y: string } | undefined
        floating.style.setProperty(`--radix-popper-transform-origin`, [origin?.x, origin?.y].join(` `))
        setPlacement(placed)
        setPositioned(true)
      })
    const stop = autoUpdate(anchor, floating, update)
    const doc = content.ownerDocument
    const onPointerDown = (e: PointerEvent) => {
      if (inside(e.target)) return
      if (interactOutside(e.target, `pointerdown`)) props.setOpen(false)
    }
    const onFocusIn = (e: FocusEvent) => {
      if (inside(e.target)) return
      if (interactOutside(e.target, `focusin`)) props.setOpen(false)
    }
    const onEscape = (e: KeyboardEvent) => {
      if (e.key !== `Escape` || !props.open) return
      e.stopPropagation()
      props.setOpen(false)
    }
    // Radix registers the outside pointer listener a tick late (the opening
    // click is not an outside one).
    const timer = setTimeout(() => doc.addEventListener(`pointerdown`, onPointerDown))
    doc.addEventListener(`focusin`, onFocusIn)
    doc.addEventListener(`keydown`, onEscape, { capture: true })
    props.presence.mount(content)
    props.onOpenAutoFocus(content)
    onCleanup(() => {
      props.presence.unmount()
      live = false
      stop()
      clearTimeout(timer)
      doc.removeEventListener(`pointerdown`, onPointerDown)
      doc.removeEventListener(`focusin`, onFocusIn)
      doc.removeEventListener(`keydown`, onEscape, { capture: true })
      // FocusScope's unmount auto-focus (a tick late): back to the trigger
      // unless the person interacted outside.
      const back = !interactedOutside
      const t = props.trigger()
      setTimeout(() => {
        if (back) t?.focus()
      })
    })
  })
  // FocusScope `loop` (not trapped): Tab wraps inside the content.
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== `Tab` || e.altKey || e.ctrlKey || e.metaKey || !content) return
    const focused = document.activeElement as HTMLElement | null
    const all = tabbables(content)
    if (!all.length) {
      if (focused === content) e.preventDefault()
      return
    }
    const first = all[0]!
    const last = all[all.length - 1]!
    if (!e.shiftKey && focused === last) {
      e.preventDefault()
      first.focus()
    } else if (e.shiftKey && focused === first) {
      e.preventDefault()
      last.focus()
    }
  }
  const style = (): JSX.CSSProperties => ({
    "--radix-popover-content-transform-origin": `var(--radix-popper-transform-origin)`,
    "--radix-popover-content-available-width": `var(--radix-popper-available-width)`,
    "--radix-popover-content-available-height": `var(--radix-popper-available-height)`,
    "--radix-popover-trigger-width": `var(--radix-popper-anchor-width)`,
    "--radix-popover-trigger-height": `var(--radix-popper-anchor-height)`,
    ...(positioned() ? {} : { animation: `none` }),
  })
  return (
    <Portal mount={ctx.portal ?? undefined} ref={mountWrapper}>
      <div
        ref={content}
        data-side={side()}
        data-align={align()}
        role="dialog"
        id={props.contentId}
        data-state={props.open ? `open` : `closed`}
        {...props.attrs}
        tabindex="-1"
        style={style()}
        onKeyDown={onKeyDown}
        onPointerOver={(e) => props.onPointerOver?.(e)}
        onFocusIn={(e) => props.onFocusIn?.(e)}
        onAnimationStart={(e) => props.presence.start(e)}
        onAnimationEnd={(e) => props.presence.end(e)}
        onAnimationCancel={(e) => props.presence.end(e)}
      >
        {props.children}
      </div>
    </Portal>
  )
}

/** PopoverPrimitive.Trigger's own attributes (the painter's win). */
const triggerAttrs = (open: boolean, contentId: string): Attrs => ({ type: `button`, "aria-haspopup": `dialog`, "aria-expanded": open, "aria-controls": contentId, "data-state": open ? `open` : `closed` })

// ── the calendar ─────────────────────────────────────────────────────────

interface CalendarProps {
  part: PartFn
  dayPart: `day`
  locale: string
  /** The surface Formatter: month and weekday names, day labels. */
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
  /** The SURFACE direction: ArrowLeft/Right mirror in RTL. */
  direction?: `ltr` | `rtl`
}

export function CalendarGrid(props: CalendarProps): JSX.Element {
  const initial = untrack(() => props.initial)
  const [focus, setFocus] = createSignal<Date>(initial)
  const [view, setView] = createSignal<Date>(new Date(initial.getFullYear(), initial.getMonth(), 1))
  let grid: HTMLDivElement | undefined
  let moved = false
  const days = createMemo(() => monthGrid(view(), props.firstDay))
  const weekdays = createMemo(() => {
    // 2023-01-01 was a Sunday; names through the surface Formatter (`EEE`;
    // the one-letter column head is the locale's narrow form, Intl).
    const narrow = new Intl.DateTimeFormat(props.formatter.locale, { weekday: `narrow` })
    return Array.from({ length: 7 }, (_, i) => {
      const d = new Date(2023, 0, 1 + ((props.firstDay + i) % 7))
      return { short: props.formatter.date(isoDate(d), { format: `EEE` }), narrow: narrow.format(d) }
    })
  })
  const monthLabel = () => props.formatter.date(isoDate(view()), { format: `MMMM yyyy` })
  const dayLabel = createMemo(() => new Intl.DateTimeFormat(props.formatter.locale, { weekday: `long`, year: `numeric`, month: `long`, day: `numeric` }))
  const disabled = (d: Date) => (props.min !== null && d < props.min) || (props.max !== null && d > props.max)
  // The month buttons move the focused day WITH the view.
  const shiftMonth = (n: number) => {
    const next = addMonths(focus(), n)
    batch(() => {
      setFocus(next)
      setView(new Date(next.getFullYear(), next.getMonth(), 1))
    })
  }
  const stop = createMemo(() => calendarStop(days(), view(), focus(), disabled))
  const goto = (d: Date) => {
    moved = true
    // One update: the refocus below runs against the NEW month's grid.
    batch(() => {
      setFocus(d)
      const v = view()
      if (d.getMonth() !== v.getMonth() || d.getFullYear() !== v.getFullYear()) setView(new Date(d.getFullYear(), d.getMonth(), 1))
    })
  }
  createEffect(
    on([focus, view], () => {
      if (!moved) return
      moved = false
      grid?.querySelector<HTMLButtonElement>(`[data-date="${isoDate(focus())}"]`)?.focus()
    })
  )
  const onKey = (e: KeyboardEvent) => {
    const k = e.key
    const f = focus()
    const rtl = props.direction === `rtl`
    let next: Date | null = null
    if (k === `ArrowLeft`) next = addDays(f, rtl ? 1 : -1)
    else if (k === `ArrowRight`) next = addDays(f, rtl ? -1 : 1)
    else if (k === `ArrowUp`) next = addDays(f, -7)
    else if (k === `ArrowDown`) next = addDays(f, 7)
    else if (k === `PageUp`) next = addMonths(f, e.shiftKey ? -12 : -1)
    else if (k === `PageDown`) next = addMonths(f, e.shiftKey ? 12 : 1)
    else if (k === `Home`) next = addDays(f, -((f.getDay() - props.firstDay + 7) % 7))
    else if (k === `End`) next = addDays(f, 6 - ((f.getDay() - props.firstDay + 7) % 7))
    else if (k === `Enter` || k === ` `) {
      e.preventDefault()
      if (!disabled(f)) props.onPick(f)
      return
    }
    if (next) {
      e.preventDefault()
      goto(next)
    }
  }
  const rows = createMemo(() => {
    const out: Date[][] = []
    const all = days()
    for (let i = 0; i < 6; i++) out.push(all.slice(i * 7, i * 7 + 7))
    return out
  })
  return (
    <>
      <div class="xui-calendar-head">
        <button type="button" class="xui-calendar-nav" aria-label={props.t(`previousMonth`)} onClick={() => shiftMonth(-1)}>
          <BuiltinIcon slot="DatePicker.previousMonth" />
        </button>
        <span aria-live="polite">{monthLabel()}</span>
        <button type="button" class="xui-calendar-nav" aria-label={props.t(`nextMonth`)} onClick={() => shiftMonth(1)}>
          <BuiltinIcon slot="DatePicker.nextMonth" />
        </button>
      </div>
      <div ref={grid} class="xui-calendar-grid" role="grid" aria-label={monthLabel()} onKeyDown={onKey}>
        <div role="row" class="xui-calendar-row">
          <Index each={weekdays()}>
            {(w) => (
              <span role="columnheader" class="xui-calendar-weekday" aria-label={w().short} title={w().short}>
                {w().narrow}
              </span>
            )}
          </Index>
        </div>
        <Index each={rows()}>
          {(week) => (
            <div role="row" class="xui-calendar-row">
              <Index each={week()}>
                {(day) => {
                  const d = day
                  const sel = () => props.isSelected(d())
                  const range = () => props.inRange?.(d()) ?? false
                  const outside = () => d().getMonth() !== view().getMonth()
                  const off = () => disabled(d())
                  const focused = () => sameDay(d(), stop())
                  const today = () => sameDay(d(), new Date())
                  return (
                    <span role="gridcell" aria-selected={sel()} class="xui-calendar-cell">
                      <button
                        type="button"
                        {...props.part(props.dayPart, sel() && `selected`, off() && `disabled`)}
                        data-date={isoDate(d())}
                        data-outside={outside() ? `` : undefined}
                        data-range={range() ? `` : undefined}
                        data-today={today() ? `` : undefined}
                        disabled={off()}
                        tabindex={focused() ? 0 : -1}
                        onFocus={() => {
                          if (!sameDay(d(), focus())) setFocus(d())
                        }}
                        aria-label={dayLabel().format(d())}
                        aria-current={today() ? `date` : undefined}
                        onClick={() => {
                          setFocus(d())
                          props.onPick(d())
                        }}
                      >
                        {range() && !sel() ? <span {...props.part(`range`)} aria-hidden="true" /> : null}
                        <span class="xui-calendar-day-label">{d().getDate()}</span>
                      </button>
                    </span>
                  )
                }}
              </Index>
            </div>
          )}
        </Index>
      </div>
    </>
  )
}

function firstDayOf(props: Record<string, unknown>, locale: string): number {
  return props.firstDayOfWeek === undefined ? weekStart(locale) : ((Math.round(num(props.firstDayOfWeek)) % 7) + 7) % 7
}

const focusGridStop = (content: HTMLElement) => content.querySelector<HTMLButtonElement>(`.xui-calendar-grid button[tabindex="0"]`)?.focus()

export function DatePickerNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const ctx = useSurfaceContext()
  const id = createUniqueId()
  const contentId = createUniqueId()
  const [value, setValue] = createBoundState(() => props.node, () => props.scope, `value`, () => str(props.props.value))
  let trigger: HTMLButtonElement | undefined
  const f = createField({ node: () => props.node, domId: () => props.domId, props: () => props.props, value, focusEl: () => trigger })
  const [open, setOpen] = createSignal(false)
  const selected = createMemo(() => (value() ? parseIsoDate(value()) : null))
  const min = createMemo(() => (props.props.min ? parseIsoDate(str(props.props.min)) : null))
  const max = createMemo(() => (props.props.max ? parseIsoDate(str(props.props.max)) : null))
  const firstDay = () => firstDayOf(props.props, ctx.locale)
  const label = () => {
    const s = selected()
    return s ? ctx.formatter.date(isoDate(s)) : ``
  }
  const disabled = () => bool(props.props.disabled) || Boolean(f.form?.disabled)
  const pick = (d: Date) => {
    const next = isoDate(d)
    setValue(next)
    f.change(next)
    f.touch()
    void props.emit(`change`, { value: next }, { value: next })
    setOpen(false)
  }
  return (
    <div {...props.rootProps}>
      <TextPart part={part(`label`)} text={props.props.label} as="label" htmlFor={id} />
      <button
        ref={trigger}
        {...triggerAttrs(open(), contentId)}
        id={id}
        {...part(`trigger`, open() && `open`, f.invalid && `invalid`, disabled() && `disabled`)}
        disabled={disabled()}
        data-value={value() || undefined}
        aria-haspopup="dialog"
        aria-invalid={f.invalid || undefined}
        aria-describedby={f.describedBy}
        onClick={() => setOpen(!open())}
      >
        <span>{label() || <span {...part(`placeholder`)}>{str(props.props.placeholder)}</span>}</span>
        <BuiltinIcon slot="DatePicker.trigger" />
      </button>
      <PickerPopover
        open={open()}
        setOpen={setOpen}
        trigger={() => trigger}
        contentId={contentId}
        attrs={{ ...part(`calendar`, open() && `open`), dir: ctx.direction, role: `dialog`, "aria-label": str(props.props.label) || undefined, "data-xui-overlay": `DatePicker` }}
        onOpenAutoFocus={focusGridStop}
      >
        <CalendarGrid part={part} dayPart="day" locale={ctx.locale} formatter={ctx.formatter} firstDay={firstDay()} min={min()} max={max()} isSelected={(d) => sameDay(d, selected())} initial={selected() ?? new Date()} onPick={pick} t={ctx.t} direction={ctx.direction} />
      </PickerPopover>
      <FieldErrors part={part} field={f} />
    </div>
  )
}

export function DateRangePickerNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const ctx = useSurfaceContext()
  const id = createUniqueId()
  const contentId = createUniqueId()
  const [start, setStart] = createBoundState(() => props.node, () => props.scope, `start`, () => str(props.props.start))
  const [end, setEnd] = createBoundState(() => props.node, () => props.scope, `end`, () => str(props.props.end))
  let trigger: HTMLButtonElement | undefined
  const f = createField({ node: () => props.node, domId: () => props.domId, props: () => props.props, value: () => ({ start: start(), end: end() }), focusEl: () => trigger })
  const [open, setOpen] = createSignal(false)
  const [pending, setPending] = createSignal<Date | null>(null)
  const [hover, setHover] = createSignal<Date | null>(null)
  const s = createMemo(() => (start() ? parseIsoDate(start()) : null))
  const e = createMemo(() => (end() ? parseIsoDate(end()) : null))
  const min = createMemo(() => (props.props.min ? parseIsoDate(str(props.props.min)) : null))
  const max = createMemo(() => (props.props.max ? parseIsoDate(str(props.props.max)) : null))
  const firstDay = () => firstDayOf(props.props, ctx.locale)
  const fmt = (d: Date) => ctx.formatter.date(isoDate(d))
  const label = () => {
    const a = s()
    const b = e()
    return a ? `${fmt(a)} – ${b ? fmt(b) : ``}` : ``
  }
  createComputed(
    on(open, (o) => {
      if (!o) {
        setPending(null)
        setHover(null)
      }
    })
  )
  const pick = (d: Date) => {
    const p = pending()
    if (!p) {
      setPending(d)
      return
    }
    const [a, b] = d < p ? [d, p] : [p, d]
    const next = { start: isoDate(a), end: isoDate(b) }
    setStart(next.start)
    setEnd(next.end)
    setPending(null)
    f.change(next)
    f.touch()
    void props.emit(`change`, next, next)
    setOpen(false)
  }
  const range = createMemo(() => {
    const lo = pending() ?? s()
    const hi = pending() ? hover() : e()
    return lo && hi ? (hi < lo ? [hi, lo] : [lo, hi]) : [lo, hi]
  })
  return (
    <div {...props.rootProps}>
      <TextPart part={part(`label`)} text={props.props.label} as="label" htmlFor={id} />
      <button
        ref={trigger}
        {...triggerAttrs(open(), contentId)}
        id={id}
        {...part(`trigger`, open() && `open`, f.invalid && `invalid`)}
        data-start={start() || undefined}
        data-end={end() || undefined}
        aria-haspopup="dialog"
        aria-describedby={f.describedBy}
        onClick={() => setOpen(!open())}
      >
        <span>{label() || <span {...part(`placeholder`)}>{str(props.props.placeholder)}</span>}</span>
        <BuiltinIcon slot="DateRangePicker.trigger" />
      </button>
      <PickerPopover
        open={open()}
        setOpen={setOpen}
        trigger={() => trigger}
        contentId={contentId}
        attrs={{ ...part(`calendar`, open() && `open`), dir: ctx.direction, role: `dialog`, "aria-label": str(props.props.label) || undefined, "data-xui-overlay": `DateRangePicker`, "data-pending": pending() ? isoDate(pending()!) : undefined }}
        onOpenAutoFocus={focusGridStop}
        onPointerOver={(ev) => {
          const date = (ev.target as HTMLElement).closest?.(`[data-date]`)?.getAttribute(`data-date`)
          if (pending() && date) setHover(parseIsoDate(date))
        }}
        onFocusIn={(ev) => {
          const date = (ev.target as HTMLElement).getAttribute?.(`data-date`)
          if (pending() && date) setHover(parseIsoDate(date))
        }}
      >
        <CalendarGrid
          part={part}
          dayPart="day"
          locale={ctx.locale}
          formatter={ctx.formatter}
          firstDay={firstDay()}
          min={min()}
          max={max()}
          isSelected={(d) => sameDay(d, pending()) || (!pending() && (sameDay(d, s()) || sameDay(d, e())))}
          inRange={(d) => {
            const [a, b] = range()
            return Boolean(a && b && d > a && d < b)
          }}
          initial={s() ?? new Date()}
          onPick={pick}
          t={ctx.t}
          direction={ctx.direction}
        />
      </PickerPopover>
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

export function TimePickerNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const ctx = useSurfaceContext()
  const id = createUniqueId()
  const contentId = createUniqueId()
  const listId = `${id}-list`
  const [value, setValue] = createBoundState(() => props.node, () => props.scope, `value`, () => str(props.props.value))
  let trigger: HTMLButtonElement | undefined
  const f = createField({ node: () => props.node, domId: () => props.domId, props: () => props.props, value, focusEl: () => trigger })
  const disabled = () => bool(props.props.disabled) || Boolean(f.form?.disabled)
  const [open, setOpen] = createSignal(false)
  const step = () => Math.max(1, Math.round(num(props.props.step, 15)))
  const lo = () => parseTime(str(props.props.min)) ?? 0
  const hi = () => parseTime(str(props.props.max)) ?? 24 * 60 - 1
  const times = createMemo(() => {
    const out: number[] = []
    for (let m = lo(); m <= hi(); m += step()) out.push(m)
    return out
  })
  const clock = createMemo(() => new Intl.DateTimeFormat(ctx.locale, { hour: `numeric`, minute: `2-digit` }))
  const show = (m: number) => clock().format(new Date(2000, 0, 1, Math.floor(m / 60), m % 60))
  const current = () => parseTime(value())
  const pick = (i: number) => {
    const next = formatHHmm(times()[i]!)
    setValue(next)
    f.change(next)
    f.touch()
    void props.emit(`change`, { value: next }, { value: next })
    setOpen(false)
    trigger?.focus()
  }
  const nav = createListNavigation({ count: () => times().length, labelOf: (i) => show(times()[i]!), onPick: pick, typeahead: true })
  let list: HTMLDivElement | undefined
  createComputed(
    on(
      open,
      (o) => {
        if (!o) return
        const c = untrack(current)
        const i = c === null ? 0 : untrack(times).findIndex((t) => t >= c)
        nav.setActive(i < 0 ? 0 : i)
      },
      { defer: true }
    )
  )
  const activeId = () => (nav.active() >= 0 ? `${listId}-${nav.active()}` : undefined)
  createEffect(
    on(activeId, (a) => {
      if (a) list?.querySelector(`[id="${a}"]`)?.scrollIntoView?.({ block: `nearest` })
    })
  )
  return (
    <div {...props.rootProps}>
      <TextPart part={part(`label`)} text={props.props.label} as="label" htmlFor={id} />
      <button
        ref={trigger}
        type="button"
        data-state={open() ? `open` : `closed`}
        id={id}
        {...part(`trigger`, open() && `open`, disabled() && `disabled`, f.invalid && `invalid`)}
        disabled={disabled()}
        role="combobox"
        aria-haspopup="listbox"
        aria-expanded={open()}
        aria-controls={open() ? listId : undefined}
        data-value={value() || undefined}
        aria-describedby={f.describedBy}
        onClick={() => setOpen(!open())}
      >
        <span>{current() !== null ? show(current()!) : <span {...part(`placeholder`)}>{str(props.props.placeholder)}</span>}</span>
        <BuiltinIcon slot="TimePicker.trigger" />
      </button>
      <PickerPopover
        open={open()}
        setOpen={setOpen}
        trigger={() => trigger}
        contentId={contentId}
        attrs={{ ...part(`list`, open() && `open`), dir: ctx.direction, "data-xui-overlay": `TimePicker` }}
        onOpenAutoFocus={() => list?.focus()}
      >
        <div ref={list} id={listId} role="listbox" tabindex={0} aria-activedescendant={activeId()} aria-labelledby={id} class="xui-TimePicker-listbox" onKeyDown={nav.onKeyDown}>
          <For each={times()}>
            {(m, i) => {
              const sel = () => m === current()
              return (
                <div
                  id={`${listId}-${i()}`}
                  role="option"
                  aria-selected={sel()}
                  data-value={formatHHmm(m)}
                  {...part(`item`, (sel() || i() === nav.active()) && `selected`)}
                  onPointerMove={() => nav.setActive(i())}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => pick(i())}
                >
                  {show(m)}
                </div>
              )
            }}
          </For>
        </div>
      </PickerPopover>
      <FieldErrors part={part} field={f} />
    </div>
  )
}

export const NATIVES_DATES: Record<string, Component<NativeProps>> = {
  DatePicker: DatePickerNative,
  DateRangePicker: DateRangePickerNative,
  TimePicker: TimePickerNative,
}
