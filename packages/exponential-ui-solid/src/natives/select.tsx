// FEED-128 (React's natives/select.tsx; contract §3 Select = the combobox;
// a11y.json Select). A plain single Select is Radix's Select on React
// (arrows, type-ahead, Home/End built in); here the same DOM and keyboard
// model hand-rolled: the trigger (role=combobox, aria-controls the content
// id, `data-state`, `data-placeholder`), inside a <form> Radix's hidden
// native <select>, and when open the popper wrapper + the listbox content
// whose items take focus (`data-highlighted`). `searchable` and/or
// `multiple` is the COMBOBOX (Radix's Popover on React): a trigger opening a
// listbox; with `searchable` the filter field keeps focus and drives the
// active option through aria-activedescendant, typing fires `search
// {query}` (150 ms debounce). Options come from the `options` prop
// (bindable: the host refills the bound path on `search`, the async
// source), or from the host's `optionSource(source, query)` when `source`
// names a host list. Local filtering applies only to inline, unbound
// options. `emptyText` (default `$string.noResults`) shows when nothing
// matches.

import { For, Show, createEffect, createMemo, createSignal, createUniqueId, on, onCleanup, onMount, untrack, type Accessor, type JSX } from "solid-js"
import { autoUpdate, computePosition, flip, limitShift, offset, shift, size, type Middleware, type Placement } from "@floating-ui/dom"
import { OVERLAY_OFFSET, OVERLAY_PADDING } from "@exponential-at/ui"
import { boundPath, nextEnabledIndex, typeaheadStep } from "@exponential-at/ui/web"
import { useSurfaceContext } from "../context"
import { DEBOUNCE_MS } from "../inputs"
import { FieldErrors, createField, type FieldState } from "../form"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { bool, BuiltinIcon, css, objects, parts, str, TextPart, type PartFn, type StyleInput } from "./shared"

export interface SelectOption {
  label: string
  value: string
  icon?: string
  disabled?: boolean
}

const asList = (raw: unknown): string[] => (Array.isArray(raw) ? raw.map(String) : raw === undefined || raw === null || raw === `` ? [] : String(raw).split(`,`))

// ---------------------------------------------------------------------------
// The listbox keyboard model (React's natives/listbox.ts)
// ---------------------------------------------------------------------------

export interface ListNavOptions {
  count: Accessor<number>
  isDisabled?: (index: number) => boolean
  labelOf?: (index: number) => string
  onPick: (index: number) => void
  /** Type-ahead (off when a search field owns the keys). */
  typeahead?: Accessor<boolean>
  initial?: number
}

/** React's `useListNavigation`: an active option (aria-activedescendant,
 *  focus stays where it is), ArrowUp/ArrowDown move it (wrapping, skipping
 *  disabled), Home/End jump, Enter picks, printable keys type-ahead by label
 *  when the list owns focus. */
export function createListNavigation(o: ListNavOptions) {
  const isDisabled = o.isDisabled ?? (() => false)
  const labelOf = o.labelOf ?? (() => ``)
  const typeahead = o.typeahead ?? (() => false)
  const [active, setActive] = createSignal(o.initial ?? -1)
  let buffer = { text: ``, at: 0 }
  createEffect(() => {
    const count = o.count()
    if (active() >= count) setActive(count - 1)
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
    } else if (e.key === `Enter` || (e.key === ` ` && typeahead() && buffer.text === ``)) {
      if (a >= 0 && a < count && !isDisabled(a)) {
        e.preventDefault()
        o.onPick(a)
      }
    } else if (typeahead() && e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const step = typeaheadStep(buffer, e.key, Date.now(), a, count, isDisabled, labelOf)
      buffer = step.buffer
      if (step.index >= 0) setActive(step.index)
    }
  }
  return { active, setActive, onKeyDown }
}

// ---------------------------------------------------------------------------
// The overlay plumbing Radix gives React: a portal without a wrapper
// element, the popper (floating-ui, Radix's middleware and wrapper style),
// the dismissable layer (Escape, outside pointer/focus).
// ---------------------------------------------------------------------------

/** Mounts `el` at the end of `container` while the owner lives (Radix's
 *  Portal: no wrapper element, unlike Solid's <Portal>). */
function mountIn(container: HTMLElement, el: Node) {
  container.appendChild(el)
  onCleanup(() => el.parentNode?.removeChild(el))
}

/** Radix's `transformOrigin` middleware (no arrow). */
const transformOrigin: Middleware = {
  name: `transformOrigin`,
  fn({ placement, rects }) {
    const [side, align = `center`] = placement.split(`-`)
    const noArrowAlign = ({ start: `0%`, center: `50%`, end: `100%` } as Record<string, string>)[align]
    let x = ``
    let y = ``
    if (side === `bottom`) [x, y] = [noArrowAlign, `0px`]
    else if (side === `top`) [x, y] = [noArrowAlign, `${rects.floating.height}px`]
    else if (side === `right`) [x, y] = [`0px`, noArrowAlign]
    else if (side === `left`) [x, y] = [`${rects.floating.width}px`, noArrowAlign]
    return { data: { x, y } }
  },
}

interface Placed {
  side: string
  align: string
  positioned: boolean
}

/** Radix's PopperContent: the fixed wrapper (`data-radix-popper-content-
 *  wrapper`, floating-ui's translate, `--radix-popper-*`), the content's
 *  placed side/align. `content` renders the content element. */
function popperWrapper(opts: { anchor: () => HTMLElement | undefined; sideOffset: number; align: `start` | `center` | `end`; collisionPadding: number; dir?: () => `ltr` | `rtl` | `auto` | undefined; onPlaced?: () => void; content: (placed: Accessor<Placed>, ref: (el: HTMLElement) => void) => JSX.Element }): HTMLElement {
  const [pos, setPos] = createSignal<{ x: number; y: number; placement: Placement; origin: [string, string] } | null>(null)
  const [zIndex, setZIndex] = createSignal<string>()
  let contentEl: HTMLElement | undefined
  const dpr = () => (typeof window === `undefined` ? 1 : window.devicePixelRatio || 1)
  const round = (v: number) => Math.round(v * dpr()) / dpr()
  const placed = createMemo<Placed>(() => {
    const p = pos()
    const [side, align = `center`] = (p?.placement ?? `bottom-${opts.align}`).split(`-`)
    return { side, align: opts.align === `center` && !p ? `center` : align, positioned: p !== null }
  })
  const style = () => {
    const p = pos()
    const s: StyleInput = { position: `fixed`, left: 0, top: 0, transform: p ? `translate(${round(p.x)}px, ${round(p.y)}px)` : `translate(0, -200%)` }
    if (dpr() >= 1.5) s.willChange = `transform`
    s.minWidth = `max-content`
    s.zIndex = zIndex()
    s[`--radix-popper-transform-origin`] = p ? p.origin.join(` `) : ` `
    return css(s)
  }
  const wrapper = (
    <div data-radix-popper-content-wrapper="" style={style()} dir={opts.dir?.()}>
      {opts.content(placed, (el) => (contentEl = el))}
    </div>
  ) as HTMLElement
  onMount(() => {
    if (contentEl) setZIndex(getComputedStyle(contentEl).zIndex)
    const anchor = opts.anchor()
    if (!anchor) return
    const detect = { padding: opts.collisionPadding, boundary: [] as Element[], altBoundary: false }
    let alive = true
    const update = () =>
      void computePosition(anchor, wrapper, {
        strategy: `fixed`,
        placement: (opts.align === `center` ? `bottom` : `bottom-${opts.align}`) as Placement,
        middleware: [
          offset({ mainAxis: opts.sideOffset, alignmentAxis: 0 }),
          shift({ mainAxis: true, crossAxis: false, limiter: limitShift(), ...detect }),
          flip({ ...detect }),
          size({
            ...detect,
            apply: ({ elements, rects, availableWidth, availableHeight }) => {
              const st = elements.floating.style
              st.setProperty(`--radix-popper-available-width`, `${availableWidth}px`)
              st.setProperty(`--radix-popper-available-height`, `${availableHeight}px`)
              st.setProperty(`--radix-popper-anchor-width`, `${rects.reference.width}px`)
              st.setProperty(`--radix-popper-anchor-height`, `${rects.reference.height}px`)
            },
          }),
          transformOrigin,
        ],
      }).then((r) => {
        if (!alive) return
        const first = pos() === null
        const origin = r.middlewareData.transformOrigin as { x: string; y: string } | undefined
        setPos({ x: r.x, y: r.y, placement: r.placement, origin: [origin?.x ?? ``, origin?.y ?? ``] })
        if (first) opts.onPlaced?.()
      })
    const stop = autoUpdate(anchor, wrapper, update, { animationFrame: false })
    onCleanup(() => {
      alive = false
      stop()
    })
  })
  return wrapper
}

/** The open overlay layers, innermost last: only the top one answers
 *  Escape (Radix's DismissableLayer stack). */
const layers: HTMLElement[] = []

/** Radix's DismissableLayer: Escape, a pointerdown outside (and, unless
 *  `keepOnFocusOutside`, focus moving outside) dismiss. `isInside` widens
 *  "inside" (the trigger of a non-modal popover). */
function dismissable(opts: { layer: () => HTMLElement | undefined; onDismiss: () => void; isInside?: (target: Node) => boolean; keepOnFocusOutside?: boolean; onOutside?: () => void }) {
  onMount(() => {
    const layer = opts.layer()
    if (!layer) return
    layers.push(layer)
    const inside = (t: EventTarget | null) => t instanceof Node && (layer.contains(t) || Boolean(opts.isInside?.(t)))
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== `Escape` || layers[layers.length - 1] !== layer) return
      e.preventDefault()
      e.stopImmediatePropagation()
      opts.onDismiss()
    }
    const onPointer = (e: PointerEvent) => {
      if (inside(e.target)) return
      opts.onOutside?.()
      opts.onDismiss()
    }
    const onFocusIn = (e: FocusEvent) => {
      if (opts.keepOnFocusOutside || inside(e.target)) return
      opts.onOutside?.()
      opts.onDismiss()
    }
    document.addEventListener(`keydown`, onKey, { capture: true })
    document.addEventListener(`focusin`, onFocusIn)
    // Radix registers the pointer listener a tick later: the event that
    // opened the layer must not close it.
    const timer = window.setTimeout(() => document.addEventListener(`pointerdown`, onPointer))
    onCleanup(() => {
      window.clearTimeout(timer)
      document.removeEventListener(`keydown`, onKey, { capture: true })
      document.removeEventListener(`focusin`, onFocusIn)
      document.removeEventListener(`pointerdown`, onPointer)
      const i = layers.indexOf(layer)
      if (i >= 0) layers.splice(i, 1)
    })
  })
}

// ---------------------------------------------------------------------------
// Select
// ---------------------------------------------------------------------------

interface SelectState {
  part: PartFn
  id: string
  listId: string
  multiple: () => boolean
  searchable: () => boolean
  placeholder: () => string
  options: () => SelectOption[]
  value: () => string | string[]
  commit: (next: string | string[]) => void
  open: () => boolean
  setOpen: (open: boolean) => void
  query: () => string
  setQuery: (q: string) => void
  disabled: () => boolean
  setTrigger: (el: HTMLButtonElement) => void
  trigger: () => HTMLButtonElement | undefined
  source: () => string
  localFilter: () => boolean
  field: FieldState
}

export function SelectNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const ctx = useSurfaceContext()
  const id = createUniqueId()
  const listId = `${id}-list`
  const multiple = () => bool(props.props.multiple)
  const searchable = () => bool(props.props.searchable)
  const placeholder = () => str(props.props.placeholder) || ctx.t(`choose`)
  const raw = () => props.props.value
  const [value, setValue] = createBoundState<string | string[]>(() => props.node, () => props.scope, `value`, () => (multiple() ? asList(raw()) : str(raw())))
  const [trigger, setTrigger] = createSignal<HTMLButtonElement>()
  const f = createField({ node: () => props.node, domId: () => props.domId, props: () => props.props, value, focusEl: trigger })
  const disabled = () => bool(props.props.disabled) || Boolean(f.form?.disabled)
  const [open, setOpen] = createSignal(false)
  const [query, setQuery] = createSignal(``)
  const source = () => str(props.props.source)
  const optionsBound = () => boundPath(props.node.props, `options`, props.scope) !== undefined
  const [sourced, setSourced] = createSignal<SelectOption[] | null>(null)
  // Every optionSource request is numbered; a response that is not the
  // latest one is dropped (an older query resolving last must not win).
  let request = 0
  const askSource = (q: string) => {
    const src = untrack(source)
    const host = untrack(() => ctx.host)
    if (!src || !host.optionSource) return
    const mine = ++request
    const r = host.optionSource(src, q)
    const take = (list: SelectOption[] | null | undefined) => {
      if (mine === request) setSourced(objects<SelectOption>(list))
    }
    if (r && typeof (r as Promise<SelectOption[]>).then === `function`) void (r as Promise<SelectOption[]>).then(take, () => undefined)
    else take(r as SelectOption[])
  }
  onCleanup(() => request++)
  const inline = createMemo(() => objects<SelectOption>(props.props.options))
  const options = () => sourced() ?? inline()
  const commit = (next: string | string[]) => {
    const r = untrack(raw)
    const out = untrack(multiple) && !Array.isArray(r) && typeof r === `string` ? (next as string[]).join(`,`) : next
    setValue(next, out)
    f.change(out)
    f.touch()
    void props.emit(`change`, { value: out }, { value: out })
  }

  // `search {query}` debounced; a host source answers the same query.
  createEffect(
    on([query, open, searchable, source], ([q, o, s]) => {
      if (!s || !o) return
      const t = setTimeout(() => {
        void props.emit(`search`, { query: q })
        askSource(q)
      }, DEBOUNCE_MS)
      onCleanup(() => clearTimeout(t))
    })
  )
  createEffect(
    on([source, () => ctx.host, searchable], ([src, host, s]) => {
      if (!src || !host.optionSource || s) return
      askSource(``)
    })
  )

  const state: SelectState = { part, id, listId, multiple, searchable, placeholder, options, value, commit, open, setOpen, query, setQuery, disabled, setTrigger, trigger, source, localFilter: () => !optionsBound() && !source(), field: f }
  return (
    <div {...props.rootProps} data-source={source() || undefined}>
      <TextPart part={part(`label`)} text={props.props.label} as="label" htmlFor={id} />
      <Show when={!multiple() && !searchable()} fallback={<Combobox s={state} emptyText={str(props.props.emptyText)} />}>
        <PlainSelect s={state} name={str(props.props.name, props.node.id)} emptyText={str(props.props.emptyText)} />
      </Show>
      <FieldErrors part={part} field={f} />
    </div>
  )
}

const OPEN_KEYS = [` `, `Enter`, `ArrowUp`, `ArrowDown`]
const SELECTION_KEYS = [` `, `Enter`]

/** Radix's Select type-ahead: a 1 s buffer, a repeated letter cycles. */
function radixTypeahead(onSearch: (search: string) => void) {
  let search = ``
  let timer = 0
  const update = (v: string) => {
    search = v
    window.clearTimeout(timer)
    if (v !== ``) timer = window.setTimeout(() => update(``), 1000)
  }
  onCleanup(() => window.clearTimeout(timer))
  return {
    get search() {
      return search
    },
    type: (key: string) => {
      const next = search + key
      onSearch(next)
      update(next)
    },
    reset: () => {
      search = ``
      window.clearTimeout(timer)
    },
  }
}

function findNextItem<T extends { label: string }>(items: T[], search: string, current: T | undefined): T | undefined {
  const repeated = search.length > 1 && Array.from(search).every((c) => c === search[0])
  const normalized = repeated ? search[0] : search
  const at = current ? items.indexOf(current) : -1
  let wrapped = items.map((_, i) => items[(Math.max(at, 0) + i) % items.length])
  if (normalized.length === 1) wrapped = wrapped.filter((v) => v !== current)
  const next = wrapped.find((item) => item.label.toLowerCase().startsWith(normalized.toLowerCase()))
  return next !== current ? next : undefined
}

/** Radix's visually-hidden native <select> (inside a <form>). */
function NativeSelect(props: { name: string; value: string | undefined; disabled: boolean; options: SelectOption[]; onChange: (v: string) => void }): JSX.Element {
  let el: HTMLSelectElement | undefined
  const initial = untrack(() => props.value)
  createEffect(
    on(
      () => props.value,
      (v) => {
        if (!el) return
        Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype, `value`)?.set?.call(el, v)
        el.dispatchEvent(new Event(`change`, { bubbles: true }))
      },
      { defer: true }
    )
  )
  const hidden = css({ position: `absolute`, border: 0, width: 1, height: 1, padding: 0, margin: -1, overflow: `hidden`, clip: `rect(0, 0, 0, 0)`, whiteSpace: `nowrap`, wordWrap: `normal` })
  return (
    <select ref={el} aria-hidden="true" tabindex={-1} name={props.name} disabled={props.disabled} style={hidden} onChange={(e) => props.onChange(e.currentTarget.value)}>
      {initial === undefined ? <option value="" /> : null}
      <For each={props.options}>
        {(o) => (
          <option value={o.value} disabled={bool(o.disabled)} attr:selected={initial !== undefined && o.value === initial ? `` : undefined}>
            {str(o.label)}
          </option>
        )}
      </For>
    </select>
  )
}

function PlainSelect(props: { s: SelectState; name: string; emptyText: string }): JSX.Element {
  const ctx = useSurfaceContext()
  const s = props.s
  const part = s.part
  const contentId = createUniqueId()
  const current = () => (s.value() as string) || undefined
  const chosen = () => s.options().find((o) => o.value === s.value())
  const enabled = () => s.options().filter((o) => !bool(o.disabled))
  const [inForm, setInForm] = createSignal(false)
  onMount(() => setInForm(Boolean(s.trigger()?.closest(`form`))))
  let pointerType = `touch`
  let downAt: { x: number; y: number } | null = null
  const typeahead = radixTypeahead((search) => {
    const items = enabled()
    const next = findNextItem(items.map((o) => ({ label: str(o.label), o })), search, items.map((o) => ({ label: str(o.label), o })).find((i) => i.o.value === current()))
    if (next) s.commit(next.o.value)
  })
  const openUp = (e?: PointerEvent | MouseEvent) => {
    if (!s.disabled()) {
      s.setOpen(true)
      typeahead.reset()
    }
    if (e) downAt = { x: Math.round(e.pageX), y: Math.round(e.pageY) }
  }
  // Radix keys the native select by its option values (a new option list
  // remounts it).
  const optionKey = createMemo(() => s.options().map((o) => o.value).join(`;`))
  return (
    <>
      <button
        type="button"
        role="combobox"
        aria-controls={contentId}
        aria-expanded={s.open()}
        aria-autocomplete="none"
        dir={ctx.direction}
        data-state={s.open() ? `open` : `closed`}
        disabled={s.disabled()}
        data-disabled={s.disabled() ? `` : undefined}
        data-placeholder={current() === undefined ? `` : undefined}
        ref={s.setTrigger}
        id={s.id}
        {...part(`trigger`, s.open() && `open`, s.disabled() && `disabled`, s.field.invalid && `invalid`)}
        aria-invalid={s.field.invalid || undefined}
        aria-describedby={s.field.describedBy}
        onClick={(e) => {
          e.currentTarget.focus()
          if (pointerType !== `mouse`) openUp(e)
        }}
        onPointerDown={(e) => {
          pointerType = e.pointerType
          const target = e.target as HTMLElement
          if (target.hasPointerCapture?.(e.pointerId)) target.releasePointerCapture(e.pointerId)
          if (e.button === 0 && e.ctrlKey === false && e.pointerType === `mouse`) {
            openUp(e)
            e.preventDefault()
          }
        }}
        onKeyDown={(e) => {
          const typing = typeahead.search !== ``
          const modifier = e.ctrlKey || e.altKey || e.metaKey
          if (!modifier && e.key.length === 1) typeahead.type(e.key)
          if (typing && e.key === ` `) return
          if (OPEN_KEYS.includes(e.key)) {
            openUp()
            e.preventDefault()
          }
        }}
      >
        <span>
          {chosen() ? <span style={css({ pointerEvents: `none` })}>{str(chosen()!.label)}</span> : <span {...part(`placeholder`)}>{s.placeholder()}</span>}
        </span>
        <BuiltinIcon slot="Select.trigger" />
      </button>
      <Show when={inForm() ? `${optionKey()}#` : false} keyed>
        {(_key) => <NativeSelect name={props.name} value={current()} disabled={s.disabled()} options={s.options()} onChange={(v) => s.commit(v)} />}
      </Show>
      <Show when={s.open()}>
        <PlainSelectContent s={s} contentId={contentId} current={current} emptyText={props.emptyText} downAt={() => downAt} clearDownAt={() => (downAt = null)} />
      </Show>
    </>
  )
}

function PlainSelectContent(props: { s: SelectState; contentId: string; current: () => string | undefined; emptyText: string; downAt: () => { x: number; y: number } | null; clearDownAt: () => void }): JSX.Element {
  const ctx = useSurfaceContext()
  const s = props.s
  const part = s.part
  let content: HTMLElement | undefined
  let viewport: HTMLDivElement | undefined
  const items: HTMLElement[] = []
  const [focused, setFocused] = createSignal(-1)
  const close = () => s.setOpen(false)
  const pick = (o: SelectOption) => {
    if (bool(o.disabled)) return
    if (o.value !== props.current()) s.commit(o.value)
    close()
  }
  const enabledNodes = () => items.filter((el, i) => el && !bool(s.options()[i]?.disabled))
  const focusFirstOf = (candidates: (HTMLElement | undefined)[]) => {
    const [first, ...rest] = items
    const [last] = rest.slice(-1)
    const previous = document.activeElement
    for (const c of candidates) {
      if (!c) continue
      if (c === previous) return
      c.scrollIntoView?.({ block: `nearest` })
      if (c === first && viewport) viewport.scrollTop = 0
      if (c === last && viewport) viewport.scrollTop = viewport.scrollHeight
      c.focus()
      if (document.activeElement !== previous) return
    }
  }
  const selectedNode = () => {
    const opts = s.options()
    const i = opts.findIndex((o) => props.current() !== undefined && o.value === props.current())
    return items[i >= 0 ? i : opts.findIndex((o) => !bool(o.disabled))]
  }
  const typeahead = radixTypeahead((search) => {
    const opts = s.options()
    const list = enabledNodes().map((el) => ({ el, label: str(opts[items.indexOf(el)]?.label) }))
    const next = findNextItem(list, search, list.find((i) => i.el === document.activeElement))
    if (next) setTimeout(() => next.el.focus())
  })
  const onWindowClose = () => close()
  onMount(() => {
    window.addEventListener(`blur`, onWindowClose)
    window.addEventListener(`resize`, onWindowClose)
    // A pointerdown on the trigger opened it: a pointerup within 10 px is
    // not a pick.
    if (props.downAt() !== null) {
      let delta = { x: 0, y: 0 }
      const move = (e: PointerEvent) => {
        const at = props.downAt()
        delta = { x: Math.abs(Math.round(e.pageX) - (at?.x ?? 0)), y: Math.abs(Math.round(e.pageY) - (at?.y ?? 0)) }
      }
      const up = (e: PointerEvent) => {
        if (delta.x <= 10 && delta.y <= 10) e.preventDefault()
        else if (!content?.contains(e.target as Node)) close()
        document.removeEventListener(`pointermove`, move)
        props.clearDownAt()
      }
      document.addEventListener(`pointermove`, move)
      document.addEventListener(`pointerup`, up, { capture: true, once: true })
      onCleanup(() => {
        document.removeEventListener(`pointermove`, move)
        document.removeEventListener(`pointerup`, up, { capture: true })
      })
    }
  })
  onCleanup(() => {
    window.removeEventListener(`blur`, onWindowClose)
    window.removeEventListener(`resize`, onWindowClose)
    // Radix's FocusScope restores focus a task later (so the Enter that
    // picked never lands on the trigger as a click).
    const trigger = s.trigger()
    setTimeout(() => trigger?.focus({ preventScroll: true }))
  })
  dismissable({ layer: () => content, onDismiss: close, keepOnFocusOutside: true })
  const wrapper = popperWrapper({
    anchor: s.trigger,
    sideOffset: OVERLAY_OFFSET,
    align: `start`,
    collisionPadding: OVERLAY_PADDING,
    onPlaced: () => focusFirstOf([selectedNode(), content]),
    content: (placed, ref) => (
      <div
        data-side={placed().side}
        data-align={placed().align}
        role="listbox"
        id={props.contentId}
        data-state="open"
        dir={ctx.direction}
        tabindex={-1}
        {...part(`content`, `open`)}
        data-xui-overlay="Select"
        ref={(el) => {
          content = el
          ref(el)
        }}
        style={css({
          boxSizing: `border-box`,
          pointerEvents: `auto`,
          display: `flex`,
          flexDirection: `column`,
          outline: `none`,
          minWidth: `var(--radix-select-trigger-width)`,
          "--radix-select-content-transform-origin": `var(--radix-popper-transform-origin)`,
          "--radix-select-content-available-width": `var(--radix-popper-available-width)`,
          "--radix-select-content-available-height": `var(--radix-popper-available-height)`,
          "--radix-select-trigger-width": `var(--radix-popper-anchor-width)`,
          "--radix-select-trigger-height": `var(--radix-popper-anchor-height)`,
          animation: placed().positioned ? undefined : `none`,
        })}
        onContextMenu={(e) => e.preventDefault()}
        onKeyDown={(e) => {
          const modifier = e.ctrlKey || e.altKey || e.metaKey
          if (e.key === `Tab`) e.preventDefault()
          if (!modifier && e.key.length === 1) typeahead.type(e.key)
          if ([`ArrowUp`, `ArrowDown`, `Home`, `End`].includes(e.key)) {
            let candidates = enabledNodes()
            if ([`ArrowUp`, `End`].includes(e.key)) candidates = candidates.slice().reverse()
            if ([`ArrowUp`, `ArrowDown`].includes(e.key)) candidates = candidates.slice(candidates.indexOf(e.target as HTMLElement) + 1)
            setTimeout(() => focusFirstOf(candidates))
            e.preventDefault()
          }
        }}
      >
        <style>{`[data-radix-select-viewport]{scrollbar-width:none;-ms-overflow-style:none;-webkit-overflow-scrolling:touch;}[data-radix-select-viewport]::-webkit-scrollbar{display:none}`}</style>
        <div ref={viewport} data-radix-select-viewport="" role="presentation" style={css({ position: `relative`, flex: 1, overflow: `hidden auto` })}>
          {s.options().length === 0 ? <div {...part(`empty`)}>{props.emptyText || ctx.t(`noResults`)}</div> : null}
          <For each={s.options()}>
            {(o, i) => {
              const textId = createUniqueId()
              const selected = () => o.value === props.current()
              const isFocused = () => focused() === i()
              const off = () => bool(o.disabled)
              let pointer = `touch`
              return (
                <div
                  role="option"
                  aria-labelledby={textId}
                  data-highlighted={isFocused() ? `` : undefined}
                  aria-selected={selected() && isFocused()}
                  data-state={selected() ? `checked` : `unchecked`}
                  aria-disabled={off() || undefined}
                  data-disabled={off() ? `` : undefined}
                  tabindex={off() ? undefined : -1}
                  {...part(`item`, o.value === s.value() && `selected`, off() && `disabled`)}
                  ref={(el) => (items[i()] = el)}
                  onFocus={() => setFocused(i())}
                  onBlur={() => setFocused((f) => (f === i() ? -1 : f))}
                  onClick={() => {
                    if (pointer !== `mouse`) pick(o)
                  }}
                  onPointerUp={(e) => {
                    if (!e.defaultPrevented && pointer === `mouse`) pick(o)
                  }}
                  onPointerDown={(e) => (pointer = e.pointerType)}
                  onPointerMove={(e) => {
                    pointer = e.pointerType
                    if (off()) content?.focus()
                    else if (pointer === `mouse`) e.currentTarget.focus({ preventScroll: true })
                  }}
                  onPointerLeave={(e) => {
                    if (e.currentTarget === document.activeElement) content?.focus()
                  }}
                  onKeyDown={(e) => {
                    if (typeahead.search !== `` && e.key === ` `) return
                    if (SELECTION_KEYS.includes(e.key)) pick(o)
                    if (e.key === ` `) e.preventDefault()
                  }}
                >
                  {o.icon ? <IconGlyph icons={ctx.host.icons} name={o.icon} /> : null}
                  <span id={textId}>{str(o.label)}</span>
                  <Show when={selected()}>
                    <span aria-hidden="true" {...part(`check`)} class={`${part(`check`).class as string} xui-Select-check`}>
                      <BuiltinIcon slot="Select.check" size={16} />
                    </span>
                  </Show>
                </div>
              )
            }}
          </For>
        </div>
      </div>
    ),
  })
  mountIn(ctx.portal ?? document.body, wrapper)
  return null
}

function Combobox(props: { s: SelectState; emptyText: string }): JSX.Element {
  const s = props.s
  const part = s.part
  const contentId = createUniqueId()
  const selected = () => {
    const v = s.value()
    return s.multiple() ? (v as string[]) : v ? [v as string] : []
  }
  const chosen = () => s.options().filter((o) => selected().includes(o.value))
  const shown = createMemo(() => {
    const q = s.query()
    return q && s.localFilter() ? s.options().filter((o) => str(o.label).toLowerCase().includes(q.toLowerCase())) : s.options()
  })
  let listEl: HTMLDivElement | undefined
  const pick = (i: number) => {
    const o = shown()[i]
    if (!o || bool(o.disabled)) return
    if (s.multiple()) {
      const sel = selected()
      s.commit(sel.includes(o.value) ? sel.filter((v) => v !== o.value) : [...sel, o.value])
    } else {
      s.commit(o.value)
      s.setOpen(false)
    }
  }
  const nav = createListNavigation({ count: () => shown().length, isDisabled: (i) => bool(shown()[i]?.disabled), labelOf: (i) => str(shown()[i]?.label), onPick: pick, typeahead: () => !s.searchable() })
  createEffect(
    on(
      s.open,
      (o) => {
        if (o) {
          const i = shown().findIndex((x) => selected().includes(x.value))
          nav.setActive(i >= 0 ? i : shown().findIndex((x) => !bool(x.disabled)))
        } else s.setQuery(``)
      }
    )
  )
  createEffect(
    on(
      s.query,
      () => {
        if (!untrack(s.searchable) || !untrack(s.open)) return
        nav.setActive(untrack(shown).findIndex((x) => !bool(x.disabled)))
      },
      { defer: true }
    )
  )
  const optionId = (i: number) => `${s.listId}-${i}`
  const activeId = () => (nav.active() >= 0 && nav.active() < shown().length ? optionId(nav.active()) : undefined)
  createEffect(() => {
    const a = activeId()
    if (a) listEl?.querySelector(`[id="${a}"]`)?.scrollIntoView?.({ block: `nearest` })
  })
  return (
    <>
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={s.open()}
        aria-controls={s.open() ? s.listId : undefined}
        data-state={s.open() ? `open` : `closed`}
        ref={s.setTrigger}
        id={s.id}
        {...part(`trigger`, s.open() && `open`, s.disabled() && `disabled`, s.field.invalid && `invalid`)}
        disabled={s.disabled()}
        role="combobox"
        aria-invalid={s.field.invalid || undefined}
        aria-describedby={s.field.describedBy}
        onKeyDown={(e) => {
          if (!s.open() && (e.key === `ArrowDown` || e.key === `ArrowUp`)) {
            e.preventDefault()
            s.setOpen(true)
          }
        }}
        onClick={() => s.setOpen(!s.open())}
      >
        <span>{chosen().length ? chosen().map((o) => str(o.label)).join(`, `) : <span {...part(`placeholder`)}>{s.placeholder()}</span>}</span>
        <BuiltinIcon slot="Select.trigger" />
      </button>
      <Show when={s.open()}>
        <ComboboxContent s={s} contentId={contentId} shown={shown} selected={selected} nav={nav} pick={pick} optionId={optionId} activeId={activeId} emptyText={props.emptyText} listRef={(el) => (listEl = el)} />
      </Show>
    </>
  )
}

function ComboboxContent(props: {
  s: SelectState
  contentId: string
  shown: Accessor<SelectOption[]>
  selected: Accessor<string[]>
  nav: ReturnType<typeof createListNavigation>
  pick: (i: number) => void
  optionId: (i: number) => string
  activeId: Accessor<string | undefined>
  emptyText: string
  listRef: (el: HTMLDivElement) => void
}): JSX.Element {
  const ctx = useSurfaceContext()
  const s = props.s
  const part = s.part
  let content: HTMLElement | undefined
  let listEl: HTMLDivElement | undefined
  let interactedOutside = false
  const close = () => s.setOpen(false)
  dismissable({ layer: () => content, onDismiss: close, isInside: (t) => Boolean(s.trigger()?.contains(t)), onOutside: () => (interactedOutside = true) })
  // FocusScope: mount auto-focus (the search field, else the listbox as
  // React's onOpenAutoFocus asks), unmount back to the trigger unless the
  // person interacted outside.
  onCleanup(() => {
    const trigger = s.trigger()
    if (!interactedOutside) setTimeout(() => trigger?.focus())
  })
  const wrapper = popperWrapper({
    anchor: s.trigger,
    sideOffset: OVERLAY_OFFSET,
    align: `start`,
    collisionPadding: OVERLAY_PADDING,
    dir: () => undefined,
    content: (placed, ref) => (
      <div
        data-side={placed().side}
        data-align={placed().align}
        data-state="open"
        role="dialog"
        id={props.contentId}
        {...part(`content`, `open`)}
        dir={ctx.direction}
        data-xui-overlay="Select"
        tabindex={-1}
        ref={(el) => {
          content = el
          ref(el)
        }}
        style={css({
          minWidth: `var(--radix-popover-trigger-width)`,
          animation: placed().positioned ? undefined : `none`,
          "--radix-popover-content-transform-origin": `var(--radix-popper-transform-origin)`,
          "--radix-popover-content-available-width": `var(--radix-popper-available-width)`,
          "--radix-popover-content-available-height": `var(--radix-popper-available-height)`,
          "--radix-popover-trigger-width": `var(--radix-popper-anchor-width)`,
          "--radix-popover-trigger-height": `var(--radix-popper-anchor-height)`,
        })}
      >
        {s.searchable() ? (
          <div class="xui-Select-searchbox">
            <BuiltinIcon slot="Select.search" size={16} />
            <input
              {...part(`search`)}
              class={`${part(`search`).class as string} xui-Select-search`}
              role="combobox"
              aria-expanded={true}
              aria-controls={s.listId}
              aria-activedescendant={props.activeId()}
              aria-autocomplete="list"
              placeholder={ctx.t(`search`)}
              aria-label={ctx.t(`search`)}
              value={s.query()}
              prop:defaultValue={s.query()}
              onInput={(e) => s.setQuery(e.currentTarget.value)}
              onKeyDown={props.nav.onKeyDown}
            />
          </div>
        ) : null}
        <div
          ref={(el) => {
            listEl = el
            props.listRef(el)
          }}
          id={s.listId}
          role="listbox"
          tabindex={s.searchable() ? -1 : 0}
          aria-multiselectable={s.multiple() || undefined}
          aria-activedescendant={s.searchable() ? undefined : props.activeId()}
          aria-labelledby={s.id}
          class="xui-Select-listbox"
          onKeyDown={(e) => {
            if (!s.searchable()) props.nav.onKeyDown(e)
          }}
        >
          {props.shown().length === 0 ? <div {...part(`empty`)}>{props.emptyText || ctx.t(`noResults`)}</div> : null}
          <For each={props.shown()}>
            {(o, i) => {
              const isSel = () => props.selected().includes(o.value)
              const isActive = () => i() === props.nav.active()
              return (
                <div
                  id={props.optionId(i())}
                  role="option"
                  aria-selected={isSel()}
                  aria-disabled={bool(o.disabled) || undefined}
                  data-active={isActive() ? `` : undefined}
                  {...part(`item`, (isSel() || isActive()) && `selected`, bool(o.disabled) && `disabled`, isActive() && `hover`)}
                  onPointerMove={() => props.nav.setActive(i())}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => props.pick(i())}
                >
                  {o.icon ? <IconGlyph icons={ctx.host.icons} name={o.icon} /> : null}
                  <span>{str(o.label)}</span>
                  {isSel() ? (
                    <span {...part(`check`)} class={`${part(`check`).class as string} xui-Select-check`}>
                      <BuiltinIcon slot="Select.check" size={16} />
                    </span>
                  ) : null}
                </div>
              )
            }}
          </For>
        </div>
      </div>
    ),
  })
  mountIn(ctx.portal ?? document.body, wrapper)
  onMount(() => {
    if (s.searchable()) content?.querySelector<HTMLInputElement>(`input`)?.focus()
    else listEl?.focus()
  })
  return null
}

export const NATIVES_SELECT: Record<string, (props: NativeProps) => JSX.Element> = { Select: SelectNative }
