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
// matches. #EXP-1629: React's `row` trigger parts and value-anchored popup,
// `align` / `popupWidth`, the value + chevron `button` part (its hover is
// theirs too), the opening at the top with the cursor on the first enabled
// option (a `multiple` one's on its first chosen), the list's bottom fade, item `hover` = the cursor,
// `selected` = chosen; an option's `trailing` after its label in the popup
// row only, `open` as the popup opens. #EXP-1765, React's: the `pill`
// trigger (no label part, `aria-label`, `Select.dropdown`) and its popup, no
// cursor when the pointer opened it, `loading`'s Skeleton rows, `mono`
// options' trailing run, `searchPlaceholder`, the `searchIcon` part.

import { For, Show, createEffect, createMemo, createSignal, createUniqueId, on, onCleanup, onMount, untrack, type Accessor, type JSX } from "solid-js"
import { boundPath, firstEnabledIndex, listMoreBelow, openingCursorIndex, revealOption, selectClearSize, selectLoadingNode, selectOpensWithoutCursor, selectPopupGeometry } from "@exponential_app/eui/web"
import { nativeRecipeProps, resolveRecipe } from "@exponential_app/eui"
import { useSurfaceContext } from "../context"
import { DEBOUNCE_MS } from "../inputs"
import { FieldErrors, createField, joinIds, type FieldState } from "../form"
import { IconGlyph } from "../icons"
import { NodeView, type NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { bool, BuiltinIcon, css, objects, parts, str, TextPart, type PartFn } from "./shared"
import { createListNavigation, dismissable, findNextItem, mountIn, NativeSelect, OPEN_KEYS, popperWrapper, radixTypeahead, SELECTION_KEYS } from "../behaviors"

export interface SelectOption {
  label: string
  value: string
  icon?: string
  disabled?: boolean
  /** #EXP-1629: reads on after the label in the popup row (`main · default`). */
  trailing?: string
  /** #EXP-1765: the label in the mono type, the `trailing` its own muted run. */
  mono?: boolean
  /** #EXP-1629: a muted second line under the label (the IDE picker's `Offline`). */
  description?: string
}

/** #EXP-1629: an option's `trailing` after its label, one space apart (the
 *  item's own style); #EXP-1765: a `mono` option's is its own run instead. */
const trailingText = (o: SelectOption) => (str(o.trailing) && !bool(o.mono) ? ` ${str(o.trailing)}` : ``)

/** #EXP-1765: a `mono` option's `trailing` run (muted, the sans type, the item's gap apart). */
const TrailingRun = (p: { o: SelectOption }) => (bool(p.o.mono) && str(p.o.trailing) ? <span class="xui-Select-trailing" aria-hidden="true">{str(p.o.trailing)}</span> : null)

const asList = (raw: unknown): string[] => (Array.isArray(raw) ? raw.map(String) : raw === undefined || raw === null || raw === `` ? [] : String(raw).split(`,`))

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
  /** #EXP-1629: the `row` trigger's label (inside it), its glyph slot and the popup geometry. */
  rowLabel: () => JSX.Element
  spacer: () => JSX.Element
  /** A `description`: a row's trailing `control` column (around the value and chevron); the trigger's describedby. */
  End: (p: { children: JSX.Element }) => JSX.Element
  describedBy: () => string | undefined
  /** The value box (a row popup's anchor). */
  setValueEl: (el: HTMLElement) => void
  valueEl: () => HTMLElement | undefined
  chevronSlot: () => string
  /** The row value button's hover (`End` wraps the value and chevron in it), lent to what it holds. */
  inButton: () => string | false
  geometry: (triggerWidthVar: string) => ReturnType<typeof selectPopupGeometry>
  /** #EXP-1765: a pill's label names its trigger. */
  ariaLabel: () => string | undefined
  /** Did the pointer open the popup (a pill then opens with no cursor)? */
  byPointer: { current: boolean }
  bare: () => boolean
  loading: () => boolean
  skeleton: () => NativeProps[`node`]
  searchPlaceholder: () => string
  /** The node's resolved props (#EXP-1765: the clear button's recipe). */
  props: () => Record<string, unknown>
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

  // #EXP-1629: `open` as the popup opens (the IDE picker's lazy fetch).
  createEffect(
    on(open, (o) => {
      if (o) void props.emit(`open`)
    }, { defer: true })
  )

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

  const row = () => str(props.props.triggerVariant) === `row`
  // #EXP-1765: a `pill` shows no label (it names the trigger) and no hint;
  // #EXP-1778: nor does a `button` (the same parts, an outline button).
  const pill = () => str(props.props.triggerVariant) === `pill` || str(props.props.triggerVariant) === `button`
  // A `description`: under a field trigger; in a row UNDER the label (the
  // `labels` column), the value and chevron in the trailing `control`.
  const description = () => str(props.props.description)
  const described = () => row() && description() !== ``
  const rowLabel = () =>
    !row() ? null : described() ? (
      <div {...part(`labels`)}>
        <TextPart part={part(`label`)} text={props.props.label} />
        <TextPart part={part(`description`)} text={description()} id={`${id}-d`} />
      </div>
    ) : (
      <TextPart part={part(`label`)} text={props.props.label} />
    )
  const spacer = () => (row() && !described() ? <span class="xui-Select-spacer" aria-hidden="true" /> : null)
  // #EXP-1629: the row's value button lends its hover to what it holds.
  const [buttonHover, setButtonHover] = createSignal(false)
  const inButton = () => row() && buttonHover() && `hover`
  const button = (children: JSX.Element) =>
    row() ? (
      <span {...part(`button`)} onPointerEnter={() => setButtonHover(true)} onPointerLeave={() => setButtonHover(false)}>
        {children}
      </span>
    ) : (
      children
    )
  const End = (p: { children: JSX.Element }) => (
    <Show when={described()} fallback={button(p.children)}>
      <div {...part(`control`)}>{button(p.children)}</div>
    </Show>
  )
  const describedBy = () => joinIds(!pill() && description() && `${id}-d`, f.describedBy)
  const [valueEl, setValueEl] = createSignal<HTMLElement>()
  const chevronSlot = () => (row() ? `Select.chevron` : pill() ? `Select.dropdown` : `Select.trigger`)
  const geometry = (triggerWidthVar: string) => selectPopupGeometry(props.props, triggerWidthVar)
  const ariaLabel = () => (pill() ? str(props.props.label) || undefined : undefined)
  const byPointer = { current: false }
  const bare = () => selectOpensWithoutCursor(untrack(() => props.props), byPointer.current)
  const loading = () => bool(props.props.loading)
  const skeleton = createMemo(() => selectLoadingNode(props.node.id))
  const searchPlaceholder = () => str(props.props.searchPlaceholder) || ctx.t(`search`)
  const state: SelectState = { part, id, listId, multiple, searchable, placeholder, options, value, commit, open, setOpen, query, setQuery, disabled, setTrigger, trigger, source, localFilter: () => !optionsBound() && !source(), field: f, rowLabel, spacer, End, describedBy, setValueEl, valueEl, chevronSlot, inButton, geometry, ariaLabel, byPointer, bare, loading, skeleton, searchPlaceholder, props: () => props.props }
  return (
    <div {...props.rootProps} data-source={source() || undefined}>
      <Show when={!row() && !pill()}>
        <TextPart part={part(`label`)} text={props.props.label} as="label" htmlFor={id} />
      </Show>
      <Show when={!multiple() && !searchable()} fallback={<Combobox s={state} emptyText={str(props.props.emptyText)} />}>
        <PlainSelect s={state} name={str(props.props.name, props.node.id)} emptyText={str(props.props.emptyText)} />
      </Show>
      <Show when={!row() && !pill()}>
        <TextPart part={part(`description`)} text={description()} id={`${id}-d`} />
      </Show>
      <FieldErrors part={part} field={f} />
    </div>
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
  // #EXP-1765: a pointer-opened pill opens with no cursor (the content keeps focus).
  let bare = false
  const typeahead = radixTypeahead((search) => {
    const items = enabled()
    const next = findNextItem(items.map((o) => ({ label: str(o.label), o })), search, items.map((o) => ({ label: str(o.label), o })).find((i) => i.o.value === current()))
    if (next) s.commit(next.o.value)
  })
  const openUp = (e?: PointerEvent | MouseEvent) => {
    if (!s.disabled()) {
      s.byPointer.current = Boolean(e)
      bare = s.bare()
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
        aria-describedby={s.describedBy()}
        aria-label={s.ariaLabel()}
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
        {s.rowLabel()}
        {s.spacer()}
        <s.End>
          <span ref={s.setValueEl}>
            {chosen() ? <span {...part.with(`value`, { disabled: s.disabled() }, s.inButton())} style={css({ pointerEvents: `none` })}>{str(chosen()!.label)}</span> : <span {...part.with(`placeholder`, { disabled: s.disabled() }, s.inButton())}>{s.placeholder()}</span>}
          </span>
          <BuiltinIcon slot={s.chevronSlot()} part={part(`chevron`, s.inButton())} />
        </s.End>
      </button>
      <Show when={inForm() ? `${optionKey()}#` : false} keyed>
        {(_key) => <NativeSelect name={props.name} value={current()} disabled={s.disabled()} options={s.options()} onChange={(v) => s.commit(v)} />}
      </Show>
      <Show when={s.open()}>
        <PlainSelectContent s={s} contentId={contentId} current={current} emptyText={props.emptyText} downAt={() => downAt} clearDownAt={() => (downAt = null)} bare={bare} />
      </Show>
    </>
  )
}

function PlainSelectContent(props: { s: SelectState; contentId: string; current: () => string | undefined; emptyText: string; downAt: () => { x: number; y: number } | null; clearDownAt: () => void; bare: boolean }): JSX.Element {
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
    // VAPP-146: "" is an ordinary value: compare with the value itself, not `current()` (undefined for "").
    if (o.value !== (s.value() as string)) s.commit(o.value)
    close()
  }
  // #EXP-1765: while `loading` the list holds the skeleton, no option.
  const shownOptions = () => (s.loading() ? [] : s.options())
  const enabledNodes = () => items.filter((el, i) => el && i < shownOptions().length && !bool(shownOptions()[i]?.disabled))
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
  // #EXP-1629: every opening is a fresh pick: the first enabled option, at the top.
  const firstNode = () => (props.bare ? undefined : items[firstEnabledIndex(shownOptions().length, (i) => bool(shownOptions()[i]?.disabled))])
  const [moreBelow, setMoreBelow] = createSignal(false)
  const measure = () => setMoreBelow(listMoreBelow(viewport))
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
  const geometry = untrack(() => s.geometry(`--radix-select-trigger-width`))
  const wrapper = popperWrapper({
    anchor: () => (geometry.anchorValue ? (s.valueEl() ?? s.trigger()) : s.trigger()),
    sideOffset: geometry.sideOffset,
    alignOffset: geometry.alignOffset,
    align: geometry.align,
    collisionPadding: geometry.collisionPadding,
    onPlaced: () => {
      focusFirstOf([firstNode(), content])
      if (viewport) viewport.scrollTop = 0
      measure()
    },
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
          minWidth: geometry.minWidth,
          width: geometry.width,
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
        <div class="xui-Select-body">
        <style>{`[data-radix-select-viewport]{scrollbar-width:none;-ms-overflow-style:none;-webkit-overflow-scrolling:touch;}[data-radix-select-viewport]::-webkit-scrollbar{display:none}`}</style>
        <div ref={viewport} data-radix-select-viewport="" role="presentation" style={css({ position: `relative`, flex: 1, overflow: `hidden auto` })} onScroll={measure}>
          {s.loading() ? <NodeView node={s.skeleton()} /> : s.options().length === 0 ? <div {...part(`empty`)}>{props.emptyText || ctx.t(`noResults`)}</div> : null}
          <For each={shownOptions()}>
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
                  {...part(`item`, o.value === s.value() && `selected`, off() && `disabled`, isFocused() && `hover`)}
                  data-mono={bool(o.mono) ? `` : undefined}
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
                  {o.icon ? <span {...part(`itemIcon`)}><IconGlyph icons={ctx.host.icons} name={o.icon} /></span> : null}
                  {str(o.description) ? (
                    <span {...part(`itemText`)}>
                      <span {...part(`itemLabel`)}><span id={textId}>{str(o.label)}</span>{trailingText(o)}</span>
                      <span {...part(`itemDescription`)}>{str(o.description)}</span>
                    </span>
                  ) : trailingText(o) ? (
                    <span>
                      <span id={textId}>{str(o.label)}</span>
                      {trailingText(o)}
                    </span>
                  ) : (
                    <span id={textId}>{str(o.label)}</span>
                  )}
                  <TrailingRun o={o} />
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
        <Show when={moreBelow()}>
          <div {...part(`fade`)} aria-hidden="true" />
        </Show>
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
    // #EXP-1765: while `loading` the list holds the skeleton, no option.
    if (s.loading()) return []
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
        if (o) nav.setActive(s.bare() ? -1 : openingCursorIndex(shown().length, (i) => bool(shown()[i]?.disabled), s.multiple() ? (i) => selected().includes(shown()[i]!.value) : undefined))
        else s.setQuery(``)
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
  // #EXP-1629: the list (never the page) scrolls to the cursor.
  createEffect(() => {
    const a = activeId()
    if (s.open() && a) revealOption(listEl, listEl?.querySelector(`[id="${a}"]`))
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
        aria-describedby={s.describedBy()}
        aria-label={s.ariaLabel()}
        onPointerDown={() => (s.byPointer.current = true)}
        onKeyDown={(e) => {
          s.byPointer.current = false
          if (!s.open() && (e.key === `ArrowDown` || e.key === `ArrowUp`)) {
            e.preventDefault()
            s.setOpen(true)
          }
        }}
        onClick={() => s.setOpen(!s.open())}
      >
        {s.rowLabel()}
        {s.spacer()}
        <s.End>
          <span ref={s.setValueEl}>{chosen().length ? <span {...part.with(`value`, { disabled: s.disabled() }, s.inButton())}>{chosen().map((o) => str(o.label)).join(`, `)}</span> : <span {...part.with(`placeholder`, { disabled: s.disabled() }, s.inButton())}>{s.placeholder()}</span>}</span>
          <BuiltinIcon slot={s.chevronSlot()} part={part(`chevron`, s.inButton())} />
        </s.End>
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
  let searchEl: HTMLInputElement | undefined
  let interactedOutside = false
  // #EXP-1765: a pill's search shows a CLEAR button while it holds text.
  const clear = createMemo(() => (str(s.props().triggerVariant) === `pill` ? selectClearSize(resolveRecipe(ctx.theme, { component: `Select`, part: `searchIcon`, props: nativeRecipeProps(`Select`, s.props(), ctx.extensionDefs) }, ctx.mode).width) : null))
  const close = () => s.setOpen(false)
  dismissable({ layer: () => content, onDismiss: close, isInside: (t) => Boolean(s.trigger()?.contains(t)), onOutside: () => (interactedOutside = true) })
  // FocusScope: mount auto-focus (the search field, else the listbox as
  // React's onOpenAutoFocus asks), unmount back to the trigger unless the
  // person interacted outside.
  onCleanup(() => {
    const trigger = s.trigger()
    if (!interactedOutside) setTimeout(() => trigger?.focus())
  })
  const geometry = untrack(() => s.geometry(`--radix-popover-trigger-width`))
  const [moreBelow, setMoreBelow] = createSignal(false)
  const measure = () => setMoreBelow(listMoreBelow(listEl))
  createEffect(on(() => props.shown().length, () => measure()))
  const wrapper = popperWrapper({
    anchor: () => (geometry.anchorValue ? (s.valueEl() ?? s.trigger()) : s.trigger()),
    sideOffset: geometry.sideOffset,
    alignOffset: geometry.alignOffset,
    align: geometry.align,
    collisionPadding: geometry.collisionPadding,
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
          minWidth: geometry.minWidth,
          width: geometry.width,
          animation: placed().positioned ? undefined : `none`,
          "--radix-popover-content-transform-origin": `var(--radix-popper-transform-origin)`,
          "--radix-popover-content-available-width": `var(--radix-popper-available-width)`,
          "--radix-popover-content-available-height": `var(--radix-popper-available-height)`,
          "--radix-popover-trigger-width": `var(--radix-popper-anchor-width)`,
          "--radix-popover-trigger-height": `var(--radix-popper-anchor-height)`,
        })}
      >
        {s.searchable() ? (
          // #EXP-1629: the `search` part is the whole box (glyph + field, one hairline).
          <div {...part(`search`)} class={`${part(`search`).class as string} xui-Select-searchbox`}>
            <BuiltinIcon slot="Select.search" part={part(`searchIcon`)} />
            <input
              ref={searchEl}
              class="xui-Select-searchfield"
              role="combobox"
              aria-expanded={true}
              aria-controls={s.listId}
              aria-activedescendant={props.activeId()}
              aria-autocomplete="list"
              placeholder={s.searchPlaceholder()}
              aria-label={ctx.t(`search`)}
              value={s.query()}
              prop:defaultValue={s.query()}
              onInput={(e) => s.setQuery(e.currentTarget.value)}
              onKeyDown={props.nav.onKeyDown}
            />
            <Show when={clear() && s.query()}>
              <button
                type="button"
                class="xui-Select-clear"
                aria-label={ctx.t(`clearSearch`)}
                style={css({ width: `${clear()!.box}px`, height: `${clear()!.box}px` })}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => {
                  s.setQuery(``)
                  searchEl?.focus()
                }}
              >
                <BuiltinIcon slot="Select.clear" size={clear()!.glyph} />
              </button>
            </Show>
          </div>
        ) : null}
        <div class="xui-Select-body">
        <div
          ref={(el) => {
            listEl = el
            props.listRef(el)
          }}
          onScroll={measure}
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
          {s.loading() ? <NodeView node={s.skeleton()} /> : props.shown().length === 0 ? <div {...part(`empty`)}>{props.emptyText || ctx.t(`noResults`)}</div> : null}
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
                  {...part(`item`, isSel() && `selected`, bool(o.disabled) && `disabled`, isActive() && `hover`)}
                  data-mono={bool(o.mono) ? `` : undefined}
                  onPointerMove={() => props.nav.setActive(i())}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => props.pick(i())}
                >
                  {o.icon ? <span {...part(`itemIcon`)}><IconGlyph icons={ctx.host.icons} name={o.icon} /></span> : null}
                  {str(o.description) ? (
                    <span {...part(`itemText`)}>
                      <span {...part(`itemLabel`)}>{str(o.label)}{trailingText(o)}</span>
                      <span {...part(`itemDescription`)}>{str(o.description)}</span>
                    </span>
                  ) : (
                    <span>{str(o.label)}{trailingText(o)}</span>
                  )}
                  <TrailingRun o={o} />
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
        <Show when={moreBelow()}>
          <div {...part(`fade`)} aria-hidden="true" />
        </Show>
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
