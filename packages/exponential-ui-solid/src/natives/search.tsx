// FEED-128 (React's natives/search.tsx; EXP-1259): the SearchList native
// and the searchable Menu submenu's body —
// packages/domain-contract/fixtures/search-list.json.
//
// ONE body: a `head` row (the `search` field with its glyph, the muted
// result `count` once a query is typed) over the `body`: a `list` of `item`
// rows (`icon`, mono `identifier`, `label`, the picked row's `check`) at
// most `maxRows` tall, scrolling under a bottom `fade` while rows sit
// below. The focus STAYS in the field: ↑/↓ move the highlighted row (no
// wrap, no animation; it scrolls into view), Enter or Tab pick it, a hover
// moves it, typing puts it back on the top row. Part ids mirror the core's
// layout tree (`<id>.head`, `<id>.item.<i>`, …).

import { For, Show, createEffect, createMemo, createSignal, createUniqueId, on, onCleanup, onMount, untrack, type Component, type JSX } from "solid-js"
import { useSurfaceContext } from "../context"
import { DEBOUNCE_MS } from "../inputs"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { createBoundState } from "./bound"
import { bool, BuiltinIcon, css, num, objects, parts, str, type Attrs, type PartFn } from "./shared"

export interface SearchItem {
  label: string
  value: string
  icon?: string
  identifier?: string
  disabled?: boolean
}

/** Does a row match the query (case-insensitive, label or identifier)?
 *  The core's `search_matches`. */
export function searchMatches(item: Pick<SearchItem, `label` | `identifier`>, query: string): boolean {
  const needle = query.trim().toLowerCase()
  return needle === `` || str(item.label).toLowerCase().includes(needle) || str(item.identifier).toLowerCase().includes(needle)
}

/** React's `SearchBodyProps` (live getters on Solid: never destructure). */
export interface SearchBodyProps {
  part: PartFn
  /** Whose body this is (a SearchList, or a Menu's searchable submenu). */
  owner: `SearchList` | `Menu`
  /** `""` for a SearchList, `.<i>` for submenu `i` (the core's suffix). */
  suffix: string
  /** The rows ALREADY filtered, each with its index in the full list. */
  rows: { index: number; item: SearchItem }[]
  query: string
  onQuery: (query: string) => void
  placeholder: string
  emptyText: string
  maxRows: number
  showCount: boolean
  picked?: string
  onPick: (item: SearchItem) => void
  autoFocus?: boolean
  /** ← with the caret at the start leaves the field's key to the menu (a
   *  searchable submenu closes back to its row). */
  leaveBubbles?: boolean
  /** The rows' recipe part (`item`) and the row id prefix. */
  rowPart?: (item: SearchItem, index: number, active: boolean) => Attrs
  renderRow?: (item: SearchItem, index: number) => JSX.Element
}

export function SearchBody(props: SearchBodyProps): JSX.Element {
  const ctx = useSurfaceContext()
  const listId = createUniqueId()
  const [active, setActive] = createSignal(0)
  let listEl: HTMLDivElement | undefined
  let inputEl: HTMLInputElement | undefined
  const [moreBelow, setMoreBelow] = createSignal(false)
  const at = createMemo(() => Math.min(active(), Math.max(0, props.rows.length - 1)))
  const single = (name: string): Attrs => {
    const attrs = props.part(name)
    if (props.suffix && attrs[`data-xui-id`]) attrs[`data-xui-id`] = `${attrs[`data-xui-id`] as string}${props.suffix}`
    return attrs
  }
  // The highlight goes back to the top row whenever the query changes.
  createEffect(on(() => props.query, () => setActive(0), { defer: true }))
  const measure = () => {
    const list = listEl
    if (list) setMoreBelow(list.scrollTop + list.clientHeight < list.scrollHeight - 1)
  }
  createEffect(
    on([() => props.rows.length, () => props.maxRows], () => {
      measure()
    })
  )
  // Keep the highlighted row inside the window (instant: keys never
  // animate). The list scrolls itself: scrollIntoView would scroll the page
  // to a list further down on mount.
  createEffect(
    on(at, () => {
      const list = listEl
      const row = list?.querySelector<HTMLElement>(`[data-active="true"]`)
      if (!list || !row) return
      const l = list.getBoundingClientRect()
      const r = row.getBoundingClientRect()
      if (r.top < l.top) list.scrollTop -= l.top - r.top
      else if (r.bottom > l.bottom) list.scrollTop += r.bottom - l.bottom
    })
  )
  onMount(() => {
    if (props.autoFocus) inputEl?.focus()
  })
  const pick = (i: number) => {
    const row = props.rows[i]
    if (row && !bool(row.item.disabled)) props.onPick(row.item)
  }
  const onKeyDown = (e: KeyboardEvent & { currentTarget: HTMLInputElement }) => {
    if (e.isComposing) return
    const count = props.rows.length
    if (e.key === `ArrowDown` || e.key === `ArrowUp`) {
      e.preventDefault()
      e.stopPropagation()
      setActive((a) => Math.max(0, Math.min(count - 1, a + (e.key === `ArrowDown` ? 1 : -1))))
    } else if (e.key === `Enter` || (e.key === `Tab` && !e.shiftKey && count > 0)) {
      e.preventDefault()
      e.stopPropagation()
      pick(untrack(at))
    } else if (props.leaveBubbles && e.key === (ctx.direction === `rtl` ? `ArrowRight` : `ArrowLeft`) && e.currentTarget.selectionStart === 0 && e.currentTarget.selectionEnd === 0) {
      // The submenu's own key handling closes it.
    } else if (e.key !== `Escape`) {
      // Typing stays in the field (a menu's typeahead never sees it).
      e.stopPropagation()
    }
  }
  const activeId = () => {
    const row = props.rows[at()]
    return row ? `${listId}-${row.index}` : undefined
  }
  // Rows keyed by their index in the full list (React's `key`).
  const indices = createMemo(() => props.rows.map((r) => r.index), undefined, { equals: (a, b) => a.length === b.length && a.every((x, i) => x === b[i]) })
  return (
    <>
      <div {...single(`head`)}>
        <div {...single(`search`)}>
          <BuiltinIcon slot="SearchList.search" />
          <input
            ref={inputEl}
            type="text"
            role="combobox"
            aria-expanded="true"
            aria-controls={listId}
            aria-activedescendant={activeId()}
            aria-autocomplete="list"
            value={props.query}
            attr:value={props.query}
            placeholder={props.placeholder}
            onInput={(e) => props.onQuery(e.currentTarget.value)}
            onKeyDown={onKeyDown}
          />
        </div>
        <Show when={props.showCount && props.query.trim() !== ``}>
          <span {...single(`count`)} role="status" aria-live="polite">
            {ctx.formatter.number(props.rows.length)}
          </span>
        </Show>
      </div>
      <div {...single(`body`)}>
        <div {...single(`list`)} ref={listEl} id={listId} role="listbox" onScroll={measure} style={css({ maxHeight: `calc(${props.maxRows} * var(--xui-control-row, 32px))` })}>
          <For each={indices()}>
            {(index) => {
              const pos = createMemo<number>((prev) => {
                const p = indices().indexOf(index)
                return p >= 0 ? p : prev!
              })
              const item = createMemo<SearchItem>((prev) => props.rows[pos()]?.item ?? prev!)
              const isActive = () => pos() === at()
              const disabled = () => bool(item().disabled)
              return (
                <div
                  id={`${listId}-${index}`}
                  role="option"
                  aria-selected={isActive()}
                  aria-disabled={disabled() || undefined}
                  data-active={isActive() ? `true` : undefined}
                  {...(props.rowPart ? props.rowPart(item(), index, isActive()) : props.part.at(`item`, `${props.suffix ? `${props.suffix.slice(1)}.` : ``}${index}`, isActive() && `selected`, disabled() && `disabled`))}
                  onPointerMove={() => pos() !== untrack(at) && setActive(untrack(pos))}
                  onPointerDown={(e) => e.preventDefault()}
                  onClick={() => pick(untrack(pos))}
                >
                  {props.renderRow ? (
                    props.renderRow(item(), index)
                  ) : (
                    <>
                      <Show when={item().icon}>
                        <span {...props.part.at(`icon`, index)} aria-hidden="true">
                          <IconGlyph icons={ctx.host.icons} name={item().icon!} />
                        </span>
                      </Show>
                      <Show when={str(item().identifier)}>
                        <span {...props.part.at(`identifier`, index)}>{str(item().identifier)}</span>
                      </Show>
                      <span {...props.part.at(`label`, index)}>{str(item().label)}</span>
                      <Show when={props.picked !== undefined && props.picked !== `` && item().value === props.picked}>
                        <span {...props.part.at(`check`, index)} aria-hidden="true">
                          <BuiltinIcon slot="SearchList.check" />
                        </span>
                      </Show>
                    </>
                  )}
                </div>
              )
            }}
          </For>
          <Show when={props.rows.length === 0}>
            <div {...single(`empty`)}>{props.emptyText}</div>
          </Show>
        </div>
        <Show when={moreBelow() && props.rows.length > props.maxRows}>
          <div {...single(`fade`)} aria-hidden="true" />
        </Show>
      </div>
    </>
  )
}

/** SearchList: the field filters the rows locally (label or identifier)
 *  unless the host searches itself (`on.search`, its rows arrive through
 *  the bound `items`); `search {query}` fires 150 ms after the last
 *  keystroke; a pick writes the bound `value` and fires `select {value}`. */
export function SearchListNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const items = createMemo(() => objects<SearchItem>(props.props.items))
  const [query, setQuery] = createBoundState(
    () => props.node,
    () => props.scope,
    `query`,
    () => str(props.props.query)
  )
  const [picked, setPicked] = createBoundState(
    () => props.node,
    () => props.scope,
    `value`,
    () => str(props.props.value)
  )
  const hostSearches = () => Boolean(props.node.on?.search)
  let typed = false
  createEffect(
    on(query, (q) => {
      if (!typed) return
      const t = setTimeout(() => void props.emit(`search`, { query: q }), DEBOUNCE_MS)
      onCleanup(() => clearTimeout(t))
    })
  )
  const rows = createMemo(() => items().map((item, index) => ({ index, item })).filter(({ item }) => hostSearches() || searchMatches(item, query())))
  const maxRows = () => Math.max(1, Math.floor(num(props.props.maxRows, 8)))
  return (
    <div {...props.rootProps}>
      <SearchBody
        part={part}
        owner="SearchList"
        suffix=""
        rows={rows()}
        query={query()}
        onQuery={(q) => {
          typed = true
          setQuery(q)
        }}
        placeholder={str(props.props.placeholder) || ctx.t(`search`)}
        emptyText={str(props.props.emptyText) || ctx.t(`noResults`)}
        maxRows={maxRows()}
        showCount={props.props.showCount === undefined ? true : bool(props.props.showCount)}
        picked={picked()}
        onPick={(item) => {
          setPicked(item.value)
          void props.emit(`select`, { value: item.value }, { value: item.value })
        }}
      />
    </div>
  )
}

export const NATIVES_SEARCH: Record<string, Component<NativeProps>> = {
  SearchList: SearchListNative,
}
