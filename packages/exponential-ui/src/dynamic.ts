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
import { ENGLISH_FORMAT_FUNCTIONS, FORMAT_FUNCTION_NAMES, englishFormatter, formatFunctions, type Formatter } from "./format"
import { resolveString } from "./strings"
import type { Action, ComponentDef, ExtensionDef, PropSchema, UiNode } from "./types"
import { MAX_POINTER_BYTES, MAX_POINTER_SEGMENTS } from "./limits"

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

/** A pointer write: the new data model, or `error` (data unchanged). */
export interface PointerWrite {
  data: unknown
  error?: string
}

const utf8Length = (text: string) => new TextEncoder().encode(text).length

/** A pointer's tokens (`` and `/` = the whole model), or the limit it breaks. */
export function writeTokens(pointer: string): string[] | string {
  if (utf8Length(pointer) > MAX_POINTER_BYTES) return POINTER_ISSUES.bytes
  if (pointer === `` || pointer === `/`) return []
  const tokens = (pointer.startsWith(`/`) ? pointer.slice(1) : pointer).split(`/`)
  if (tokens.length > MAX_POINTER_SEGMENTS) return POINTER_ISSUES.segments
  return tokens.map(unescape)
}

/** VAPP-103: the pointer-write refusals, byte-identical in every core. */
export const POINTER_ISSUES = {
  bytes: `data: pointer longer than ${MAX_POINTER_BYTES} bytes`,
  segments: `data: pointer has more than ${MAX_POINTER_SEGMENTS} segments`,
  notIndex: (token: string) => `data: ${JSON.stringify(token)} is not an array index`,
  pastEnd: (token: string, length: number) => `data: index ${token} is past the end of the array (${length} items)`,
} as const

/** The index a token names in an array of `length` items: digits up to
 *  `length` (`length` and `-` append), else the refusal (JSON Pointer:
 *  never a gap, never a key). */
function arrayIndex(token: string, length: number): number | string {
  if (token === `-`) return length
  if (!/^[0-9]+$/.test(token)) return POINTER_ISSUES.notIndex(token)
  const index = Number(token)
  return index <= length ? index : POINTER_ISSUES.pastEnd(token, length)
}

function setOwn(target: Record<string, unknown>, key: string, value: unknown): void {
  Object.defineProperty(target, key, { value, writable: true, enumerable: true, configurable: true })
}

/** A copy of `data` with `value` written at the absolute pointer
 *  (`undefined` removes). Missing containers are OBJECTS; an array takes an
 *  index up to its length (`length` and `-` append). Refused (data
 *  unchanged, `error` set): a pointer past `maxPointerBytes` /
 *  `maxPointerSegments`, a non-index token or an index past the end of an
 *  array. Iterative: any depth, no recursion. */
export function writePointer(data: unknown, pointer: string, value: unknown): PointerWrite {
  const tokens = writeTokens(pointer)
  if (typeof tokens === `string`) return { data, error: tokens }
  if (tokens.length === 0) return { data: value === undefined ? {} : value }
  const chain: (unknown[] | Record<string, unknown>)[] = []
  const keys: (number | string)[] = []
  let cur: unknown = data
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i]!
    const last = i === tokens.length - 1
    if (Array.isArray(cur)) {
      const index = arrayIndex(token, cur.length)
      if (typeof index === `string`) return { data, error: index }
      const next = [...cur]
      if (!last) cur = next[index]
      else if (value === undefined) {
        if (index < next.length) next.splice(index, 1)
      } else next[index] = value
      chain.push(next)
      keys.push(index)
    } else {
      const next: Record<string, unknown> = {}
      if (cur !== null && typeof cur === `object`) for (const [k, v] of Object.entries(cur)) setOwn(next, k, v)
      if (!last) cur = Object.prototype.hasOwnProperty.call(next, token) ? next[token] : undefined
      else if (value === undefined) delete next[token]
      else setOwn(next, token, value)
      chain.push(next)
      keys.push(token)
    }
  }
  for (let i = chain.length - 2; i >= 0; i--) {
    const parent = chain[i]!
    if (Array.isArray(parent)) parent[keys[i] as number] = chain[i + 1]
    else setOwn(parent, keys[i] as string, chain[i + 1])
  }
  return { data: chain[0] }
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

/** The reference function table: the core functions + and/or/not/required
 *  + (round 2) the format functions through the English fallback Formatter
 *  (`formatNumber`, `formatCurrency`, `formatPercent`, `formatDate`,
 *  `formatRelativeTime`, `pluralize`; ResolveOptions.formatter replaces it).
 *  `set` is an action, never a value. The expander EMITS `not`/`and`/`or`
 *  too, so a renderer must implement this whole table (generated as
 *  `bindFunctionNames`), not just the core names. */
export const BIND_FUNCTIONS: FunctionTable = { ...LOGIC_FUNCTIONS, ...CORE_FUNCTIONS, ...ENGLISH_FORMAT_FUNCTIONS }
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
  /** Round 2: the surface's Formatter (SurfaceSettings.locale + timeZone);
   *  the format functions run through it. Absent = the English fallback. */
  formatter?: Formatter
  /** The clock `formatRelativeTime` reads without a `now` argument. */
  now?: () => number
}

const FORMAT_NAMES: ReadonlySet<string> = new Set(FORMAT_FUNCTION_NAMES)
const FORMAT_TABLES = new WeakMap<Formatter, FunctionTable>()

function formatTable(options: ResolveOptions): FunctionTable | undefined {
  if (!options.formatter && !options.now) return undefined
  const formatter = options.formatter ?? (english ??= englishFormatter())
  if (options.now) return formatFunctions(formatter, options.now)
  let table = FORMAT_TABLES.get(formatter)
  if (!table) FORMAT_TABLES.set(formatter, (table = formatFunctions(formatter)))
  return table
}

let english: Formatter | undefined

/** A prop or style value with every binding and call resolved, at any
 *  depth. An unknown function resolves to undefined. */
export function resolveDynamic(value: unknown, data: unknown, options: ResolveOptions = {}): unknown {
  if (isBinding(value)) return readPath(data, value.path, options.scope)
  if (isCall(value)) {
    const args: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(value.args ?? {})) args[k] = resolveDynamic(v, data, options)
    const fn = options.functions?.[value.call] ?? (FORMAT_NAMES.has(value.call) ? formatTable(options)?.[value.call] : undefined) ?? BIND_FUNCTIONS[value.call]
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
  /** VAPP-103: the `set` was refused (writePointer's reason). */
  error?: string
  /** The event to dispatch, its context resolved BEFORE the write. */
  event?: { name: string; context?: Record<string, unknown> }
  /** A function other than `set` the host must run (openUrl…), args resolved. */
  call?: { call: string; args: Record<string, unknown> }
}

/** Round 4 (VAPP-103): the data an interaction's action resolves against =
 *  the data model WITH the source component's OWN write applied first.
 *  `own` = EXACTLY what the component wrote, by prop (an Input's `value`,
 *  a Checkbox's `checked`, an overlay's `open`, a FileUpload's MERGED
 *  `files`), never inferred from the event payload (an `upload` payload's
 *  `added` is the new batch, the write is the whole list): each prop that
 *  is a `{path}` binding is written at that path, so a `context.query`
 *  bound to the path the Input writes reads the text just typed. Renderers
 *  apply it before `runAction` (Rust: every write lands before
 *  `Surface::fire`). */
export function withOwnWrites(data: unknown, props: Record<string, unknown>, own: Record<string, unknown> | undefined, scope: DataScope = {}): unknown {
  let out = data
  for (const [key, value] of Object.entries(own ?? {})) {
    const bound = props[key]
    if (!isBinding(bound)) continue
    if (!bound.path.startsWith(`/`) && hasItem(scope)) continue
    // A refused write (limits, index rules) leaves the data as it is.
    out = writePointer(out, absolutePath(bound.path, scope), value).data
  }
  return out
}

/** The overlay components (catalog group `overlay`; AlertDialog expands to
 *  a non-dismissible Dialog). */
export const OVERLAY_COMPONENTS: readonly string[] = [`Dialog`, `Drawer`, `AlertDialog`, `Popover`, `Tooltip`, `Menu`, `Toast`]
const OVERLAYS: ReadonlySet<string> = new Set(OVERLAY_COMPONENTS)

/** Round 4 (VAPP-103): a successful Form submit CLOSES the overlay around
 *  the form (the author never resets the bound `open` flag). The NEAREST
 *  enclosing overlay of ANY kind decides: it closes only when it is a
 *  Dialog or Drawer that is dismissible (`dismissible: false` — an
 *  AlertDialog — never auto-closes); a nearest Popover, Menu, Tooltip or
 *  Toast closes nothing, nor does anything around it. The id of that
 *  overlay in the EXPANDED tree, null when nothing closes. Renderers close
 *  it like a dismiss (`open` false written through, `change {open:
 *  false}`) right after the form's `submit`. */
export function submitClosesOverlay(root: UiNode, formId: string): string | null {
  const walk = (n: UiNode, overlay: UiNode | null): UiNode | null | undefined => {
    const here = OVERLAYS.has(n.component) && n.id !== formId ? n : overlay
    if (n.id === formId && n.component === `Form`) return here
    for (const slot of Object.values(n.slots ?? {})) {
      const hit = walk(slot, here)
      if (hit !== undefined) return hit
    }
    for (const child of n.children) {
      const hit = walk(child, here)
      if (hit !== undefined) return hit
    }
    return undefined
  }
  const nearest = walk(root, null)
  if (!nearest || (nearest.component !== `Dialog` && nearest.component !== `Drawer`) || nearest.props.dismissible === false) return null
  return nearest.id
}

/** What a press does: resolve the function args and the event context
 *  against the data AS IT IS (after `withOwnWrites`), then apply `set`, then
 *  hand back the event. */
export function runAction(action: Action, data: unknown, options: ResolveOptions = {}): ActionOutcome {
  const out: ActionOutcome = { data }
  const event = action.event
    ? { name: action.event.name, ...(action.event.context ? { context: resolveDynamic(action.event.context, data, options) as Record<string, unknown> } : {}) }
    : undefined
  // A2UI's `functionCall` (round 4: the only key; no legacy `function`).
  const fn = action.functionCall
  if (fn) {
    const args = resolveDynamic(fn.args ?? {}, data, options) as Record<string, unknown>
    if (fn.call === `set`) {
      // A relative path under a literal-item scope names no data: no write.
      if (typeof args.path === `string` && (args.path.startsWith(`/`) || !hasItem(options.scope))) {
        const pointer = absolutePath(args.path, options.scope)
        // No value: the key exists without one, which JSON drops (the
        // containers on the way are still created).
        const written = writePointer(data, pointer, args.value === undefined ? null : args.value)
        const final = args.value === undefined && !written.error ? writePointer(written.data, pointer, undefined) : written
        out.data = final.data
        if (final.error) out.error = final.error
      }
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

/** Round 2 (docs/round-2-contract.md §4): the scope a List's `section`
 *  header binds in — a LITERAL item `{value, count, index}` (relative paths
 *  read it: `{path: "value"}`; absolute ones the surface data). */
export function sectionScope(section: { value: string; count: number }, index: number, scope: DataScope = {}): DataScope {
  return { ...(scope.base !== undefined ? { base: scope.base } : {}), item: { value: section.value, count: section.count, index } }
}

/** A List `section` header bound for section `index` (see `sectionScope`). */
export function bindSectionHeader(slot: UiNode, section: { value: string; count: number }, index: number, data: unknown, options: ResolveOptions = {}): UiNode | null {
  return bindTree(slot, data, { ...options, scope: sectionScope(section, index, options.scope) })
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
