// VAPP-141 + VAPP-137 + VAPP-130: what a node re-renders FOR. The surface
// context is stable across data, breakpoint, `data-xq` and mode changes
// (those four are getters over the surface's latest render,
// `SurfaceContextValue.subscribe` announces a change); each node subscribes to the values ITS bindings
// read — the absolute pointers in its props, `visible`, dynamic style,
// recipe props, accessibility and template path, the breakpoint when it
// has a responsive prop, `data-xq` when an `@media` block sets its
// direction, the mode when its painter reads it (the stylesheet carries
// both modes) — and re-renders only when one of them is a new value. A call
// whose function may read the model on its own (`formatString`, a host
// function) or a host-painted component re-renders on every change.

import { useMemo, useRef, useSyncExternalStore } from "react"
import { sharedCatalogView } from "@exponential-at/ui"
import type { ExtensionDef, UiNode } from "@exponential-at/ui"
import { CLIENT_FUNCTIONS, absolutePath, isBinding, isCall, pointerTokens } from "@exponential-at/ui/web"
import type { SurfaceContextValue } from "./context"
import { extensionComponent } from "./extensions"

export interface NodeDeps {
  paths: string[]
  /** `paths` as pointer tokens (read on every change of every node). */
  tokens: string[][]
  /** Re-render on any data change. */
  all: boolean
  breakpoint: boolean
  xq: boolean
  mode: boolean
}

/** The natives that read `ctx.mode` while painting (a recipe size or
 *  colour resolved in JS); every other node's look follows the root's
 *  `data-xui-mode` in CSS. */
const MODE_READERS = new Set([`Ring`, `Spinner`, `TreeGuides`, `Avatar`])

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
    const def = sharedCatalogView(extensions as ExtensionDef[]).components[component]
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
  const unique = [...new Set(paths)]
  return { paths: unique, tokens: unique.map(pointerTokens), all, breakpoint: hosted || responsiveProps(node.component, ctx.extensionDefs).length > 0, xq: hosted || mediaDirection(node.style as Record<string, unknown> | undefined), mode: hosted || MODE_READERS.has(node.component) }
}

const INDEX = /^[0-9]+$/

/** `getPointer` over pre-split tokens (the same rule: real indices and
 *  OWN keys only). */
function readTokens(data: unknown, tokens: readonly string[]): unknown {
  let cur = data
  for (const token of tokens) {
    if (cur === null || typeof cur !== `object`) return undefined
    if (Array.isArray(cur)) cur = INDEX.test(token) ? cur[Number(token)] : undefined
    else cur = Object.prototype.hasOwnProperty.call(cur, token) ? (cur as Record<string, unknown>)[token] : undefined
  }
  return cur
}

const NONE: readonly unknown[] = []
const noSubscription = () => () => {}

/** The live values a node reads, as one array that keeps its identity
 *  until one of them changes (the node's re-render key). */
export function useLive(ctx: SurfaceContextValue, node: UiNode, scope: string): readonly unknown[] {
  const deps = useMemo(() => nodeDeps(node, scope, ctx), [node, scope, ctx])
  const last = useRef<readonly unknown[]>(NONE)
  const watches = deps.all || deps.breakpoint || deps.xq || deps.mode || deps.paths.length > 0
  const read = (): readonly unknown[] => {
    if (!watches) return NONE
    const data = ctx.data
    const next: unknown[] = deps.tokens.map((t) => readTokens(data, t))
    next.push(deps.all ? data : undefined, deps.breakpoint ? ctx.breakpoint : undefined, deps.xq ? ctx.xq : undefined, deps.mode ? ctx.mode : undefined)
    const prev = last.current
    if (prev.length === next.length && prev.every((v, i) => Object.is(v, next[i]))) return prev
    last.current = next
    return next
  }
  return useSyncExternalStore(watches ? ctx.subscribe : noSubscription, read, read)
}

