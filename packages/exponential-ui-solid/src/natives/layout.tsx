// FEED-128 (React's natives/layout.tsx): Box, List and the Unknown
// placeholder. A List renders its children then its data template's items
// (keyed by `template.key`), optionally in sections; past
// `WINDOW_THRESHOLD` (50, the catalog's) items it windows on its axis.

import { For, Show, createEffect, createMemo, onCleanup, untrack, type Accessor, type JSX } from "solid-js"
import { listSections, sectionRows, WINDOW_THRESHOLD as CATALOG_WINDOW_THRESHOLD, type ListSection, type UiNode } from "@exponential_app/eui"
import { getPointer, LITERAL_ROWS_ROOT, setPointer } from "@exponential_app/eui/web"
import { InstanceContext, ScopeContext, SurfaceContext, deriveContext, useInstance, useSurfaceContext } from "../context"
import { WindowedList, type WindowedListHandle } from "../list"
import { NodeView, TemplateItemView, latest, mergeStyle, templateItems, type NativeProps } from "../node-view"
import { css } from "../style"
import { bool, BuiltinIcon, num, parts, str, type PartFn } from "./shared"

export function BoxNative(props: NativeProps): JSX.Element {
  const pressable = () => bool(props.props.pressable) || Boolean(props.node.on?.press)
  const onKey = (e: KeyboardEvent) => {
    if (e.target !== e.currentTarget) return
    if (e.key === `Enter` || e.key === ` `) {
      e.preventDefault()
      void props.emit(`press`)
    }
  }
  return (
    <Show when={pressable()} fallback={<div {...props.rootProps}>{props.children}</div>}>
      <div {...props.rootProps} data-pressable="true" role="button" tabindex="0" onClick={() => void props.emit(`press`)} onKeyDown={onKey}>
        {props.children}
      </div>
    </Show>
  )
}

/** Past this many items a List windows. The catalog's number. */
export const WINDOW_THRESHOLD = CATALOG_WINDOW_THRESHOLD

/** One row of a List: an item (a child or a template item, by its index in
 *  the list) or a section header. */
type ListRow = { item: number } | { header: number }

/** List (round 2 §5): children then the data template's items; `sectionBy`
 *  groups consecutive template items under a `section` header; past
 *  `WINDOW_THRESHOLD` items, or with `stickyHeaders`, the list WINDOWS.
 *  Every item is a listitem with its position in the whole list;
 *  `scrollToIndex` (the host command) takes the data index. */
export function ListNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const divided = () => bool(props.props.divided)
  const horizontal = () => props.props.direction === `horizontal`
  const gap = () => str(props.props.gap, `none`)
  const tokens = () => ctx.theme.tokens as unknown as { spacing?: Record<string, number>; control?: Record<string, number> }
  const gapPx = () => (gap() === `none` ? 0 : num(tokens().spacing?.[gap()], 0))
  const hairline = () => num(tokens().control?.hairline, 1)
  const rowExtent = () => num(tokens().control?.row, 40)
  const tpl = createMemo(() => templateItems(ctx, props.node, props.scope))
  const childCount = () => props.node.children.length
  const itemCount = () => childCount() + (tpl()?.items.length ?? 0)
  const sectionBy = () => str(props.props.sectionBy)
  const sections = createMemo(() => {
    const t = tpl()
    return sectionBy() && t ? listSections(t.items.map((item) => getPointer(ctx.data, item.path)), sectionBy()) : null
  })
  const rows = createMemo((): ListRow[] => {
    const out: ListRow[] = []
    const cc = childCount()
    for (let i = 0; i < cc; i++) out.push({ item: i })
    const s = sections()
    if (s) for (const r of sectionRows(s)) out.push(`header` in r ? r : { item: cc + r.item })
    else for (let i = cc, n = itemCount(); i < n; i++) out.push({ item: i })
    return out
  })
  const sticky = () => bool(props.props.stickyHeaders) && sections() !== null
  const windowed = () => itemCount() > WINDOW_THRESHOLD || sticky()
  // §5: rows sit `gap` apart; a divided list adds the hairline to the gap
  // and centres the divider in it.
  const step = () => gapPx() + (divided() ? hairline() : 0)
  const flatGap = () => (gap() === `none` ? (divided() ? `${hairline()}px` : undefined) : divided() ? `calc(var(--xui-spacing-${gap()}) + ${hairline()}px)` : `var(--xui-spacing-${gap()})`)
  const style = () => mergeStyle(props.rootProps, { gap: windowed() ? undefined : flatGap() })
  const rtl = () => horizontal() && ctx.direction === `rtl`
  // Every row's key in one pass per rows change (VAPP-148): the windowed
  // list asks for all 100,000 on each measurement, and a lookup through the
  // node's props per row was most of a jump.
  const rowKeys = createMemo(() => {
    const cc = childCount()
    const children = props.node.children
    const items = tpl()?.items
    return rows().map((row) => (`header` in row ? `s:${row.header}` : row.item < cc ? children[row.item].id : `t:${items![row.item - cc].key}`))
  })
  const keyOf = (r: number) => rowKeys()[r]
  const dividerBefore = (r: number): number | null => {
    if (!divided() || r === 0) return null
    const row = rows()[r]
    const prev = rows()[r - 1]
    return `item` in row && `item` in prev ? row.item : null
  }
  let windowHandle: WindowedListHandle | undefined
  let rootEl: HTMLDivElement | undefined
  const rowOfIndex = (index: number) => {
    const item = tpl() ? childCount() + index : index
    return rows().findIndex((r) => `item` in r && r.item === item)
  }
  createEffect(() => {
    const id = props.domId
    onCleanup(
      ctx.registerScroller(id, (index, align) => {
        const r = rowOfIndex(index)
        if (r < 0) return
        if (windowed()) windowHandle?.scrollToIndex(r, align)
        else {
          // Own rows only (`:scope >`): a nested List has rows of its own.
          const el = rootEl?.querySelector<HTMLElement>(`:scope > [data-xui-row="${r}"]`)
          const block = align === `center` ? `center` : align === `end` ? `end` : align === `start` ? `start` : `nearest`
          el?.scrollIntoView(horizontal() ? { inline: block, block: `nearest` } : { block, inline: `nearest` })
        }
      })
    )
  })
  const renderRow = (r: Accessor<number>) => <ListRowView row={rows()[r()]} node={props.node} tpl={tpl()} sections={sections()} childCount={childCount()} part={part} />
  // Round 3: a Section body with `tree` asks for role `tree`.
  const listRole = () => ((props.rootProps as Record<string, unknown>).role === `tree` ? `tree` : `list`)
  const itemRole = () => (listRole() === `tree` ? `treeitem` : `listitem`)
  const itemAttrs = (r: number): Record<string, unknown> => {
    const row = rows()[r]
    return `item` in row ? { role: itemRole(), "aria-setsize": itemCount(), "aria-posinset": row.item + 1 } : { role: itemRole() }
  }
  const divider = (r: number) => {
    const before = dividerBefore(r)
    return before === null ? null : <div {...part.at(`divider`, before)} role="separator" />
  }
  const axis = () => (horizontal() ? `horizontal` : `vertical`)
  // Flat rows, keyed like React's (`keyOf`).
  const flatKeys = createMemo(() => rowKeys(), undefined, { equals: (a, b) => a.length === b.length && a.every((k, i) => k === b[i]) })
  const flatIndex = createMemo(() => new Map(flatKeys().map((k, r) => [k, r])))
  return (
    <Show
      when={windowed()}
      fallback={
        <div ref={rootEl} {...props.rootProps} style={style()} role={listRole()}>
          <For each={flatKeys()}>
            {(key) => {
              const r = createMemo<number>((prev) => flatIndex().get(key) ?? prev!)
              const line = createMemo(() => divider(r()))
              return (
                <div class="xui-list-row" data-axis={axis()} data-xui-row={r()} {...itemAttrs(r())}>
                  {line() ? (
                    <div class="xui-list-gap" aria-hidden="true" data-axis={axis()} style={css(horizontal() ? { [rtl() ? `right` : `left`]: -(step() / 2) } : { top: -(step() / 2) })}>
                      {line()}
                    </div>
                  ) : null}
                  {renderRow(r)}
                </div>
              )
            }}
          </For>
        </div>
      }
    >
      <div ref={rootEl} {...props.rootProps} style={style()} role={listRole()}>
        <WindowedList
          ref={(h) => (windowHandle = h)}
          count={rows().length}
          itemKey={keyOf}
          estimatedItemHeight={rowExtent()}
          axis={axis()}
          direction={ctx.direction}
          gap={step()}
          divider={divider}
          stickyRows={sticky() ? rows().flatMap((row, r) => (`header` in row ? [r] : [])) : undefined}
          itemProps={itemAttrs}
          renderItem={renderRow}
        />
      </div>
    </Show>
  )
}

function ListRowView(props: { row: ListRow; node: UiNode; tpl: ReturnType<typeof templateItems>; sections: ListSection[] | null; childCount: number; part: PartFn }): JSX.Element {
  // The row's identity as a primitive: a data change hands new row objects
  // for the same rows and must not repaint them.
  const kind = createMemo(() => {
    const row = props.row
    if (`header` in row) return `h${row.header}`
    return row.item < props.childCount ? `c${row.item}` : `t${row.item - props.childCount}`
  })
  const paint = (k: string): JSX.Element => {
    const n = Number(k.slice(1))
    if (k[0] === `h`)
      return (
        <Show when={props.sections?.[n]}>
          {(() => {
            const section = latest(() => props.sections?.[n])
            return (
              <div {...props.part.at(`section`, n)} role="heading" aria-level={3}>
                {props.node.slots?.section ? (
                  <LiteralScope value={{ value: section().value, count: section().count, index: n }} pointer={`${LITERAL_ROWS_ROOT}/${props.node.id.replace(/[~/]/g, `_`)}/sections/${n}`} suffix={`.${n}`} node={props.node.slots.section} />
                ) : (
                  (section().value as JSX.Element)
                )}
              </div>
            )
          })()}
        </Show>
      )
    if (k[0] === `c`) return <NodeView node={props.node.children[n]} />
    return (
      <Show when={props.tpl && props.tpl.items[n]}>
        {(() => {
          const tpl = latest(() => props.tpl)
          const item = latest(() => props.tpl?.items[n])
          return <TemplateItemView tpl={tpl().tpl} item={item()} />
        })()}
      </Show>
    )
  }
  return <>{(() => {
    const k = kind()
    return untrack(() => paint(k))
  })()}</>
}

/** A node bound against a LITERAL scope (a List section header's `{value,
 *  count, index}`): a read view of the data model at a synthetic pointer
 *  (writes under it go nowhere). */
function LiteralScope(props: { value: unknown; pointer: string; suffix: string; node: UiNode }): JSX.Element {
  const ctx = useSurfaceContext()
  const outer = useInstance()
  const view = deriveContext(ctx, {
    data: () => setPointer(ctx.data as Record<string, unknown>, props.pointer, props.value),
    setData: () => (p: string, v: unknown) => {
      if (p !== props.pointer && !p.startsWith(`${props.pointer}/`)) ctx.setData(p, v)
    },
  })
  return (
    <SurfaceContext.Provider value={view}>
      <ScopeContext.Provider value={() => props.pointer}>
        <InstanceContext.Provider value={() => `${outer()}${props.suffix}`}>
          <NodeView node={props.node} />
        </InstanceContext.Provider>
      </ScopeContext.Provider>
    </SurfaceContext.Provider>
  )
}

export function UnknownNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  return (
    <div {...props.rootProps} role="note" data-xui-unknown={str(props.props.component, props.node.component)}>
      <BuiltinIcon slot="Unknown.root" size={16} />
      <span {...part(`label`)}>
        {ctx.t(`unknownComponent`)} {str(props.props.component, props.node.component)}
      </span>
    </div>
  )
}
