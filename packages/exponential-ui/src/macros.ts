// VAPP-85: the macro expander — applies catalog/macros.json (plus extension
// macros) to a normalized tree until every node is native. The rules are in
// the table's `$comment`; fixtures/catalog-macros.json locks the output and
// the Rust core replays the same table.

import { catalogView } from "./catalog"
import type { CatalogView } from "./catalog"
import { evalCondition, evalValue, truthy, type ExprContext } from "./expr"
import type { Action, ExtensionDef, MacroDef, MacroTemplate, UiNode } from "./types"

const MAX_DEPTH = 16

interface Expansion {
  view: CatalogView
  macro: string
  def: MacroDef
  source: UiNode
  claimed: Set<string>
}

function clone<T>(value: T): T {
  return value === undefined ? value : (JSON.parse(JSON.stringify(value)) as T)
}

function mergeStyle(
  base: Record<string, unknown> | undefined,
  over: Record<string, unknown> | undefined
): Record<string, unknown> | undefined {
  if (!base && !over) return undefined
  const out: Record<string, unknown> = { ...(base ?? {}) }
  for (const [k, v] of Object.entries(over ?? {})) {
    const nested = typeof v === `object` && v !== null && !Array.isArray(v)
    out[k] = nested
      ? { ...((out[k] as Record<string, unknown>) ?? {}), ...(v as Record<string, unknown>) }
      : v
  }
  return out
}

function routedAction(
  action: Action,
  context: Record<string, unknown> | undefined
): Action {
  const out = clone(action)
  if (context && out.event) {
    out.event.context = { ...(out.event.context ?? {}), ...context }
  }
  return out
}

/** The macro's recipe discriminators: the author's value, else the prop's
 *  catalog default, so a theme always sees a concrete value. */
function recipeProps(exp: Expansion, ctx: ExprContext, extra: unknown): Record<string, unknown> {
  const props: Record<string, unknown> = {}
  const schema = exp.view.components[exp.macro]?.props ?? {}
  for (const name of exp.def.recipeProps) {
    const value = exp.source.props[name] ?? schema[name]?.default
    if (value !== undefined) props[name] = value
  }
  if (extra) {
    const evaluated = evalValue(extra, ctx) as Record<string, unknown>
    for (const [k, v] of Object.entries(evaluated)) props[k] = v
  }
  return props
}

function expandTemplate(
  tpl: MacroTemplate,
  exp: Expansion,
  ctx: ExprContext,
  parentId: string
): UiNode[] {
  if (tpl.$if !== undefined) {
    const conds = Array.isArray(tpl.$if) ? tpl.$if : [tpl.$if]
    if (!conds.every((c) => evalCondition(c, ctx))) return []
  }
  if (tpl.$any !== undefined && !tpl.$any.some((c) => evalCondition(c, ctx))) return []
  if (tpl.$each !== undefined) {
    const items = evalValue(`{${tpl.$each}}`, ctx)
    if (!Array.isArray(items)) return []
    const as = tpl.$as ?? `item`
    return items.map((item, index) =>
      buildNode(tpl, exp, { ...ctx, vars: { ...ctx.vars, [as]: item, index } }, `${parentId}.${tpl.part}.${index}`)
    )
  }
  const id = tpl.part === `root` ? exp.source.id : `${parentId}.${tpl.part}`
  return [buildNode(tpl, exp, ctx, id)]
}

function buildNode(
  tpl: MacroTemplate,
  exp: Expansion,
  ctx: ExprContext,
  id: string
): UiNode {
  const props = (evalValue(tpl.props ?? {}, ctx) as Record<string, unknown>) ?? {}
  const style = tpl.style ? (evalValue(tpl.style, ctx) as Record<string, unknown>) : undefined
  const node: UiNode = { id, component: tpl.component, props, children: [] }
  if (style && Object.keys(style).length > 0) node.style = style
  node.recipe = { macro: exp.macro, part: tpl.part, props: recipeProps(exp, ctx, tpl.$recipe) }
  if (tpl.$on) {
    const on: Record<string, Action> = {}
    for (const [partEvent, macroEvent] of Object.entries(tpl.$on)) {
      const action = exp.source.on?.[macroEvent]
      if (!action) continue
      exp.claimed.add(macroEvent)
      const context = tpl.$context
        ? (evalValue(tpl.$context, ctx) as Record<string, unknown>)
        : undefined
      on[partEvent] = routedAction(action, context)
    }
    if (Object.keys(on).length > 0) node.on = on
  }
  if (tpl.slots) {
    for (const [slot, ref] of Object.entries(tpl.slots)) {
      const name = ref.startsWith(`$slot:`) ? ref.slice(6) : null
      const value = name ? exp.source.slots?.[name] : undefined
      if (value) (node.slots ??= {})[slot] = value
    }
  }
  for (const child of tpl.children ?? []) {
    if (child === `$children`) {
      node.children.push(...exp.source.children)
      if (exp.source.template) node.template = exp.source.template
      continue
    }
    node.children.push(...expandTemplate(child, exp, ctx, id))
  }
  return node
}

/** Expand one macro node (children already native) into its template. */
function expandMacro(node: UiNode, macro: string, def: MacroDef, view: CatalogView, depth: number): UiNode {
  if (depth > MAX_DEPTH) throw new Error(`macro ${macro}: expansion deeper than ${MAX_DEPTH}`)
  const exp: Expansion = { view, macro, def, source: node, claimed: new Set() }
  const ctx: ExprContext = { id: node.id, props: node.props, vars: {} }
  const [root] = expandTemplate(def.root, exp, ctx, node.id)
  root.style = mergeStyle(root.style, node.style)
  if (root.style && Object.keys(root.style).length === 0) delete root.style
  if (node.accessibility) root.accessibility = node.accessibility
  const remaining: Record<string, Action> = { ...(root.on ?? {}) }
  for (const [event, action] of Object.entries(node.on ?? {})) {
    if (!exp.claimed.has(event)) remaining[event] = action
  }
  if (Object.keys(remaining).length > 0) root.on = remaining
  else delete root.on
  return expandTree(root, view, depth + 1)
}

function expandTree(node: UiNode, view: CatalogView, depth: number): UiNode {
  const out: UiNode = { ...node, children: node.children.map((c) => expandTree(c, view, depth)) }
  if (node.slots) {
    out.slots = {}
    for (const [slot, child] of Object.entries(node.slots)) out.slots[slot] = expandTree(child, view, depth)
  }
  const def = view.macros[out.component]
  if (def && view.components[out.component]?.kind === `macro`) {
    return expandMacro(out, out.component, def, view, depth)
  }
  return out
}

/** The normalized tree with every macro (core + extensions) expanded into
 *  natives, recipes attached. Pure: the input is not mutated. */
export function expandMacros(
  root: UiNode,
  options: { extensions?: readonly ExtensionDef[] } = {}
): UiNode {
  return expandTree(clone(root), catalogView(options.extensions), 0)
}

/** True for a value the expander would keep (see expr.ts). */
export const keeps = truthy
