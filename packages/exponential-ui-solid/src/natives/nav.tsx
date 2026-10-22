// FEED-128 (React's natives/nav.tsx): Tabs, Segmented (was ToggleGroup),
// Accordion — Radix behaviour (roving tab stop, arrows wrap, Home/End:
// a11y.json), recipe paint, a bound `value` written through (multiple =
// comma-joined when it came as a string, an array when it came as one).
//
// The parts are HAND-ROLLED on plain elements rather than Kobalte's
// Tabs/ToggleGroup/Accordion: Kobalte's roving model puts the tab stop on
// the selected trigger (`tabindex="0"`) and none on the list, writes
// `data-selected`/`data-expanded`/`data-key`, and has no equivalent of
// Radix's focusable group (`tabindex="0"` + `outline: none` on the
// tablist / toggle group, every item `-1` until one takes focus) nor of
// Radix's mount-animation styles (`animation-duration: 0s` on the first
// active panel, the `--radix-accordion-content-*` variables). The DOM the
// theme CSS and the web-parity check read is Radix's, so the painters
// reproduce Radix's DOM and its keyboard model (RovingFocusGroup:
// `focusFirst` in a timeout; Accordion: the root's arrow handler) here.

import { For, Show, createEffect, createMemo, createSignal, createUniqueId, on, onCleanup, onMount, untrack, type Accessor, type Component, type JSX } from "solid-js"
import type { UiNode } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { NodeView } from "../node-view"
import { createBoundState } from "./bound"
import { bool, BuiltinIcon, css, num, objects, parts, str, type PartFn } from "./shared"

interface Tab {
  label: string
  value: string
  icon?: string
  count?: number
}

const sameKeys = (a: readonly string[], b: readonly string[]): boolean => a.length === b.length && a.every((x, i) => x === b[i])

/** The values of a list as a memo that only changes when the KEYS change
 *  (React keys its rows by `value`: a row keeps its subtree). */
function keysOf<T>(list: Accessor<T[]>, key: (item: T) => string): Accessor<string[]> {
  return createMemo(() => list().map(key), undefined, { equals: sameKeys })
}

/** The child node at `index`, followed live. */
const childAt = (node: () => UiNode, index: Accessor<number>) => createMemo(() => node().children[index()] as UiNode | undefined)

// ── RovingFocusGroup (Radix) ────────────────────────────────────────────

type Intent = `first` | `last` | `prev` | `next`
const KEY_INTENT: Record<string, Intent> = { ArrowLeft: `prev`, ArrowUp: `prev`, ArrowRight: `next`, ArrowDown: `next`, PageUp: `first`, Home: `first`, PageDown: `last`, End: `last` }

function focusIntent(e: KeyboardEvent, orientation: `horizontal` | `vertical` | undefined, dir: `ltr` | `rtl`): Intent | undefined {
  let key = e.key
  if (dir === `rtl`) key = key === `ArrowLeft` ? `ArrowRight` : key === `ArrowRight` ? `ArrowLeft` : key
  if (orientation === `vertical` && (key === `ArrowLeft` || key === `ArrowRight`)) return undefined
  if (orientation === `horizontal` && (key === `ArrowUp` || key === `ArrowDown`)) return undefined
  return KEY_INTENT[key]
}

function focusFirst(candidates: HTMLElement[]): void {
  const previous = document.activeElement
  for (const candidate of candidates) {
    if (candidate === document.activeElement) return
    candidate.focus({ preventScroll: false })
    if (document.activeElement !== previous) return
  }
}

const wrap = <T,>(list: T[], start: number): T[] => list.map((_, i) => list[(start + i) % list.length]!)

/** Radix's RovingFocusGroup: the group's own attributes + handlers, and
 *  each item's. `items()` = the item elements in order, `focusable(el)`,
 *  `active(el)` = the selected one (the group's entry focus). */
function createRovingGroup(opts: { orientation?: `horizontal` | `vertical`; dir: () => `ltr` | `rtl`; loop: boolean; items: () => HTMLElement[]; focusable: (el: HTMLElement) => boolean; active: (el: HTMLElement) => boolean }) {
  const [stop, setStop] = createSignal<HTMLElement | null>(null)
  const [tabbingBackOut, setTabbingBackOut] = createSignal(false)
  let clickFocus = false
  const group = (): Record<string, unknown> => ({ tabindex: tabbingBackOut() ? -1 : 0, "data-orientation": opts.orientation })
  // React's onFocus/onBlur BUBBLE (focusin/focusout): a click on an item
  // resets the click flag through the group.
  const groupHandlers = {
    onMouseDown: () => void (clickFocus = true),
    onFocusIn: (e: FocusEvent) => {
      const keyboard = !clickFocus
      if (e.target === e.currentTarget && keyboard && !tabbingBackOut()) {
        const items = opts.items().filter(opts.focusable)
        const activeItem = items.find(opts.active)
        const current = items.find((el) => el === stop())
        focusFirst([activeItem, current, ...items].filter((el): el is HTMLElement => Boolean(el)))
      }
      clickFocus = false
    },
    onFocusOut: () => setTabbingBackOut(false),
  }
  const item = (el: () => HTMLElement | undefined, focusable: () => boolean) => ({
    tabindex: () => (stop() !== null && stop() === el() ? 0 : -1),
    onMouseDown: (e: MouseEvent) => {
      if (!focusable()) e.preventDefault()
      else setStop(el() ?? null)
    },
    onFocus: () => setStop(el() ?? null),
    onKeyDown: (e: KeyboardEvent) => {
      if (e.key === `Tab` && e.shiftKey) {
        setTabbingBackOut(true)
        return
      }
      if (e.target !== e.currentTarget) return
      const intent = focusIntent(e, opts.orientation, opts.dir())
      if (intent === undefined) return
      if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
      e.preventDefault()
      let candidates = opts.items().filter(opts.focusable)
      if (intent === `last`) candidates.reverse()
      else if (intent === `prev` || intent === `next`) {
        if (intent === `prev`) candidates.reverse()
        const at = candidates.indexOf(e.currentTarget as HTMLElement)
        candidates = opts.loop ? wrap(candidates, at + 1) : candidates.slice(at + 1)
      }
      setTimeout(() => focusFirst(candidates))
    },
  })
  return { group, groupHandlers, item }
}

// ── Tabs ────────────────────────────────────────────────────────────────

export function TabsNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const tabs = createMemo(() => objects<Tab>(props.props.tabs))
  const external = () => str(props.props.value) || tabs()[0]?.value || ``
  const [value, setValue] = createBoundState(() => props.node, () => props.scope, `value`, external)
  const fill = () => bool(props.props.fill)
  // EXP-1259: the close glyph per tab (`closable`; `hover` = only while its
  // tab or itself is hovered or keyboard-focused: the tab LENDS it those
  // states) and the + after the last tab (`addable`).
  const closable = () => props.props.closable === `hover` || props.props.closable === `always`
  const [lent, setLent] = createSignal<{ index: number; state: `hover` | `focus` } | null>(null)
  const lend = (index: number, state: `hover` | `focus` | null) => setLent((cur) => (state ? { index, state } : cur?.index === index ? null : cur))
  const lending = () => closable() && props.props.closable === `hover`
  const baseId = createUniqueId()
  const triggerId = (v: string) => `${baseId}-trigger-${v}`
  const contentId = (v: string) => `${baseId}-content-${v}`
  let list: HTMLDivElement | undefined
  const triggers = () => (list ? Array.from(list.querySelectorAll<HTMLElement>(`:scope > [role="tab"]`)) : [])
  const roving = createRovingGroup({ orientation: `horizontal`, dir: () => ctx.direction, loop: true, items: triggers, focusable: (el) => !(el as HTMLButtonElement).disabled, active: (el) => el.getAttribute(`aria-selected`) === `true` })
  const select = (next: string) => {
    if (next === untrack(value)) return
    setValue(next)
    void props.emit(`change`, { value: next }, { value: next })
  }
  const keys = keysOf(tabs, (t) => t.value)
  // Radix: the panel active at MOUNT renders `animation-duration: 0s`
  // until the first render after its next frame.
  const initial = untrack(value)
  let mountPrevented = true
  onMount(() => {
    const raf = requestAnimationFrame(() => (mountPrevented = false))
    onCleanup(() => cancelAnimationFrame(raf))
  })
  return (
    <div {...props.rootProps} dir={ctx.direction} data-orientation="horizontal">
      <div ref={list} role="tablist" aria-orientation="horizontal" {...part(`list`)} data-fill={fill() ? `true` : undefined} {...roving.group()} style={{ outline: `none` }} {...roving.groupHandlers}>
        <For each={keys()}>
          {(key, index) => {
            const tab = createMemo(() => tabs()[index()]!)
            const selected = () => key === value()
            let el: HTMLButtonElement | undefined
            const item = roving.item(() => el, () => true)
            const trigger = (
              <button
                ref={el}
                type="button"
                role="tab"
                aria-selected={selected()}
                aria-controls={contentId(key)}
                data-state={selected() ? `active` : `inactive`}
                id={triggerId(key)}
                {...part.at(`tab`, index(), selected() && `selected`)}
                tabindex={item.tabindex()}
                data-orientation="horizontal"
                data-radix-collection-item=""
                onPointerEnter={() => lending() && lend(index(), `hover`)}
                onPointerLeave={() => lending() && lend(index(), null)}
                onBlur={() => lending() && lend(index(), null)}
                onMouseDown={(e) => {
                  item.onMouseDown(e)
                  if (e.button === 0 && !e.ctrlKey) select(key)
                  else e.preventDefault()
                }}
                onKeyDown={(e) => {
                  item.onKeyDown(e)
                  if (e.key === ` ` || e.key === `Enter`) select(key)
                }}
                onFocus={(e) => {
                  item.onFocus()
                  if (lending() && e.currentTarget.matches(`:focus-visible`)) lend(index(), `focus`)
                  if (!selected()) select(key)
                }}
              >
                <span class="xui-Tabs-tab-body">
                  {tab().icon ? <IconGlyph icons={ctx.host.icons} name={tab().icon!} class="xui-icon" width={16} height={16} /> : null}
                  <span>{str(tab().label)}</span>
                  {tab().count !== undefined ? <span data-count="">{ctx.formatter.number(num(tab().count))}</span> : null}
                </span>
                <span {...part(`indicator`, selected() && `selected`)} aria-hidden="true" />
              </button>
            )
            return (
              <>
                {trigger}
                <Show when={closable()}>
                  <button
                    type="button"
                    {...part.at(`close`, index(), lent()?.index === index() && lent()!.state)}
                    aria-label={ctx.t(`closeTab`, { name: str(tab().label) })}
                    onPointerEnter={() => lending() && lend(index(), `hover`)}
                    onPointerLeave={() => lending() && lend(index(), null)}
                    onClick={() => void props.emit(`close`, { value: key })}
                  >
                    <BuiltinIcon slot="Tabs.close" />
                  </button>
                </Show>
              </>
            )
          }}
        </For>
        <Show when={bool(props.props.addable)}>
          <button type="button" {...part(`new`)} aria-label={ctx.t(`newTab`)} onClick={() => void props.emit(`new`)}>
            <BuiltinIcon slot="Tabs.new" />
          </button>
        </Show>
      </div>
      <For each={keys()}>
        {(key, index) => {
          const child = childAt(() => props.node, index)
          const present = () => key === value()
          // Only the ACTIVE panel is placed (`<id>.content`); the others are hidden.
          const attrs = () => {
            const a = part(`content`)
            if (!present()) delete a[`data-xui-id`]
            return a
          }
          const styled = key === initial
          const style = () => {
            value()
            return styled ? (mountPrevented ? { "animation-duration": `0s` } : {}) : undefined
          }
          return (
            <div data-state={present() ? `active` : `inactive`} data-orientation="horizontal" role="tabpanel" aria-labelledby={triggerId(key)} hidden={!present()} id={contentId(key)} tabindex={0} {...attrs()} style={style()}>
              <Show when={present() && child()}>
                <NodeView node={child()!} />
              </Show>
            </div>
          )
        }}
      </For>
    </div>
  )
}

// ── Segmented ───────────────────────────────────────────────────────────

interface Item {
  label: string
  value: string
  icon?: string
  disabled?: boolean
}

const joinLike = (raw: unknown, next: string[]): string | string[] => (Array.isArray(raw) ? next : next.join(`,`))
const asArray = (v: string | string[]): string[] => (Array.isArray(v) ? v : v ? v.split(`,`) : [])

/** Segmented (round 3): ONE row of segments.
 *  `segmented` (default, the pill track) / `toggles` (bare) / `outline`
 *  (joined) = Radix's ToggleGroup DOM (radiogroup single, toolbar of
 *  toggle buttons multiple; roving tab stop, arrows wrap, Home/End). `bar`
 *  = the full-width bottom destinations: role navigation, each item a
 *  column button (icon above a caption label), the selected one
 *  `aria-current="page"`, the same roving keys. Parts: root, item, icon,
 *  label (the selected state rides every part of the selected item). */
export function SegmentedNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const items = createMemo(() => objects<Item>(props.props.items))
  const variant = () => str(props.props.variant, `segmented`)
  const bar = () => variant() === `bar`
  const multiple = () => !bar() && props.props.type === `multiple`
  const external = (): string | string[] => {
    const raw = props.props.value
    return multiple() ? (Array.isArray(raw) ? raw.map(String) : raw ? String(raw).split(`,`) : []) : str(raw)
  }
  const [value, setBound] = createBoundState<string | string[]>(() => props.node, () => props.scope, `value`, external)
  const setValue = (next: string | string[]) => setBound(Array.isArray(next) ? (joinLike(untrack(() => props.props.value), next) as string[]) : next)
  const isSelected = (item: Item) => (multiple() ? asArray(value()).includes(item.value) : value() === item.value)
  const content = (item: Accessor<Item>, selected: Accessor<boolean>) => (
    <>
      {item().icon ? (
        <span {...part(`icon`, selected() && `selected`)} aria-hidden="true">
          <IconGlyph icons={ctx.host.icons} name={item().icon!} class="xui-icon" width={bar() ? 20 : 16} height={bar() ? 20 : 16} />
        </span>
      ) : null}
      {item().label ? <span {...part(`label`, selected() && `selected`)}>{str(item().label)}</span> : null}
    </>
  )
  const keys = keysOf(items, (i) => i.value)
  let root: HTMLDivElement | undefined
  const roving = createRovingGroup({ dir: () => ctx.direction, loop: true, items: () => (root ? Array.from(root.querySelectorAll<HTMLElement>(`:scope > button`)) : []), focusable: (el) => !(el as HTMLButtonElement).disabled, active: (el) => el.getAttribute(`data-state`) === `on` })
  const press = (item: Item) => {
    const raw = untrack(() => props.props.value)
    if (multiple()) {
      const cur = asArray(untrack(value))
      const next = cur.includes(item.value) ? cur.filter((v) => v !== item.value) : [...cur, item.value]
      setValue(next)
      void props.emit(`change`, { value: joinLike(raw, next) }, { value: joinLike(raw, next) })
      return
    }
    // Radix single: pressing the pressed item asks for `` (ignored).
    if (untrack(value) === item.value) return
    setValue(item.value)
    void props.emit(`change`, { value: item.value }, { value: item.value })
  }
  return (
    <Show
      when={bar()}
      fallback={
        <div ref={root} {...roving.group()} role="group" {...props.rootProps} dir={ctx.direction} style={{ outline: `none`, ...((props.rootProps.style as Record<string, string> | undefined) ?? {}) }} {...roving.groupHandlers}>
          <For each={keys()}>
            {(key, index) => {
              const item = createMemo(() => items()[index()]!)
              const selected = () => isSelected(item())
              const disabled = () => bool(item().disabled)
              let el: HTMLButtonElement | undefined
              const r = roving.item(() => el, () => !disabled())
              return (
                <button
                  ref={el}
                  type="button"
                  role={multiple() ? undefined : `radio`}
                  aria-checked={multiple() ? undefined : selected()}
                  aria-pressed={multiple() ? selected() : undefined}
                  data-state={selected() ? `on` : `off`}
                  data-disabled={disabled() ? `` : undefined}
                  disabled={disabled()}
                  aria-label={item().label ? undefined : key}
                  {...part(`item`, selected() && `selected`, disabled() && `disabled`)}
                  tabindex={r.tabindex()}
                  data-radix-collection-item=""
                  onMouseDown={r.onMouseDown}
                  onFocus={r.onFocus}
                  onKeyDown={r.onKeyDown}
                  onClick={() => {
                    if (!disabled()) press(item())
                  }}
                >
                  {content(item, selected)}
                </button>
              )
            }}
          </For>
        </div>
      }
    >
      <SegmentedBar rootProps={props.rootProps} items={items} keys={keys} value={() => value() as string} part={part} content={content} onPick={(next) => {
        setValue(next)
        void props.emit(`change`, { value: next }, { value: next })
      }} />
    </Show>
  )
}

/** The `bar` variant: a navigation landmark of buttons with ONE roving tab
 *  stop (the selected item, else the first enabled one). */
function SegmentedBar(props: { rootProps: Record<string, unknown>; items: Accessor<Item[]>; keys: Accessor<string[]>; value: Accessor<string>; part: PartFn; content: (item: Accessor<Item>, selected: Accessor<boolean>) => JSX.Element; onPick: (value: string) => void }): JSX.Element {
  const ctx = useSurfaceContext()
  const refs: (HTMLButtonElement | undefined)[] = []
  const enabled = createMemo(() => props.items().map((item, i) => (bool(item.disabled) ? -1 : i)).filter((i) => i >= 0))
  const selectedIndex = () => props.items().findIndex((item) => item.value === props.value() && !bool(item.disabled))
  const [focusIndex, setFocusIndex] = createSignal<number | null>(null)
  const stop = () => focusIndex() ?? (selectedIndex() >= 0 ? selectedIndex() : (enabled()[0] ?? -1))
  const move = (from: number, key: string) => {
    const en = enabled()
    if (!en.length) return
    const at = en.indexOf(from)
    const rtl = ctx.direction === `rtl`
    const forward = key === `ArrowDown` || key === (rtl ? `ArrowLeft` : `ArrowRight`)
    const back = key === `ArrowUp` || key === (rtl ? `ArrowRight` : `ArrowLeft`)
    let next: number | null | undefined = null
    if (forward) next = en[(at + 1) % en.length]
    else if (back) next = en[(at - 1 + en.length) % en.length]
    else if (key === `Home`) next = en[0]
    else if (key === `End`) next = en[en.length - 1]
    if (next === null || next === undefined) return false
    setFocusIndex(next)
    refs[next]?.focus()
    return true
  }
  return (
    <nav {...props.rootProps} dir={ctx.direction}>
      <For each={props.keys()}>
        {(key, index) => {
          const item = createMemo(() => props.items()[index()]!)
          const selected = () => key === props.value()
          const disabled = () => bool(item().disabled)
          return (
            <button
              ref={(el) => void (refs[index()] = el)}
              type="button"
              {...props.part(`item`, selected() && `selected`, disabled() && `disabled`)}
              disabled={disabled()}
              tabindex={index() === stop() ? 0 : -1}
              aria-current={selected() ? `page` : undefined}
              aria-label={item().label ? undefined : key}
              onFocus={() => setFocusIndex(index())}
              onKeyDown={(e) => {
                if (move(index(), e.key)) e.preventDefault()
              }}
              onClick={() => props.onPick(key)}
            >
              {props.content(item, selected)}
            </button>
          )
        }}
      </For>
    </nav>
  )
}

// ── Accordion ───────────────────────────────────────────────────────────

interface AccordionItem {
  title: string
  value: string
  count?: number
}

const ACCORDION_KEYS = new Set([`Home`, `End`, `ArrowDown`, `ArrowUp`, `ArrowLeft`, `ArrowRight`])

export function AccordionNative(props: NativeProps): JSX.Element {
  const part = parts(props)
  const items = createMemo(() => objects<AccordionItem>(props.props.items))
  const multiple = () => props.props.type === `multiple`
  const external = (): string | string[] => {
    const raw = props.props.value
    return multiple() ? (Array.isArray(raw) ? raw.map(String) : raw ? String(raw).split(`,`) : []) : str(raw)
  }
  const [value, setBound] = createBoundState<string | string[]>(() => props.node, () => props.scope, `value`, external)
  const setValue = (next: string | string[]) => setBound(Array.isArray(next) ? (joinLike(untrack(() => props.props.value), next) as string[]) : next)
  const isOpen = (v: string) => (multiple() ? asArray(value()).includes(v) : value() === v)
  const toggle = (v: string) => {
    const raw = untrack(() => props.props.value)
    if (multiple()) {
      const cur = asArray(untrack(value))
      const next = cur.includes(v) ? cur.filter((x) => x !== v) : [...cur, v]
      setValue(next)
      void props.emit(`change`, { value: joinLike(raw, next) }, { value: joinLike(raw, next) })
      return
    }
    // Single + collapsible: the open item closes to ``.
    const next = untrack(value) === v ? `` : v
    setValue(next)
    void props.emit(`change`, { value: next }, { value: next })
  }
  let root: HTMLDivElement | undefined
  // Radix: the arrows move between the (enabled) triggers, wrapping.
  const onKeyDown = (e: KeyboardEvent) => {
    if (!ACCORDION_KEYS.has(e.key) || !root) return
    const triggers = Array.from(root.querySelectorAll<HTMLButtonElement>(`:scope > div > div > button[data-radix-collection-item]`)).filter((b) => !b.disabled)
    const at = triggers.indexOf(e.target as HTMLButtonElement)
    if (at === -1) return
    e.preventDefault()
    const count = triggers.length
    let next = at
    if (e.key === `Home`) next = 0
    else if (e.key === `End`) next = count - 1
    else if (e.key === `ArrowDown`) next = at + 1 > count - 1 ? 0 : at + 1
    else if (e.key === `ArrowUp`) next = at - 1 < 0 ? count - 1 : at - 1
    triggers[next % count]?.focus()
  }
  const keys = keysOf(items, (i) => i.value)
  // Radix's Accordion root drops `dir` (it reads it, never renders it).
  const rootAttrs = () => {
    const { dir: _dir, ...rest } = props.rootProps
    return rest
  }
  return (
    <div ref={root} {...rootAttrs()} data-orientation="vertical" onKeyDown={onKeyDown}>
      <For each={keys()}>
        {(key, index) => {
          const item = createMemo(() => items()[index()]!)
          return <AccordionRow part={part} index={index} item={item} open={() => isOpen(key)} child={childAt(() => props.node, index)} onToggle={() => toggle(key)} />
        }}
      </For>
    </div>
  )
}

/** One Radix Accordion.Item (a Collapsible): item, header, trigger, content.
 *  The content reproduces Radix's CollapsibleContent: it stays painted for
 *  the frame that measures it while closing, `hidden` (and empty) when
 *  closed, and its style carries Radix's variables in Radix's order (the
 *  measured `--radix-collapsible-content-*` appended by the render after a
 *  toggle, and the mount measurement by the render after mount;
 *  `transition-duration: 0s; animation-name: none` left in place on an item
 *  open at mount). */
function AccordionRow(props: { part: PartFn; index: Accessor<number>; item: Accessor<AccordionItem>; open: Accessor<boolean>; child: Accessor<UiNode | undefined>; onToggle: () => void }): JSX.Element {
  const ctx = useSurfaceContext()
  const triggerId = createUniqueId()
  const contentId = createUniqueId()
  const [present, setPresent] = createSignal(untrack(props.open))
  const shown = () => props.open() || present()
  const [dims, setDims] = createSignal<{ h: number; w: number }>({ h: 0, w: 0 })
  let content: HTMLDivElement | undefined
  let original: { transitionDuration: string; animationName: string } | null = null
  let measured = { h: 0, w: 0 }
  let mountPrevented = untrack(shown)
  const measure = () => {
    const node = content
    if (!node) return
    original ??= { transitionDuration: node.style.transitionDuration, animationName: node.style.animationName }
    node.style.transitionDuration = `0s`
    node.style.animationName = `none`
    const rect = node.getBoundingClientRect()
    measured = { h: rect.height, w: rect.width }
    if (!mountPrevented) {
      node.style.transitionDuration = original.transitionDuration
      node.style.animationName = original.animationName
    }
  }
  // Radix Presence: a closing content stays painted while its exit
  // animation (`xui-accordion-close`) runs, until its `animationend`.
  let prevAnimation = `none`
  let waiting: (() => void) | null = null
  const animationName = () => (content ? getComputedStyle(content).animationName || `none` : `none`)
  const unmount = () => {
    measure()
    if (untrack(present)) {
      setPresent(false)
      setDims(measured)
    }
  }
  onMount(() => {
    measure()
    // React's surface re-renders once after mount (its measured box): that
    // render writes the mount measurement.
    setDims(measured)
    prevAnimation = untrack(shown) ? animationName() : `none`
    const onStart = (e: AnimationEvent) => {
      if (e.target === content) prevAnimation = animationName()
    }
    content?.addEventListener(`animationstart`, onStart)
    const raf = requestAnimationFrame(() => (mountPrevented = false))
    onCleanup(() => {
      cancelAnimationFrame(raf)
      content?.removeEventListener(`animationstart`, onStart)
      waiting?.()
    })
  })
  createEffect(
    on(
      props.open,
      (o) => {
        waiting?.()
        measure()
        if (o) {
          if (!untrack(present)) {
            setPresent(true)
            setDims(measured)
          }
          prevAnimation = animationName()
          return
        }
        const node = content
        const name = animationName()
        if (!node || name === `none` || getComputedStyle(node).display === `none` || prevAnimation === name) return unmount()
        const done = (e: AnimationEvent) => {
          if (e.target !== node || !animationName().includes(CSS.escape(e.animationName))) return
          waiting?.()
          prevAnimation = `none`
          unmount()
        }
        node.addEventListener(`animationend`, done)
        node.addEventListener(`animationcancel`, done)
        waiting = () => {
          node.removeEventListener(`animationend`, done)
          node.removeEventListener(`animationcancel`, done)
          waiting = null
        }
      },
      { defer: true }
    )
  )
  const state = () => (props.open() ? `open` : `closed`)
  const style = () => {
    const d = dims()
    const out: Record<string, string> = {}
    if (d.h) out[`--radix-collapsible-content-height`] = `${d.h}px`
    if (d.w) out[`--radix-collapsible-content-width`] = `${d.w}px`
    out[`--radix-accordion-content-height`] = `var(--radix-collapsible-content-height)`
    out[`--radix-accordion-content-width`] = `var(--radix-collapsible-content-width)`
    return out
  }
  return (
    <div data-orientation="vertical" {...props.part.at(`item`, props.index(), props.open() && `open`)} data-state={state()}>
      <div data-orientation="vertical" data-state={state()} style={css({ display: `flex` })}>
        <button
          type="button"
          aria-controls={contentId}
          aria-expanded={props.open()}
          data-state={state()}
          {...props.part.at(`trigger`, props.index(), props.open() && `open`)}
          data-orientation="vertical"
          id={triggerId}
          data-radix-collection-item=""
          onClick={() => props.onToggle()}
        >
          <span class="xui-accordion-title">
            {str(props.item().title)}
            {/* Round 2 §7: the count is its own muted part after the title. */}
            {props.item().count !== undefined ? <span {...props.part.at(`count`, props.index())}>{ctx.formatter.number(num(props.item().count))}</span> : null}
          </span>
          <BuiltinIcon slot="Accordion.trigger" size={16} />
        </button>
      </div>
      <div ref={content} data-state={state()} id={contentId} hidden={!shown()} role="region" aria-labelledby={triggerId} data-orientation="vertical" {...props.part.at(`content`, props.index(), props.open() && `open`)} style={style()}>
        <Show when={shown() && props.child()}>
          <NodeView node={props.child()!} />
        </Show>
      </div>
    </div>
  )
}

export const NATIVES_NAV: Record<string, Component<NativeProps>> = {
  Tabs: TabsNative,
  Segmented: SegmentedNative,
  Accordion: AccordionNative,
}
