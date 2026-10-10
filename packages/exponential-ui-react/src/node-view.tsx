// VAPP-87 + round 1: ONE normalized node → one React element. The BIND pass
// of the contract (§1, `@exponential-at/ui` `bindTree` is the reference)
// runs here per render: a node whose `visible` resolves falsy is not
// rendered at all; props, dynamic style values (the expander's `percent{}`
// for a bound Progress) and macro recipe props resolve against the data
// model, `$string.<id>` through the surface's string table; a RESPONSIVE
// prop of a native (`Drawer.side`) takes its value at the surface's active
// breakpoint before the recipe sees it. Then the painter is picked (an
// extension component, a painter override, or the native) and handed the
// root attributes (the node class, the theme scope, the recipe part
// classes, the `data-r-*`/`data-m-*` recipe props, the forced states) and
// an emitter that runs an action in the contract's order: resolve the
// function args and the event context against the data AS IT IS, apply
// `set`, then dispatch the event. A data template renders its component
// once per item under the item's scope, keyed by `template.key`.

import { Component, Fragment, memo, useCallback, useContext, useEffect, useMemo, type CSSProperties, type ReactNode } from "react"
import { UNKNOWN_COMPONENT, catalogView, instanceSegment, isResponsiveValue, nativeRecipeProps, responsiveAt, templateItemKeys, templateSiteKey, withOwnWrites } from "@exponential-at/ui"
import type { ExtensionDef, UiNode } from "@exponential-at/ui"
import { dynamicStyleEntries, nodeClass, queryToken, styleDirection } from "./box-css"
import { InstanceContext, ScopeContext, SurfaceContext, resolveContextOf, useSurfaceContext, type SurfaceContextValue } from "./context"
import { absolutePath, ariaAttributes, getPointer, resolveAccessibility, resolveNodeProps, resolveValue, resolveVisible, type ResolveContext } from "./data"
import { extensionComponent, extensionMacroNames } from "./extensions"
import type { ExtensionComponentProps, SurfaceActionEvent } from "./host"
import { NATIVES } from "./natives"
import { cssValue, isMacroComponent, partClass, propAttribute } from "./theme-css"

export interface NativeProps extends ExtensionComponentProps {
  /** The data scope (template item) this node renders under. */
  scope: string
  /** The node's id as painted (`data-xui-id`): its id + the template
   *  instance suffix. DOM ids derive from it (`<id>.error`). */
  domId: string
}

const CONTAINERS = new Set([`Box`, `List`])

const responsiveCache = new WeakMap<readonly ExtensionDef[], Map<string, string[]>>()

/** The props of a component marked `responsive: true` in the catalog. */
function responsiveProps(component: string, extensions: readonly ExtensionDef[]): string[] {
  let byComponent = responsiveCache.get(extensions)
  if (!byComponent) {
    byComponent = new Map()
    responsiveCache.set(extensions, byComponent)
  }
  let hit = byComponent.get(component)
  if (!hit) {
    const def = catalogView(extensions as ExtensionDef[]).components[component]
    hit = def ? Object.entries(def.props).filter(([, s]) => s.responsive).map(([n]) => n) : []
    byComponent.set(component, hit)
  }
  return hit
}

/** The id a node's style rules are keyed by. */
export function styleIdOf(node: UiNode): string {
  return (node as UiNode & { styleId?: string }).styleId ?? node.id
}

/** The attributes every painted root carries. */
export function useRootProps(node: UiNode, props: Record<string, unknown>, domId: string = node.id, recipeProps?: Record<string, unknown>, style?: CSSProperties): Record<string, unknown> {
  const ctx = useSurfaceContext()
  const scope = useContext(ScopeContext)
  return useMemo(() => {
    const macros = extensionMacroNames(ctx.extensions)
    const classes = [`xui-el`, nodeClass(styleIdOf(node)), ctx.compiled.scope, partClass(node.component, `root`)]
    const attrs: Record<string, unknown> = {}
    if (node.recipe) {
      classes.push(partClass(node.recipe.macro, node.recipe.part))
      attrs[`data-xui-part`] = `${node.recipe.macro}/${node.recipe.part}`
      const macro = isMacroComponent(node.recipe.macro, macros)
      for (const [k, v] of Object.entries(recipeProps ?? node.recipe.props)) if (v !== undefined && v !== null && typeof v !== `object`) attrs[propAttribute(macro, k)] = String(v)
    }
    for (const [k, v] of Object.entries(nativeRecipeProps(node.component, props, ctx.extensionDefs))) if (typeof v !== `object`) attrs[propAttribute(false, k)] = String(v)
    if (ctx.states.length) attrs[`data-xs`] = ctx.states.join(` `)
    // Contract §6: the node's accessibility (author label/description + a
    // macro part's role and states) resolved, then mapped to ARIA. A
    // painter's own role/aria-* set after the spread wins.
    if (node.accessibility) Object.assign(attrs, ariaAttributes(resolveAccessibility(node.accessibility, { data: ctx.data, scope, functions: ctx.functions, locale: ctx.locale, strings: ctx.strings })))
    if (style) attrs.style = style
    // Round 2: a node's own direction is also the `dir` attribute (`:dir()`
    // selectors, the bidi paragraph direction of its text).
    const dir = ownDirection(node, ctx)
    if (dir) attrs.dir = dir
    return { className: classes.join(` `), "data-xui-id": domId, "data-xui-c": node.component, ...attrs }
  }, [node, props, domId, recipeProps, style, scope, ctx.compiled.scope, ctx.states, ctx.extensions, ctx.extensionDefs, ctx.data, ctx.functions, ctx.locale, ctx.strings, ctx.theme, ctx.xq])
}

/** `className` on top of the renderer's own. */
export function withClass(rootProps: Record<string, unknown>, ...extra: (string | false | undefined | null)[]): Record<string, unknown> {
  const more = extra.filter(Boolean).join(` `)
  return more ? { ...rootProps, className: `${rootProps.className as string} ${more}` } : rootProps
}

/** A painter's own inline style merged over the root's (the dynamic style
 *  variables live there and must survive). */
export function mergeStyle(rootProps: Record<string, unknown>, style: CSSProperties | undefined): CSSProperties | undefined {
  const own = rootProps.style as CSSProperties | undefined
  if (!own) return style
  if (!style) return own
  return { ...own, ...style }
}

/** Forced states on a part element (`data-xs`). */
export function forced(states: readonly string[], ...extra: (string | false | undefined)[]): string | undefined {
  const all = [...states, ...extra.filter((s): s is string => Boolean(s))]
  return all.length ? all.join(` `) : undefined
}

/** Run one of a node's actions (round-1 contract §1 `runAction`): the
 *  function args AND the event context resolve against the data WITH the
 *  component's own write applied (round 4 `withOwnWrites`: `own` = what
 *  the component just wrote, by prop, so the context of an Input's
 *  `change` reads the text just typed, even before React has re-rendered
 *  the write), then `set` writes (relative paths against the node's scope), any other
 *  function runs, then the event reaches the host with the component's
 *  payload merged OVER the author's context (the payload wins a clashing
 *  key, like the Rust core's `fire`: the author's context was resolved
 *  before the write, the payload carries the value just written). */
export function runNodeAction(ctx: SurfaceContextValue, node: UiNode, domId: string, scope: string, event: string, payload?: Record<string, unknown>, own?: Record<string, unknown>): Promise<void> | void {
  const action = node.on?.[event]
  if (!action) return undefined
  const base = resolveContextOf(ctx, scope)
  const rctx: ResolveContext = own ? { ...base, data: withOwnWrites(ctx.data, node.props, own, scope ? { base: scope } : {}) } : base
  const fn = action.functionCall
  const args = fn ? ((resolveValue(fn.args ?? {}, rctx) as Record<string, unknown>) ?? {}) : undefined
  const context = action.event ? ((resolveValue(action.event.context ?? {}, rctx) as Record<string, unknown>) ?? {}) : undefined
  let pending: unknown
  if (fn && args) {
    if (fn.call === `set`) {
      if (typeof args.path === `string`) ctx.setData(absolutePath(args.path, scope), args.value)
    } else {
      const f = ctx.functions[fn.call]
      // A name outside the client functions is a HOST function (VAPP-91):
      // the host's registry + policy gate (`ExponentialHost.callFunction`).
      if (f) f(args, rctx)
      else pending = ctx.host.onFunctionCall?.({ surfaceId: ctx.surfaceId, componentId: domId, name: fn.call, args })
    }
  }
  if (pending instanceof Promise) {
    // The source control stays pending until the host function settles.
    const sent = action.event ? runNodeEvent(ctx, action.event, domId, event, context, payload) : undefined
    return Promise.all([pending, sent]).then(() => undefined)
  }
  if (action.event) return runNodeEvent(ctx, action.event, domId, event, context, payload)
  return undefined
}

function runNodeEvent(ctx: SurfaceContextValue, ev: { name: string }, domId: string, event: string, context: Record<string, unknown> | undefined, payload?: Record<string, unknown>): Promise<void> | void {
  const e: SurfaceActionEvent = {
    surfaceId: ctx.surfaceId,
    event,
    name: ev.name,
    componentId: domId,
    context: { ...(context ?? {}), ...(payload ?? {}) },
    payload,
    timestamp: new Date().toISOString(),
  }
  return ctx.host.onAction?.(e) ?? undefined
}

export function useEmitter(node: UiNode, scope: string, domId: string = node.id) {
  const ctx = useSurfaceContext()
  return useCallback((event: string, payload?: Record<string, unknown>, own?: Record<string, unknown>): Promise<void> | void => runNodeAction(ctx, node, domId, scope, event, payload, own), [node, ctx, scope, domId])
}

/** A node, or nothing when its `visible` resolves falsy (no layout, not in
 *  the a11y tree). Memoized on the node: a parent re-render does not cascade;
 *  a context change (data, breakpoint, theme…) still reaches every node. */
export const NodeView = memo(function NodeView({ node }: { node: UiNode }) {
  const ctx = useSurfaceContext()
  const scope = useContext(ScopeContext)
  if (node.visible !== undefined && !resolveVisible(node.visible, resolveContextOf(ctx, scope))) return null
  const own = ownDirection(node, ctx)
  if (own && own !== ctx.direction) return <DirectedNode node={node} scope={scope} direction={own} />
  return <BoundNode node={node} scope={scope} />
})

/** A node's own style `direction` (round 2 §2: any node, inherited),
 *  matching `@media` blocks included (`nodeDirections` is the reference). */
export function ownDirection(node: UiNode, ctx?: Pick<SurfaceContextValue, `theme` | `xq`>): `ltr` | `rtl` | undefined {
  if (!node.style) return undefined
  const tokens = ctx?.xq ? ctx.xq.split(` `) : []
  return styleDirection(node.style, ctx?.theme, (key) => tokens.includes(queryToken(key)))
}

/** A node whose `direction` differs from its parent's: it and its subtree
 *  paint with it (Radix `dir`, arrow keys, mirrored glyphs, the chart
 *  mirror read `ctx.direction`). */
function DirectedNode({ node, scope, direction }: { node: UiNode; scope: string; direction: `ltr` | `rtl` }) {
  const ctx = useSurfaceContext()
  const value = useMemo(() => ({ ...ctx, direction }), [ctx, direction])
  return (
    <SurfaceContext.Provider value={value}>
      <BoundNode node={node} scope={scope} />
    </SurfaceContext.Provider>
  )
}

const BoundNode = memo(function BoundNode({ node, scope }: { node: UiNode; scope: string }) {
  const ctx = useSurfaceContext()
  const instance = useContext(InstanceContext)
  const domId = `${node.id}${instance}`
  const rctx = useMemo(() => resolveContextOf(ctx, scope), [ctx, scope])
  const props = useMemo(() => {
    const out = resolveNodeProps(node.component, node.props, rctx, ctx.extensionDefs)
    for (const name of responsiveProps(node.component, ctx.extensionDefs)) {
      const v = out[name]
      if (isResponsiveValue(v)) out[name] = responsiveAt(v, ctx.breakpoint)
    }
    return out
  }, [node.props, node.component, rctx, ctx.extensionDefs, ctx.breakpoint])
  const recipeProps = useMemo(() => (node.recipe ? (resolveValue(node.recipe.props, rctx) as Record<string, unknown>) : undefined), [node.recipe, rctx])
  const dynamicStyle = useMemo(() => {
    const entries = dynamicStyleEntries(node.style)
    if (entries.length === 0) return undefined
    const out: Record<string, string> = {}
    // EVERY entry is set: a value that resolves to nothing is `initial` (the
    // guaranteed-invalid value), so the property falls back instead of
    // inheriting an ancestor's `--xd-<n>`.
    for (const e of entries) {
      const css = cssValue(e.key, resolveValue(e.value, rctx), ctx.theme.fonts)
      out[e.name] = css ?? `initial`
    }
    return out as CSSProperties
  }, [node.style, rctx, ctx.theme.fonts])
  const rootProps = useRootProps(node, props, domId, recipeProps, dynamicStyle)
  const emit = useEmitter(node, scope, domId)
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
        <div className={`xui-el ${nodeClass(node.id)} xui-leaf`} data-xui-id={domId} data-xui-c={node.component}>
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

  const painterProps: NativeProps = { node, props, theme: ctx.theme, mode: ctx.mode, tokens: ctx.theme.tokens, children, slots, emit, rootProps, scope, domId }
  if (UNGUARDED.has(node.component) && !Override) return <Painter {...painterProps} />
  return (
    <PaintBoundary node={node} domId={domId}>
      <Painter {...painterProps} />
    </PaintBoundary>
  )
})

/** A failed node's identity for the once-per-props rule. */
function propsSignature(node: UiNode): string {
  try {
    return `${node.component}\u0000${JSON.stringify(node.props)}`
  } catch {
    return node.component
  }
}

/** The plain painters (no formatting, no host code) skip the boundary. */
const UNGUARDED = new Set([`Box`, `Text`])

/** One node's painter that throws (a bad agent-written prop, a host
 *  override's bug) paints an EMPTY box in its place; the rest of the
 *  surface and the host stay mounted, and the host hears it through
 *  `onPaintError` (catalog/host.json paint). A new node (a re-reduce)
 *  retries. */
class PaintBoundary extends Component<{ node: UiNode; domId: string; children: ReactNode }, { node: UiNode; failed: boolean }> {
  static contextType = SurfaceContext
  declare context: SurfaceContextValue | null
  constructor(props: { node: UiNode; domId: string; children: ReactNode }) {
    super(props)
    this.state = { node: props.node, failed: false }
  }
  static getDerivedStateFromProps(props: { node: UiNode }, state: { node: UiNode }): { node: UiNode; failed: boolean } | null {
    return props.node === state.node ? null : { node: props.node, failed: false }
  }
  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true }
  }
  componentDidCatch(error: unknown): void {
    const { node, domId } = this.props
    // Reported ONCE per painted node until its props change (a re-reduce
    // hands every node a new object; that alone never re-reports).
    const failures = this.context?.paintFailures
    const signature = propsSignature(node)
    if (failures?.get(domId) === signature) return
    failures?.set(domId, signature)
    const message = `${node.component} failed to paint: ${error instanceof Error ? error.message : String(error)}`
    const report = this.context?.host.onPaintError
    if (report) {
      try {
        report({ surfaceId: this.context!.surfaceId, componentId: node.id, message })
      } catch (e) {
        console.error(`[exponential-ui] onPaintError threw`, e)
      }
    } else console.error(`[exponential-ui] ${node.component} "${node.id}" failed to paint`, error)
  }
  componentDidMount(): void {
    this.painted()
  }
  componentDidUpdate(): void {
    this.painted()
  }
  componentWillUnmount(): void {
    this.context?.paintFailures?.delete(this.props.domId)
  }
  /** A node that paints again is no longer a remembered failure. */
  private painted(): void {
    if (!this.state.failed) this.context?.paintFailures?.delete(this.props.domId)
  }
  render(): ReactNode {
    const { node, domId, children } = this.props
    if (this.state.failed) return <div className={`xui-el ${nodeClass(node.id)}`} data-xui-id={domId} data-xui-c={node.component} data-xui-error="" />
    return children
  }
}

export interface TemplateItem {
  /** The item's instance key (its React key; escaped, its id suffix): the
   *  `template.key` value; the index when the template has no key; `#<index>`
   *  when the key is missing, null, empty or a DUPLICATE of an earlier item
   *  (so ids stay unique). */
  key: string
  /** The item's data pointer (its scope). */
  path: string
  index: number
}

/** The items of a node's data template (`template.key` = a pointer relative
 *  to each item whose value identifies it). Instance ids ACCUMULATE: an
 *  item's nodes are `<id><outer suffixes>.<key>`, so a nested template's
 *  items stay unique across the outer items (`cell.a.0`, `cell.b.0`). */
export function templateItems(ctx: SurfaceContextValue, node: UiNode, scope: string): { items: TemplateItem[]; tpl: UiNode } | null {
  const template = node.template
  if (!template) return null
  const path = absolutePath(template.path, scope)
  const list = getPointer(ctx.data, path)
  const tpl = ctx.templateNode(template.component)
  if (!Array.isArray(list) || !tpl) return null
  const keys = templateItemKeys(list, template.key)
  // VAPP-103: the surface's `maxTemplateItems` budget for this site.
  const count = ctx.templateBudget?.allowed.get(templateSiteKey(node.id, scope)) ?? list.length
  const items = list.slice(0, count).map((_, index) => ({ key: keys[index], path: `${path}/${index}`, index }))
  return { items, tpl }
}

/** A template item's instance suffix: the enclosing one + `.<key>`, the
 *  key escaped (`instanceSegment`: `.` → `~1`, `~` → `~0`) as the core's
 *  `templateInstances`, so `a.b` never reads as two levels. */
export const itemInstance = (instance: string, key: string): string => `${instance}.${instanceSegment(key)}`

/** One template item: its scope + instance suffix around the node. */
export function TemplateItemView({ tpl, item }: { tpl: UiNode; item: TemplateItem }) {
  const instance = useContext(InstanceContext)
  return (
    <ScopeContext.Provider value={item.path}>
      <InstanceContext.Provider value={itemInstance(instance, item.key)}>
        <NodeView node={tpl} />
      </InstanceContext.Provider>
    </ScopeContext.Provider>
  )
}

function TemplateChildren({ node, scope }: { node: UiNode; scope: string }) {
  const ctx = useSurfaceContext()
  const t = templateItems(ctx, node, scope)
  if (!t) return null
  return (
    <>
      {t.items.map((item) => (
        <TemplateItemView key={item.key} tpl={t.tpl} item={item} />
      ))}
    </>
  )
}

export { Fragment }
