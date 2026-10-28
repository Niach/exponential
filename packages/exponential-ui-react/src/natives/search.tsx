// EXP-1259 (the UI batch): the SearchList native and the searchable Menu
// submenu's body — packages/domain-contract/fixtures/search-list.json.
//
// ONE body: a `head` row (the `search` field with its glyph, the muted
// result `count` once a query is typed) over the `body`: a `list` of `item`
// rows (`icon`, mono `identifier`, `label`, the picked row's `check`) at
// most `maxRows` tall, scrolling under a bottom `fade` while rows sit
// below. The focus STAYS in the field: ↑/↓ move the highlighted row (no
// wrap, no animation; it scrolls into view), Enter or Tab pick it, a hover
// moves it, typing puts it back on the top row. Part ids mirror the core's
// layout tree (`<id>.head`, `<id>.item.<i>`, …) so the conformance dump
// compares the same boxes.

import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react"
import { elementScale } from "@exponential_app/ui/web"
import { useSurfaceContext } from "../context"
import { DEBOUNCE_MS } from "../inputs"
import { IconGlyph } from "../icons"
import type { NativeProps } from "../node-view"
import { useBoundState } from "./bound"
import { bool, BuiltinIcon, num, objects, str, useParts, type PartFn } from "./shared"

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
  rowPart?: (item: SearchItem, index: number, active: boolean) => Record<string, string>
  renderRow?: (item: SearchItem, index: number) => ReactNode
}

export function SearchBody({ part, owner, suffix, rows, query, onQuery, placeholder, emptyText, maxRows, showCount, picked, onPick, autoFocus, leaveBubbles, rowPart, renderRow }: SearchBodyProps) {
  const ctx = useSurfaceContext()
  void owner
  const listId = useId()
  const [active, setActive] = useState(0)
  const listRef = useRef<HTMLDivElement | null>(null)
  const [moreBelow, setMoreBelow] = useState(false)
  const at = Math.min(active, Math.max(0, rows.length - 1))
  const single = (name: string) => {
    const attrs = part(name) as Record<string, string>
    if (suffix && attrs[`data-xui-id`]) attrs[`data-xui-id`] = `${attrs[`data-xui-id`]}${suffix}`
    return attrs
  }
  // The highlight goes back to the top row whenever the query changes.
  useEffect(() => setActive(0), [query])
  const measure = () => {
    const list = listRef.current
    if (list) setMoreBelow(list.scrollTop + list.clientHeight < list.scrollHeight - 1)
  }
  useLayoutEffect(measure, [rows.length, maxRows])
  // Keep the highlighted row inside the window (instant: keys never animate).
  // The list scrolls itself: scrollIntoView would scroll the page to a list
  // further down on mount.
  useLayoutEffect(() => {
    const list = listRef.current
    const row = list?.querySelector<HTMLElement>(`[data-active="true"]`)
    if (!list || !row) return
    const l = list.getBoundingClientRect()
    const r = row.getBoundingClientRect()
    // NEW-82: rect deltas are visual px, scrollTop is layout px.
    const sy = elementScale(list, l).y
    if (r.top < l.top) list.scrollTop -= (l.top - r.top) / sy
    else if (r.bottom > l.bottom) list.scrollTop += (r.bottom - l.bottom) / sy
  }, [at])
  const pick = (i: number) => {
    const row = rows[i]
    if (row && !bool(row.item.disabled)) onPick(row.item)
  }
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.nativeEvent.isComposing) return
    if (e.key === `ArrowDown` || e.key === `ArrowUp`) {
      e.preventDefault()
      e.stopPropagation()
      setActive((a) => Math.max(0, Math.min(rows.length - 1, a + (e.key === `ArrowDown` ? 1 : -1))))
    } else if (e.key === `Enter` || (e.key === `Tab` && !e.shiftKey && rows.length > 0)) {
      e.preventDefault()
      e.stopPropagation()
      pick(at)
    } else if (leaveBubbles && e.key === (ctx.direction === `rtl` ? `ArrowRight` : `ArrowLeft`) && e.currentTarget.selectionStart === 0 && e.currentTarget.selectionEnd === 0) {
      // The submenu's own key handling closes it.
    } else if (e.key !== `Escape`) {
      // Typing stays in the field (a menu's typeahead never sees it).
      e.stopPropagation()
    }
  }
  const activeId = rows[at] ? `${listId}-${rows[at].index}` : undefined
  return (
    <>
      <div {...single(`head`)}>
        <div {...single(`search`)}>
          <BuiltinIcon slot="SearchList.search" />
          <input
            type="text"
            role="combobox"
            aria-expanded="true"
            aria-controls={listId}
            aria-activedescendant={activeId}
            aria-autocomplete="list"
            autoFocus={autoFocus}
            value={query}
            placeholder={placeholder}
            onChange={(e) => onQuery(e.target.value)}
            onKeyDown={onKeyDown}
          />
        </div>
        {showCount && query.trim() !== `` ? (
          <span {...single(`count`)} role="status" aria-live="polite">
            {ctx.formatter.number(rows.length)}
          </span>
        ) : null}
      </div>
      <div {...single(`body`)}>
        <div {...single(`list`)} ref={listRef} id={listId} role="listbox" onScroll={measure} style={{ maxHeight: `calc(${maxRows} * var(--xui-control-row, 32px))` }}>
          {rows.map(({ index, item }, pos) => {
            const isActive = pos === at
            const disabled = bool(item.disabled)
            return (
              <div
                key={`${index}`}
                id={`${listId}-${index}`}
                role="option"
                aria-selected={isActive}
                aria-disabled={disabled || undefined}
                data-active={isActive ? `true` : undefined}
                {...(rowPart ? rowPart(item, index, isActive) : (part.at(`item`, `${suffix ? `${suffix.slice(1)}.` : ``}${index}`, isActive && `selected`, disabled && `disabled`) as Record<string, string>))}
                onPointerMove={() => pos !== at && setActive(pos)}
                onPointerDown={(e) => e.preventDefault()}
                onClick={() => pick(pos)}
              >
                {renderRow ? (
                  renderRow(item, index)
                ) : (
                  <>
                    {item.icon ? (
                      <span {...(part.at(`icon`, index) as Record<string, string>)} aria-hidden="true">
                        <IconGlyph icons={ctx.host.icons} name={item.icon} />
                      </span>
                    ) : null}
                    {str(item.identifier) ? <span {...(part.at(`identifier`, index) as Record<string, string>)}>{str(item.identifier)}</span> : null}
                    <span {...(part.at(`label`, index) as Record<string, string>)}>{str(item.label)}</span>
                    {picked !== undefined && picked !== `` && item.value === picked ? (
                      <span {...(part.at(`check`, index) as Record<string, string>)} aria-hidden="true">
                        <BuiltinIcon slot="SearchList.check" />
                      </span>
                    ) : null}
                  </>
                )}
              </div>
            )
          })}
          {rows.length === 0 ? <div {...single(`empty`)}>{emptyText}</div> : null}
        </div>
        {moreBelow && rows.length > maxRows ? <div {...single(`fade`)} aria-hidden="true" /> : null}
      </div>
    </>
  )
}

/** SearchList: the field filters the rows locally (label or identifier)
 *  unless the host searches itself (`on.search`, its rows arrive through
 *  the bound `items`); `search {query}` fires 150 ms after the last
 *  keystroke; a pick writes the bound `value` and fires `select {value}`. */
export function SearchListNative({ node, props, rootProps, emit, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const items = objects<SearchItem>(props.items)
  const [query, setQuery] = useBoundState(node, scope, `query`, str(props.query))
  const [picked, setPicked] = useBoundState(node, scope, `value`, str(props.value))
  const hostSearches = Boolean(node.on?.search)
  const typed = useRef(false)
  useEffect(() => {
    if (!typed.current) return
    const t = setTimeout(() => void emit(`search`, { query }), DEBOUNCE_MS)
    return () => clearTimeout(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query])
  const rows = items.map((item, index) => ({ index, item })).filter(({ item }) => hostSearches || searchMatches(item, query))
  const maxRows = Math.max(1, Math.floor(num(props.maxRows, 8)))
  return (
    <div {...(rootProps as Record<string, unknown>)}>
      <SearchBody
        part={part}
        owner="SearchList"
        suffix=""
        rows={rows}
        query={query}
        onQuery={(q) => {
          typed.current = true
          setQuery(q)
        }}
        placeholder={str(props.placeholder) || ctx.t(`search`)}
        emptyText={str(props.emptyText) || ctx.t(`noResults`)}
        maxRows={maxRows}
        showCount={props.showCount === undefined ? true : bool(props.showCount)}
        picked={picked}
        onPick={(item) => {
          setPicked(item.value)
          void emit(`select`, { value: item.value }, { value: item.value })
        }}
      />
    </div>
  )
}
