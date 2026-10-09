// VAPP-85: the macro expander — applies catalog/macros.json (plus extension
// macros) to a normalized tree until every node is native. The rules are in
// the table's `$comment`; fixtures/catalog-macros.json locks the output and
// the Rust core replays the same table.
//
// Round 1 (docs/round-1-contract.md): a `$if`/`$any` that cannot be decided
// at expansion time (a bound input) emits the part WITH `visible` = the
// bound condition; `"$slot:name"` children splice an author slot in place
// and a template `slots` entry may be a template node; a RESPONSIVE prop
// (`{base, sm?, md?…}`) is expanded per breakpoint into `@media (min-width:
// $breakpoint.<bp>)` blocks on every part whose style it changes; a `$set`
// on a part whose macro prop is BOUND adds the `set` write-back to that
// part's action (two-way binding without a host round trip); a part's
// `$a11y` becomes the node's `accessibility` (role, states, name: §6).

import { TOKEN_GROUPS, catalogView } from "./catalog"
import { applyTreeGuides } from "./tree-guides"
import type { CatalogView } from "./catalog"
import { evalConditionValue, evalValue, isBinding, isDynamic, truthy, type ExprContext } from "./expr"
import type { Action, ExtensionDef, MacroChild, MacroDef, MacroTemplate, ReduceIssue, UiNode, Visible } from "./types"

const MAX_DEPTH = 16
/** The breakpoint names in ascending order (tokens.json). */
export const BREAKPOINTS: readonly string[] = TOKEN_GROUPS.breakpoint

interface Variant {
  breakpoint: string
  props: Record<string, unknown>
}

interface Expansion {
  view: CatalogView
  macro: string
  def: MacroDef
  source: UiNode
  /** The author's props with every responsive value at its `base`. */
  baseProps: Record<string, unknown>
  /** One per breakpoint a responsive prop names, ascending. */
  variants: Variant[]
  claimed: Set<string>
  /** Where expansion problems go (the reducer's issue list), if anyone listens. */
  issues?: ReduceIssue[]
}

function clone<T>(value: T): T {
  return value === undefined ? value : (JSON.parse(JSON.stringify(value)) as T)
}

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

/** `{base, sm?, md?, lg?, xl?}`: an object with `base` and only breakpoint
 *  keys besides. */
export function isResponsiveValue(value: unknown): value is Record<string, unknown> & { base: unknown } {
  if (!isObj(value) || !(`base` in value)) return false
  return Object.keys(value).every((k) => k === `base` || BREAKPOINTS.includes(k))
}

/** The value a responsive prop takes at a breakpoint: the nearest defined
 *  key at or below it, `base` under the first one. `null` = the base. */
export function responsiveAt(value: Record<string, unknown> & { base: unknown }, breakpoint: string | null): unknown {
  let out = value.base
  if (breakpoint === null) return out
  for (const bp of BREAKPOINTS) {
    if (bp in value) out = value[bp]
    if (bp === breakpoint) break
  }
  return out
}

/** The author's props with every responsive object replaced by its value at
 *  `breakpoint`. */
export function propsAt(props: Record<string, unknown>, breakpoint: string | null): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(props)) out[k] = isResponsiveValue(v) ? responsiveAt(v, breakpoint) : v
  return out
}

function responsiveVariants(props: Record<string, unknown>): Variant[] {
  const used = new Set<string>()
  for (const v of Object.values(props)) if (isResponsiveValue(v)) for (const k of Object.keys(v)) if (k !== `base`) used.add(k)
  return BREAKPOINTS.filter((bp) => used.has(bp)).map((bp) => ({ breakpoint: bp, props: propsAt(props, bp) }))
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
    const value = exp.baseProps[name] ?? schema[name]?.default
    if (value !== undefined) props[name] = value
  }
  if (extra) {
    const evaluated = evalValue(extra, ctx) as Record<string, unknown>
    for (const [k, v] of Object.entries(evaluated)) props[k] = v
  }
  return props
}

function andOf(conditions: unknown[], op: `and` | `or`): Visible {
  if (conditions.length === 1) return conditions[0] as Visible
  return { call: op, args: { values: conditions } }
}

/** The part's `$if`/`$any`: `false` = leave it out, `true` = emit it, a
 *  dynamic value = emit it with that as `visible`. */
function decideConditions(tpl: MacroTemplate, ctx: ExprContext): boolean | Visible {
  const bound: unknown[] = []
  if (tpl.$if !== undefined) {
    const conds = Array.isArray(tpl.$if) ? tpl.$if : [tpl.$if]
    for (const c of conds) {
      const r = evalConditionValue(c, ctx)
      if (r === false) return false
      if (r !== true) bound.push(r)
    }
  }
  if (tpl.$any !== undefined) {
    let decided = false
    const any: unknown[] = []
    for (const c of tpl.$any) {
      const r = evalConditionValue(c, ctx)
      if (r === true) {
        decided = true
        break
      }
      if (r !== false) any.push(r)
    }
    if (!decided) {
      if (any.length === 0) return false
      bound.push(andOf(any, `or`))
    }
  }
  return bound.length === 0 ? true : andOf(bound, `and`)
}

function expandTemplate(
  tpl: MacroTemplate,
  exp: Expansion,
  ctx: ExprContext,
  parentId: string
): UiNode[] {
  const decided = decideConditions(tpl, ctx)
  if (decided === false) return []
  const visible = decided === true ? undefined : decided
  if (tpl.$each !== undefined) {
    const items = evalValue(`{${tpl.$each}}`, ctx)
    if (!Array.isArray(items)) return []
    const as = tpl.$as ?? `item`
    return items.map((item, index) =>
      buildNode(tpl, exp, { ...ctx, vars: { ...ctx.vars, [as]: item, index } }, `${parentId}.${tpl.part}.${index}`, visible)
    )
  }
  const id = tpl.part === `root` ? exp.source.id : `${parentId}.${tpl.part}`
  return [buildNode(tpl, exp, ctx, id, visible)]
}

const canon = (v: unknown) => JSON.stringify(v)

/** The part's style: evaluated with the base props, then once per
 *  breakpoint variant; every key a variant changes lands in its media block. */
function partStyle(tpl: MacroTemplate, exp: Expansion, ctx: ExprContext): Record<string, unknown> | undefined {
  if (!tpl.style) return undefined
  const style = evalValue(tpl.style, ctx) as Record<string, unknown>
  let previous = style
  for (const variant of exp.variants) {
    const at = evalValue(tpl.style, { ...ctx, props: variant.props }) as Record<string, unknown>
    const diff: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(at)) if (!k.startsWith(`@`) && !k.startsWith(`:`) && canon(previous[k]) !== canon(v)) diff[k] = v
    if (Object.keys(diff).length > 0) {
      const key = `@media (min-width: $breakpoint.${variant.breakpoint})`
      style[key] = { ...((style[key] as Record<string, unknown>) ?? {}), ...diff }
    }
    previous = at
  }
  return Object.keys(style).length > 0 ? style : undefined
}

function buildNode(
  tpl: MacroTemplate,
  exp: Expansion,
  ctx: ExprContext,
  id: string,
  visible: Visible | undefined
): UiNode {
  const props = (evalValue(tpl.props ?? {}, ctx) as Record<string, unknown>) ?? {}
  const style = partStyle(tpl, exp, ctx)
  const node: UiNode = { id, component: tpl.component, props, children: [] }
  if (style) node.style = style
  if (visible !== undefined) node.visible = visible
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
  if (tpl.$set) {
    for (const [partEvent, spec] of Object.entries(tpl.$set)) {
      const bound = exp.baseProps[spec.prop]
      if (!isBinding(bound)) continue
      const on = (node.on ??= {})
      if (on[partEvent]?.functionCall) {
        // An Action carries ONE `functionCall`: the author's routed function call
        // wins and the bound prop is not written back (reported, never lost
        // silently). Route an `event` to keep the two-way write.
        const macroEvent = tpl.$on?.[partEvent] ?? partEvent
        const message = `on.${macroEvent}: a function action replaces the two-way set of props.${spec.prop}; write it in that function's handler or use an event`
        if (!exp.issues?.some((i) => i.id === exp.source.id && i.message === message)) exp.issues?.push({ id: exp.source.id, message })
        continue
      }
      const value = evalValue(spec.value, ctx)
      on[partEvent] = { ...(on[partEvent] ?? {}), functionCall: { call: `set`, args: value === undefined ? { path: bound.path } : { path: bound.path, value } } }
    }
  }
  if (tpl.$a11y) {
    const a11y = evalValue(tpl.$a11y, ctx) as Record<string, unknown>
    if (Object.keys(a11y).length > 0) node.accessibility = a11y
  }
  if (tpl.slots) {
    for (const [slot, ref] of Object.entries(tpl.slots)) {
      if (typeof ref === `string`) {
        const name = ref.startsWith(`$slot:`) ? ref.slice(6) : null
        const value = name ? exp.source.slots?.[name] : undefined
        if (value) (node.slots ??= {})[slot] = value
        continue
      }
      const [built] = expandTemplate(ref, exp, ctx, id)
      if (built) (node.slots ??= {})[slot] = built
    }
  }
  for (const child of tpl.children ?? []) {
    if (child === `$children`) {
      node.children.push(...exp.source.children)
      if (exp.source.template) node.template = exp.source.template
      continue
    }
    if (typeof child === `string`) {
      if (child.startsWith(`$slot:`)) {
        const value = exp.source.slots?.[child.slice(6)]
        if (value) node.children.push(value)
      }
      continue
    }
    node.children.push(...expandTemplate(child as MacroTemplate, exp, ctx, id))
  }
  return node
}

/** Expand one macro node (children already native) into its template. */
function expandMacro(node: UiNode, macro: string, def: MacroDef, view: CatalogView, depth: number, issues?: ReduceIssue[]): UiNode {
  if (depth > MAX_DEPTH) throw new Error(`macro ${macro}: expansion deeper than ${MAX_DEPTH}`)
  const baseProps = propsAt(node.props, null)
  // Round 3: an `on.press` implies `pressable` on a macro that offers it (Row, Chip).
  if (node.on?.press && baseProps.pressable === undefined && view.components[macro]?.props.pressable) baseProps.pressable = true
  const exp: Expansion = { view, macro, def, source: node, baseProps, variants: responsiveVariants(node.props), claimed: new Set(), issues }
  const ctx: ExprContext = { id: node.id, props: baseProps, vars: {} }
  const [root] = expandTemplate(def.root, exp, ctx, node.id)
  root.style = mergeStyle(root.style, node.style)
  if (root.style && Object.keys(root.style).length === 0) delete root.style
  if (node.visible !== undefined) root.visible = node.visible
  // The author's label/description (or an outer macro part's $a11y) win
  // over the root part's own.
  if (node.accessibility) root.accessibility = { ...(root.accessibility ?? {}), ...node.accessibility }
  const remaining: Record<string, Action> = { ...(root.on ?? {}) }
  for (const [event, action] of Object.entries(node.on ?? {})) {
    if (!exp.claimed.has(event)) remaining[event] = action
  }
  if (Object.keys(remaining).length > 0) root.on = remaining
  else delete root.on
  return expandTree(root, view, depth + 1, issues)
}

function expandTree(node: UiNode, view: CatalogView, depth: number, issues?: ReduceIssue[]): UiNode {
  const out: UiNode = { ...node, children: node.children.map((c) => expandTree(c, view, depth, issues)) }
  if (node.slots) {
    out.slots = {}
    for (const [slot, child] of Object.entries(node.slots)) out.slots[slot] = expandTree(child, view, depth, issues)
  }
  const def = view.macros[out.component]
  if (def && view.components[out.component]?.kind === `macro`) {
    return expandMacro(out, out.component, def, view, depth, issues)
  }
  return out
}

/** The normalized tree with every macro (core + extensions) expanded into
 *  natives, recipes attached. Pure: the input is not mutated; expansion
 *  problems (an author function that replaces a `$set` write) are pushed to
 *  `options.issues` when given. */
export function expandMacros(
  root: UiNode,
  options: { extensions?: readonly ExtensionDef[]; issues?: ReduceIssue[] } = {}
): UiNode {
  // Round 3: the tree guides of nested Rows come from their siblings, so
  // they are filled AFTER the whole tree is native (src/tree-guides.ts).
  return applyTreeGuides(expandTree(clone(root), catalogView(options.extensions), 0, options.issues))
}

/** True for a value the expander would keep (see expr.ts). */
export const keeps = truthy
export { isDynamic }
export type { MacroChild }
