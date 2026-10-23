// FEED-128 (React's natives/table.tsx; round 1 contract §3 Table, a11y.json
// Table): the data TABLE as a native. Object columns (`{key, label, type,
// slot, width, align, sortable}`) and rows; cells by type (text, a LOCALE
// number, a locale date, a tick, the value in a Badge look, or the Table
// slot named by `slot` rendered once per row with the ROW as its data
// scope). Sorting: a sortable header cycles asc → desc; a BOUND `sort` is
// the host's (it orders `rows`), an unbound one sorts a local copy
// (`sortRows`). Selection: `single` = a row press selects, `multiple` = a
// checkbox column with a select-all header (Radix's Checkbox DOM, painted
// by hand: `button[role=checkbox]` + the `data-state` indicator); a bound
// `selected` is written. Past `WINDOW_THRESHOLD` (50) rows the body windows
// (`WindowedList`). Keyboard: ONE roving tab stop on the rows; ArrowUp/Down,
// Home/End move it BY INDEX, Enter presses, Space selects.

import { For, Index, Show, createEffect, createMemo, createSignal, onCleanup, onMount, untrack, type Accessor, type Component, type JSX } from "solid-js"
import { displayString, tableRowKeys, WINDOW_THRESHOLD, type UiNode } from "@exponential_app/ui"
import { boundPath, getPointer, LITERAL_ROWS_ROOT, partClass, setPointer, sortRows, type TableColumn, type TableSort as Sort } from "@exponential_app/ui/web"
import { InstanceContext, ScopeContext, SurfaceContext, deriveContext, useInstance, useSurfaceContext } from "../context"
import { bindAttrs } from "../bind-attrs"
import { WindowedList, type WindowedListHandle } from "../list"
import { NodeView, type NativeProps } from "../node-view"
import { css } from "../style"
import { createBoundState } from "./bound"
import { arr, bool, BuiltinIcon, num, objects, parts, phrase, str, type Attrs } from "./shared"

export { sortRows }
export type { TableColumn }

const CURRENCY_CODE = /^[A-Za-z]{3}$/

/** A cell value as a number (a number, or a numeric string); NaN otherwise. */
const cellNumber = (v: unknown): number => (typeof v === `number` ? v : typeof v === `string` && v.trim() !== `` ? Number(v) : NaN)

type Checked = boolean | `indeterminate`
const checkState = (c: Checked) => (c === `indeterminate` ? `indeterminate` : c ? `checked` : `unchecked`)

/** Radix's `Checkbox.Root` + `Checkbox.Indicator` DOM: a `button` with
 *  `role=checkbox`, `aria-checked` (`mixed` when indeterminate),
 *  `data-state`, `value="on"`; the indicator span (`pointer-events: none`)
 *  only while checked or indeterminate; the hidden bubble input only inside
 *  a `<form>` (Radix's `isFormControl`). */
function TableCheckbox(props: { attrs: Attrs; class: string; checked: Checked; label: string; stop?: boolean; onChange: (next: boolean) => void }): JSX.Element {
  let button: HTMLButtonElement | undefined
  // Radix renders the bubble input until the control is known, then only
  // inside a form.
  const [inForm, setInForm] = createSignal(true)
  const [size, setSize] = createSignal<{ width: number; height: number } | undefined>(undefined)
  const initial = untrack(() => props.checked === true)
  onMount(() => {
    setInForm(Boolean(button?.closest(`form`)))
    if (button) setSize({ width: button.offsetWidth, height: button.offsetHeight })
  })
  const state = () => checkState(props.checked)
  return (
    <>
      <button
        ref={button}
        {...props.attrs}
        class={props.class}
        type="button"
        role="checkbox"
        aria-checked={props.checked === `indeterminate` ? `mixed` : props.checked}
        data-state={state()}
        value="on"
        aria-label={props.label}
        onKeyDown={(e) => {
          if (e.key === `Enter`) e.preventDefault()
        }}
        onClick={(e) => {
          if (props.stop) e.stopPropagation()
          const c = props.checked
          props.onChange(c === `indeterminate` ? true : !c)
          if (inForm() && !props.stop) e.stopPropagation()
        }}
      >
        <Show when={props.checked !== false}>
          <span data-state={state()} class="xui-Checkbox-check" style={css({ pointerEvents: `none` })}>
            <BuiltinIcon slot="Checkbox.check" />
          </span>
        </Show>
      </button>
      <Show when={inForm()}>
        <input
          type="checkbox"
          aria-hidden="true"
          attr:checked={initial ? `` : undefined}
          value="on"
          tabindex="-1"
          style={css({ ...(size() ?? {}), position: `absolute`, pointerEvents: `none`, opacity: 0, margin: 0, transform: `translateX(-100%)` })}
        />
      </Show>
    </>
  )
}

export function TableNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const columns = createMemo(() => objects<TableColumn>(props.props.columns))
  const rows = createMemo(() => objects<Record<string, unknown>>(props.props.rows))
  const rowKey = () => str(props.props.rowKey, `id`)
  const selectable = () => str(props.props.selectable, `none`)
  const striped = () => bool(props.props.striped)
  const rowsPath = createMemo(() => boundPath(props.node.props, `rows`, props.scope))
  const sortBound = createMemo(() => boundPath(props.node.props, `sort`, props.scope) !== undefined)
  const [sort, setSort] = createBoundState<Sort | null>(
    () => props.node,
    () => props.scope,
    `sort`,
    () => {
      const raw = props.props.sort as Sort | undefined
      return raw && raw.key ? { key: raw.key, direction: raw.direction === `desc` ? `desc` : `asc` } : null
    }
  )
  const [selected, setSelected] = createBoundState<string[]>(
    () => props.node,
    () => props.scope,
    `selected`,
    () => arr<unknown>(props.props.selected).map(String)
  )
  // Round 2 §4: the row keys (`rowKey` field; missing, empty or duplicate →
  // `#<index>`), the instance suffix of every row part and slot cell.
  const rowKeys = createMemo(() => tableRowKeys(rows(), rowKey()))
  const keyOf = (i: number) => rowKeys()[i] ?? String(i)
  const order = createMemo(() => {
    const s = sort()
    return sortBound() ? rows().map((_, i) => i) : sortRows(rows(), s, columns().find((c) => c.key === s?.key), ctx.locale)
  })
  // The checkbox column is as wide as the checkbox (round 2 §7).
  const template = createMemo(() => [selectable() === `multiple` ? `var(--xui-control-checkbox, 16px)` : null, ...columns().map((c) => (c.width ? `${c.width}px` : `minmax(0, 1fr)`))].filter(Boolean).join(` `))
  const cycle = (c: TableColumn) => {
    const cur = untrack(sort)
    const next: Sort = cur?.key === c.key && cur.direction === `asc` ? { key: c.key, direction: `desc` } : { key: c.key, direction: `asc` }
    setSort(next)
    void props.emit(`sort`, { sort: next }, { sort: next })
  }
  const select = (next: string[]) => {
    setSelected(next)
    void props.emit(`select`, { selected: next }, { selected: next })
  }
  const allKeys = createMemo(() => rows().map((_, i) => keyOf(i)))
  const allSelected = createMemo(() => allKeys().length > 0 && allKeys().every((k) => selected().includes(k)))
  const someSelected = createMemo(() => !allSelected() && allKeys().some((k) => selected().includes(k)))
  const interactive = () => selectable() !== `none` || Boolean(props.node.on?.rowPress)
  const windowed = () => rows().length > WINDOW_THRESHOLD

  // The roving tab stop (by row KEY, so a re-sort keeps it on its row).
  const [focusKey, setFocusKey] = createSignal<string | null>(null)
  const orderKeys = createMemo(() => order().map((i) => keyOf(i)), undefined, { equals: (a, b) => a.length === b.length && a.every((k, i) => k === b[i]) })
  const stopKey = createMemo(() => {
    const f = focusKey()
    const keys = orderKeys()
    return f !== null && keys.includes(f) ? f : (keys.find((k) => selected().includes(k)) ?? keys[0] ?? null)
  })
  let bodyEl: HTMLDivElement | undefined
  let windowHandle: WindowedListHandle | undefined
  let pendingFocus: string | null = null
  // OWN rows only (flat: the body's children; windowed: the window's
  // items'): a nested Table in a slot cell has rows of its own.
  const focusRow = (key: string) => {
    const k = `[role="row"][data-key="${typeof CSS !== `undefined` && CSS.escape ? CSS.escape(key) : key.replace(/"/g, `\\"`)}"]`
    return bodyEl?.querySelector<HTMLElement>(`:scope > ${k}, :scope > .xui-list-window > .xui-list-item > ${k}`) ?? null
  }
  const moveTo = (pos: number) => {
    const keys = untrack(orderKeys)
    if (keys.length === 0) return
    const clamped = Math.max(0, Math.min(keys.length - 1, pos))
    const key = keys[clamped]
    setFocusKey(key)
    const el = focusRow(key)
    if (el) el.focus()
    else {
      // Windowed and not mounted yet: bring it into the window; the row
      // focuses itself when it mounts.
      pendingFocus = key
      windowHandle?.scrollToIndex(clamped)
    }
  }
  // The `scrollToIndex` host command (round 2 §5): the DATA index (before a
  // local sort) → its position in the shown order.
  createEffect(() => {
    const id = props.domId
    onCleanup(
      ctx.registerScroller(id, (index, align) => {
        const pos = untrack(order).indexOf(index)
        if (pos < 0) return
        if (untrack(windowed)) windowHandle?.scrollToIndex(pos, align)
        else focusRow(untrack(() => keyOf(index)))?.scrollIntoView({ block: align === `center` || align === `start` || align === `end` ? align : `nearest` })
      })
    )
  })
  // Literal (unbound) rows: a slot cell's edits live HERE, per row key,
  // never in the surface data model (there is no path to write them to).
  const [literalEdits, setLiteralEdits] = createSignal<Record<string, Record<string, unknown>>>({})
  const editLiteral = (key: string, base: Record<string, unknown>, pointer: string, value: unknown) => {
    setLiteralEdits((cur) => ({ ...cur, [key]: setPointer(cur[key] ?? base, pointer, value) as Record<string, unknown> }))
  }

  /** A formatted (non-slot) cell's content. */
  const formatted = (c: TableColumn, row: Record<string, unknown>): JSX.Element => {
    const v = row?.[c.key]
    const missing = v === undefined || v === null || v === ``
    const decimals = c.decimals === undefined ? undefined : num(c.decimals)
    const n = cellNumber(v)
    const text = (s: string) => (s === `` ? null : s)
    switch (c.type) {
      case `number`:
        return text(Number.isFinite(n) ? ctx.formatter.number(n, { decimals }) : ``)
      case `currency`: {
        const code = c.currency === undefined ? `USD` : c.currency
        return text(Number.isFinite(n) && typeof code === `string` && CURRENCY_CODE.test(code) ? ctx.formatter.currency(n, code, { decimals }) : ``)
      }
      case `percent`:
        return text(Number.isFinite(n) ? ctx.formatter.percent(n, { decimals }) : ``)
      case `date`:
        return text(missing ? `` : ctx.formatter.date(v))
      case `relativeTime`:
        return text(missing ? `` : ctx.formatter.relativeTime(v, ctx.now()))
      case `boolean`:
        return v ? <BuiltinIcon slot="Checkbox.check" size={16} /> : <span class="xui-sr-only">—</span>
      case `badge`:
        return missing ? null : (
          <span class={`${ctx.compiled.scope} ${partClass(`Badge`, `root`)} xui-table-badge`} data-xui-part="Badge/root" data-m-variant="secondary">
            <span class={`${ctx.compiled.scope} ${partClass(`Badge`, `label`)}`} data-xui-part="Badge/label" data-m-variant="secondary">
              {String(v)}
            </span>
          </span>
        )
      default:
        return text(displayString(v))
    }
  }

  /** One body row at shown position `pos` (keyed by its row key). */
  const Row = (rp: { rowKey: string; pos: Accessor<number> }): JSX.Element => {
    const key = rp.rowKey
    const index = createMemo<number>((prev) => order()[rp.pos()] ?? prev!)
    const row = () => rows()[index()]
    const isSel = () => selected().includes(key)
    const odd = () => rp.pos() % 2 === 1
    const press = () => {
      const sel = untrack(isSel)
      if (untrack(selectable) === `single`) select(sel ? [] : [key])
      if (props.node.on?.rowPress) void props.emit(`rowPress`, { key, row: untrack(row) })
    }
    return (
      <div
        role="row"
        aria-rowindex={rp.pos() + 2}
        aria-selected={selectable() !== `none` ? isSel() : undefined}
        tabindex={interactive() ? (key === stopKey() ? 0 : -1) : undefined}
        data-key={key}
        onFocus={(e) => {
          if (interactive() && e.target === e.currentTarget && key !== focusKey()) setFocusKey(key)
        }}
        ref={(el: HTMLDivElement) => {
          bindAttrs(el, () => part.atWith(`row`, key, { striped: striped() && odd() }, isSel() && `selected`))
          // A row the keyboard moved to before it was mounted (a windowed
          // body): focus it the moment it mounts.
          if (untrack(interactive) && pendingFocus === key) {
            pendingFocus = null
            queueMicrotask(() => el.focus())
          }
        }}
        style={css({ gridTemplateColumns: template() })}
        onClick={() => {
          if ((interactive() && selectable() !== `multiple`) || props.node.on?.rowPress) press()
        }}
        onKeyDown={(e: KeyboardEvent) => {
          if (e.target !== e.currentTarget) return
          if (e.key === `Enter` || (e.key === ` ` && selectable() === `single`)) {
            e.preventDefault()
            press()
          } else if (e.key === `ArrowDown` || e.key === `ArrowUp`) {
            e.preventDefault()
            // From the row the keyboard is GOING to (a pending, not yet
            // mounted one) so fast presses are never lost.
            const keys = untrack(orderKeys)
            const pending = pendingFocus === null ? -1 : keys.indexOf(pendingFocus)
            moveTo((pending >= 0 ? pending : untrack(rp.pos)) + (e.key === `ArrowDown` ? 1 : -1))
          } else if (e.key === `Home` || e.key === `End`) {
            e.preventDefault()
            moveTo(e.key === `Home` ? 0 : untrack(orderKeys).length - 1)
          }
        }}
      >
        <Show when={selectable() === `multiple`}>
          <div role="gridcell" class="xui-table-check">
            <TableCheckbox
              attrs={part.at(`checkbox`, key, isSel() && `checked`)}
              class={`${part(`checkbox`).class as string} xui-Checkbox-box`}
              checked={isSel()}
              label={ctx.t(`selectRow`)}
              stop
              onChange={(next) => {
                const cur = untrack(selected)
                select(next ? [...cur, key] : cur.filter((k) => k !== key))
              }}
            />
          </div>
        </Show>
        <Index each={columns()}>
          {(c, ci) => {
            const kind = createMemo(() => c().type)
            return (
              <div ref={(el) => bindAttrs(el, () => part.atWith(`cell`, `${key}.${ci}`, { align: c().align ?? `start` }))} role={interactive() ? `gridcell` : `cell`} data-type={c().type ?? `text`}>
                {(() => {
                  if (kind() !== `slot`) return formatted(c(), row())
                  return untrack(() => {
                    const slot = createMemo(() => (c().slot ? props.node.slots?.[c().slot as string] : undefined))
                    return (
                      <Show when={slot()}>
                        <RowScope rowsPath={rowsPath()} row={rowsPath() ? row() : (literalEdits()[key] ?? row())} rowKey={key} index={index()} tableId={props.domId} node={slot()!} onLiteralEdit={editLiteral} />
                      </Show>
                    )
                  })
                })()}
              </div>
            )
          }}
        </Index>
      </div>
    )
  }

  const caption = () => str(props.props.caption)
  return (
    <div
      {...props.rootProps}
      role={interactive() ? `grid` : `table`}
      aria-rowcount={rows().length + 1}
      aria-multiselectable={selectable() === `multiple` || undefined}
      aria-describedby={caption() ? `${props.domId}.caption` : undefined}
      aria-label={caption() ? undefined : ((props.rootProps[`aria-label`] as string | undefined) ?? ctx.t(`table`))}
      data-windowed={windowed() ? `true` : undefined}
    >
      <div role="rowgroup" {...part(`header`)} data-sticky={bool(props.props.stickyHeader) ? `true` : undefined}>
        <div role="row" aria-rowindex={1} class="xui-table-row" style={css({ gridTemplateColumns: template() })}>
          <Show when={selectable() === `multiple`}>
            <div role="columnheader" class="xui-table-check">
              <TableCheckbox
                attrs={part.at(`checkbox`, `header`, allSelected() && `checked`)}
                class={`${part(`checkbox`).class as string} xui-Checkbox-box`}
                checked={allSelected() ? true : someSelected() ? `indeterminate` : false}
                label={ctx.t(`selectAll`)}
                onChange={(next) => select(next ? untrack(allKeys) : [])}
              />
            </div>
          </Show>
          <Index each={columns()}>
            {(c, ci) => {
              const active = () => {
                const s = sort()
                return s?.key === c().key ? s.direction : null
              }
              const ariaSort = () => (active() === `asc` ? `ascending` : active() === `desc` ? `descending` : c().sortable ? `none` : undefined)
              return (
                <div role="columnheader" aria-sort={ariaSort()} {...part.atWith(`headerCell`, ci, { align: c().align ?? `start` })}>
                  <Show when={c().sortable} fallback={<span>{str(c().label)}</span>}>
                    <button
                      type="button"
                      class="xui-table-sort"
                      aria-label={phrase(ctx, active() === `asc` ? `sortByDescending` : `sortByAscending`, { name: str(c().label) }, () => `${str(c().label)}: ${ctx.t(active() === `asc` ? `sortDescending` : `sortAscending`)}`)}
                      onClick={() => cycle(untrack(c))}
                    >
                      <span>{str(c().label)}</span>
                      <span {...part(`sortIcon`)} data-direction={active() ?? undefined} aria-hidden="true">
                        {active() ? <BuiltinIcon slot={`Table.sortIcon.${active()}`} /> : null}
                      </span>
                    </button>
                  </Show>
                </div>
              )
            }}
          </Index>
        </div>
      </div>
      <div role="rowgroup" {...part(`body`)} class={`${part(`body`).class as string} xui-table-body`} ref={bodyEl}>
        <Show
          when={rows().length > 0}
          fallback={
            <div role="row">
              <div role="cell" {...part(`empty`)}>
                {str(props.props.emptyText) || ctx.t(`noResults`)}
              </div>
            </div>
          }
        >
          <Show when={windowed()} fallback={<For each={orderKeys()}>{(key) => <Row rowKey={key} pos={createMemo<number>((prev) => { const p = orderKeys().indexOf(key); return p >= 0 ? p : prev! })} />}</For>}>
            <WindowedList
              ref={(h) => (windowHandle = h)}
              count={order().length}
              itemKey={(i) => keyOf(order()[i])}
              estimatedItemHeight={num(ctx.theme.tokens.control?.row, 40)}
              overscan={8}
              renderItem={(i) => <Row rowKey={untrack(() => keyOf(order()[i()]))} pos={i} />}
            />
          </Show>
        </Show>
      </div>
      <Show when={caption()}>
        <div {...part(`caption`)} id={`${props.domId}.caption`}>
          {caption()}
        </div>
      </Show>
      <Show when={selectable() === `multiple` && selected().length}>
        <span class="xui-sr-only" aria-live="polite">
          {ctx.t(`selectedCount`, { count: selected().length })}
        </span>
      </Show>
    </div>
  )
}

/** A slot cell: the ROW is the slot's data scope and its instance suffix is
 *  APPENDED to the outer one (`<id><outer>.<row key>`). A bound `rows`
 *  scopes to the row's pointer. Literal rows are mounted into a read view
 *  of the data model at a synthetic pointer: absolute bindings still read
 *  (and write) the real model, a write UNDER the row goes to the table's
 *  local per-row state (`onLiteralEdit`). */
function RowScope(props: { rowsPath?: string; row: Record<string, unknown>; rowKey: string; index: number; tableId: string; node: UiNode; onLiteralEdit: (key: string, base: Record<string, unknown>, pointer: string, value: unknown) => void }): JSX.Element {
  const ctx = useSurfaceContext()
  const outer = useInstance()
  const literalPath = () => `${LITERAL_ROWS_ROOT}/${props.tableId.replace(/[~/]/g, `_`)}/${props.index}`
  const literalData = createMemo(() => (props.rowsPath ? null : setPointer(ctx.data as Record<string, unknown>, literalPath(), props.row)))
  const view = deriveContext(ctx, {
    data: () => literalData() ?? ctx.data,
    setData: () =>
      props.rowsPath
        ? ctx.setData
        : (pointer: string, value: unknown) => {
            const lp = untrack(literalPath)
            if (pointer === lp || pointer.startsWith(`${lp}/`)) {
              const data = untrack(literalData)
              props.onLiteralEdit(props.rowKey, (getPointer(data, lp) as Record<string, unknown>) ?? untrack(() => props.row), pointer.slice(lp.length), value)
            } else ctx.setData(pointer, value)
          },
  })
  return (
    <SurfaceContext.Provider value={view}>
      <ScopeContext.Provider value={() => (props.rowsPath ? `${props.rowsPath}/${props.index}` : literalPath())}>
        <InstanceContext.Provider value={() => `${outer()}.${props.rowKey}`}>
          <NodeView node={props.node} />
        </InstanceContext.Provider>
      </ScopeContext.Provider>
    </SurfaceContext.Provider>
  )
}

export const NATIVES_TABLE: Record<string, Component<NativeProps>> = {
  Table: TableNative,
}
