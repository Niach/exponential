// VAPP-87 + rounds 1–2: Box, List and the Unknown placeholder. A List renders
// its children then its data template's items (keyed by `template.key`, so
// reordering keeps each item's state), optionally in sections; past
// `WINDOW_THRESHOLD` (50, the catalog's) items it windows on its axis.

import { memo, useCallback, useContext, useEffect, useMemo, useRef, type KeyboardEvent, type ReactNode } from "react"
import { listSections, sectionRows, WINDOW_THRESHOLD as CATALOG_WINDOW_THRESHOLD, type ListSection } from "@exponential-at/ui"
import { InstanceContext, ScopeContext, SurfaceContext, useSurfaceContext } from "../context"
import { getPointer, LITERAL_ROWS_ROOT, setPointer } from "../data"
import { WindowedList, type WindowedListHandle } from "../list"
import type { NativeProps } from "../node-view"
import { NodeView, TemplateItemView, mergeStyle, templateItems } from "../node-view"
import { bool, BuiltinIcon, num, str, useParts, type PartFn } from "./shared"

export function BoxNative({ node, props, rootProps, emit, children }: NativeProps) {
  const pressable = bool(props.pressable) || Boolean(node.on?.press)
  if (!pressable) return <div {...(rootProps as Record<string, unknown>)}>{children}</div>
  const onKey = (e: KeyboardEvent) => {
    if (e.target !== e.currentTarget) return
    if (e.key === `Enter` || e.key === ` `) {
      e.preventDefault()
      void emit(`press`)
    }
  }
  return (
    <div {...(rootProps as Record<string, unknown>)} data-pressable="true" role="button" tabIndex={0} onClick={() => void emit(`press`)} onKeyDown={onKey}>
      {children}
    </div>
  )
}

/** Past this many items a List windows (fewer render flat, so fixtures
 *  snapshot every row). The catalog's number (`layout.json`). */
export const WINDOW_THRESHOLD = CATALOG_WINDOW_THRESHOLD

/** One row of a List: an item (a child or a template item, by its index in
 *  the list) or a section header. */
type ListRow = { item: number } | { header: number }

/** List (round 2 §5): children then the data template's items; `sectionBy`
 *  groups consecutive template items under a `section` header (the slot,
 *  bound once per section with the literal scope `{value, count, index}`,
 *  else the value); past `WINDOW_THRESHOLD` items, or with `stickyHeaders`,
 *  the list WINDOWS on its axis (vertical or horizontal) against its own
 *  scroll, the nearest scroller or the viewport. Every item is a listitem
 *  with its position in the whole list; `scrollToIndex` (the host command)
 *  takes the data index. */
export function ListNative({ node, props, rootProps, scope, domId }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const divided = bool(props.divided)
  const horizontal = props.direction === `horizontal`
  const gap = str(props.gap, `none`)
  const tokens = ctx.theme.tokens as unknown as { spacing?: Record<string, number>; control?: Record<string, number> }
  const gapPx = gap === `none` ? 0 : num(tokens.spacing?.[gap], 0)
  const hairline = num(tokens.control?.hairline, 1)
  const rowExtent = num(tokens.control?.row, 40)
  // Keyed once per data change (a 100,000-row list must not re-key on scroll).
  const tpl = useMemo(() => templateItems(ctx, node, scope), [ctx.data, ctx.templateNode, node, scope]) // eslint-disable-line react-hooks/exhaustive-deps
  const childCount = node.children.length
  const itemCount = childCount + (tpl?.items.length ?? 0)
  const sectionBy = str(props.sectionBy)
  const sections = useMemo(() => (sectionBy && tpl ? listSections(tpl.items.map((item) => getPointer(ctx.data, item.path)), sectionBy) : null), [sectionBy, tpl, ctx.data])
  const rows = useMemo((): ListRow[] => {
    const out: ListRow[] = []
    for (let i = 0; i < childCount; i++) out.push({ item: i })
    if (sections) for (const r of sectionRows(sections)) out.push(`header` in r ? r : { item: childCount + r.item })
    else for (let i = childCount; i < itemCount; i++) out.push({ item: i })
    return out
  }, [childCount, itemCount, sections])
  const sticky = bool(props.stickyHeaders) && sections !== null
  const windowed = itemCount > WINDOW_THRESHOLD || sticky
  // §5: rows sit `gap` apart; a divided list adds the hairline to the gap
  // and centres the divider in it. Flat and windowed lay out the same.
  const step = gapPx + (divided ? hairline : 0)
  const flatGap = gap === `none` ? (divided ? `${hairline}px` : undefined) : divided ? `calc(var(--xui-spacing-${gap}) + ${hairline}px)` : `var(--xui-spacing-${gap})`
  const style = mergeStyle(rootProps, { gap: windowed ? undefined : flatGap })
  const rtl = horizontal && ctx.direction === `rtl`
  const keyOf = (r: number) => {
    const row = rows[r]
    if (`header` in row) return `s:${row.header}`
    return row.item < childCount ? node.children[row.item].id : `t:${tpl!.items[row.item - childCount].key}`
  }
  // A divider sits before every item that follows another item.
  const dividerBefore = (r: number): number | null => {
    if (!divided || r === 0) return null
    const row = rows[r]
    const prev = rows[r - 1]
    return `item` in row && `item` in prev ? row.item : null
  }
  const windowRef = useRef<WindowedListHandle | null>(null)
  const rootRef = useRef<HTMLDivElement | null>(null)
  // `scrollToIndex` {id, index (data order), align}: the template's index
  // (children come first), else the child's.
  const rowOfIndex = useCallback((index: number) => rows.findIndex((r) => `item` in r && r.item === (tpl ? childCount + index : index)), [rows, tpl, childCount])
  useEffect(
    () =>
      ctx.registerScroller(domId, (index, align) => {
        const r = rowOfIndex(index)
        if (r < 0) return
        if (windowed) windowRef.current?.scrollToIndex(r, align)
        else {
          // Own rows only (`:scope >`): a nested List has rows of its own.
          const el = rootRef.current?.querySelector<HTMLElement>(`:scope > [data-xui-row="${r}"]`)
          const block = align === `center` ? `center` : align === `end` ? `end` : align === `start` ? `start` : `nearest`
          el?.scrollIntoView(horizontal ? { inline: block, block: `nearest` } : { block, inline: `nearest` })
        }
      }),
    [ctx, domId, rowOfIndex, windowed, horizontal]
  )
  const renderRow = (r: number) => <ListRowView row={rows[r]} node={node} tpl={tpl} sections={sections} childCount={childCount} part={part} />
  // Round 3: a Section body with `tree` asks for role `tree` ($a11y); its
  // items are then treeitems (a tree owns treeitems only).
  const listRole = (rootProps as Record<string, unknown>).role === `tree` ? `tree` : `list`
  const itemRole = listRole === `tree` ? `treeitem` : `listitem`
  const itemAttrs = (r: number): Record<string, unknown> => {
    const row = rows[r]
    // A section header is a listitem holding its heading (a list owns
    // listitems only); items carry their position among the ITEMS.
    return `item` in row ? { role: itemRole, "aria-setsize": itemCount, "aria-posinset": row.item + 1 } : { role: itemRole }
  }
  const divider = (r: number) => {
    const before = dividerBefore(r)
    return before === null ? null : <div {...(part.at(`divider`, before) as Record<string, string>)} role="separator" />
  }
  if (windowed) {
    const headerRows = sticky ? rows.flatMap((row, r) => (`header` in row ? [r] : [])) : undefined
    return (
      <div ref={rootRef} {...(rootProps as Record<string, unknown>)} style={style} role={listRole}>
        <WindowedList
          ref={windowRef}
          count={rows.length}
          itemKey={keyOf}
          estimatedItemHeight={rowExtent}
          axis={horizontal ? `horizontal` : `vertical`}
          direction={ctx.direction}
          gap={step}
          divider={divider}
          stickyRows={headerRows}
          itemProps={itemAttrs}
          renderItem={renderRow}
        />
      </div>
    )
  }
  // Flat: each row is a real box (the listitem; a `display: contents`
  // role is dropped by some engines) stretching its node as the List would;
  // the divider sits centred in the gap before it, as WindowedList draws it.
  const axis = horizontal ? `horizontal` : `vertical`
  const out: ReactNode[] = []
  for (let r = 0; r < rows.length; r++) {
    const line = divider(r)
    out.push(
      <div key={keyOf(r)} className="xui-list-row" data-axis={axis} data-xui-row={r} {...itemAttrs(r)}>
        {line ? (
          <div className="xui-list-gap" aria-hidden="true" data-axis={axis} style={horizontal ? { [rtl ? `right` : `left`]: -(step / 2) } : { top: -(step / 2) }}>
            {line}
          </div>
        ) : null}
        {renderRow(r)}
      </div>
    )
  }
  return (
    <div ref={rootRef} {...(rootProps as Record<string, unknown>)} style={style} role={listRole}>
      {out}
    </div>
  )
}

const ListRowView = memo(function ListRowView({ row, node, tpl, sections, childCount, part }: { row: ListRow; node: NativeProps[`node`]; tpl: ReturnType<typeof templateItems>; sections: ListSection[] | null; childCount: number; part: PartFn }) {
  if (`header` in row) {
    const section = sections?.[row.header]
    if (!section) return null
    return (
      <div {...(part.at(`section`, row.header) as Record<string, string>)} role="heading" aria-level={3}>
        {node.slots?.section ? <LiteralScope value={{ value: section.value, count: section.count, index: row.header }} pointer={`${LITERAL_ROWS_ROOT}/${node.id.replace(/[~/]/g, `_`)}/sections/${row.header}`} suffix={`.${row.header}`} node={node.slots.section} /> : section.value}
      </div>
    )
  }
  if (row.item < childCount) return <NodeView node={node.children[row.item]} />
  const item = tpl?.items[row.item - childCount]
  if (!tpl || !item) return null
  return <TemplateItemView tpl={tpl.tpl} item={item} />
})

/** A node bound against a LITERAL scope (a List section header's `{value,
 *  count, index}`): mounted into a read view of the data model at a
 *  synthetic pointer (relative paths read the literal, absolute ones the
 *  surface data; writes under it go nowhere). */
function LiteralScope({ value, pointer, suffix, node }: { value: unknown; pointer: string; suffix: string; node: NativeProps[`node`] }) {
  const ctx = useSurfaceContext()
  const outer = useContext(InstanceContext)
  const view = useMemo(() => {
    const data = setPointer(ctx.data as Record<string, unknown>, pointer, value)
    const setData = (p: string, v: unknown) => {
      if (p !== pointer && !p.startsWith(`${pointer}/`)) ctx.setData(p, v)
    }
    return { ...ctx, data, setData }
  }, [ctx, pointer, value])
  return (
    <SurfaceContext.Provider value={view}>
      <ScopeContext.Provider value={pointer}>
        <InstanceContext.Provider value={`${outer}${suffix}`}>
          <NodeView node={node} />
        </InstanceContext.Provider>
      </ScopeContext.Provider>
    </SurfaceContext.Provider>
  )
}

export function UnknownNative({ node, props, rootProps }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  return (
    <div {...(rootProps as Record<string, unknown>)} role="note" data-xui-unknown={str(props.component, node.component)}>
      <BuiltinIcon slot="Unknown.root" size={16} />
      <span {...(part(`label`) as Record<string, string>)}>
        {ctx.t(`unknownComponent`)} {str(props.component, node.component)}
      </span>
    </div>
  )
}
