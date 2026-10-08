// Round 1 (docs/round-1-contract.md §1): the BIND-TIME half of the macro
// contract. The expander leaves a data binding `{path}` or a function call
// `{call, args}` wherever an input was bound; a renderer resolves those
// against its data model when it binds a node. This module is the reference
// every renderer's resolver mirrors: JSON-pointer lookup (absolute, relative
// to the node's data scope, or inside a literal row), the core functions plus
// the basic catalog's logic functions, values resolved at ANY depth EXCEPT
// inside DATA (a shape-less object prop such as Table `rows`, and anything a
// binding returns), `$string.<id>` built-in copy through the surface's string
// table (never inside data), the action order (evaluate everything, then
// `set`, then the event), and the per-ROW binding of row-scoped slot cells
// (Table). Pure.

import { catalogView } from "./catalog"
import { CORE_FUNCTIONS, isBinding, isCall, isDynamic, truthy } from "./expr"
import { resolveString } from "./strings"
import type { Action, ComponentDef, ExtensionDef, PropSchema, UiNode } from "./types"

/** Where relative paths resolve: the pointer of the current template item
 *  (`/rows/3`), or the root; `item` (when the key is present) = a LITERAL
 *  item that is not in the data model (a Table row of literal `rows`):
 *  relative paths then read inside it, absolute ones still the data. */
export interface DataScope {
  base?: string
  item?: unknown
}

/** RFC 6901 unescaping of one reference token. */
const unescape = (token: string) => token.replace(/~1/g, `/`).replace(/~0/g, `~`)

/** The absolute pointer a binding path names in a scope: `/a/b` stays,
 *  `b/c` joins the scope's base, `` (empty) is the base itself. */
export function absolutePath(path: string, scope: DataScope = {}): string {
  if (path.startsWith(`/`)) return path
  const base = scope.base ?? ``
  if (path === ``) return base
  return `${base}/${path}`
}

/** True when relative paths of this scope read a literal item. */
export function hasItem(scope: DataScope | undefined): boolean {
  return scope !== undefined && Object.prototype.hasOwnProperty.call(scope, `item`)
}

/** The value a binding path names in a scope: inside the literal item for a
 *  relative path under an item scope, else at its absolute pointer. */
export function readPath(data: unknown, path: string, scope: DataScope = {}): unknown {
  if (!path.startsWith(`/`) && hasItem(scope)) return path === `` ? scope.item : readPointer(scope.item, `/${path}`)
  return readPointer(data, absolutePath(path, scope))
}

/** The value at an absolute pointer; undefined when any step is missing. */
export function readPointer(data: unknown, pointer: string): unknown {
  if (pointer === `` || pointer === `/`) return pointer === `` ? data : (data as Record<string, unknown> | undefined)?.[``]
  let cur: unknown = data
  for (const raw of pointer.slice(1).split(`/`)) {
    if (cur === undefined || cur === null || typeof cur !== `object`) return undefined
    const token = unescape(raw)
    cur = Array.isArray(cur) ? cur[Number(token)] : (cur as Record<string, unknown>)[token]
  }
  return cur
}

/** A copy of `data` with `value` written at the absolute pointer (missing
 *  objects are created; an array index past the end appends). */
export function writePointer(data: unknown, pointer: string, value: unknown): unknown {
  if (pointer === ``) return value
  const [raw, ...rest] = pointer.slice(1).split(`/`)
  const token = unescape(raw)
  const tail = rest.length ? `/${rest.join(`/`)}` : ``
  if (Array.isArray(data)) {
    const out = [...data]
    const index = Number(token)
    out[index] = writePointer(out[index], tail, value)
    return out
  }
  const obj = typeof data === `object` && data !== null ? (data as Record<string, unknown>) : {}
  return { ...obj, [token]: writePointer(obj[token], tail, value) }
}

/** The basic catalog's logic functions as the core evaluates them (the
 *  validators and formatters are the renderers' own; they are locale work). */
const LOGIC_FUNCTIONS: Record<string, (args: Record<string, unknown>) => unknown> = {
  and: ({ values }) => Array.isArray(values) && values.every(truthy),
  or: ({ values }) => Array.isArray(values) && values.some(truthy),
  not: ({ value }) => !truthy(value),
  required: ({ value }) => truthy(value) && !(Array.isArray(value) && value.length === 0),
}

export type FunctionTable = Record<string, (args: Record<string, unknown>) => unknown>

/** The reference function table: the core functions + and/or/not/required.
 *  `set` is an action, never a value. The expander EMITS `not`/`and`/`or`
 *  too, so a renderer must implement this whole table (generated as
 *  `bindFunctionNames`), not just the core names. */
export const BIND_FUNCTIONS: FunctionTable = { ...LOGIC_FUNCTIONS, ...CORE_FUNCTIONS }
export const BIND_FUNCTION_NAMES: readonly string[] = Object.keys(BIND_FUNCTIONS)

export interface ResolveOptions {
  scope?: DataScope
  /** Extra functions (the host's own, the locale formatters). */
  functions?: FunctionTable
  /** The surface's built-in string table (`stringTable(overrides)`): a
   *  `$string.<id>` value resolves through it. Absent = left as written. */
  strings?: Readonly<Record<string, string>>
  /** The surface's extensions: their components' prop schemas decide which
   *  props are DATA (bindTree). */
  extensions?: readonly ExtensionDef[]
}

/** A prop or style value with every binding and call resolved, at any
 *  depth. An unknown function resolves to undefined. */
export function resolveDynamic(value: unknown, data: unknown, options: ResolveOptions = {}): unknown {
  if (isBinding(value)) return readPath(data, value.path, options.scope)
  if (isCall(value)) {
    const args: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(value.args ?? {})) args[k] = resolveDynamic(v, data, options)
    const fn = options.functions?.[value.call] ?? BIND_FUNCTIONS[value.call]
    return fn ? fn(args) : undefined
  }
  if (typeof value === `string`) return options.strings ? resolveString(value, options.strings) : value
  if (Array.isArray(value)) return value.map((v) => resolveDynamic(v, data, options))
  if (typeof value === `object` && value !== null) {
    const out: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(value)) {
      const r = resolveDynamic(v, data, options)
      if (r !== undefined) out[k] = r
    }
    return out
  }
  return value
}

/** A node's `visible`: absent = shown; else its resolved truthiness. */
export function isVisible(visible: unknown, data: unknown, options: ResolveOptions = {}): boolean {
  if (visible === undefined) return true
  return truthy(resolveDynamic(visible, data, options))
}

export interface ActionOutcome {
  /** The data model after the action's `set` (unchanged without one). */
  data: unknown
  /** The event to dispatch, its context resolved BEFORE the write. */
  event?: { name: string; context?: Record<string, unknown> }
  /** A function other than `set` the host must run (openUrl…), args resolved. */
  call?: { call: string; args: Record<string, unknown> }
}

/** What a press does: resolve the function args and the event context
 *  against the data AS IT IS, then apply `set`, then hand back the event. */
export function runAction(action: Action, data: unknown, options: ResolveOptions = {}): ActionOutcome {
  const out: ActionOutcome = { data }
  const event = action.event
    ? { name: action.event.name, ...(action.event.context ? { context: resolveDynamic(action.event.context, data, options) as Record<string, unknown> } : {}) }
    : undefined
  // A2UI's `functionCall` (VAPP-91) or the legacy `function` key.
  const fn = action.functionCall ?? action.function
  if (fn) {
    const args = resolveDynamic(fn.args ?? {}, data, options) as Record<string, unknown>
    if (fn.call === `set`) {
      // A relative path under a literal-item scope names no data: no write.
      if (typeof args.path === `string` && (args.path.startsWith(`/`) || !hasItem(options.scope)))
        out.data = writePointer(data, absolutePath(args.path, options.scope), args.value)
    } else out.call = { call: fn.call, args }
  }
  if (event) out.event = event
  return out
}

/** A DATA schema: a shape-less `object` (any object) or an array of them —
 *  Table `rows` in the core catalog. A literal value there is the author's
 *  data, copied verbatim: no binding, call or `$string.<id>` inside it is
 *  interpreted (a row `{path: "/etc/hosts"}` is a row). A dynamic value AT
 *  the position still resolves (and its result is never descended). */
export function isDataSchema(schema: PropSchema | undefined): boolean {
  if (!schema) return false
  if (schema.type === `object`) return schema.shape === undefined
  if (schema.type === `array`) return schema.items !== undefined && isDataSchema(schema.items)
  return false
}

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

/** One prop value resolved along its schema: dynamic → resolved, DATA
 *  literal → verbatim, arrays and shaped objects → per item / property,
 *  anything else → `resolveDynamic` (any depth). */
export function resolveProp(value: unknown, schema: PropSchema | undefined, data: unknown, options: ResolveOptions = {}, defs: Record<string, { properties: Record<string, PropSchema> }> = {}): unknown {
  if (isDynamic(value)) return resolveDynamic(value, data, options)
  if (isDataSchema(schema)) return value
  if (schema?.type === `array` && schema.items && Array.isArray(value)) return value.map((item) => resolveProp(item, schema.items, data, options, defs))
  const shape = schema?.type === `object` && schema.shape ? defs[schema.shape] : undefined
  if (shape && isObj(value)) {
    const out: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(value)) {
      const r = resolveProp(v, shape.properties[k], data, options, defs)
      if (r !== undefined) out[k] = r
    }
    return out
  }
  return resolveDynamic(value, data, options)
}

/** A node's props resolved along its component's schema (an unknown
 *  component's props resolve at any depth). */
export function resolveNodeProps(def: ComponentDef | undefined, props: Record<string, unknown>, data: unknown, options: ResolveOptions = {}, defs: Record<string, { properties: Record<string, PropSchema> }> = {}): Record<string, unknown> {
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(props)) {
    const r = resolveProp(v, def?.props[k], data, options, defs)
    if (r !== undefined) out[k] = r
  }
  return out
}

/** True when the component's slots are ROW-SCOPED (catalog `slotScope:
 *  "row"`, Table): each slot is a cell template the painter binds once per
 *  row (`bindRowSlot`), never against the surface. */
export function hasRowSlots(def: ComponentDef | undefined): boolean {
  return def?.slotScope === `row`
}

/** The scope row `index` of a row-scoped component binds its slot cells in.
 *  `rowsProp` = the UNBOUND `rows` value: a binding → `{base: <its pointer>/
 *  <index>}` (relative paths read the row in the data model and `set` writes
 *  into it); a literal or a call → `{item: rows[index]}` (relative paths read
 *  inside the row; a relative `set` writes nothing). `index` = the row's index
 *  in `rows` as given, before any local sort. */
export function rowScope(rowsProp: unknown, rows: readonly unknown[], index: number, scope: DataScope = {}): DataScope {
  if (isBinding(rowsProp)) return { base: `${absolutePath(rowsProp.path, scope)}/${index}` }
  return { ...(scope.base !== undefined ? { base: scope.base } : {}), item: rows[index] }
}

/** A row-scoped slot cell bound for row `index` (see `rowScope`). */
export function bindRowSlot(slot: UiNode, rowsProp: unknown, rows: readonly unknown[], index: number, data: unknown, options: ResolveOptions = {}): UiNode | null {
  return bindTree(slot, data, { ...options, scope: rowScope(rowsProp, rows, index, options.scope) })
}

/** The BIND pass a renderer runs over an expanded tree for one data model:
 *  every prop resolved along its schema (`resolveNodeProps`: DATA props
 *  verbatim), every style value, recipe prop and `accessibility` value
 *  resolved (`resolveDynamic`), a node whose `visible` resolves falsy dropped
 *  with its subtree, `visible` itself removed. Actions stay unresolved (they
 *  evaluate at press time, `runAction`); a `template` and the slots of a
 *  ROW-SCOPED component (Table cells) stay UNBOUND — the painter binds them
 *  per item / per row (`bindRowSlot`). fixtures/bind-time.json locks it. Pure. */
const CORE_VIEW = catalogView()
const VIEWS = new WeakMap<readonly ExtensionDef[], ReturnType<typeof catalogView>>()
function viewFor(extensions: readonly ExtensionDef[] | undefined) {
  if (!extensions || extensions.length === 0) return CORE_VIEW
  let view = VIEWS.get(extensions)
  if (!view) VIEWS.set(extensions, (view = catalogView(extensions)))
  return view
}

export function bindTree(node: UiNode, data: unknown, options: ResolveOptions = {}): UiNode | null {
  const view = viewFor(options.extensions)
  const walk = (n: UiNode): UiNode | null => {
    if (!isVisible(n.visible, data, options)) return null
    const def = view.components[n.component]
    const out: UiNode = { id: n.id, component: n.component, props: resolveNodeProps(def, n.props, data, options, view.defs), children: [] }
    if (n.style) out.style = resolveDynamic(n.style, data, options) as Record<string, unknown>
    if (n.on) out.on = n.on
    if (n.accessibility) {
      const a11y = resolveDynamic(n.accessibility, data, options) as Record<string, unknown>
      if (Object.keys(a11y).length > 0) out.accessibility = a11y
    }
    for (const child of n.children) {
      const bound = walk(child)
      if (bound) out.children.push(bound)
    }
    if (n.slots) {
      if (hasRowSlots(def)) out.slots = n.slots
      else
        for (const [slot, child] of Object.entries(n.slots)) {
          const bound = walk(child)
          if (bound) (out.slots ??= {})[slot] = bound
        }
    }
    if (n.template) out.template = n.template
    if (n.recipe) out.recipe = { ...n.recipe, props: resolveDynamic(n.recipe.props, data, options) as Record<string, unknown> }
    return out
  }
  return walk(node)
}
