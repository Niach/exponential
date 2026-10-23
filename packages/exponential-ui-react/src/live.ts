// VAPP-141 + VAPP-137: what a node re-renders FOR. The surface context is
// stable across data, breakpoint and `data-xq` changes (those three are
// getters over the surface's latest render, `SurfaceContextValue.subscribe`
// announces a change); each node subscribes to the values ITS bindings
// read — the absolute pointers in its props, `visible`, dynamic style,
// recipe props, accessibility and template path, the breakpoint when it
// has a responsive prop, `data-xq` when an `@media` block sets its
// direction — and re-renders only when one of them is a new value. A call
// whose function may read the model on its own (`formatString`, a host
// function) or a host-painted component re-renders on every change.

import { useMemo, useRef, useSyncExternalStore } from "react"
import { catalogView } from "@exponential-at/ui"
import type { ExtensionDef, UiNode } from "@exponential-at/ui"
import { CLIENT_FUNCTIONS, absolutePath, getPointer, isBinding, isCall } from "@exponential-at/ui/web"
import type { SurfaceContextValue } from "./context"
import { extensionComponent } from "./extensions"

export interface NodeDeps {
  paths: string[]
  /** Re-render on any data change. */
  all: boolean
  breakpoint: boolean
  xq: boolean
}

/** The client functions that read nothing but their arguments. */
const PURE = new Set(Object.keys(CLIENT_FUNCTIONS).filter((name) => name !== `formatString`))

/** Collects the binding pointers of a value; true = it may read anything. */
function collect(value: unknown, scope: string, paths: string[], functions: SurfaceContextValue[`functions`]): boolean {
  if (isBinding(value)) {
    paths.push(absolutePath(value.path, scope))
    return false
  }
  if (isCall(value)) {
    const fn = functions[value.call] ?? CLIENT_FUNCTIONS[value.call]
    if (!PURE.has(value.call) || fn !== CLIENT_FUNCTIONS[value.call]) return true
    return collect(value.args, scope, paths, functions)
  }
  if (Array.isArray(value)) {
    for (const v of value) if (collect(v, scope, paths, functions)) return true
    return false
  }
  if (typeof value === `object` && value !== null) {
    for (const v of Object.values(value)) if (collect(v, scope, paths, functions)) return true
  }
  return false
}

const responsiveCache = new WeakMap<readonly ExtensionDef[], Map<string, string[]>>()

/** The props of a component marked `responsive: true` in the catalog. */
export function responsiveProps(component: string, extensions: readonly ExtensionDef[]): string[] {
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

function mediaDirection(style: Record<string, unknown> | undefined): boolean {
  if (!style) return false
  for (const [key, value] of Object.entries(style)) if (key.startsWith(`@`) && typeof value === `object` && value !== null && `direction` in value) return true
  return false
}

export function nodeDeps(node: UiNode, scope: string, ctx: SurfaceContextValue): NodeDeps {
  const paths: string[] = []
  // A host-painted component may read any live member itself.
  const hosted = extensionComponent(ctx.extensions, node.component) !== undefined
  const all =
    hosted ||
    collect(node.props, scope, paths, ctx.functions) ||
    collect(node.visible, scope, paths, ctx.functions) ||
    collect(node.style, scope, paths, ctx.functions) ||
    collect(node.recipe?.props, scope, paths, ctx.functions) ||
    collect(node.accessibility, scope, paths, ctx.functions)
  if (node.template) paths.push(absolutePath(node.template.path, scope))
  return { paths: [...new Set(paths)], all, breakpoint: hosted || responsiveProps(node.component, ctx.extensionDefs).length > 0, xq: hosted || mediaDirection(node.style as Record<string, unknown> | undefined) }
}

const NONE: readonly unknown[] = []
const noSubscription = () => () => {}

/** The live values a node reads, as one array that keeps its identity
 *  until one of them changes (the node's re-render key). */
export function useLive(ctx: SurfaceContextValue, node: UiNode, scope: string): readonly unknown[] {
  const deps = useMemo(() => nodeDeps(node, scope, ctx), [node, scope, ctx])
  const last = useRef<readonly unknown[]>(NONE)
  const watches = deps.all || deps.breakpoint || deps.xq || deps.paths.length > 0
  const read = (): readonly unknown[] => {
    if (!watches) return NONE
    const data = ctx.data
    const next: unknown[] = deps.paths.map((p) => getPointer(data, p))
    next.push(deps.all ? data : undefined, deps.breakpoint ? ctx.breakpoint : undefined, deps.xq ? ctx.xq : undefined)
    const prev = last.current
    if (prev.length === next.length && prev.every((v, i) => Object.is(v, next[i]))) return prev
    last.current = next
    return next
  }
  return useSyncExternalStore(watches ? ctx.subscribe : noSubscription, read, read)
}

const isPlainObject = (v: object): boolean => {
  const proto = Object.getPrototypeOf(v)
  return proto === Object.prototype || proto === null
}
const idOf = (v: unknown): string | undefined => (typeof v === `object` && v !== null && typeof (v as { id?: unknown }).id === `string` ? (v as { id: string }).id : undefined)

/** `next` with every part deep-equal to `prev`'s replaced by `prev`'s
 *  object (arrays of nodes matched by `id`, other arrays by index): a
 *  re-reduce (`updateComponents`) hands every unchanged node its old
 *  object, so the memoized node views skip it. Plain JSON only; anything
 *  else is taken as it is. */
export function share<T>(prev: unknown, next: T): T {
  if (Object.is(prev, next) || typeof prev !== `object` || typeof next !== `object` || prev === null || next === null) return next
  if (Array.isArray(next)) {
    if (!Array.isArray(prev)) return next
    const byId = prev.length > 0 && idOf(prev[0]) !== undefined ? new Map(prev.map((p) => [idOf(p), p])) : null
    let same = prev.length === next.length
    const out = next.map((v, i) => {
      const id = byId ? idOf(v) : undefined
      const s = share(id !== undefined ? byId!.get(id) : prev[i], v)
      if (s !== prev[i]) same = false
      return s
    })
    return (same ? prev : out) as T
  }
  if (Array.isArray(prev) || !isPlainObject(prev) || !isPlainObject(next)) return next
  const p = prev as Record<string, unknown>
  const keys = Object.keys(next)
  let same = keys.length === Object.keys(p).length
  const out: Record<string, unknown> = {}
  for (const k of keys) {
    const had = Object.prototype.hasOwnProperty.call(p, k)
    const s = share(had ? p[k] : undefined, (next as Record<string, unknown>)[k])
    out[k] = s
    if (!had || s !== p[k]) same = false
  }
  return (same ? prev : out) as T
}
