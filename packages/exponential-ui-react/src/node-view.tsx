// VAPP-87: ONE normalized node → one React element. Resolves the node's
// bindings for this render, picks its painter (an extension component, a
// painter override, or the native), hands it the root attributes (the node
// class, the theme scope, the recipe part classes, the `data-r-*`/`data-m-*`
// recipe props the theme's selectors key on, the forced states) and an
// emitter for its `on` handlers. Children render recursively; a data
// template renders its component once per item, each under its own scope.

import { Fragment, useCallback, useContext, useEffect, useMemo, type ReactNode } from "react"
import { UNKNOWN_COMPONENT, nativeRecipeProps } from "@exponential-at/ui"
import type { UiNode } from "@exponential-at/ui"
import { nodeClass } from "./box-css"
import { ScopeContext, useSurfaceContext } from "./context"
import { CLIENT_FUNCTIONS, absolutePath, getPointer, resolveProps, resolveValue } from "./data"
import { extensionComponent, extensionMacroNames } from "./extensions"
import type { ExtensionComponentProps, SurfaceActionEvent } from "./host"
import { NATIVES } from "./natives"
import { isMacroComponent, partClass, propAttribute } from "./theme-css"

export interface NativeProps extends ExtensionComponentProps {
  /** The data scope (template item) this node renders under. */
  scope: string
}

const CONTAINERS = new Set([`Box`, `List`])

/** A template item: ids suffixed per item (`row.0`), the node sheet's class
 *  kept on `styleId` (the template component's own id) so every item wears
 *  its styles. */
export function suffixIds(node: UiNode, suffix: string): UiNode {
  const out: UiNode & { styleId?: string } = { ...node, id: `${node.id}${suffix}`, styleId: styleIdOf(node), children: node.children.map((c) => suffixIds(c, suffix)) }
  if (node.slots) {
    out.slots = {}
    for (const [k, v] of Object.entries(node.slots)) out.slots[k] = suffixIds(v, suffix)
  }
  return out
}

/** The id a node's style rules are keyed by. */
export function styleIdOf(node: UiNode): string {
  return (node as UiNode & { styleId?: string }).styleId ?? node.id
}

/** The attributes every painted root carries. */
export function useRootProps(node: UiNode, props: Record<string, unknown>): Record<string, unknown> {
  const ctx = useSurfaceContext()
  return useMemo(() => {
    const macros = extensionMacroNames(ctx.extensions)
    const classes = [`xui-el`, nodeClass(styleIdOf(node)), ctx.compiled.scope, partClass(node.component, `root`)]
    const attrs: Record<string, unknown> = {}
    if (node.recipe) {
      classes.push(partClass(node.recipe.macro, node.recipe.part))
      attrs[`data-xui-part`] = `${node.recipe.macro}/${node.recipe.part}`
      const macro = isMacroComponent(node.recipe.macro, macros)
      for (const [k, v] of Object.entries(node.recipe.props)) if (v !== undefined && v !== null) attrs[propAttribute(macro, k)] = String(v)
    }
    for (const [k, v] of Object.entries(nativeRecipeProps(node.component, props, ctx.extensionDefs))) attrs[propAttribute(false, k)] = String(v)
    if (ctx.states.length) attrs[`data-xs`] = ctx.states.join(` `)
    if (node.accessibility?.label !== undefined) attrs[`aria-label`] = String(resolveValue(node.accessibility.label, { data: ctx.data }) ?? ``)
    if (node.accessibility?.description !== undefined) attrs[`aria-description`] = String(resolveValue(node.accessibility.description, { data: ctx.data }) ?? ``)
    return { className: classes.join(` `), "data-xui-id": node.id, "data-xui-c": node.component, ...attrs }
  }, [node, props, ctx.compiled.scope, ctx.states, ctx.extensions, ctx.extensionDefs, ctx.data])
}

/** `className` on top of the renderer's own. */
export function withClass(rootProps: Record<string, unknown>, ...extra: (string | false | undefined | null)[]): Record<string, unknown> {
  const more = extra.filter(Boolean).join(` `)
  return more ? { ...rootProps, className: `${rootProps.className as string} ${more}` } : rootProps
}

/** Forced states on a part element (`data-xs`). */
export function forced(states: readonly string[], ...extra: (string | false | undefined)[]): string | undefined {
  const all = [...states, ...extra.filter((s): s is string => Boolean(s))]
  return all.length ? all.join(` `) : undefined
}

export function useEmitter(node: UiNode, scope: string) {
  const ctx = useSurfaceContext()
  return useCallback(
    (event: string, payload?: Record<string, unknown>): Promise<void> | void => {
      const action = node.on?.[event]
      if (!action) return undefined
      const resolveCtx = { data: ctx.data, scope, functions: ctx.functions, openUrl: ctx.openUrl }
      // A2UI's `functionCall` (the core also reads the legacy `function`).
      const fn = ((action as { functionCall?: unknown }).functionCall ?? action.function) as { call?: unknown; args?: unknown } | undefined
      if (fn) {
        const name = typeof fn.call === `string` ? fn.call : ``
        if (name in CLIENT_FUNCTIONS) {
          resolveValue(fn, resolveCtx)
          return undefined
        }
        const args = (resolveValue(fn.args ?? {}, resolveCtx) as Record<string, unknown>) ?? {}
        const out = ctx.host.onFunctionCall?.({ surfaceId: ctx.surfaceId, componentId: node.id, name, args })
        return out instanceof Promise ? out.then(() => undefined) : undefined
      }
      if (action.event) {
        const context = (resolveValue(action.event.context ?? {}, resolveCtx) as Record<string, unknown>) ?? {}
        const e: SurfaceActionEvent = {
          surfaceId: ctx.surfaceId,
          event,
          name: action.event.name,
          componentId: node.id,
          context,
          payload,
          timestamp: new Date().toISOString(),
        }
        return ctx.host.onAction?.(e) ?? undefined
      }
      return undefined
    },
    [node, ctx, scope]
  )
}

export function NodeView({ node }: { node: UiNode }) {
  const ctx = useSurfaceContext()
  const scope = useContext(ScopeContext)
  const props = useMemo(() => resolveProps(node.props, { data: ctx.data, scope, functions: ctx.functions, openUrl: ctx.openUrl }), [node.props, ctx.data, scope, ctx.functions, ctx.openUrl])
  const rootProps = useRootProps(node, props)
  const emit = useEmitter(node, scope)
  const Override = extensionComponent(ctx.extensions, node.component)
  const Native = NATIVES[node.component]
  const Painter = Override ?? Native ?? NATIVES[UNKNOWN_COMPONENT]
  const unknown = node.component === UNKNOWN_COMPONENT || (!Native && !Override)
  const onUnknown = ctx.host.onUnknown
  useEffect(() => {
    if (unknown) onUnknown?.(node)
  }, [unknown, onUnknown, node])

  // Geometry mode: a leaf is a fixed box (the core's fake measure) with
  // NO recipe on it — only its node style lays out, like the core.
  if (ctx.measure && !CONTAINERS.has(node.component)) {
    const size = ctx.measure(node)
    if (size) {
      return (
        <div className={`xui-el ${nodeClass(styleIdOf(node))} xui-leaf`} data-xui-id={node.id} data-xui-c={node.component}>
          <div className="xui-measured" style={{ width: size.w, height: size.h }} />
        </div>
      )
    }
  }

  const children: ReactNode = (
    <>
      {node.children.map((child) => (
        <NodeView key={child.id} node={child} />
      ))}
      {node.template ? <TemplateChildren node={node} scope={scope} /> : null}
    </>
  )
  const slots: Record<string, ReactNode> = {}
  for (const [name, slot] of Object.entries(node.slots ?? {})) slots[name] = <NodeView key={slot.id} node={slot} />

  const painterProps: NativeProps = { node, props, theme: ctx.theme, mode: ctx.mode, tokens: ctx.theme.tokens, children, slots, emit, rootProps, scope }
  return <Painter {...painterProps} />
}

function TemplateChildren({ node, scope }: { node: UiNode; scope: string }) {
  const ctx = useSurfaceContext()
  const template = node.template!
  const path = absolutePath(template.path, scope)
  const items = getPointer(ctx.data, path)
  const tpl = ctx.templateNode(template.component)
  if (!Array.isArray(items) || !tpl) return null
  return (
    <>
      {items.map((_, i) => (
        <ScopeContext.Provider key={i} value={`${path}/${i}`}>
          <NodeView node={suffixIds(tpl, `.${i}`)} />
        </ScopeContext.Provider>
      ))}
    </>
  )
}

export { Fragment }
