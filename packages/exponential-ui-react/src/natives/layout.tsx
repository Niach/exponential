// VAPP-87 + round 1: Box, List and the Unknown placeholder. A List renders
// its children then its data template's items (keyed by `template.key`, so
// reordering keeps each item's state); past `WINDOW_THRESHOLD` (50, the
// catalog's) items a vertical List windows.

import { type KeyboardEvent, type ReactNode } from "react"
import { WINDOW_THRESHOLD as CATALOG_WINDOW_THRESHOLD } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import { WindowedList } from "../list"
import type { NativeProps } from "../node-view"
import { NodeView, TemplateItemView, mergeStyle, templateItems } from "../node-view"
import { bool, BuiltinIcon, str, useParts, type PartFn } from "./shared"

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
 *  snapshot every row). The catalog's number (`chart.ts`). */
export const WINDOW_THRESHOLD = CATALOG_WINDOW_THRESHOLD

export function ListNative({ node, props, rootProps, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const divided = bool(props.divided)
  const horizontal = props.direction === `horizontal`
  const gap = str(props.gap, `none`)
  const tpl = templateItems(ctx, node, scope)
  const count = node.children.length + (tpl?.items.length ?? 0)
  const style = mergeStyle(rootProps, { gap: gap === `none` ? undefined : `var(--xui-spacing-${gap})` })
  const keyOf = (i: number) => (i < node.children.length ? node.children[i].id : `t:${tpl!.items[i - node.children.length].key}`)
  const windowed = count > WINDOW_THRESHOLD && !horizontal
  if (windowed) {
    return (
      <div {...(rootProps as Record<string, unknown>)} style={style} role="list">
        <WindowedList count={count} itemKey={keyOf} estimatedItemHeight={40} overscan={6} renderItem={(i) => <ListItem index={i} node={node} tpl={tpl} divided={divided} part={part} />} />
      </div>
    )
  }
  const rows: ReactNode[] = []
  for (let i = 0; i < count; i++) rows.push(<ListItem key={keyOf(i)} index={i} node={node} tpl={tpl} divided={divided} part={part} />)
  return (
    <div {...(rootProps as Record<string, unknown>)} style={style} role="list">
      {rows}
    </div>
  )
}

function ListItem({ index, node, tpl, divided, part }: { index: number; node: NativeProps[`node`]; tpl: ReturnType<typeof templateItems>; divided: boolean; part: PartFn }) {
  const divider = divided && index > 0 ? <div {...(part(`divider`) as Record<string, string>)} role="separator" /> : null
  if (index < node.children.length) {
    return (
      <>
        {divider}
        <NodeView node={node.children[index]} />
      </>
    )
  }
  const item = tpl?.items[index - node.children.length]
  if (!tpl || !item) return null
  return (
    <>
      {divider}
      <TemplateItemView tpl={tpl.tpl} item={item} />
    </>
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
