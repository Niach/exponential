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

import { sharedCatalogView, type CatalogView } from "./catalog"
import { A2UI_BASIC_CATALOG_ID, CORE_CATALOG_ID } from "./catalog"
import { FUNCTION_NAMES } from "./catalog.generated"
import { basicMap } from "./basic-map"
import { CORE_FUNCTIONS, coerceBoolean, isBinding, isCall, isDynamic, present } from "./expr"
import { regexCheck } from "./regex"
import { ENGLISH_FORMAT_FUNCTIONS, FORMAT_FUNCTION_NAMES, englishFormatter, formatFunctions, type Formatter, type ZoneOffset } from "./format"
import { EXPRESSION_BIND_FUNCTIONS, EXPRESSION_CALL_NAMES, callExpressionFunction, type ExpressionEnv } from "./cel"
import { resolveString } from "./strings"
import { displayString } from "./format"
import type { Action, ComponentDef, ExtensionDef, FunctionCall, PropSchema, UiNode } from "./types"
import { MAX_POINTER_BYTES, MAX_POINTER_SEGMENTS } from "./limits"

/** Where relative paths resolve: the pointer of the current template item
 *  (`/rows/3`), or the root; `item` (when the key is present) = a LITERAL
 *  item that is not in the data model (a Table row of literal `rows`):
 *  relative paths then read inside it, absolute ones still the data. */
export interface DataScope {
  base?: string
  item?: unknown
  /** Round 9 §6.2: the 0-based index of the innermost template item (List /
   *  Stack / Grid ChildList item in data order, sections included; a Table
   *  row), what `@index` answers. */
  index?: number
}

/** RFC 6901 unescaping of one reference token. */
const unescape = (token: string) => token.replace(/~1/g, `/`).replace(/~0/g, `~`)

/** The absolute pointer a binding path names in a scope: `/a/b` stays,
 *  `b/c` joins the scope's base, `` (empty) is the base itself. */
/** A table's OWN entry for an agent-chosen key, never `Object.prototype`'s. */
function own<T>(table: Record<string, T> | undefined, key: string): T | undefined {
  return table !== undefined && Object.prototype.hasOwnProperty.call(table, key) ? table[key] : undefined
}

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

/** An array index token: ASCII digits only (a read and a write alike). */
const INDEX = /^[0-9]+$/

/** The value at an absolute pointer; undefined when any step is missing.
 *  `` and `/` = the whole model (as `writeTokens` writes it); only OWN
 *  keys and real indices are read (`/constructor` names nothing); `-` reads
 *  an array's LAST element, the one an append just landed on (EXP-1385:
 *  a landed write reads back, `fixtures/pointer.json`). */
export function readPointer(data: unknown, pointer: string): unknown {
  if (pointer === `` || pointer === `/`) return data
  let cur: unknown = data
  // VAPP-107 (property-tested): a pointer without its leading slash reads
  // the same tokens it writes (`writeTokens`, the Rust core's
  // `pointer_tokens`), not an empty first token.
  for (const raw of (pointer.startsWith(`/`) ? pointer.slice(1) : pointer).split(`/`)) {
    if (cur === undefined || cur === null || typeof cur !== `object`) return undefined
    const token = unescape(raw)
    if (Array.isArray(cur)) cur = token === `-` ? cur.at(-1) : INDEX.test(token) ? cur[Number(token)] : undefined
    else cur = Object.prototype.hasOwnProperty.call(cur, token) ? (cur as Record<string, unknown>)[token] : undefined
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
  if (!INDEX.test(token)) return POINTER_ISSUES.notIndex(token)
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

/** The basic catalog's logic functions as the core evaluates them. Round 9
 *  §6.7: `and`/`or`/`not` coerce their args with the A2UI 1.0 table. */
const LOGIC_FUNCTIONS: Record<string, (args: Record<string, unknown>) => unknown> = {
  and: ({ values }) => Array.isArray(values) && values.every(coerceBoolean),
  or: ({ values }) => Array.isArray(values) && values.some(coerceBoolean),
  not: ({ value }) => !coerceBoolean(value),
}

const num = (v: unknown) => (typeof v === `number` ? v : Number(v))
const textOf = (v: unknown) => (v === undefined || v === null ? `` : typeof v === `string` ? v : typeof v === `object` ? JSON.stringify(v) : String(v))

/** Round 9 §6.6: the validators return EXACTLY `{"valid": true}` or
 *  `{"valid": false}`. `required` = presence and not an empty array; the
 *  others as round 1; `regex` = the §9.3 dialect (src/regex.ts; a refused
 *  pattern THROWS: an EXPRESSION_ERROR failure). */
export const VALIDATOR_FUNCTIONS: Record<string, (args: Record<string, unknown>) => unknown> = {
  required: ({ value }) => ({ valid: present(value) && !(Array.isArray(value) && value.length === 0) }),
  regex: ({ value, pattern }) => {
    const out = regexCheck(value, pattern)
    if (`error` in out) throw new FunctionFailure(`EXPRESSION_ERROR`, out.error)
    return { valid: out.result.valid }
  },
  length: ({ value, min, max }) => {
    const n = [...textOf(value)].length
    return { valid: (min === undefined || min === null || n >= num(min)) && (max === undefined || max === null || n <= num(max)) }
  },
  numeric: ({ value, min, max }) => {
    const n = typeof value === `string` && value.trim() === `` ? NaN : num(value)
    return { valid: Number.isFinite(n) && (min === undefined || min === null || n >= num(min)) && (max === undefined || max === null || n <= num(max)) }
  },
  email: ({ value }) => ({ valid: /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(textOf(value)) }),
}

/** Round 9 §6.8: why an evaluation failed, as the error the renderer sends. */
export interface Failure {
  code: `EXPRESSION_ERROR` | `EXECUTION_ERROR` | `UNKNOWN_FUNCTION` | `INVALID_FUNCTION_CALL`
  message: string
  /** A routed call's id (the error then names it instead of the surface). */
  functionCallId?: string
}

/** A function that fails with a specific code (a refused regex pattern). */
export class FunctionFailure extends Error {
  constructor(
    readonly code: Failure[`code`],
    message: string
  ) {
    super(message)
    this.name = `FunctionFailure`
  }
}

export type FunctionTable = Record<string, (args: Record<string, unknown>) => unknown>

/** The reference function table: the validators, the logic functions, the
 *  core functions and (round 2) the format functions through the English
 *  fallback Formatter (`formatNumber`, `formatCurrency`, `formatPercent`,
 *  `formatDate`, `formatRelativeTime`, `pluralize`; ResolveOptions.formatter
 *  replaces it). `set`, `bind` and `openUrl` are actions, never values;
 *  `formatString` and `@index` read the scope (evaluateCall). The expander
 *  EMITS `not`/`and`/`or`/`cond` too, so a renderer must implement this
 *  whole table (generated as `bindFunctionNames`). */
export const BIND_FUNCTIONS: FunctionTable = { ...VALIDATOR_FUNCTIONS, ...LOGIC_FUNCTIONS, ...CORE_FUNCTIONS, ...ENGLISH_FORMAT_FUNCTIONS, ...EXPRESSION_BIND_FUNCTIONS }
export const BIND_FUNCTION_NAMES: readonly string[] = Object.keys(BIND_FUNCTIONS)

/** Round 9 §6.1/§6.4: the effectful catalog functions (never in a reactive
 *  position): `set` writes, `bind` is a host effect, `openUrl` needs a user gesture. */
export const EFFECT_FUNCTIONS: ReadonlySet<string> = new Set([`set`, `bind`, `openUrl`])
/** Round 9 §6.1: `openUrl` needs a user gesture. */
export const USER_ACTIVATION_FUNCTIONS: ReadonlySet<string> = new Set([`openUrl`])
/** Round 9 §6.1: `allowedCallers: rendererOnly`. */
export const RENDERER_ONLY_FUNCTIONS: ReadonlySet<string> = new Set([`set`, `bind`, `openUrl`])

/** Round 9 §6.3: a host function name (an exact host name or a reserved prefix). */
export function isHostFunctionName(name: string): boolean {
  return HOST_NAMES.has(name) || HOST_PREFIXES.some((p) => name.startsWith(p) && name.length > p.length)
}
const HOST_PREFIXES: readonly string[] = [`actions_`, `harness_`, `exponential_`, `tools_`, `upstream_`, `camera_`, `clipboard_`, `files_`, `http_`, `location_`, `notifications_`, `photos_`, `print_`, `share_`, `app_`]
const HOST_NAMES: ReadonlySet<string> = new Set([`app_navigate`, `app_toast`])

/** The catalog functions a catalog id offers (§5.5): core and core-lite =
 *  every core + basic name; basic = the 14; an extension = the core's. */
export function catalogFunctionNames(catalogId: string): ReadonlySet<string> {
  return catalogId === A2UI_BASIC_CATALOG_ID ? BASIC_NAMES : CORE_NAMES
}
const CORE_NAMES: ReadonlySet<string> = new Set(FUNCTION_NAMES)
const BASIC_NAMES: ReadonlySet<string> = new Set(Object.keys(basicMap.functions))

/** Round 9 §6.5: a call the host routes (a host, host-prefixed or unknown name). */
export interface RoutedCall {
  name: string
  catalogId: string
  args: Record<string, unknown>
  /** `reactive` (a binding, a check) or `action` (a press). */
  position: `reactive` | `action`
}
/** The host's answer: a value (local run, a settled agent answer, the
 *  pending placeholder) or a failure. */
export type RouteAnswer = { value: unknown } | { failure: Failure }

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
  /** VAPP-191: the team zone the expression functions (`today`, `dateAdd`…,
   *  src/cel.ts) run in; absent = the formatter's `offsetMinutes`, else UTC. */
  zone?: ZoneOffset
  /** Round 9 §5.5: the surface's default catalog (a call's own `catalogId`
   *  wins); absent = the core. */
  catalogId?: string
  /** Round 9 §5.5: the router's available catalog ids (absent = no check). */
  available?: readonly string[]
  /** Round 9 §6.4: `reactive` (bindings, visible, accessibility, checks;
   *  the default) or `action` (a press: effects may run). */
  position?: `reactive` | `action`
  /** Round 9 §6.5: the host's routing of a name that is no local catalog
   *  function (policy, registered host functions, the agent channel). */
  route?: (call: RoutedCall) => RouteAnswer
  /** Round 9 §6.8: a failed evaluation (the host sends it). */
  onError?: (failure: Failure) => void
}

const FORMAT_NAMES: ReadonlySet<string> = new Set(FORMAT_FUNCTION_NAMES)
const EXPRESSION_NAMES: ReadonlySet<string> = new Set(EXPRESSION_CALL_NAMES)

function expressionEnv(options: ResolveOptions): ExpressionEnv {
  const zone = options.zone ?? (options.formatter?.offsetMinutes ? (ms: number) => options.formatter!.offsetMinutes!(ms) : undefined)
  return { ...(options.now ? { now: options.now() } : {}), ...(zone ? { offset: zone } : {}) }
}
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

const isPlainObject = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

/** Where a resolution failure is recorded: the first one wins. */
interface FailBox {
  failure?: Failure
}

/** A prop or style value with every binding and call resolved, at any
 *  depth (round 9 §3.2: a literal object's `@@` keys lose one `@`). A
 *  failing call resolves to `null` (its `default` arg when it has one,
 *  VAPP-195) and the failure goes to `options.onError` (§6.8). */
export function resolveDynamic(value: unknown, data: unknown, options: ResolveOptions = {}): unknown {
  const box: FailBox = {}
  const out = resolveIn(value, data, options, box)
  if (box.failure) options.onError?.(box.failure)
  return out
}

/** `resolveDynamic` that also answers the first failure (§6.8). */
export function resolveWithFailure(value: unknown, data: unknown, options: ResolveOptions = {}): { value: unknown; failure?: Failure } {
  const box: FailBox = {}
  const out = resolveIn(value, data, options, box)
  return box.failure ? { value: out, failure: box.failure } : { value: out }
}

function resolveIn(value: unknown, data: unknown, options: ResolveOptions, box: FailBox): unknown {
  if (isBinding(value)) return readPath(data, value[`@path`], options.scope)
  if (isCall(value)) return evaluateCallIn(value, data, options, box)
  if (typeof value === `string`) return options.strings ? resolveString(value, options.strings) : value
  if (Array.isArray(value)) return value.map((v) => resolveIn(v, data, options, box))
  if (isPlainObject(value)) {
    const out: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(value)) {
      const r = resolveIn(v, data, options, box)
      if (r !== undefined) out[k.startsWith(`@@`) ? k.slice(1) : k] = r
    }
    return out
  }
  return value
}

const fail = (box: FailBox, failure: Failure, args?: Record<string, unknown>): unknown => {
  // VAPP-195: a call with a `default` shows it instead of failing.
  if (args && args.default !== undefined) return args.default
  box.failure ??= failure
  return null
}

/** Round 9 §6.5: one call evaluated (local `@index`, a catalog function of
 *  the resolved catalog, else the host's route): its value, or `null` plus
 *  the failure. `position` defaults to `reactive`. */
export function evaluateCall(call: FunctionCall, data: unknown, options: ResolveOptions = {}): { value: unknown; failure?: Failure } {
  const box: FailBox = {}
  const value = evaluateCallIn(call, data, options, box)
  return box.failure ? { value, failure: box.failure } : { value }
}

function evaluateCallIn(call: FunctionCall, data: unknown, options: ResolveOptions, box: FailBox): unknown {
  const name = call[`@call`]
  const reactive = (options.position ?? `reactive`) === `reactive`
  // 1. `@index`: the innermost template item's index + offset.
  if (name === `@index`) {
    const offset = isPlainObject(call.args) && call.args.offset !== undefined ? resolveIn(call.args.offset, data, options, box) : 0
    if (options.scope?.index === undefined) return fail(box, { code: `EXPRESSION_ERROR`, message: `@index outside a template` })
    return options.scope.index + (typeof offset === `number` ? offset : Number(offset) || 0)
  }
  // 2. The call's catalog: its own, else the surface default.
  const catalogId = typeof call.catalogId === `string` ? call.catalogId : (options.catalogId ?? CORE_CATALOG_ID)
  if (options.available && !options.available.includes(catalogId)) return fail(box, { code: `EXPRESSION_ERROR`, message: `catalog ${catalogId} is not available` })
  // 3. The args, at any depth; a failing arg fails the call.
  const before = box.failure
  const args: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(isPlainObject(call.args) ? call.args : {})) args[k] = resolveIn(v, data, options, box)
  if (box.failure !== before) return null
  // A var bound to nothing (a row's missing optional field) reads null in
  // the expression, never an unknown name (spec §4.2, expressions.json).
  if (name === `expr` && isPlainObject(call.args?.vars) && isPlainObject(args.vars))
    for (const k of Object.keys(call.args.vars)) if (!(k in (args.vars as object))) (args.vars as Record<string, unknown>)[k] = null
  // 4. A catalog function of the resolved catalog.
  if (catalogFunctionNames(catalogId).has(name)) {
    if (reactive && EFFECT_FUNCTIONS.has(name)) return fail(box, { code: `EXPRESSION_ERROR`, message: `${name} cannot run in a binding` })
    if (name === `formatString`) return formatStringValue(args.value, data, options)
    const run = (fn: (args: Record<string, unknown>) => unknown): unknown => {
      try {
        const out = fn(args)
        return out === undefined ? (args.default !== undefined ? args.default : out) : out
      } catch (e) {
        return fail(box, { code: e instanceof FunctionFailure ? e.code : `EXPRESSION_ERROR`, message: e instanceof Error ? e.message : String(e) }, args)
      }
    }
    const fn = own(options.functions, name) ?? (FORMAT_NAMES.has(name) ? own(formatTable(options), name) : undefined)
    if (fn) return run(fn)
    // VAPP-191: the expression functions read the clock and the team zone.
    if (EXPRESSION_NAMES.has(name)) return callExpressionFunction(name, args, expressionEnv(options))?.value
    const core = own(BIND_FUNCTIONS, name)
    if (core) return run(core)
    // `set`, `bind`, `openUrl` in an action: the host runs them.
  }
  // 5. Any other name: a host function, a host-prefixed name, an agent function.
  if (reactive && (isHostFunctionName(name) || EFFECT_FUNCTIONS.has(name))) return fail(box, { code: `EXPRESSION_ERROR`, message: `${name} cannot run in a binding` })
  const local = own(options.functions, name)
  if (local) {
    try {
      return local(args)
    } catch (e) {
      return fail(box, { code: `EXPRESSION_ERROR`, message: e instanceof Error ? e.message : String(e) }, args)
    }
  }
  if (options.route) {
    const answer = options.route({ name, catalogId, args, position: options.position ?? `reactive` })
    if (`failure` in answer) return fail(box, answer.failure, args)
    return answer.value
  }
  return fail(box, { code: `UNKNOWN_FUNCTION`, message: `no function ${name}` }, args)
}

/** The basic `formatString`: `${<pointer>}` (absolute, or relative to the
 *  scope) and `${@index()}` / `${@index(offset: <number literal>)}` (§6.2). */
function formatStringValue(template: unknown, data: unknown, options: ResolveOptions): string {
  return textOf(template).replace(/\$\{([^}]*)\}/g, (_, raw: string) => {
    const expr = raw.trim()
    const index = /^@index\(\s*(?:offset\s*:\s*(-?\d+(?:\.\d+)?)\s*)?\)$/.exec(expr)
    if (index) return options.scope?.index === undefined ? `` : String(options.scope.index + (index[1] ? Number(index[1]) : 0))
    return displayString(readPath(data, expr, options.scope))
  })
}

/** Round 9 §6.6: a ValidationResult. */
export interface ValidationResult {
  valid: boolean
  code?: string
  message?: string
  severity?: `error` | `warning` | `info`
  [key: string]: unknown
}

/** Round 9 §6.6: a check's RESOLVED condition → its ValidationResult
 *  (`failure` = the condition failed, §6.8). */
export function checkResult(condition: unknown, message: unknown, failure?: Failure): ValidationResult {
  const msg = typeof message === `string` ? message : undefined
  if (failure) return { valid: false, message: msg ?? failure.message, severity: `error` }
  if (isPlainObject(condition) && typeof condition.valid === `boolean`) {
    const out: ValidationResult = { ...(condition as ValidationResult) }
    if (out.message === undefined && msg !== undefined) out.message = msg
    if (out.severity === undefined) out.severity = `error`
    return out.valid ? (condition as ValidationResult) : out
  }
  if (coerceBoolean(condition)) return { valid: true }
  return msg === undefined ? { valid: false, severity: `error` } : { valid: false, message: msg, severity: `error` }
}

/** Round 9 §6.6: one CheckRule evaluated against the data. */
export function evaluateCheck(rule: { condition?: unknown; message?: unknown }, data: unknown, options: ResolveOptions = {}): ValidationResult {
  const { value, failure } = resolveWithFailure(rule.condition, data, options)
  if (failure) options.onError?.(failure)
  return checkResult(value, rule.message, failure)
}

/** Round 9 §6.6: the ONE result a field shows: the first failing `error`,
 *  else the first `warning`, else the first `info`; null = all valid. */
export function shownResult(results: readonly ValidationResult[]): ValidationResult | null {
  for (const severity of [`error`, `warning`, `info`] as const) {
    const hit = results.find((r) => !r.valid && (r.severity ?? `error`) === severity)
    if (hit) return hit
  }
  return null
}

/** A node's `visible`: absent = shown; else its resolved value under the
 *  A2UI 1.0 boolean coercion (round 9 §6.7). */
export function isVisible(visible: unknown, data: unknown, options: ResolveOptions = {}): boolean {
  if (visible === undefined) return true
  return coerceBoolean(resolveDynamic(visible, data, options))
}

export interface ActionOutcome {
  /** The data model after the action's `set` (unchanged without one). */
  data: unknown
  /** VAPP-103: the `set` was refused (writePointer's reason). */
  error?: string
  /** The event to dispatch, its context resolved BEFORE the write. */
  event?: { name: string; context?: Record<string, unknown>; userMessage?: string }
  /** A function other than `set` the host must run (openUrl…), args resolved. */
  call?: { call: string; args: Record<string, unknown>; catalogId?: string }
  /** Round 9 §6.8: resolving failed: the action stopped (no event). */
  failure?: Failure
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
    if (!bound[`@path`].startsWith(`/`) && hasItem(scope)) continue
    // A refused write (limits, index rules) leaves the data as it is.
    out = writePointer(out, absolutePath(bound[`@path`], scope), value).data
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
 *  (and `userMessage`) against the data AS IT IS (after `withOwnWrites`), in
 *  an ACTION position, then apply `set`, then hand back the event. Round 9
 *  §6.8: a failure while resolving stops the action (no write, no event,
 *  `failure` set). */
export function runAction(action: Action, data: unknown, options: ResolveOptions = {}): ActionOutcome {
  const out: ActionOutcome = { data }
  const opts: ResolveOptions = { ...options, position: `action`, onError: undefined }
  const box: FailBox = {}
  let event: ActionOutcome[`event`]
  if (action.event) {
    event = { name: action.event.name }
    if (action.event.context) event.context = resolveIn(action.event.context, data, opts, box) as Record<string, unknown>
    if (action.event.userMessage !== undefined) {
      const message = resolveIn(action.event.userMessage, data, opts, box)
      if (typeof message === `string`) event.userMessage = message
    }
  }
  const fn = action.functionCall
  const args = fn ? (resolveIn(fn.args ?? {}, data, opts, box) as Record<string, unknown>) : undefined
  if (box.failure) {
    out.failure = box.failure
    return out
  }
  if (fn && args) {
    if (fn[`@call`] === `set`) {
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
    } else out.call = { call: fn[`@call`], args, ...(typeof fn.catalogId === `string` ? { catalogId: fn.catalogId } : {}) }
  }
  if (event) out.event = event
  return out
}

/** A DATA schema: a shape-less `object` (any object) or an array of them —
 *  Table `rows` in the core catalog. A literal value there is the author's
 *  data, copied verbatim: no binding, call or `$string.<id>` inside it is
 *  interpreted (a row `{"@path": "/etc/hosts"}` is a row). A dynamic value AT
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
  if (isBinding(rowsProp)) return { base: `${absolutePath(rowsProp[`@path`], scope)}/${index}`, index }
  return { ...(scope.base !== undefined ? { base: scope.base } : {}), item: rows[index], index }
}

/** A row-scoped slot cell bound for row `index` (see `rowScope`). */
export function bindRowSlot(slot: UiNode, rowsProp: unknown, rows: readonly unknown[], index: number, data: unknown, options: ResolveOptions = {}): UiNode | null {
  return bindTree(slot, data, { ...options, scope: rowScope(rowsProp, rows, index, options.scope) })
}

/** Round 2 (docs/round-2-contract.md §4): the scope a List's `section`
 *  header binds in — a LITERAL item `{value, count, index}` (relative paths
 *  read it: `{"@path": "value"}`; absolute ones the surface data). */
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
const viewFor = (extensions?: readonly ExtensionDef[]): CatalogView => sharedCatalogView(extensions)

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
