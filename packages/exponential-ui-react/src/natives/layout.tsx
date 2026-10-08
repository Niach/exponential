// VAPP-87: Box, List and the Unknown placeholder.

import { useContext, useMemo, type KeyboardEvent, type ReactNode } from "react"
import { ScopeContext, useSurfaceContext } from "../context"
import { absolutePath, getPointer } from "../data"
import { WindowedList } from "../list"
import type { NativeProps } from "../node-view"
import { NodeView } from "../node-view"
import { bool, str, useParts } from "./shared"

export function BoxNative({ node, props, rootProps, emit, children }: NativeProps) {
  const pressable = bool(props.pressable) || Boolean(node.on?.press)
  if (!pressable) return <div {...(rootProps as Record<string, unknown>)}>{children}</div>
  const onKey = (e: KeyboardEvent) => {
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
 *  snapshot every row). */
export const WINDOW_THRESHOLD = 24

export function ListNative({ node, props, rootProps, scope }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const divided = bool(props.divided)
  const horizontal = props.direction === `horizontal`
  const gap = str(props.gap, `none`)
  const templateItems = useMemo(() => {
    if (!node.template) return null
    const path = absolutePath(node.template.path, scope)
    const items = getPointer(ctx.data, path)
    const tpl = ctx.templateNode(node.template.component)
    if (!Array.isArray(items) || !tpl) return null
    return { path, tpl, count: items.length }
  }, [node.template, scope, ctx])
  const count = node.children.length + (templateItems?.count ?? 0)
  const style = { gap: gap === `none` ? undefined : `var(--xui-spacing-${gap})` }
  const windowed = count > WINDOW_THRESHOLD && !horizontal
  if (windowed) {
    const keys: string[] = [...node.children.map((c) => c.id), ...Array.from({ length: templateItems?.count ?? 0 }, (_, i) => `${templateItems!.tpl.id}.${i}`)]
    return (
      <div {...(rootProps as Record<string, unknown>)} style={style} role="list">
        <WindowedList
          count={count}
          itemKey={(i) => keys[i]}
          estimatedItemHeight={40}
          overscan={6}
          renderItem={(i) => (
            <ListItem index={i} node={node} templateItems={templateItems} divided={divided} part={part} />
          )}
        />
      </div>
    )
  }
  const rows: ReactNode[] = []
  for (let i = 0; i < count; i++) {
    if (i > 0 && divided) rows.push(<div key={`d${i}`} {...(part(`divider`) as Record<string, string>)} role="separator" />)
    rows.push(<ListItem key={i} index={i} node={node} templateItems={templateItems} divided={false} part={part} />)
  }
  return (
    <div {...(rootProps as Record<string, unknown>)} style={style} role="list">
      {rows}
    </div>
  )
}

function ListItem({ index, node, templateItems, divided, part }: { index: number; node: NativeProps[`node`]; templateItems: { path: string; tpl: NativeProps[`node`]; count: number } | null; divided: boolean; part: ReturnType<typeof useParts> }) {
  const scope = useContext(ScopeContext)
  void scope
  const divider = divided && index > 0 ? <div {...(part(`divider`) as Record<string, string>)} role="separator" /> : null
  if (index < node.children.length) {
    return (
      <>
        {divider}
        <NodeView node={node.children[index]} />
      </>
    )
  }
  const i = index - node.children.length
  if (!templateItems) return null
  return (
    <>
      {divider}
      <ScopeContext.Provider value={`${templateItems.path}/${i}`}>
        <NodeView node={suffix(templateItems.tpl, `.${i}`)} />
      </ScopeContext.Provider>
    </>
  )
}

function suffix(node: NativeProps[`node`], s: string): NativeProps[`node`] {
  const out = { ...node, id: `${node.id}${s}`, children: node.children.map((c) => suffix(c, s)) }
  if (node.slots) {
    out.slots = {}
    for (const [k, v] of Object.entries(node.slots)) out.slots[k] = suffix(v, s)
  }
  return out
}

export function UnknownNative({ node, props, rootProps }: NativeProps) {
  const part = useParts(node, props)
  return (
    <div {...(rootProps as Record<string, unknown>)} role="note">
      <span {...(part(`label`) as Record<string, string>)}>Unknown component {str(props.component, node.component)}</span>
    </div>
  )
}
