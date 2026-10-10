// VAPP-85: the macro template's value language (catalog/macros.json
// `$comment`): `{props.x}` paths with `|fallback`, `len(path)`, `range(n)`,
// `!path`, string interpolation, and the $map / $cond / $eq / $lt / $add /
// $sub / $clamp / $percent / $coalesce / $text / $not / $fill value objects. Pure, so the
// Rust core can mirror it line by line.
//
// Round 1 (docs/round-1-contract.md §1): an input may be an A2UI DYNAMIC
// value — a data binding `{path}` or a function call `{call, args}` — which
// has no value at expansion time. The evaluator then EMITS a function call
// that the renderer evaluates at bind time, instead of computing. The calls it
// emits are the CORE functions (percent, add, sub, eq, lt, clamp, cond,
// fallback, concat, coalesce, text, map, len, fill; `CORE_FUNCTIONS` below)
// PLUS the basic catalog's logic functions `not`, `and` and `or` (`{!x}`,
// `$not`, bound `$if`/`$any`), evaluated with the same `truthy`: a renderer
// MUST implement the whole bind table (`src/dynamic.ts BIND_FUNCTIONS`,
// generated as `bindFunctionNames`). A whole-string `{props.x}` /
// `{props.x|fallback}` that is a MEMBER of a props/style/context object (or
// an array item) and whose value is dynamic passes the dynamic value through
// UNCHANGED (the fallback is dropped), so a two-way `{path}` binding survives
// expansion; as an OPERAND of a `$`-object it keeps the fallback
// (`fallback{value, default}`), so `$eq: ["{props.page|1}", 1]` still sees 1
// when the bound page is missing. `$fill` ALWAYS emits `fill` (its template is
// a `$string.<id>` the surface's string table resolves at bind time).

import { formatString } from "./strings"
import { ENGLISH_FORMAT_FUNCTIONS } from "./format"

export interface ExprContext {
  id: string
  props: Record<string, unknown>
  vars: Record<string, unknown>
}

/** An A2UI data binding: an object with exactly one key, `path`. */
export interface DataBinding {
  path: string
}
/** An A2UI function call. */
export interface FunctionCall {
  call: string
  args?: Record<string, unknown>
  returnType?: string
}
export type Dynamic = DataBinding | FunctionCall

const WHOLE = /^\{([^{}]+)\}$/
const EMBEDDED = /\{([^{}]+)\}/g
const FUNCTION = /^(len|range)\((.+)\)$/

export function isBinding(value: unknown): value is DataBinding {
  return typeof value === `object` && value !== null && !Array.isArray(value) && typeof (value as DataBinding).path === `string` && Object.keys(value as object).length === 1
}
export function isCall(value: unknown): value is FunctionCall {
  return typeof value === `object` && value !== null && !Array.isArray(value) && typeof (value as FunctionCall).call === `string`
}
/** A value the expander cannot evaluate: the renderer will. */
export function isDynamic(value: unknown): value is Dynamic {
  return isBinding(value) || isCall(value)
}

/** `undefined`, `null`, `false` and `""` are false; 0 is TRUE (a count of
 *  zero still shows). */
export function truthy(value: unknown): boolean {
  return value !== undefined && value !== null && value !== false && value !== ``
}

function call(name: string, args: Record<string, unknown>): FunctionCall {
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(args)) if (v !== undefined) out[k] = v
  return { call: name, args: out }
}

function parseLiteral(text: string): unknown {
  if (text === `true`) return true
  if (text === `false`) return false
  if (text === `null`) return null
  if (/^-?\d+(\.\d+)?$/.test(text)) return Number(text)
  return text
}

function lookup(path: string, ctx: ExprContext): unknown {
  if (path === `id`) return ctx.id
  const [head, ...rest] = path.split(`.`)
  let cur: unknown
  if (head === `props`) cur = ctx.props
  else if (head in ctx.vars) cur = ctx.vars[head]
  else return undefined
  for (const key of rest) {
    if (cur === undefined || cur === null) return undefined
    cur = (cur as Record<string, unknown>)[key]
  }
  return cur
}

function toNumber(value: unknown): number {
  return typeof value === `number` ? value : Number(value ?? 0) || 0
}

/** A string that spells a finite number (`"5"`, `" 2.5 "`), else undefined. */
function numericString(value: string): number | undefined {
  const text = value.trim()
  if (text === ``) return undefined
  const n = Number(text)
  return Number.isFinite(n) ? n : undefined
}

/** `eq` (and `$eq`): strict equality, except that a NUMBER and a string
 *  spelling that number are equal (`5` vs `"5"`: form and URL state arrive as
 *  strings). Nothing else coerces (`0` vs `""`, `1` vs `true` differ). */
export function valuesEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true
  if (typeof a === `number` && typeof b === `string`) return numericString(b) === a
  if (typeof b === `number` && typeof a === `string`) return numericString(a) === b
  return false
}

/** `clamp`: the value as a number held inside [min, max]; a missing bound
 *  (undefined or null) does not clamp; `min` wins when max < min. */
export function clampNumber(value: unknown, min: unknown, max: unknown): number {
  let n = toNumber(value)
  if (max !== undefined && max !== null) n = Math.min(n, toNumber(max))
  if (min !== undefined && min !== null) n = Math.max(n, toNumber(min))
  return n
}

/** `fill`: the template (already resolved through the string table) with its
 *  `{name}` placeholders filled from params (strings.ts formatString). */
export function fillTemplate(template: unknown, params: unknown): string {
  const text = template === undefined || template === null ? `` : String(template)
  return formatString(text, typeof params === `object` && params !== null ? (params as Record<string, unknown>) : {})
}

/** The most indexes `range(n)` yields (R8 F51): a Rating's `max` of 1e8 or
 *  1e300 would otherwise allocate that many items before the component
 *  budget could refuse them. Equals `maxTemplateItems`; a larger `n` is
 *  silently clamped (the reducer's `maxComponents` reports the overflow).
 *  Mirrors `MAX_RANGE_ITEMS` in the Rust core's `expr.rs`. */
export const MAX_RANGE_ITEMS = 10_000

/** One expression: `len(path)`, `range(n)`, `path|fallback`, `!path`, `path`.
 *  The function forms wrap the whole expression and are tried first, so a
 *  fallback inside their parentheses stays theirs. */
export function evalExpr(expr: string, ctx: ExprContext): unknown {
  const text = expr.trim()
  const fn = FUNCTION.exec(text)
  if (fn) {
    const inner = evalExpr(fn[2], ctx)
    if (fn[1] === `len`) {
      if (isDynamic(inner)) return call(`len`, { value: inner })
      return Array.isArray(inner) ? inner.length : typeof inner === `string` ? inner.length : 0
    }
    // range: the indexes 0..n-1 (at most MAX_RANGE_ITEMS; NaN = 0); a bound
    // count cannot be expanded (→ no items).
    if (isDynamic(inner)) return undefined
    const n = Math.min(MAX_RANGE_ITEMS, Math.max(0, Math.floor(toNumber(inner))) || 0)
    return Array.from({ length: n }, (_, i) => i)
  }
  const bar = text.indexOf(`|`)
  if (bar >= 0) {
    const value = evalExpr(text.slice(0, bar), ctx)
    const fallback = parseLiteral(text.slice(bar + 1))
    if (value === undefined || value === null) return fallback
    if (isDynamic(value)) return call(`fallback`, { value, default: fallback })
    return value
  }
  if (text.startsWith(`!`)) {
    const value = evalExpr(text.slice(1), ctx)
    if (isDynamic(value)) return call(`not`, { value })
    return !truthy(value)
  }
  return lookup(text, ctx)
}

/** The member rule: a dynamic value passes through untouched; the
 *  `fallback` a `|fallback` wrapped it in is unwrapped again. */
function passThrough(value: unknown): unknown {
  if (isCall(value) && value.call === `fallback` && isDynamic(value.args?.value)) return value.args?.value
  return value
}

/** An operand of a `$`-object (or its result): a whole string evaluates as
 *  its expression, keeping a `|fallback` as `fallback{…}` when dynamic. */
function evalOperand(value: unknown, ctx: ExprContext): unknown {
  if (typeof value === `string`) {
    const whole = WHOLE.exec(value)
    if (whole) return evalExpr(whole[1], ctx)
  }
  return evalValue(value, ctx)
}

/** A condition: a string expression or a value object ($eq …). `true` or
 *  `false` when it can be decided now; the dynamic value when it cannot. */
export function evalConditionValue(cond: unknown, ctx: ExprContext): boolean | Dynamic {
  const value = typeof cond === `string` ? evalExpr(cond, ctx) : evalOperand(cond, ctx)
  if (isDynamic(value)) return value
  return truthy(value)
}

/** A condition decided NOW: a dynamic condition counts as true (the part
 *  exists; `visible` carries the bound test, see macros.ts). */
export function evalCondition(cond: unknown, ctx: ExprContext): boolean {
  const value = evalConditionValue(cond, ctx)
  return value === true || isDynamic(value)
}

function interpolate(value: string, ctx: ExprContext): unknown {
  const parts: unknown[] = []
  let dynamic = false
  let last = 0
  for (const match of value.matchAll(EMBEDDED)) {
    const at = match.index ?? 0
    if (at > last) parts.push(value.slice(last, at))
    const v = evalExpr(match[1], ctx)
    if (isDynamic(v)) dynamic = true
    parts.push(v)
    last = at + match[0].length
  }
  if (last < value.length) parts.push(value.slice(last))
  if (dynamic) return call(`concat`, { values: parts.filter((p) => p !== undefined && p !== null && p !== ``) })
  return parts.map((p) => (p === undefined || p === null ? `` : String(p))).join(``)
}

/** Any template value: strings interpolate, arrays and objects recurse, the
 *  `$`-objects compute (or emit a call when an input is dynamic). `undefined`
 *  results are dropped by the caller. */
export function evalValue(value: unknown, ctx: ExprContext): unknown {
  if (typeof value === `string`) {
    const whole = WHOLE.exec(value)
    if (whole) return passThrough(evalExpr(whole[1], ctx))
    return interpolate(value, ctx)
  }
  if (Array.isArray(value)) {
    return value.map((item) => evalValue(item, ctx)).filter((item) => item !== undefined)
  }
  if (typeof value !== `object` || value === null) return value
  const obj = value as Record<string, unknown>
  if (`$map` in obj) {
    const spec = obj.$map as { from: string; cases: Record<string, unknown>; default?: unknown }
    const from = evalExpr(spec.from, ctx)
    if (isDynamic(from)) {
      const cases: Record<string, unknown> = {}
      for (const [k, v] of Object.entries(spec.cases)) {
        const evaluated = evalOperand(v, ctx)
        if (evaluated !== undefined) cases[k] = evaluated
      }
      return call(`map`, { value: from, cases, default: evalOperand(spec.default, ctx) })
    }
    const key = from === undefined || from === null ? `` : String(from)
    const hit = Object.prototype.hasOwnProperty.call(spec.cases, key) ? spec.cases[key] : spec.default
    return evalOperand(hit, ctx)
  }
  if (`$cond` in obj) {
    const [cond, then, otherwise] = obj.$cond as [unknown, unknown, unknown]
    const decided = evalConditionValue(cond, ctx)
    if (isDynamic(decided)) return call(`cond`, { if: decided, then: evalOperand(then, ctx), else: evalOperand(otherwise, ctx) })
    return evalOperand(decided ? then : otherwise, ctx)
  }
  if (`$eq` in obj) {
    const [a, b] = (obj.$eq as [unknown, unknown]).map((x) => evalOperand(x, ctx))
    if (isDynamic(a) || isDynamic(b)) return call(`eq`, { a, b })
    return valuesEqual(a, b)
  }
  if (`$lt` in obj) {
    const [a, b] = (obj.$lt as [unknown, unknown]).map((x) => evalOperand(x, ctx))
    if (isDynamic(a) || isDynamic(b)) return call(`lt`, { a, b })
    return toNumber(a) < toNumber(b)
  }
  if (`$add` in obj) {
    const [a, b] = (obj.$add as [unknown, unknown]).map((x) => evalOperand(x, ctx))
    if (isDynamic(a) || isDynamic(b)) return call(`add`, { a, b })
    return toNumber(a) + toNumber(b)
  }
  if (`$sub` in obj) {
    const [a, b] = (obj.$sub as [unknown, unknown]).map((x) => evalOperand(x, ctx))
    if (isDynamic(a) || isDynamic(b)) return call(`sub`, { a, b })
    return toNumber(a) - toNumber(b)
  }
  if (`$clamp` in obj) {
    const [v, min, max] = (obj.$clamp as [unknown, unknown, unknown]).map((x) => evalOperand(x, ctx))
    if (isDynamic(v) || isDynamic(min) || isDynamic(max)) return call(`clamp`, { value: v, min, max })
    return clampNumber(v, min, max)
  }
  if (`$fill` in obj) {
    // Always emitted: the template is (usually) a `$string.<id>` only the
    // surface's string table can resolve, at bind time.
    const [template, params] = obj.$fill as [unknown, Record<string, unknown> | undefined]
    const filled: Record<string, unknown> = {}
    for (const [k, v] of Object.entries(params ?? {})) {
      const evaluated = evalOperand(v, ctx)
      if (evaluated !== undefined) filled[k] = evaluated
    }
    return call(`fill`, { template: evalOperand(template, ctx), params: filled })
  }
  if (`$percent` in obj) {
    const [v, max] = (obj.$percent as [unknown, unknown]).map((x) => evalOperand(x, ctx))
    if (isDynamic(v) || isDynamic(max)) return call(`percent`, { value: v, max })
    return percent(v, max)
  }
  if (`$text` in obj) {
    const spec = obj.$text
    const v = typeof spec === `string` ? evalExpr(spec, ctx) : evalOperand(spec, ctx)
    if (isDynamic(v)) return call(`text`, { value: v })
    return v === undefined || v === null ? undefined : String(v)
  }
  if (`$coalesce` in obj) {
    const out: unknown[] = []
    for (const candidate of obj.$coalesce as unknown[]) {
      const v = evalOperand(candidate, ctx)
      if (v === undefined) continue
      if (isDynamic(v)) {
        out.push(v)
        continue
      }
      if (!truthy(v)) continue
      if (out.length === 0) return v
      out.push(v)
      break
    }
    if (out.length === 0) return undefined
    if (out.length === 1) return out[0]
    return call(`coalesce`, { values: out })
  }
  if (`$not` in obj) {
    const decided = evalConditionValue(obj.$not, ctx)
    if (isDynamic(decided)) return call(`not`, { value: decided })
    return !decided
  }
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(obj)) {
    const evaluated = evalValue(v, ctx)
    if (evaluated === undefined) continue
    const key = k.includes(`{`) ? evalValue(k, ctx) : k
    out[typeof key === `string` ? key : k] = evaluated
  }
  return out
}

/** `$percent`'s arithmetic, shared with the core `percent` function: the
 *  share of `max` as "N%", clamped to 0..100, two decimals. */
export function percent(value: unknown, max: unknown): string {
  const m = toNumber(max)
  const pct = m > 0 ? (toNumber(value) / m) * 100 : 0
  const clamped = Math.min(100, Math.max(0, pct))
  return `${Math.round(clamped * 100) / 100}%`
}

/** Round 4 (VAPP-103) `filter`: the items of an array that match, in order,
 *  so a List or Table binds to a filtered view without an agent round trip.
 *  `where` = field → value, every entry must hold (`eq` equality); an entry
 *  whose value is missing, null or "" does NOT constrain (a bound "all"
 *  choice). `query` = a case-insensitive substring of ANY of `fields` (the
 *  item's own string and number values when `fields` is absent; a string or
 *  number item matches itself); a missing, null or blank query does not
 *  constrain. Not an array → []. */
export function filterItems(items: unknown, query: unknown, fields: unknown, where: unknown): unknown[] {
  if (!Array.isArray(items)) return []
  const needle = typeof query === `string` ? query.trim().toLowerCase() : typeof query === `number` ? String(query) : ``
  const keys = Array.isArray(fields) ? fields.filter((f): f is string => typeof f === `string`) : null
  const conditions = typeof where === `object` && where !== null && !Array.isArray(where) ? Object.entries(where as Record<string, unknown>).filter(([, v]) => v !== undefined && v !== null && v !== ``) : []
  const searchable = (v: unknown) => (typeof v === `string` ? v : typeof v === `number` ? String(v) : null)
  return items.filter((item) => {
    const record = typeof item === `object` && item !== null && !Array.isArray(item) ? (item as Record<string, unknown>) : null
    for (const [field, want] of conditions) if (!valuesEqual(record?.[field], want)) return false
    if (needle === ``) return true
    const values = record ? (keys ?? Object.keys(record)).map((k) => record[k]) : [item]
    return values.some((v) => searchable(v)?.toLowerCase().includes(needle) ?? false)
  })
}

/** The core value functions (core.catalog.json `functions.core`; `set` is
 *  an action, src/dynamic.ts runAction) as the renderer
 *  evaluates them once the arguments are resolved: the reference every
 *  client function table mirrors. `undefined` = no value. */
export const CORE_FUNCTIONS: Record<string, (args: Record<string, unknown>) => unknown> = {
  percent: ({ value, max }) => percent(value, max),
  add: ({ a, b }) => toNumber(a) + toNumber(b),
  sub: ({ a, b }) => toNumber(a) - toNumber(b),
  eq: ({ a, b }) => valuesEqual(a, b),
  lt: ({ a, b }) => toNumber(a) < toNumber(b),
  clamp: ({ value, min, max }) => clampNumber(value, min, max),
  cond: ({ if: test, then, else: otherwise }) => (truthy(test) ? then : otherwise),
  fallback: ({ value, default: fallback }) => (value === undefined || value === null ? fallback : value),
  concat: ({ values }) => (Array.isArray(values) ? values.map((v) => (v === undefined || v === null ? `` : String(v))).join(``) : ``),
  coalesce: ({ values }) => (Array.isArray(values) ? values.find(truthy) : undefined),
  text: ({ value }) => (value === undefined || value === null ? undefined : String(value)),
  map: ({ value, cases, default: fallback }) => {
    const key = value === undefined || value === null ? `` : String(value)
    const table = (cases ?? {}) as Record<string, unknown>
    return Object.prototype.hasOwnProperty.call(table, key) ? table[key] : fallback
  },
  len: ({ value }) => (Array.isArray(value) ? value.length : typeof value === `string` ? value.length : 0),
  fill: ({ template, params }) => fillTemplate(template, params),
  filter: ({ items, query, fields, where }) => filterItems(items, query, fields, where),
  // Round 2 (docs/round-2-contract.md §3): the two core format functions, here
  // through the English fallback; a renderer runs them (and the basic
  // format* ones) through the surface's Formatter (src/dynamic.ts).
  formatPercent: ENGLISH_FORMAT_FUNCTIONS.formatPercent,
  formatRelativeTime: ENGLISH_FORMAT_FUNCTIONS.formatRelativeTime,
}
export const CORE_FUNCTION_NAMES: readonly string[] = Object.keys(CORE_FUNCTIONS)
