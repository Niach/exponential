// FEED-128: ONE normalized node → its painted DOM (React's node-view.tsx on
// Solid). The BIND pass (`@exponential-at/ui` `bindTree` is the reference)
// is a set of memos per node: a node whose `visible` resolves falsy is not
// rendered at all; props, dynamic style values and macro recipe props
// resolve against the data model, `$string.<id>` through the string table;
// a RESPONSIVE prop takes its value at the active breakpoint. Then the
// painter is picked (an extension component, a painter override, or the
// native) through `<Dynamic>` and handed the root attributes and an emitter
// that runs an action in the contract's order (resolve the function args
// and the event context, apply `set`, dispatch the event).
//
// Identity: children are KEYED BY ID (`NodeList`) and template items by
// `template.key` (`TemplateChildren`), and a node's painter is kept while
// its component stays the same, so a re-reduce (new node objects, same ids)
// re-binds in place and keeps every control's state, as React's keyed
// reconciliation does.

import { ErrorBoundary, For, Show, createComputed, createEffect, createMemo, on, onCleanup, onMount, untrack, type Accessor, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { UNKNOWN_COMPONENT, catalogView, instanceSegment, isResponsiveValue, nativeRecipeProps, responsiveAt, templateItemKeys, templateSiteKey, withOwnWrites } from "@exponential-at/ui"
import type { ExtensionDef, UiNode } from "@exponential-at/ui"
import { absolutePath, ariaAttributes, cssValue, dynamicStyleEntries, getPointer, isMacroComponent, nodeClass, partClass, propAttribute, queryToken, resolveAccessibility, resolveNodeProps, resolveValue, resolveVisible, styleDirection, type ResolveContext } from "@exponential-at/ui/web"
import { InstanceContext, ScopeContext, SurfaceContext, deriveContext, resolveContextOf, useInstance, useScope, useSurfaceContext, type SurfaceContextValue } from "./context"
import { extensionComponent, extensionMacroNames } from "./extensions"
import type { ExtensionComponentProps, SurfaceActionEvent } from "./host"
import { NATIVES } from "./natives"
import { css, type SolidStyle, type StyleInput } from "./style"

/** What a painter receives: React's `NativeProps` as a Solid props object.
 *  EVERY field is a live getter: never destructure it (`props.props.label`
 *  inside JSX or a memo follows the data), read `children` and each slot
 *  ONCE. */
export interface NativeProps extends ExtensionComponentProps {
  /** The data scope (template item) this node renders under. */
  readonly scope: string
  /** The node's id as painted (`data-xui-id`): its id + the template
   *  instance suffix. DOM ids derive from it (`<id>.error`). */
  readonly domId: string
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

/** The attributes every painted root carries (React's `useRootProps`):
 *  `class` (`xui-el xui-n-<id> <scope> xui-<Component>-root [macro part]`),
 *  `data-xui-id`, `data-xui-c`, `data-xui-part`, the recipe props
 *  (`data-m-*` macro / `data-r-*` native), `data-xs`, the resolved
 *  accessibility, `style` (Solid form), `dir`. A live accessor. */
export function createRootProps(node: Accessor<UiNode>, props: Accessor<Record<string, unknown>>, domId: Accessor<string> = () => node().id, recipeProps?: Accessor<Record<string, unknown> | undefined>, style?: Accessor<SolidStyle | undefined>): Accessor<Record<string, unknown>> {
  const ctx = useSurfaceContext()
  const scope = useScope()
  return createMemo(() => {
    const n = node()
    const macros = extensionMacroNames(ctx.extensions)
    const classes = [`xui-el`, nodeClass(styleIdOf(n)), ctx.compiled.scope, partClass(n.component, `root`)]
    const attrs: Record<string, unknown> = {}
    if (n.recipe) {
      classes.push(partClass(n.recipe.macro, n.recipe.part))
      attrs[`data-xui-part`] = `${n.recipe.macro}/${n.recipe.part}`
      const macro = isMacroComponent(n.recipe.macro, macros)
      for (const [k, v] of Object.entries(recipeProps?.() ?? n.recipe.props)) if (v !== undefined && v !== null && typeof v !== `object`) attrs[propAttribute(macro, k)] = String(v)
    }
    for (const [k, v] of Object.entries(nativeRecipeProps(n.component, props(), ctx.extensionDefs))) if (typeof v !== `object`) attrs[propAttribute(false, k)] = String(v)
    if (ctx.states.length) attrs[`data-xs`] = ctx.states.join(` `)
    // Contract §6: the node's accessibility resolved, then mapped to ARIA. A
    // painter's own role/aria-* set after the spread wins.
    if (n.accessibility) Object.assign(attrs, ariaAttributes(resolveAccessibility(n.accessibility, { data: ctx.data, scope: scope(), functions: ctx.functions, locale: ctx.locale, strings: ctx.strings })))
    const s = style?.()
    if (s) attrs.style = s
    const dir = ownDirection(n, ctx)
    if (dir) attrs.dir = dir
    return { class: classes.join(` `), "data-xui-id": domId(), "data-xui-c": n.component, ...attrs }
  })
}

/** `class` on top of the renderer's own. */
export function withClass(rootProps: Record<string, unknown>, ...extra: (string | false | undefined | null)[]): Record<string, unknown> {
  const more = extra.filter(Boolean).join(` `)
  return more ? { ...rootProps, class: `${rootProps.class as string} ${more}` } : rootProps
}

/** A painter's own inline style (React form: camelCase, numbers = px)
 *  merged over the root's (the dynamic style variables live there and must
 *  survive), in Solid's form. */
export function mergeStyle(rootProps: Record<string, unknown>, style: StyleInput | undefined): SolidStyle | undefined {
  const own = rootProps.style as SolidStyle | undefined
  const mine = css(style)
  if (!own) return mine
  if (!mine) return own
  return { ...own, ...mine }
}

/** Forced states on a part element (`data-xs`). */
export function forced(states: readonly string[], ...extra: (string | false | undefined)[]): string | undefined {
  const all = [...states, ...extra.filter((s): s is string => Boolean(s))]
  return all.length ? all.join(` `) : undefined
}

/** Run one of a node's actions (round-1 contract §1 `runAction`), exactly
 *  React's `runNodeAction`. */
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
      // A name outside the client functions is a HOST function (VAPP-91).
      if (f) f(args, rctx)
      else pending = ctx.host.onFunctionCall?.({ surfaceId: ctx.surfaceId, componentId: domId, name: fn.call, args })
    }
  }
  if (pending instanceof Promise) {
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

/** React's `useEmitter`: the node's emitter (reads the node, scope and id
 *  at call time). */
export function createEmitter(node: Accessor<UiNode>, scope: Accessor<string>, domId: Accessor<string> = () => node().id) {
  const ctx = useSurfaceContext()
  return (event: string, payload?: Record<string, unknown>, own?: Record<string, unknown>): Promise<void> | void => untrack(() => runNodeAction(ctx, node(), domId(), scope(), event, payload, own))
}

/** A node's own style `direction` (round 2 §2), matching `@media` blocks
 *  included. */
export function ownDirection(node: UiNode, ctx?: Pick<SurfaceContextValue, `theme` | `xq`>): `ltr` | `rtl` | undefined {
  if (!node.style) return undefined
  const tokens = ctx?.xq ? ctx.xq.split(` `) : []
  return styleDirection(node.style, ctx?.theme, (key) => tokens.includes(queryToken(key)))
}

/** A node, or nothing when its `visible` resolves falsy. `props.node` is
 *  followed live (a re-reduce re-binds in place). */
export function NodeView(props: { node: UiNode }): JSX.Element {
  const ctx = useSurfaceContext()
  const scope = useScope()
  const visible = createMemo(() => props.node.visible === undefined || resolveVisible(props.node.visible, resolveContextOf(ctx, scope())))
  const own = createMemo(() => ownDirection(props.node, ctx))
  const directed = createMemo(() => {
    const d = own()
    return d && d !== ctx.direction ? d : undefined
  })
  return (
    <Show when={visible()}>
      <Show when={directed()} fallback={<BoundNode node={props.node} />}>
        <SurfaceContext.Provider value={deriveContext(ctx, { direction: () => directed() ?? ctx.direction })}>
          <BoundNode node={props.node} />
        </SurfaceContext.Provider>
      </Show>
    </Show>
  )
}

/** Nodes painted in order, KEYED BY ID: a re-reduce hands new node objects
 *  for the same ids and each keeps its painted subtree. */
export function NodeList(props: { nodes: readonly UiNode[] }): JSX.Element {
  const byId = createMemo(() => new Map(props.nodes.map((n) => [n.id, n])))
  const ids = createMemo(() => props.nodes.map((n) => n.id), undefined, { equals: sameList })
  return (
    <For each={ids()}>
      {(id) => {
        const node = createMemo<UiNode>((prev) => byId().get(id) ?? prev!)
        return <NodeView node={node()} />
      }}
    </For>
  )
}

/** The latest non-empty value of `fn` (what a `<Show>` child reads instead
 *  of Show's accessor: a child disposed after its condition went falsy
 *  never reads a stale accessor). */
export function latest<T>(fn: () => T | null | undefined | false): Accessor<T> {
  return createMemo<T>((prev) => (fn() || prev) as T)
}

const sameList = (a: readonly string[], b: readonly string[]): boolean => a.length === b.length && a.every((x, i) => x === b[i])

function BoundNode(props: { node: UiNode }): JSX.Element {
  const ctx = useSurfaceContext()
  const scope = useScope()
  const instance = useInstance()
  const node = () => props.node
  const domId = createMemo(() => `${props.node.id}${instance()}`)
  const rctx = createMemo(() => resolveContextOf(ctx, scope()))
  const resolved = createMemo(() => {
    const n = props.node
    const out = resolveNodeProps(n.component, n.props, rctx(), ctx.extensionDefs)
    for (const name of responsiveProps(n.component, ctx.extensionDefs)) {
      const v = out[name]
      if (isResponsiveValue(v)) out[name] = responsiveAt(v, ctx.breakpoint)
    }
    return out
  })
  const recipeProps = createMemo(() => (props.node.recipe ? (resolveValue(props.node.recipe.props, rctx()) as Record<string, unknown>) : undefined))
  const dynamicStyle = createMemo(() => {
    const entries = dynamicStyleEntries(props.node.style)
    if (entries.length === 0) return undefined
    const out: SolidStyle = {}
    // EVERY entry is set: a value that resolves to nothing is `initial`.
    for (const e of entries) {
      const v = cssValue(e.key, resolveValue(e.value, rctx()), ctx.theme.fonts)
      out[e.name] = v ?? `initial`
    }
    return out
  })
  const rootProps = createRootProps(node, resolved, domId, recipeProps, dynamicStyle)
  const emit = createEmitter(node, scope, domId)
  const component = createMemo(() => props.node.component)
  const override = createMemo(() => extensionComponent(ctx.extensions, component()))
  const Painter = createMemo(() => override() ?? NATIVES[component()] ?? NATIVES[UNKNOWN_COMPONENT])
  const unknown = createMemo(() => component() === UNKNOWN_COMPONENT || (!NATIVES[component()] && !override()))
  createEffect(() => {
    if (unknown()) {
      const n = props.node
      untrack(() => ctx.host.onUnknown?.(n))
    }
  })

  // Geometry mode: a leaf is a fixed box (the core's fake measure) with NO
  // recipe on it — only its node style lays out, like the core.
  const measured = createMemo(() => (ctx.measure && !CONTAINERS.has(component()) ? ctx.measure(props.node) : null))
  const latestSize = latest(measured)

  const painterProps: NativeProps = {
    get node() {
      return props.node
    },
    get props() {
      return resolved()
    },
    get theme() {
      return ctx.theme
    },
    get mode() {
      return ctx.mode
    },
    get tokens() {
      return ctx.theme.tokens
    },
    get children() {
      return (
        <>
          <NodeList nodes={props.node.children} />
          <Show when={props.node.template}>
            <TemplateChildren node={props.node} scope={scope()} />
          </Show>
        </>
      )
    },
    get slots() {
      const out: Record<string, JSX.Element> = {}
      for (const name of Object.keys(props.node.slots ?? {})) {
        Object.defineProperty(out, name, { enumerable: true, get: () => <SlotView node={props.node} name={name} /> })
      }
      return out
    },
    emit,
    get rootProps() {
      return rootProps()
    },
    get scope() {
      return scope()
    },
    get domId() {
      return domId()
    },
  }

  const paint = () => <Dynamic component={Painter()} {...painterProps} />
  const guarded = createMemo(() => !(UNGUARDED.has(component()) && !override()))
  return (
    <Show
      when={measured()}
      fallback={
        <Show when={guarded()} fallback={paint()}>
          <PaintBoundary node={props.node} domId={domId()}>
            {paint()}
          </PaintBoundary>
        </Show>
      }
    >
      <div class={`xui-el ${nodeClass(props.node.id)} xui-leaf`} data-xui-id={domId()} data-xui-c={props.node.component}>
        <div class="xui-measured" style={css({ width: latestSize().w, height: latestSize().h })} />
      </div>
    </Show>
  )
}

/** One slot of a node, followed by name (a re-reduce keeps it mounted). */
function SlotView(props: { node: UiNode; name: string }): JSX.Element {
  const slot = createMemo<UiNode | undefined>((prev) => props.node.slots?.[props.name] ?? prev)
  return (
    <Show when={slot()}>
      <NodeView node={slot()!} />
    </Show>
  )
}

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

/** One node's painter that throws paints an EMPTY box in its place; the
 *  rest of the surface stays mounted and the host hears it through
 *  `onPaintError` (once per painted node until its props change). A new
 *  node (a re-reduce) retries. */
function PaintBoundary(props: { node: UiNode; domId: string; children: JSX.Element }): JSX.Element {
  const ctx = useSurfaceContext()
  let reset: (() => void) | undefined
  createEffect(
    on(
      () => props.node,
      () => {
        const r = reset
        reset = undefined
        r?.()
      },
      { defer: true }
    )
  )
  // The painted id as last seen (props are not read during disposal: the
  // node may already be gone).
  let paintedId = untrack(() => props.domId)
  createComputed(() => (paintedId = props.domId))
  onCleanup(() => ctx.paintFailures?.delete(paintedId))
  const report = (error: unknown) => {
    const node = untrack(() => props.node)
    const domId = untrack(() => props.domId)
    const failures = ctx.paintFailures
    const signature = propsSignature(node)
    if (failures?.get(domId) === signature) return
    failures?.set(domId, signature)
    const message = `${node.component} failed to paint: ${error instanceof Error ? error.message : String(error)}`
    const onPaintError = ctx.host.onPaintError
    if (onPaintError) {
      try {
        onPaintError({ surfaceId: ctx.surfaceId, componentId: node.id, message })
      } catch (e) {
        console.error(`[exponential-ui] onPaintError threw`, e)
      }
    } else console.error(`[exponential-ui] ${node.component} "${node.id}" failed to paint`, error)
  }
  return (
    <ErrorBoundary
      fallback={(error, r) => {
        reset = r
        report(error)
        return <div class={`xui-el ${nodeClass(props.node.id)}`} data-xui-id={props.domId} data-xui-c={props.node.component} data-xui-error="" />
      }}
    >
      <Painted domId={props.domId} />
      {props.children}
    </ErrorBoundary>
  )
}

/** A node that paints again is no longer a remembered failure. */
function Painted(props: { domId: string }): JSX.Element {
  const ctx = useSurfaceContext()
  onMount(() => ctx.paintFailures?.delete(props.domId))
  return null
}

export interface TemplateItem {
  /** The item's instance key: the `template.key` value; the index when the
   *  template has no key; `#<index>` when missing / empty / a duplicate. */
  key: string
  /** The item's data pointer (its scope). */
  path: string
  index: number
}

/** The items of a node's data template (React's `templateItems`). Reads
 *  the surface's getters: call it inside a memo. */
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
 *  key escaped (`instanceSegment`). */
export const itemInstance = (instance: string, key: string): string => `${instance}.${instanceSegment(key)}`

/** One template item: its scope + instance suffix around the node (both
 *  followed live: a moved item keeps its subtree and re-scopes). */
export function TemplateItemView(props: { tpl: UiNode; item: TemplateItem }): JSX.Element {
  const outer = useInstance()
  return (
    <ScopeContext.Provider value={() => props.item.path}>
      <InstanceContext.Provider value={() => itemInstance(outer(), props.item.key)}>
        <NodeView node={props.tpl} />
      </InstanceContext.Provider>
    </ScopeContext.Provider>
  )
}

/** The template items of a list, keyed by `template.key`. */
export function TemplateItems(props: { items: readonly TemplateItem[]; tpl: UiNode }): JSX.Element {
  const byKey = createMemo(() => new Map(props.items.map((i) => [i.key, i])))
  const keys = createMemo(() => props.items.map((i) => i.key), undefined, { equals: sameList })
  return (
    <For each={keys()}>
      {(key) => {
        const item = createMemo<TemplateItem>((prev) => byKey().get(key) ?? prev!)
        return <TemplateItemView tpl={props.tpl} item={item()} />
      }}
    </For>
  )
}

function TemplateChildren(props: { node: UiNode; scope: string }): JSX.Element {
  const ctx = useSurfaceContext()
  const t = createMemo(() => templateItems(ctx, props.node, props.scope))
  const last = latest(t)
  return (
    <Show when={t()}>
      <TemplateItems items={last().items} tpl={last().tpl} />
    </Show>
  )
}
