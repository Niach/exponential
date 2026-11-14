// VAPP-87 + rounds 1–2: Box, List and the Unknown placeholder. A List renders
// its children then its data template's items (keyed by `template.key`, so
// reordering keeps each item's state), optionally in sections; past
// `WINDOW_THRESHOLD` (50, the catalog's) items it windows on its axis.

import { memo, useCallback, useContext, useEffect, useMemo, useRef, type FocusEvent, type KeyboardEvent, type MouseEvent, type PointerEvent as ReactPointerEvent, type ReactNode } from "react"
import { foldKey, listSections, sectionRows, TREE_GUIDE_COLUMN, WINDOW_THRESHOLD as CATALOG_WINDOW_THRESHOLD, type ListSection } from "@exponential_app/eui"
import { InstanceContext, ScopeContext, SurfaceContext, deriveContext, readView, useSurfaceContext } from "../context"
import { getPointer, isKeyRecorderField, isPickerPick, keyRecorderKeyPayload, keyRecorderMousePayload, listReorderRows, listReorderStart, LITERAL_ROWS_ROOT, REORDER_DEPTH_ATTR, REORDER_GROUP_ATTR, REORDER_INDENT_ATTR, REORDER_KEY_ATTR, startListDrag, treeKeyTarget } from "@exponential_app/eui/web"
import { ClosePickerContext } from "../form"
import { WindowedList, type WindowedListHandle } from "../list"
import type { NativeProps } from "../node-view"
import { NodeView, TemplateItemView, mergeStyle, templateItems } from "../node-view"
import { bool, BuiltinIcon, num, str, useParts, type PartFn } from "./shared"

export function BoxNative({ node, props, rootProps, emit, children }: NativeProps) {
  const ctx = useSurfaceContext()
  const closePicker = useContext(ClosePickerContext)
  // #EXP-1674: `Row.href` lands here and opens like a Link's (the policy decides).
  const href = str(props.href)
  const pressable = bool(props.pressable) || Boolean(node.on?.press) || Boolean(href)
  // #EDE-65: `on.hover` fires as the pointer enters, like Motion's (no payload).
  const onPointerEnter = node.on?.hover ? () => void emit(`hover`) : undefined
  // #EXP-1629: a KeyRecorder field, while its `recording` recipe prop (read
  // live, the resolved `data-m-recording`) is on, takes every key and mouse
  // buttons 3-5 as `record`; a press only starts it; leaving it = `blur`.
  const recorder = isKeyRecorderField(node.recipe)
  const recording = recorder && (rootProps as Record<string, unknown>)[`data-m-recording`] === `true`
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (recording && ref.current && document.activeElement !== ref.current) ref.current.focus()
  }, [recording])
  if (!pressable) return <div {...(rootProps as Record<string, unknown>)} onPointerEnter={onPointerEnter}>{children}</div>
  const press = (e?: MouseEvent) => {
    // #VAPP-284: a press target inside another (a foldable Row's fold
    // button in its head) fires only its own press.
    e?.stopPropagation()
    if (recording) return
    if (href) ctx.openUrl(href)
    void emit(`press`)
    if (isPickerPick(node.recipe)) closePicker?.()
  }
  const record = (e: KeyboardEvent<HTMLDivElement>) => {
    e.preventDefault()
    e.stopPropagation()
    const payload = keyRecorderKeyPayload(e)
    if (payload) void emit(`record`, { ...payload })
  }
  const onMouseDown = (e: MouseEvent<HTMLDivElement>) => {
    if (!recording || e.button === 0 || e.button === 2) return
    e.preventDefault()
    e.stopPropagation()
    const payload = keyRecorderMousePayload(e)
    if (payload) void emit(`record`, { ...payload })
  }
  // The back / forward buttons' release would navigate; the middle one's click.
  const swallowAux = (e: MouseEvent<HTMLDivElement>) => {
    if (recording && e.button !== 0 && e.button !== 2) e.preventDefault()
  }
  const onBlur = (e: FocusEvent<HTMLDivElement>) => {
    if (recording && !e.currentTarget.contains(e.relatedTarget as Node | null)) void emit(`blur`)
  }
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (recording) return record(e)
    if (e.target !== e.currentTarget) return
    if (e.key === `Enter` || e.key === ` `) {
      e.preventDefault()
      press()
      return
    }
    // #VAPP-285: a Tree row moves the focus (treeKey); a fold falls through.
    const tree = treeKeyTarget(e.currentTarget, e.key, ctx.direction === `rtl`)
    if (tree && tree !== `toggle`) {
      e.preventDefault()
      if (tree !== `none`) tree.focus.focus()
      return
    }
    // #VAPP-284: ArrowRight / ArrowLeft open / close a node that folds.
    const expanded = String((rootProps as Record<string, unknown>)[`aria-expanded`])
    if (node.on?.toggle && (expanded === `true` || expanded === `false`)) {
      const rtl = ctx.direction === `rtl`
      if (foldKey(e.key, expanded === `true`, rtl)) {
        e.preventDefault()
        void emit(`toggle`)
      }
    }
  }
  return (
    <div {...(rootProps as Record<string, unknown>)} data-pressable="true" role={treeRole(rootProps)} tabIndex={0} onClick={press} onKeyDown={onKey} onPointerEnter={onPointerEnter} {...(recorder ? { ref, onKeyUp: (e: KeyboardEvent<HTMLDivElement>) => recording && record(e), onMouseDown, onMouseUp: swallowAux, onAuxClick: swallowAux, onBlur } : {})}>
      {children}
    </div>
  )
}

/** #VAPP-285: a pressable Box is a button unless its accessibility names a
 *  treeitem (a Tree row). */
function treeRole(rootProps: unknown): string {
  return (rootProps as Record<string, unknown>)[`role`] === `treeitem` ? `treeitem` : `button`
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
export function ListNative({ node, props, rootProps, scope, domId, emit }: NativeProps) {
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
  // EXP-1832 (fixtures/list-reorder.json): a vertical `reorderable` List's
  // items carry the web controller's DOM contract (key = the template key,
  // else the child's id; `depth` / `group` = the item's fields; indent = a
  // Row's TreeGuides gutter at that depth).
  const reorderable = bool(props.reorderable) && !horizontal
  const reorderAttrs = (item: number): Record<string, unknown> => {
    if (item < childCount) return { [REORDER_KEY_ATTR]: node.children[item].id, [REORDER_DEPTH_ATTR]: 0, [REORDER_INDENT_ATTR]: 0 }
    const it = tpl!.items[item - childCount]
    const data = getPointer(ctx.data, it.path) as Record<string, unknown> | undefined
    const depth = Math.max(0, Math.floor(num(data?.depth, 0)))
    return { [REORDER_KEY_ATTR]: it.key, [REORDER_DEPTH_ATTR]: depth, [REORDER_GROUP_ATTR]: data?.group === true ? `true` : undefined, [REORDER_INDENT_ATTR]: depth * TREE_GUIDE_COLUMN }
  }
  const itemAttrs = (r: number): Record<string, unknown> => {
    const row = rows[r]
    // A section header is a listitem holding its heading (a list owns
    // listitems only); items carry their position among the ITEMS.
    return `item` in row ? { role: itemRole, "aria-setsize": itemCount, "aria-posinset": row.item + 1, ...(reorderable ? reorderAttrs(row.item) : {}) } : { role: itemRole }
  }
  // A press on one of this list's own rows (never inside a field) may become
  // the drag; the drop fires `move` {id, before | after | into}.
  const onPointerDown = reorderable
    ? (e: ReactPointerEvent<HTMLDivElement>) => {
        const start = listReorderStart(e.currentTarget, e.target)
        if (!start) return
        const root = e.currentTarget
        startListDrag(e.nativeEvent, start.key, { rows: () => listReorderRows(root), maxDepth: Math.max(1, Math.floor(num(props.maxDepth, 1))) }, { move: (context) => void emit(`move`, context) })
      }
    : undefined
  const divider = (r: number) => {
    const before = dividerBefore(r)
    return before === null ? null : <div {...(part.at(`divider`, before) as Record<string, string>)} role="separator" />
  }
  if (windowed) {
    const headerRows = sticky ? rows.flatMap((row, r) => (`header` in row ? [r] : [])) : undefined
    return (
      <div ref={rootRef} {...(rootProps as Record<string, unknown>)} style={style} role={listRole} onPointerDown={onPointerDown}>
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
    <div ref={rootRef} {...(rootProps as Record<string, unknown>)} style={style} role={listRole} onPointerDown={onPointerDown}>
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
    const data = readView(ctx, pointer, value)
    const setData = (p: string, v: unknown) => {
      if (p !== pointer && !p.startsWith(`${pointer}/`)) ctx.setData(p, v)
    }
    return deriveContext(ctx, { setData }, data)
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
