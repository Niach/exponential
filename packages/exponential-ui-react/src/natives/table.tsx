// Round 1 (contract §3 Table, a11y.json Table): the data TABLE as a native.
// Object columns (`{key, label, type, slot, width, align, sortable}`) and
// rows; cells by type (text, a LOCALE number, a locale date, a tick, the
// value in a Badge look, or the Table slot named by `slot` rendered once per
// row with the ROW as its data scope). Sorting: a sortable header cycles
// asc → desc; a BOUND `sort` is the host's (it orders `rows`), an unbound
// one sorts a local copy (numbers numerically, strings by the locale
// collator, missing last). Selection: `single` = a row press selects,
// `multiple` = a checkbox column with a select-all header; a bound
// `selected` is written. `striped` queries the row recipe with `striped:
// true` on ODD rows; header/cell parts carry their column's `align`. Past
// `WINDOW_THRESHOLD` (50) rows the body windows (`WindowedList`).
// Keyboard (a11y.json Table): an interactive table has ONE roving tab stop
// on its rows (the focused row, else the first selected, else the first);
// ArrowUp/Down, Home/End move it BY INDEX (scrolling a windowed body to the
// row first), Enter presses, Space selects.

import { useCallback, useContext, useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react"
import { Checkbox as CheckboxPrimitive } from "radix-ui"
import { tableRowKeys, WINDOW_THRESHOLD } from "@exponential-at/ui"
import { InstanceContext, ScopeContext, SurfaceContext, useSurfaceContext } from "../context"
import { boundPath, getPointer, setPointer, LITERAL_ROWS_ROOT } from "../data"
import { WindowedList, type WindowedListHandle } from "../list"
import { NodeView, type NativeProps } from "../node-view"
import { partClass } from "../theme-css"
import { useBoundState } from "./bound"
import { displayString } from "@exponential-at/ui"
import { arr, bool, BuiltinIcon, num, phrase, str, useParts, objects } from "./shared"

const CURRENCY_CODE = /^[A-Za-z]{3}$/

/** A cell value as a number (a number, or a numeric string); NaN otherwise. */
const cellNumber = (v: unknown): number => (typeof v === `number` ? v : typeof v === `string` && v.trim() !== `` ? Number(v) : NaN)

export interface TableColumn {
  key: string
  /** The header text (a slot or checkbox column may have none). */
  label?: string
  type?: `text` | `number` | `date` | `boolean` | `badge` | `slot` | `currency` | `percent` | `relativeTime`
  slot?: string
  /** ISO 4217 code of a `currency` column. */
  currency?: string
  /** Fixed fraction digits of a number / currency / percent column. */
  decimals?: number
  width?: number
  align?: `start` | `center` | `end`
  sortable?: boolean
}

interface Sort {
  key: string
  direction: `asc` | `desc`
}

/** The renderer's local order of `rows` for a sort (unbound sort). */
export function sortRows(rows: readonly Record<string, unknown>[], sort: Sort | null, column: TableColumn | undefined, locale: string): number[] {
  const order = rows.map((_, i) => i)
  if (!sort || !column) return order
  const collator = new Intl.Collator(locale, { numeric: true, sensitivity: `base` })
  const dir = sort.direction === `desc` ? -1 : 1
  const missing = (v: unknown) => v === undefined || v === null || v === ``
  return order.sort((a, b) => {
    const va = rows[a]?.[sort.key]
    const vb = rows[b]?.[sort.key]
    if (missing(va) && missing(vb)) return a - b
    if (missing(va)) return 1
    if (missing(vb)) return -1
    let c: number
    if (typeof va === `number` && typeof vb === `number`) c = va - vb
    else if (column.type === `number`) c = num(va) - num(vb)
    else if (column.type === `date`) c = new Date(String(va)).getTime() - new Date(String(vb)).getTime()
    else if (typeof va === `boolean` || typeof vb === `boolean`) c = Number(Boolean(va)) - Number(Boolean(vb))
    else c = collator.compare(String(va), String(vb))
    return c === 0 ? a - b : c * dir
  })
}

export function TableNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const columns = objects<TableColumn>(props.columns)
  const rows = objects<Record<string, unknown>>(props.rows)
  const rowKey = str(props.rowKey, `id`)
  const selectable = str(props.selectable, `none`)
  const striped = bool(props.striped)
  const rowsPath = boundPath(node.props, `rows`, scope)
  const sortBound = boundPath(node.props, `sort`, scope) !== undefined
  const rawSort = props.sort as Sort | undefined
  const [sort, setSort] = useBoundState<Sort | null>(node, scope, `sort`, rawSort && rawSort.key ? { key: rawSort.key, direction: rawSort.direction === `desc` ? `desc` : `asc` } : null)
  const [selected, setSelected] = useBoundState<string[]>(node, scope, `selected`, arr<unknown>(props.selected).map(String))
  // Round 2 §4: the row keys (`rowKey` field; missing, empty or duplicate →
  // `#<index>`), the instance suffix of every row part and slot cell.
  const rowKeys = useMemo(() => tableRowKeys(rows, rowKey), [rows, rowKey])
  const keyOf = (_row: Record<string, unknown>, i: number) => rowKeys[i] ?? String(i)
  const order = useMemo(() => (sortBound ? rows.map((_, i) => i) : sortRows(rows, sort, columns.find((c) => c.key === sort?.key), ctx.locale)), [rows, sort, sortBound, columns, ctx.locale])
  // The checkbox column is as wide as the checkbox (round 2 §7).
  const template = [selectable === `multiple` ? `var(--xui-control-checkbox, 16px)` : null, ...columns.map((c) => (c.width ? `${c.width}px` : `minmax(0, 1fr)`))].filter(Boolean).join(` `)
  const cycle = (c: TableColumn) => {
    const next: Sort = sort?.key === c.key && sort.direction === `asc` ? { key: c.key, direction: `desc` } : { key: c.key, direction: `asc` }
    setSort(next)
    void emit(`sort`, { sort: next }, { sort: next })
  }
  const select = (next: string[]) => {
    setSelected(next)
    void emit(`select`, { selected: next }, { selected: next })
  }
  const allKeys = rows.map(keyOf)
  const allSelected = allKeys.length > 0 && allKeys.every((k) => selected.includes(k))
  const someSelected = !allSelected && allKeys.some((k) => selected.includes(k))
  const interactive = selectable !== `none` || Boolean(node.on?.rowPress)
  const windowed = rows.length > WINDOW_THRESHOLD

  // The roving tab stop (by row KEY, so a re-sort keeps it on its row).
  const [focusKey, setFocusKey] = useState<string | null>(null)
  const orderKeys = order.map((i) => keyOf(rows[i], i))
  const stopKey = focusKey !== null && orderKeys.includes(focusKey) ? focusKey : (orderKeys.find((k) => selected.includes(k)) ?? orderKeys[0] ?? null)
  const bodyRef = useRef<HTMLDivElement | null>(null)
  const windowRef = useRef<WindowedListHandle | null>(null)
  const pendingFocus = useRef<string | null>(null)
  // OWN rows only (flat: the body's children; windowed: the window's
  // items'): a nested Table in a slot cell has rows of its own.
  const focusRow = useCallback((key: string) => {
    const k = `[role="row"][data-key="${typeof CSS !== `undefined` && CSS.escape ? CSS.escape(key) : key.replace(/"/g, `\\"`)}"]`
    return bodyRef.current?.querySelector<HTMLElement>(`:scope > ${k}, :scope > .xui-list-window > .xui-list-item > ${k}`) ?? null
  }, [])
  const moveTo = (pos: number) => {
    if (orderKeys.length === 0) return
    const clamped = Math.max(0, Math.min(orderKeys.length - 1, pos))
    const key = orderKeys[clamped]
    setFocusKey(key)
    const el = focusRow(key)
    if (el) el.focus()
    else {
      // Windowed and not mounted yet: bring it into the window; the row's
      // ref focuses it when it mounts.
      pendingFocus.current = key
      windowRef.current?.scrollToIndex(clamped)
    }
  }
  // The `scrollToIndex` host command (round 2 §5): the DATA index (before a
  // local sort) → its position in the shown order.
  useEffect(
    () =>
      ctx.registerScroller(domId, (index, align) => {
        const pos = order.indexOf(index)
        if (pos < 0) return
        if (windowed) windowRef.current?.scrollToIndex(pos, align)
        else focusRow(keyOf(rows[index], index))?.scrollIntoView({ block: align === `center` || align === `start` || align === `end` ? align : `nearest` })
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [ctx, domId, order, windowed, rows, rowKeys]
  )
  // Literal (unbound) rows: a slot cell's edits live HERE, per row key, never
  // in the surface data model (there is no path to write them to).
  const [literalEdits, setLiteralEdits] = useState<Record<string, Record<string, unknown>>>({})
  const editLiteral = useCallback((key: string, base: Record<string, unknown>, pointer: string, value: unknown) => {
    setLiteralEdits((cur) => ({ ...cur, [key]: setPointer(cur[key] ?? base, pointer, value) as Record<string, unknown> }))
  }, [])

  const cell = (c: TableColumn, row: Record<string, unknown>, index: number): ReactNode => {
    const v = row?.[c.key]
    const missing = v === undefined || v === null || v === ``
    const decimals = c.decimals === undefined ? undefined : num(c.decimals)
    // Round 2 §3: every formatted cell goes through the surface Formatter,
    // guarded as the bind table's format functions are: a value that is not
    // a finite number = an empty cell, a currency that is not 3 letters =
    // an empty cell (Intl would throw).
    const n = cellNumber(v)
    switch (c.type) {
      case `number`:
        return Number.isFinite(n) ? ctx.formatter.number(n, { decimals }) : ``
      case `currency`: {
        const code = c.currency === undefined ? `USD` : c.currency
        return Number.isFinite(n) && typeof code === `string` && CURRENCY_CODE.test(code) ? ctx.formatter.currency(n, code, { decimals }) : ``
      }
      case `percent`:
        return Number.isFinite(n) ? ctx.formatter.percent(n, { decimals }) : ``
      case `date`:
        return missing ? `` : ctx.formatter.date(v)
      case `relativeTime`:
        return missing ? `` : ctx.formatter.relativeTime(v, ctx.now())
      case `boolean`:
        return v ? <BuiltinIcon slot="Checkbox.check" size={16} /> : <span className="xui-sr-only">—</span>
      case `badge`:
        return v === undefined || v === null || v === `` ? null : (
          <span className={`${ctx.compiled.scope} ${partClass(`Badge`, `root`)} xui-table-badge`} data-xui-part="Badge/root" data-m-variant="secondary">
            <span className={`${ctx.compiled.scope} ${partClass(`Badge`, `label`)}`} data-xui-part="Badge/label" data-m-variant="secondary">
              {String(v)}
            </span>
          </span>
        )
      case `slot`: {
        const slot = c.slot ? node.slots?.[c.slot] : undefined
        if (!slot) return null
        const key = keyOf(row, index)
        return <RowScope rowsPath={rowsPath} row={rowsPath ? row : (literalEdits[key] ?? row)} rowKey={key} index={index} tableId={domId} node={slot} onLiteralEdit={editLiteral} />
      }
      default:
        return displayString(v)
    }
  }

  const renderRow = (pos: number) => {
    const index = order[pos]
    const row = rows[index]
    const key = keyOf(row, index)
    const isSel = selected.includes(key)
    const odd = pos % 2 === 1
    const press = () => {
      if (selectable === `single`) select(isSel ? [] : [key])
      if (node.on?.rowPress) void emit(`rowPress`, { key, row })
    }
    return (
      <div
        key={key}
        role="row"
        aria-rowindex={pos + 2}
        aria-selected={selectable !== `none` ? isSel : undefined}
        tabIndex={interactive ? (key === stopKey ? 0 : -1) : undefined}
        data-key={key}
        onFocus={interactive ? (e) => e.target === e.currentTarget && key !== focusKey && setFocusKey(key) : undefined}
        ref={
          interactive
            ? (el: HTMLDivElement | null) => {
                // A row the keyboard moved to before it was mounted (a
                // windowed body): focus it the moment it mounts.
                if (el && pendingFocus.current === key) {
                  pendingFocus.current = null
                  el.focus()
                }
              }
            : undefined
        }
        {...(part.atWith(`row`, key, { striped: striped && odd }, isSel && `selected`) as Record<string, string>)}
        style={{ gridTemplateColumns: template }}
        onClick={interactive && selectable !== `multiple` ? press : node.on?.rowPress ? press : undefined}
        onKeyDown={(e: KeyboardEvent) => {
          if (e.target !== e.currentTarget) return
          if (e.key === `Enter` || (e.key === ` ` && selectable === `single`)) {
            e.preventDefault()
            press()
          } else if (e.key === `ArrowDown` || e.key === `ArrowUp`) {
            e.preventDefault()
            // From the row the keyboard is GOING to (a pending, not yet
            // mounted one) so fast presses are never lost.
            const pending = pendingFocus.current === null ? -1 : orderKeys.indexOf(pendingFocus.current)
            moveTo((pending >= 0 ? pending : pos) + (e.key === `ArrowDown` ? 1 : -1))
          } else if (e.key === `Home` || e.key === `End`) {
            e.preventDefault()
            moveTo(e.key === `Home` ? 0 : orderKeys.length - 1)
          }
        }}
      >
        {selectable === `multiple` ? (
          <div role="gridcell" className="xui-table-check">
            <CheckboxPrimitive.Root
              {...(part.at(`checkbox`, key, isSel && `checked`) as Record<string, string>)}
              className={`${part(`checkbox`).className as string} xui-Checkbox-box`}
              checked={isSel}
              aria-label={ctx.t(`selectRow`)}
              onClick={(e) => e.stopPropagation()}
              onCheckedChange={(next) => select(next === true ? [...selected, key] : selected.filter((k) => k !== key))}
            >
              <CheckboxPrimitive.Indicator className="xui-Checkbox-check">
                <BuiltinIcon slot="Checkbox.check" />
              </CheckboxPrimitive.Indicator>
            </CheckboxPrimitive.Root>
          </div>
        ) : null}
        {columns.map((c, ci) => (
          <div key={c.key} role={interactive ? `gridcell` : `cell`} {...(part.atWith(`cell`, `${key}.${ci}`, { align: c.align ?? `start` }) as Record<string, string>)} data-type={c.type ?? `text`}>
            {cell(c, row, index)}
          </div>
        ))}
      </div>
    )
  }

  const header = (
    <div role="rowgroup" {...(part(`header`) as Record<string, string>)} data-sticky={bool(props.stickyHeader) ? `true` : undefined}>
      <div role="row" aria-rowindex={1} className="xui-table-row" style={{ gridTemplateColumns: template }}>
        {selectable === `multiple` ? (
          <div role="columnheader" className="xui-table-check">
            <CheckboxPrimitive.Root
              {...(part.at(`checkbox`, `header`, allSelected && `checked`) as Record<string, string>)}
              className={`${part(`checkbox`).className as string} xui-Checkbox-box`}
              checked={allSelected ? true : someSelected ? `indeterminate` : false}
              aria-label={ctx.t(`selectAll`)}
              onCheckedChange={(next) => select(next === true ? allKeys : [])}
            >
              <CheckboxPrimitive.Indicator className="xui-Checkbox-check">
                <BuiltinIcon slot="Checkbox.check" />
              </CheckboxPrimitive.Indicator>
            </CheckboxPrimitive.Root>
          </div>
        ) : null}
        {columns.map((c, ci) => {
          const active = sort?.key === c.key ? sort.direction : null
          const ariaSort = active === `asc` ? `ascending` : active === `desc` ? `descending` : c.sortable ? `none` : undefined
          return (
            <div key={c.key} role="columnheader" aria-sort={ariaSort} {...(part.atWith(`headerCell`, ci, { align: c.align ?? `start` }) as Record<string, string>)}>
              {c.sortable ? (
                <button type="button" className="xui-table-sort" aria-label={phrase(ctx, active === `asc` ? `sortByDescending` : `sortByAscending`, { name: str(c.label) }, () => `${str(c.label)}: ${ctx.t(active === `asc` ? `sortDescending` : `sortAscending`)}`)} onClick={() => cycle(c)}>
                  <span>{str(c.label)}</span>
                  <span {...(part(`sortIcon`) as Record<string, string>)} data-direction={active ?? undefined} aria-hidden="true">
                    {active ? <BuiltinIcon slot={`Table.sortIcon.${active}`} /> : null}
                  </span>
                </button>
              ) : (
                <span>{str(c.label)}</span>
              )}
            </div>
          )
        })}
      </div>
    </div>
  )

  const caption = str(props.caption)
  return (
    <div
      {...(rootProps as Record<string, unknown>)}
      role={interactive ? `grid` : `table`}
      aria-rowcount={rows.length + 1}
      aria-multiselectable={selectable === `multiple` || undefined}
      aria-describedby={caption ? `${domId}.caption` : undefined}
      aria-label={caption ? undefined : ((rootProps as Record<string, string | undefined>)[`aria-label`] ?? ctx.t(`table`))}
      data-windowed={windowed ? `true` : undefined}
    >
      {header}
      <div role="rowgroup" {...(part(`body`) as Record<string, string>)} className={`${(part(`body`) as { className: string }).className} xui-table-body`} ref={bodyRef}>
        {rows.length === 0 ? (
          <div role="row">
            <div role="cell" {...(part(`empty`) as Record<string, string>)}>
              {str(props.emptyText) || ctx.t(`noResults`)}
            </div>
          </div>
        ) : windowed ? (
          <WindowedList ref={windowRef} count={order.length} itemKey={(i) => keyOf(rows[order[i]], order[i])} estimatedItemHeight={num(ctx.theme.tokens.control?.row, 40)} overscan={8} renderItem={renderRow} />
        ) : (
          order.map((_, pos) => renderRow(pos))
        )}
      </div>
      {caption ? (
        <div {...(part(`caption`) as Record<string, string>)} id={`${domId}.caption`}>
          {caption}
        </div>
      ) : null}
      {selectable === `multiple` && selected.length ? <span className="xui-sr-only" aria-live="polite">{ctx.t(`selectedCount`, { count: selected.length })}</span> : null}
    </div>
  )
}

/** A slot cell: the ROW is the slot's data scope and its instance suffix is
 *  APPENDED to the outer one (`<id><outer>.<row key>`). A bound `rows` scopes
 *  to the row's pointer. Literal rows are mounted into a read view of the
 *  data model at a synthetic pointer: absolute bindings still read (and
 *  write) the real model, a write UNDER the row goes to the table's local
 *  per-row state (`onLiteralEdit`), so it survives the next render. */
function RowScope({ rowsPath, row, rowKey, index, tableId, node, onLiteralEdit }: { rowsPath?: string; row: Record<string, unknown>; rowKey: string; index: number; tableId: string; node: NativeProps[`node`]; onLiteralEdit: (key: string, base: Record<string, unknown>, pointer: string, value: unknown) => void }) {
  const ctx = useSurfaceContext()
  const outer = useContext(InstanceContext)
  const literalPath = `${LITERAL_ROWS_ROOT}/${tableId.replace(/[~/]/g, `_`)}/${index}`
  const view = useMemo(() => {
    if (rowsPath) return ctx
    const data = setPointer(ctx.data as Record<string, unknown>, literalPath, row)
    const setData = (pointer: string, value: unknown) => {
      if (pointer === literalPath || pointer.startsWith(`${literalPath}/`)) onLiteralEdit(rowKey, (getPointer(data, literalPath) as Record<string, unknown>) ?? row, pointer.slice(literalPath.length), value)
      else ctx.setData(pointer, value)
    }
    return { ...ctx, data, setData }
  }, [ctx, rowsPath, literalPath, row, rowKey, onLiteralEdit])
  return (
    <SurfaceContext.Provider value={view}>
      <ScopeContext.Provider value={rowsPath ? `${rowsPath}/${index}` : literalPath}>
        <InstanceContext.Provider value={`${outer}.${rowKey}`}>
          <NodeView node={node} />
        </InstanceContext.Provider>
      </ScopeContext.Provider>
    </SurfaceContext.Provider>
  )
}
