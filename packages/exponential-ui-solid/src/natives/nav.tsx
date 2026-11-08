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
// `focusFirst` in a timeout, `createRovingGroup` in ../behaviors; Accordion:
// the root's arrow handler here, the content = `createCollapsibleContent`).

import { For, Show, batch, createEffect, createMemo, createSignal, createUniqueId, onCleanup, onMount, untrack, type Accessor, type Component, type JSX } from "solid-js"
import type { UiNode } from "@exponential_app/eui"
import { useSurfaceContext } from "../context"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { NodeView } from "../node-view"
import { createBoundState } from "./bound"
import { bool, BuiltinIcon, css, num, objects, parts, str, type PartFn } from "./shared"
import { createCollapsibleContent, createRovingGroup } from "../behaviors"
import { isReorderKey, sameSpans, segmentedBadge, startStripDrag, startTabSwipe, stripFit, stripKeyMove, tabPage, tabPageStyle, tabSpans, tabThumbStyle, type TabSpan, type TabSwipeDrag } from "@exponential_app/eui/web"

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

/** FEED-200 (React's `useTabSwipe`): a `swipeable` Tabs' pager
 *  (`@exponential_app/eui/web` tabs-swipe, the core's `tab_pages` /
 *  `tab_thumb` / `tab_swipe` twin): the live drag, each header's span
 *  (measured after every change and resize) and the `thumb` they place; a
 *  settle selects like a header press. */
function createTabSwipe(o: { on: Accessor<boolean>; tabs: Accessor<Tab[]>; value: Accessor<string>; rtl: Accessor<boolean>; select: (value: string) => void; header: (index: number) => HTMLElement | undefined; layout?: () => unknown }) {
  const [drag, setDrag] = createSignal<TabSwipeDrag | null>(null)
  const [spans, setSpans] = createSignal<(TabSpan | null)[] | null>(null)
  let list: HTMLElement | undefined
  const active = createMemo(() => Math.max(0, o.tabs().findIndex((t) => t.value === o.value())))
  const measure = () => {
    if (!untrack(o.on)) return
    const next = tabSpans(untrack(o.tabs).map((_, i) => o.header(i)))
    if (!sameSpans(untrack(spans), next)) setSpans(next)
  }
  createEffect(() => {
    o.on()
    o.tabs()
    o.value()
    o.layout?.()
    measure()
  })
  onMount(() => {
    if (!list || typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(() => measure())
    observer.observe(list)
    onCleanup(() => observer.disconnect())
  })
  const thumb = createMemo(() => {
    const s = spans()
    return o.on() && s ? tabThumbStyle(s, active(), drag(), o.rtl()) : null
  })
  return {
    on: o.on,
    drag,
    thumb,
    active,
    list: (el: HTMLElement) => void (list = el),
    pointerDown: (e: PointerEvent, pager: HTMLElement) =>
      startTabSwipe(e, pager, () => ({ index: untrack(active), count: untrack(o.tabs).length, rtl: untrack(o.rtl) }), {
        drag: (d) => setDrag(d),
        settle: (i) => {
          const next = untrack(o.tabs)[i]?.value
          if (next !== undefined) batch(() => o.select(next))
        },
      }),
  }
}

type TabSwipe = ReturnType<typeof createTabSwipe>

/** The `thumb` wears the selected header's recipe (`tab`, `item` on a
 *  strip) so every theme paints it like that header. */
const thumbAttrs = (part: PartFn, header: `tab` | `item`) => {
  const attrs = part(`thumb`, `selected`)
  return { ...attrs, class: String(attrs.class).replace(`xui-Tabs-thumb`, `xui-Tabs-${header} xui-Tabs-thumb`) }
}

/** The panels (React's `TabPanels`): only the ACTIVE one is placed
 *  (`<id>.content`); the others are hidden, but a swipeable Tabs draws the
 *  active one's neighbours (inert, out of flow) inside the clipping
 *  `pager`. Radix: the panel active at MOUNT renders
 *  `animation-duration: 0s` until the first render after its next frame. */
function tabPanels(o: { node: () => UiNode; keys: Accessor<string[]>; value: Accessor<string>; part: PartFn; swipe: TabSwipe; rtl: Accessor<boolean>; triggerId: (v: string) => string; contentId: (v: string) => string; initial: string; mountPrevented: () => boolean }): JSX.Element {
  const panels = () => (
    <For each={o.keys()}>
      {(key, index) => {
        const child = childAt(o.node, index)
        const present = () => key === o.value()
        const side = () => (o.swipe.on() ? tabPage(index(), o.swipe.active()) : null)
        const shown = () => present() || side() !== null
        const attrs = () => {
          const a = o.part(`content`)
          if (!present()) delete a[`data-xui-id`]
          return a
        }
        const styled = key === o.initial
        const style = () => {
          o.value()
          const mount = styled ? (o.mountPrevented() ? { "animation-duration": `0s` } : {}) : undefined
          const s = side()
          return s === null ? mount : { ...tabPageStyle(s, o.swipe.drag(), o.rtl()), ...mount }
        }
        return (
          <div
            data-state={present() ? `active` : `inactive`}
            data-orientation="horizontal"
            role="tabpanel"
            aria-labelledby={o.triggerId(key)}
            hidden={!shown()}
            id={o.contentId(key)}
            tabindex={0}
            {...attrs()}
            data-xui-page={side() === -1 ? `prev` : side() === 1 ? `next` : undefined}
            aria-hidden={side() ? `true` : undefined}
            attr:inert={side() ? `` : undefined}
            style={style()}
          >
            <Show when={shown() && child()}>
              <NodeView node={child()!} />
            </Show>
          </div>
        )
      }}
    </For>
  )
  return (
    <>
      {o.swipe.on() ? (
        <div class="xui-Tabs-pager" onPointerDown={(e) => o.swipe.pointerDown(e, e.currentTarget)}>
          {panels()}
        </div>
      ) : (
        panels()
      )}
    </>
  )
}

// ── Tabs ────────────────────────────────────────────────────────────────

export function TabsNative(props: NativeProps): JSX.Element {
  return (
    <Show when={props.props.variant === `strip`} fallback={<TabsPills {...props} />}>
      <TabsStrip {...props} />
    </Show>
  )
}

/** VAPP-279 (React's `useTabStrip` + strip branch): the `strip` header row
 *  — per tab an `item` (the chip) holding the trigger and its close, the
 *  folded tabs laid out out of flow (`data-folded`), the `+N` `more`
 *  button, the + — with the fit (`stripFit` after every change and resize),
 *  the reorder drag (`startStripDrag`) and the Ctrl/Cmd+Shift+Arrow move. */
function TabsStrip(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const tabs = createMemo(() => objects<Tab>(props.props.tabs))
  const external = () => str(props.props.value) || tabs()[0]?.value || ``
  const [value, setValue] = createBoundState(() => props.node, () => props.scope, `value`, external)
  const closable = () => props.props.closable === `hover` || props.props.closable === `always`
  const [lent, setLent] = createSignal<{ index: number; state: `hover` | `focus` } | null>(null)
  const lend = (index: number, state: `hover` | `focus` | null) => setLent((cur) => (state ? { index, state } : cur?.index === index ? null : cur))
  const lending = () => closable() && props.props.closable === `hover`
  const rtl = () => ctx.direction === `rtl`
  const baseId = createUniqueId()
  const triggerId = (v: string) => `${baseId}-trigger-${v}`
  const contentId = (v: string) => `${baseId}-content-${v}`
  const [preview, setPreview] = createSignal<number[] | null>(null)
  const [fit, setFit] = createSignal<number[] | null>(null)
  const order = createMemo(() => {
    const p = preview()
    return p && p.length === tabs().length ? p : tabs().map((_, i) => i)
  }, undefined, { equals: (a, b) => a.length === b.length && a.every((x, k) => x === b[k]) })
  const shown = (i: number) => {
    const f = fit()
    return f === null || f.includes(i)
  }
  const slots = () => order().filter(shown)
  const hidden = () => tabs().map((_, i) => i).filter((i) => !shown(i))
  const items = new Map<number, HTMLElement>()
  let list: HTMLDivElement | undefined
  let more: HTMLButtonElement | undefined
  let add: HTMLButtonElement | undefined
  const triggers = () => (list ? Array.from(list.querySelectorAll<HTMLElement>(`[role="tab"]`)) : [])
  const roving = createRovingGroup({ orientation: `horizontal`, dir: () => ctx.direction, loop: true, items: triggers, focusable: (el) => !(el as HTMLButtonElement).disabled, active: (el) => el.getAttribute(`aria-selected`) === `true` })
  const select = (next: string) => {
    if (next === untrack(value)) return
    setValue(next)
    void props.emit(`change`, { value: next }, { value: next })
  }
  const refit = () => {
    if (!list || untrack(preview)) return
    const o = untrack(order)
    const els = o.map((i) => items.get(i)).filter((x): x is HTMLElement => Boolean(x))
    if (els.length !== o.length) return
    const active = o.findIndex((i) => untrack(tabs)[i]?.value === untrack(value))
    const visible = stripFit(list, els, more ?? null, add ?? null, active < 0 ? null : active).map((k) => o[k]!).sort((a, b) => a - b)
    const next = visible.length === o.length ? null : visible
    const cur = untrack(fit)
    if (cur === next || (cur && next && cur.length === next.length && cur.every((x, k) => x === next[k]))) return
    setFit(next)
  }
  createEffect(() => {
    tabs()
    value()
    order()
    fit()
    refit()
  })
  onMount(() => {
    if (!list || typeof ResizeObserver === `undefined`) return
    const observer = new ResizeObserver(() => refit())
    observer.observe(list)
    onCleanup(() => observer.disconnect())
  })
  const fire = (move: { from: number; to: number } | null) => {
    if (move) void props.emit(`reorder`, { from: move.from, to: move.to, value: untrack(tabs)[move.from]?.value ?? null })
  }
  const keys = keysOf(tabs, (t) => t.value)
  const swipe = createTabSwipe({ on: () => bool(props.props.swipeable), tabs, value, rtl, select, header: (i) => items.get(i), layout: () => [order(), fit()] })
  const initial = untrack(value)
  let mountPrevented = true
  onMount(() => {
    const raf = requestAnimationFrame(() => (mountPrevented = false))
    onCleanup(() => cancelAnimationFrame(raf))
  })
  return (
    <div {...props.rootProps} dir={ctx.direction} data-orientation="horizontal">
      <div
        ref={(el) => {
          list = el
          swipe.list(el)
        }}
        role="tablist"
        aria-orientation="horizontal"
        {...part(`list`)}
        data-xui-swipe={swipe.on() ? `` : undefined}
        {...roving.group()}
        style={{ outline: `none` }}
        {...roving.groupHandlers}
      >
        <Show when={swipe.thumb()}>{(style) => <div {...thumbAttrs(part, `item`)} aria-hidden="true" style={style()} />}</Show>
        <For each={order()}>
          {(i) => {
            const tab = createMemo(() => tabs()[i]!)
            const key = () => tab().value
            const selected = () => key() === value()
            const folded = () => !shown(i)
            let el: HTMLButtonElement | undefined
            const item = roving.item(() => el, () => !folded())
            return (
              <div ref={(node) => items.set(i, node)} {...part.at(`item`, i, selected() && `selected`)} data-folded={folded() ? `` : undefined} style={selected() && swipe.thumb() ? { "background-color": `transparent` } : undefined}>
                <button
                  ref={el}
                  type="button"
                  role="tab"
                  aria-selected={selected()}
                  aria-controls={contentId(key())}
                  data-state={selected() ? `active` : `inactive`}
                  data-disabled={folded() ? `` : undefined}
                  disabled={folded()}
                  id={triggerId(key())}
                  {...part.at(`tab`, i, selected() && `selected`)}
                  tabindex={item.tabindex()}
                  data-orientation="horizontal"
                  data-radix-collection-item=""
                  onPointerEnter={() => lending() && lend(i, `hover`)}
                  onPointerLeave={() => lending() && lend(i, null)}
                  onBlur={() => lending() && lend(i, null)}
                  onPointerDown={(e) => startStripDrag(e, e.currentTarget, i, () => ({ order: untrack(order), slots: untrack(slots), item: (k) => items.get(k) ?? null, rtl: untrack(rtl) }), { preview: setPreview, drop: fire })}
                  onMouseDown={(e) => {
                    item.onMouseDown(e)
                    if (folded()) return
                    if (e.button === 0 && !e.ctrlKey) select(key())
                    else e.preventDefault()
                  }}
                  onKeyDown={(e) => {
                    if (isReorderKey(e)) {
                      e.preventDefault()
                      e.stopPropagation()
                      fire(stripKeyMove(untrack(slots), i, e.key, untrack(rtl)))
                      return
                    }
                    item.onKeyDown(e)
                    if (e.key === ` ` || e.key === `Enter`) select(key())
                  }}
                  onFocus={(e) => {
                    item.onFocus()
                    if (lending() && e.currentTarget.matches(`:focus-visible`)) lend(i, `focus`)
                    if (!selected()) select(key())
                  }}
                >
                  <span class="xui-Tabs-tab-body">
                    {tab().icon ? <IconGlyph icons={ctx.host.icons} name={tab().icon!} class="xui-icon" width={16} height={16} /> : null}
                    <span>{str(tab().label)}</span>
                    {tab().count !== undefined ? <span data-count="">{ctx.formatter.number(num(tab().count))}</span> : null}
                  </span>
                  <span {...part(`indicator`, selected() && `selected`)} aria-hidden="true" />
                </button>
                <Show when={closable()}>
                  <button
                    type="button"
                    {...part.at(`close`, i, lent()?.index === i && lent()!.state)}
                    aria-label={ctx.t(`closeTab`, { name: str(tab().label) })}
                    tabindex={folded() ? -1 : undefined}
                    onPointerEnter={() => lending() && lend(i, `hover`)}
                    onPointerLeave={() => lending() && lend(i, null)}
                    onClick={() => void props.emit(`close`, { value: key() })}
                  >
                    <BuiltinIcon slot="Tabs.close" />
                  </button>
                </Show>
              </div>
            )
          }}
        </For>
        <button
          type="button"
          ref={more}
          {...part(`more`)}
          data-folded={hidden().length === 0 ? `` : undefined}
          tabindex={hidden().length === 0 ? -1 : undefined}
          aria-label={ctx.t(`moreTabs`, { count: String(hidden().length) })}
          onClick={() => void props.emit(`more`, { values: hidden().map((i) => tabs()[i]!.value) })}
        >
          {ctx.t(`morePictures`, { count: String(hidden().length) })}
        </button>
        <Show when={bool(props.props.addable)}>
          <button type="button" ref={add} {...part(`new`)} aria-label={ctx.t(`newTab`)} onClick={() => void props.emit(`new`)}>
            <BuiltinIcon slot="Tabs.new" />
          </button>
        </Show>
      </div>
      {tabPanels({ node: () => props.node, keys, value, part, swipe, rtl, triggerId, contentId, initial, mountPrevented: () => mountPrevented })}
    </div>
  )
}

function TabsPills(props: NativeProps): JSX.Element {
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
  const heads = new Map<string, HTMLElement>()
  const rtl = () => ctx.direction === `rtl`
  const swipe = createTabSwipe({ on: () => bool(props.props.swipeable), tabs, value, rtl, select, header: (i) => heads.get(untrack(tabs)[i]?.value ?? ``) })
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
      <div
        ref={(el) => {
          list = el
          swipe.list(el)
        }}
        role="tablist"
        aria-orientation="horizontal"
        {...part(`list`)}
        data-fill={fill() ? `true` : undefined}
        data-xui-swipe={swipe.on() ? `` : undefined}
        {...roving.group()}
        style={{ outline: `none` }}
        {...roving.groupHandlers}
      >
        <Show when={swipe.thumb()}>{(style) => <div {...thumbAttrs(part, `tab`)} aria-hidden="true" style={style()} />}</Show>
        <For each={keys()}>
          {(key, index) => {
            const tab = createMemo(() => tabs()[index()]!)
            const selected = () => key === value()
            let el: HTMLButtonElement | undefined
            const item = roving.item(() => el, () => true)
            const trigger = (
              <button
                ref={(node) => {
                  el = node
                  heads.set(key, node)
                }}
                type="button"
                role="tab"
                aria-selected={selected()}
                aria-controls={contentId(key)}
                data-state={selected() ? `active` : `inactive`}
                id={triggerId(key)}
                {...part.at(`tab`, index(), selected() && `selected`)}
                style={selected() && swipe.thumb() ? { "background-color": `transparent`, "box-shadow": `none` } : undefined}
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
      {tabPanels({ node: () => props.node, keys, value, part, swipe, rtl, triggerId, contentId, initial, mountPrevented: () => mountPrevented })}
    </div>
  )
}

// ── Segmented ───────────────────────────────────────────────────────────

interface Item {
  label: string
  value: string
  icon?: string
  disabled?: boolean
  /** VAPP-162: a `bar` item's count badge. */
  count?: number
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
 *  label (the selected state rides every part of the selected item).
 *  VAPP-162 (round-5 contract §3): `orientation: vertical` stacks every
 *  variant top to bottom (ArrowUp/ArrowDown move focus); a vertical `bar`
 *  is THE navigation rail (72 px items). A `bar` item's `count` paints the
 *  `badge` at its icon's top-end corner (`99+` above 99, none at 0) and
 *  reads after the label (`Chats, 3`). */
export function SegmentedNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const items = createMemo(() => objects<Item>(props.props.items))
  const variant = () => str(props.props.variant, `segmented`)
  const bar = () => variant() === `bar`
  const vertical = () => props.props.orientation === `vertical`
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
          <Show when={bar() ? segmentedBadge(item().count) : null}>{(badge) => <span {...part(`badge`, selected() && `selected`)}>{badge()}</span>}</Show>
        </span>
      ) : null}
      {item().label ? <span {...part(`label`, selected() && `selected`)}>{str(item().label)}</span> : null}
    </>
  )
  const keys = keysOf(items, (i) => i.value)
  let root: HTMLDivElement | undefined
  const roving = createRovingGroup({ orientation: () => (vertical() ? `vertical` : undefined), dir: () => ctx.direction, loop: true, items: () => (root ? Array.from(root.querySelectorAll<HTMLElement>(`:scope > button`)) : []), focusable: (el) => !(el as HTMLButtonElement).disabled, active: (el) => el.getAttribute(`data-state`) === `on` })
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
                  data-orientation={vertical() ? `vertical` : undefined}
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
      <SegmentedBar rootProps={props.rootProps} vertical={vertical} items={items} keys={keys} value={() => value() as string} part={part} content={content} onPick={(next) => {
        setValue(next)
        void props.emit(`change`, { value: next }, { value: next })
      }} />
    </Show>
  )
}

/** The `bar` variant: a navigation landmark of buttons with ONE roving tab
 *  stop (the selected item, else the first enabled one). */
function SegmentedBar(props: { rootProps: Record<string, unknown>; vertical: Accessor<boolean>; items: Accessor<Item[]>; keys: Accessor<string[]>; value: Accessor<string>; part: PartFn; content: (item: Accessor<Item>, selected: Accessor<boolean>) => JSX.Element; onPick: (value: string) => void }): JSX.Element {
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
    const vertical = props.vertical()
    const forward = key === `ArrowDown` || (!vertical && key === (rtl ? `ArrowLeft` : `ArrowRight`))
    const back = key === `ArrowUp` || (!vertical && key === (rtl ? `ArrowRight` : `ArrowLeft`))
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
              aria-label={segmentedBadge(item().count) ? `${str(item().label) || item().value}, ${segmentedBadge(item().count)}` : item().label ? undefined : key}
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
  const collapsible = createCollapsibleContent(props.open, `accordion`)
  const shown = collapsible.shown
  const state = () => (props.open() ? `open` : `closed`)
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
      <div ref={collapsible.ref} data-state={state()} id={contentId} hidden={!shown()} role="region" aria-labelledby={triggerId} data-orientation="vertical" {...props.part.at(`content`, props.index(), props.open() && `open`)} style={collapsible.style()}>
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
