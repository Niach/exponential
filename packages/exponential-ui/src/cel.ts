// VAPP-191 (eApps Q2): expressions. The compact string form is a SUBSET of
// CEL (the Common Expression Language), hand-written here and in its Rust
// twin (`eui_core::cel`) so both replay ONE fixture (`fixtures/expressions.json`)
// exactly: numbers, dates, error texts. Apps are data, never code: an
// expression has no loops, no user functions, a length cap, no nested
// comprehensions and a step limit, so it always terminates.
//
// The subset: number, string ('…' or "…"), true, false, null and list
// literals; names; field paths `a.b` and indexes `a[0]`, `m["k"]`;
// `+ - * / %`, unary `-` and `!`; `== != < <= > >=`; `&& ||`; `c ? a : b`;
// function calls (`FUNCTIONS` below); the macros `list.map(x, e)`,
// `.filter(x, p)`, `.all(x, p)`, `.exists(x, p)`, single level. ONE number
// type (a double, like JSON: `7 / 2` = 3.5), where CEL splits int, uint and
// double. A missing field reads null; arithmetic on null is an error.
//
// The functions are ONE table shared with the A2UI function form
// (`{"call": "mul", "args": {"a": 2, "b": 3}}`, src/dynamic.ts): both forms
// run the same code, so they agree. Dates are calendar days (`YYYY-MM-DD`),
// timestamps ISO instants; everything date-like happens in the TEAM zone
// (the surface zone the host passes, an offset per instant: `env.offset`).

import { EXPRESSION_CALL_NAMES } from "./cel-names"
import { dayFromNumber, dayNumber, daysInMonth, formatDay, isoWeek, parseDay, weekdayOf } from "./dates"

/** The cost caps (spec §4.2): characters (code points), nesting, steps. */
export const EXPRESSION_LIMITS = { maxLength: 1000, maxDepth: 32, maxSteps: 10_000 } as const

/** VAPP-194 (E1): what calling a function does. Expressions are PURE: a
 *  binding, `visible`, a computed field or a query param never writes. A
 *  `write` changes records or the surface data, a `host` function reaches
 *  outside the surface (a toast, an MCP call, a link, another action); both
 *  run only as steps of a package action (host/actions.ts). */
export type FunctionEffect = `pure` | `write` | `host`

/** The functions that write (records, the surface data model). */
export const WRITE_FUNCTIONS: readonly string[] = [`set`, `records.upsert`, `records.delete`, `records.batch`]
/** The built-in host functions; every name under a host prefix is one too. */
export const HOST_FUNCTIONS: readonly string[] = [`openUrl`, `bind`]
export const HOST_FUNCTION_PREFIXES: readonly string[] = [`actions.`, `harness.`, `exponential.`, `tools.`, `upstream.`]

/** The effect of a function by name: the signature table's (every
 *  expression function, every bind-time function) is `pure`. */
export function functionEffect(name: string): FunctionEffect {
  if (WRITE_FUNCTIONS.includes(name)) return `write`
  if (HOST_FUNCTIONS.includes(name) || HOST_FUNCTION_PREFIXES.some((p) => name.startsWith(p))) return `host`
  return `pure`
}

/** Why an effectful function is refused in a pure place. */
export function effectWords(name: string, effect: FunctionEffect): string {
  return `${name} ${effect === `write` ? `writes` : `is a host function`}`
}

/** A type in the checker, spelled as JSON: `"number" | "string" | "bool" |
 *  "null" | "date" | "timestamp" | "map" | "dyn"`, `[T]` = a list of T, an
 *  object = a map with exactly those fields (a record row). */
export type ExprType = string | ExprType[] | { [field: string]: ExprType }

export interface ExpressionEnv {
  /** The clock (epoch ms) `today()` and `now()` read; absent = the wall clock. */
  now?: number
  /** The team zone: minutes east of UTC in force at an instant; absent = UTC. */
  offset?: (utcMs: number) => number
  /** The step limit (default `EXPRESSION_LIMITS.maxSteps`). */
  maxSteps?: number
}

export type ExpressionResult = { value: unknown } | { error: string }

class ExprError extends Error {}
const fail = (message: string): never => {
  throw new ExprError(message)
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

const isMap = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)

/** The runtime type name errors print. */
export function typeName(v: unknown): string {
  if (v === undefined || v === null) return `null`
  if (typeof v === `number`) return `number`
  if (typeof v === `string`) return `string`
  if (typeof v === `boolean`) return `bool`
  return Array.isArray(v) ? `list` : `map`
}

/** A finite result with `-0` as 0; anything else is out of range. */
function num(v: number): number {
  if (!Number.isFinite(v)) fail(`number out of range`)
  return v === 0 ? 0 : v
}

/** JS `String(number)` for the integers and decimals error texts print. */
const numText = (n: number) => String(n)

function deepEqual(a: unknown, b: unknown): boolean {
  a = a ?? null
  b = b ?? null
  if (a === b) return true
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((x, i) => deepEqual(x, b[i]))
  if (isMap(a) && isMap(b)) {
    const ka = Object.keys(a)
    const kb = Object.keys(b)
    return ka.length === kb.length && ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && deepEqual(a[k], b[k]))
  }
  return false
}

/** Round half away from zero (CEL `math.round`), on a non-negative value
 *  without `x + 0.5` (0.49999999999999994 stays 0). */
function roundHalfAway(x: number): number {
  const a = Math.abs(x)
  const f = Math.floor(a)
  const r = a - f >= 0.5 ? f + 1 : f
  return x < 0 ? -r : r
}

// ---------------------------------------------------------------------------
// Dates (the team zone)
// ---------------------------------------------------------------------------

const DAY_MS = 86_400_000
const UNIT_MS: Record<string, number> = { second: 1000, minute: 60_000, hour: 3_600_000 }
/** Every unit `dateAdd` / `dateDiff` take; `startOf` / `endOf` take the calendar ones. */
export const DATE_UNITS: readonly string[] = [`second`, `minute`, `hour`, `day`, `week`, `month`, `quarter`, `year`]
export const CALENDAR_UNITS: readonly string[] = [`day`, `week`, `month`, `quarter`, `year`]
/** The formats `formatDuration` writes. */
export const DURATION_FORMATS: readonly string[] = [`h:mm`, `hh:mm`]

type Temporal = { date: true; day: number } | { date: false; ms: number }

const TIMESTAMP = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d+))?)?(Z|[+-]\d{2}:\d{2})?$/

/** An ISO instant (`YYYY-MM-DDTHH:MM[:SS[.fff]][Z|±HH:MM]`, no offset =
 *  UTC) as epoch ms; null when it is not one. */
export function parseTimestamp(text: string): number | null {
  const m = TIMESTAMP.exec(text)
  if (!m) return null
  const day = parseDay(text.slice(0, 10))
  if (!day) return null
  const h = Number(m[4])
  const mi = Number(m[5])
  const s = m[6] === undefined ? 0 : Number(m[6])
  if (h > 23 || mi > 59 || s > 59) return null
  const ms = m[7] === undefined ? 0 : Number(m[7].slice(0, 3).padEnd(3, `0`))
  let offset = 0
  if (m[8] !== undefined && m[8] !== `Z`) {
    const oh = Number(m[8].slice(1, 3))
    const om = Number(m[8].slice(4, 6))
    if (oh > 23 || om > 59) return null
    offset = (m[8][0] === `-` ? -1 : 1) * (oh * 60 + om)
  }
  return dayNumber(day.y, day.m, day.d) * DAY_MS + h * 3_600_000 + mi * 60_000 + s * 1000 + ms - offset * 60_000
}

const pad = (n: number, w: number) => String(n).padStart(w, `0`)

/** The days a date may be: 0000-01-01 .. 9999-12-31 (checked BEFORE the
 *  calendar maths, so a huge or non-finite day never reaches it). */
const MIN_DAY = -719_528
const MAX_DAY = 2_932_896
/** The most calendar steps (days) any `dateAdd` can take and stay in range. */
const MAX_CALENDAR_STEPS = 4_000_000

/** An instant as `YYYY-MM-DDTHH:MM:SS.sssZ` (JS `toISOString`), years 0..9999. */
export function formatTimestamp(ms: number): string {
  if (!(ms >= MIN_DAY * DAY_MS && ms < (MAX_DAY + 1) * DAY_MS)) fail(`date out of range`)
  const day = Math.floor(ms / DAY_MS)
  const tod = ms - day * DAY_MS
  const d = dayFromNumber(day)
  if (d.y < 0 || d.y > 9999) fail(`date out of range`)
  return `${formatDay(d)}T${pad(Math.floor(tod / 3_600_000), 2)}:${pad(Math.floor(tod / 60_000) % 60, 2)}:${pad(Math.floor(tod / 1000) % 60, 2)}.${pad(tod % 1000, 3)}Z`
}

function dayText(day: number): string {
  if (!(day >= MIN_DAY && day <= MAX_DAY)) fail(`date out of range`)
  const d = dayFromNumber(day)
  if (d.y < 0 || d.y > 9999) fail(`date out of range`)
  return formatDay(d)
}

interface Zone {
  offset: (utcMs: number) => number
}

/** The wall clock (as if UTC) of an instant in the zone. */
const toLocal = (zone: Zone, ms: number) => ms + zone.offset(ms) * 60_000

/** The instant of a wall-clock time in the zone: a time in a DST gap moves
 *  forward by the gap; a repeated time takes its later (standard) instant. */
function fromLocal(zone: Zone, wall: number): number {
  const first = zone.offset(wall)
  const utc = wall - first * 60_000
  const second = zone.offset(utc)
  return second === first ? utc : wall - second * 60_000
}

function temporal(fn: string, v: unknown): Temporal {
  if (typeof v !== `string`) return fail(`${fn} needs a date, got ${typeName(v)}`)
  if (v.length === 10) {
    const d = parseDay(v)
    if (d) return { date: true, day: dayNumber(d.y, d.m, d.d) }
  }
  const ms = parseTimestamp(v)
  if (ms === null) fail(`not a date: ${v}`)
  return { date: false, ms: ms! }
}

/** A temporal's wall clock: a date = its midnight. */
const wallOf = (zone: Zone, t: Temporal) => (t.date ? t.day * DAY_MS : toLocal(zone, t.ms))

function unitOf(fn: string, v: unknown, units: readonly string[]): string {
  if (typeof v !== `string`) return fail(`${fn} needs a string, got ${typeName(v)}`)
  if (!units.includes(v)) fail(`unknown unit ${v}`)
  return v
}

function addMonths(day: number, months: number): number {
  const c = dayFromNumber(day)
  const total = c.y * 12 + (c.m - 1) + months
  const y = Math.floor(total / 12)
  const m = total - y * 12 + 1
  return dayNumber(y, m, Math.min(c.d, daysInMonth(y, m)))
}

function addCalendar(day: number, n: number, unit: string): number {
  if (!(Math.abs(n) <= MAX_CALENDAR_STEPS)) fail(`date out of range`)
  if (unit === `day`) return day + n
  if (unit === `week`) return day + 7 * n
  return addMonths(day, n * (unit === `month` ? 1 : unit === `quarter` ? 3 : 12))
}

function startDay(day: number, unit: string): number {
  const c = dayFromNumber(day)
  if (unit === `day`) return day
  if (unit === `week`) return day - ((weekdayOf(c.y, c.m, c.d) || 7) - 1)
  if (unit === `month`) return dayNumber(c.y, c.m, 1)
  if (unit === `quarter`) return dayNumber(c.y, Math.floor((c.m - 1) / 3) * 3 + 1, 1)
  return dayNumber(c.y, 1, 1)
}

function nextStartDay(day: number, unit: string): number {
  return addCalendar(startDay(day, unit), 1, unit)
}

function wholeNumber(fn: string, v: unknown): number {
  const n = number(fn, v)
  if (!Number.isInteger(n)) fail(`${fn} needs a whole number, got ${numText(n)}`)
  return n
}

function number(fn: string, v: unknown): number {
  if (typeof v !== `number`) return fail(`${fn} needs a number, got ${typeName(v)}`)
  return v
}

function text(fn: string, v: unknown): string {
  if (typeof v !== `string`) return fail(`${fn} needs a string, got ${typeName(v)}`)
  return v
}

// ---------------------------------------------------------------------------
// The function table (CEL calls and the A2UI function form)
// ---------------------------------------------------------------------------

type Param = `number` | `string` | `temporal` | `sized` | `any`

interface FunctionDef {
  /** The CEL parameters in order; `variadic` repeats the last one. */
  params: readonly Param[]
  /** The fewest arguments (default = params.length). */
  min?: number
  variadic?: boolean
  /** The A2UI argument names, in CEL order (`values` = the variadic list). */
  args: readonly string[]
  run: (args: unknown[], env: Required<Pick<ExpressionEnv, `now`>> & Zone) => unknown
  /** The result type, from the argument types. */
  returns: (types: ExprType[]) => ExprType
  /** Every expression function is pure (`functionEffect`). */
  effect?: `pure`
}

const always =
  (t: ExprType) =>
  (): ExprType =>
    t
const sameTemporal = (types: ExprType[]): ExprType => (types[0] === `date` || types[0] === `timestamp` ? types[0] : `dyn`)

function dateAdd(args: unknown[], env: Zone): string {
  const t = temporal(`dateAdd`, args[0])
  const n = wholeNumber(`dateAdd`, args[1])
  const unit = unitOf(`dateAdd`, args[2], DATE_UNITS)
  if (t.date) {
    if (UNIT_MS[unit] !== undefined) fail(`unit ${unit} needs a timestamp`)
    return dayText(addCalendar(t.day, n, unit))
  }
  if (UNIT_MS[unit] !== undefined) return formatTimestamp(num(t.ms + n * UNIT_MS[unit]!))
  const wall = toLocal(env, t.ms)
  const day = Math.floor(wall / DAY_MS)
  return formatTimestamp(fromLocal(env, addCalendar(day, n, unit) * DAY_MS + (wall - day * DAY_MS)))
}

function dateDiff(args: unknown[], env: Zone): number {
  const a = temporal(`dateDiff`, args[0])
  const b = temporal(`dateDiff`, args[1])
  const unit = unitOf(`dateDiff`, args[2], DATE_UNITS)
  if (UNIT_MS[unit] !== undefined) {
    const instant = (t: Temporal) => (t.date ? fromLocal(env, t.day * DAY_MS) : t.ms)
    return num(Math.trunc((instant(b) - instant(a)) / UNIT_MS[unit]!))
  }
  const wa = wallOf(env, a)
  const wb = wallOf(env, b)
  if (unit === `day` || unit === `week`) return num(Math.trunc((wb - wa) / (unit === `day` ? DAY_MS : 7 * DAY_MS)))
  const da = Math.floor(wa / DAY_MS)
  const db = Math.floor(wb / DAY_MS)
  const ca = dayFromNumber(da)
  const cb = dayFromNumber(db)
  let months = (cb.y - ca.y) * 12 + (cb.m - ca.m)
  // Within the month: the day, then the time of day.
  const restA = ca.d * DAY_MS + (wa - da * DAY_MS)
  const restB = cb.d * DAY_MS + (wb - db * DAY_MS)
  if (months > 0 && restB < restA) months--
  else if (months < 0 && restB > restA) months++
  return num(Math.trunc(months / (unit === `month` ? 1 : unit === `quarter` ? 3 : 12)))
}

function startEnd(fn: `startOf` | `endOf`, args: unknown[], env: Zone): string {
  const t = temporal(fn, args[0])
  const unit = unitOf(fn, args[1], CALENDAR_UNITS)
  const day = t.date ? t.day : Math.floor(toLocal(env, t.ms) / DAY_MS)
  if (fn === `startOf`) return t.date ? dayText(startDay(day, unit)) : formatTimestamp(fromLocal(env, startDay(day, unit) * DAY_MS))
  const next = nextStartDay(day, unit)
  return t.date ? dayText(next - 1) : formatTimestamp(fromLocal(env, next * DAY_MS) - 1)
}

function localDay(fn: string, v: unknown, env: Zone) {
  const t = temporal(fn, v)
  return dayFromNumber(t.date ? t.day : Math.floor(toLocal(env, t.ms) / DAY_MS))
}

function formatDuration(args: unknown[]): string {
  const h = number(`formatDuration`, args[0])
  const format = text(`formatDuration`, args[1])
  if (!DURATION_FORMATS.includes(format)) fail(`unknown duration format ${format}`)
  const total = roundHalfAway(Math.abs(h) * 60)
  if (total > 1e15) fail(`number out of range`)
  const hours = Math.floor(total / 60)
  const minutes = total - hours * 60
  return `${h < 0 && total > 0 ? `-` : ``}${format === `hh:mm` ? pad(hours, 2) : String(hours)}:${pad(minutes, 2)}`
}

function round(args: unknown[]): number {
  const x = number(`round`, args[0])
  const digits = args.length > 1 ? args[1] : 0
  if (typeof digits !== `number` || !Number.isInteger(digits) || digits < 0 || digits > 10) fail(`round needs 0 to 10 digits`)
  let p = 1
  for (let i = 0; i < (digits as number); i++) p *= 10
  return num(roundHalfAway(x * p) / p)
}

function size(v: unknown): number {
  if (typeof v === `string`) return [...v].length
  if (Array.isArray(v)) return v.length
  if (isMap(v)) return Object.keys(v).length
  return fail(`size needs a list, string or map, got ${typeName(v)}`)
}

/** THE function table (spec §4.2): CEL calls them positionally, the A2UI
 *  form by the names in `args`. Generic only (decision §18.8): no working
 *  days, no holidays. */
export const EXPRESSION_FUNCTIONS: Readonly<Record<string, FunctionDef>> = {
  mul: { params: [`number`, `number`], args: [`a`, `b`], run: ([a, b]) => num(number(`mul`, a) * number(`mul`, b)), returns: always(`number`) },
  div: {
    params: [`number`, `number`],
    args: [`a`, `b`],
    run: ([a, b]) => {
      const x = number(`div`, a)
      const y = number(`div`, b)
      if (y === 0) fail(`division by zero`)
      return num(x / y)
    },
    returns: always(`number`),
  },
  mod: {
    params: [`number`, `number`],
    args: [`a`, `b`],
    run: ([a, b]) => {
      const x = number(`mod`, a)
      const y = number(`mod`, b)
      if (y === 0) fail(`modulo by zero`)
      return num(x % y)
    },
    returns: always(`number`),
  },
  round: { params: [`number`, `number`], min: 1, args: [`value`, `digits`], run: round, returns: always(`number`) },
  floor: { params: [`number`], args: [`value`], run: ([v]) => num(Math.floor(number(`floor`, v))), returns: always(`number`) },
  ceil: { params: [`number`], args: [`value`], run: ([v]) => num(Math.ceil(number(`ceil`, v))), returns: always(`number`) },
  abs: { params: [`number`], args: [`value`], run: ([v]) => num(Math.abs(number(`abs`, v))), returns: always(`number`) },
  min: { params: [`number`], variadic: true, args: [`values`], run: (vs) => num(Math.min(...vs.map((v) => number(`min`, v)))), returns: always(`number`) },
  max: { params: [`number`], variadic: true, args: [`values`], run: (vs) => num(Math.max(...vs.map((v) => number(`max`, v)))), returns: always(`number`) },
  today: { params: [], args: [], run: (_, env) => dayText(Math.floor(toLocal(env, env.now) / DAY_MS)), returns: always(`date`) },
  now: { params: [], args: [], run: (_, env) => formatTimestamp(env.now), returns: always(`timestamp`) },
  dateAdd: { params: [`temporal`, `number`, `string`], args: [`date`, `n`, `unit`], run: dateAdd, returns: sameTemporal },
  dateDiff: { params: [`temporal`, `temporal`, `string`], args: [`a`, `b`, `unit`], run: dateDiff, returns: always(`number`) },
  startOf: { params: [`temporal`, `string`], args: [`date`, `unit`], run: (a, env) => startEnd(`startOf`, a, env), returns: sameTemporal },
  endOf: { params: [`temporal`, `string`], args: [`date`, `unit`], run: (a, env) => startEnd(`endOf`, a, env), returns: sameTemporal },
  weekday: {
    params: [`temporal`],
    args: [`date`],
    run: ([v], env) => {
      const d = localDay(`weekday`, v, env)
      return weekdayOf(d.y, d.m, d.d) || 7
    },
    returns: always(`number`),
  },
  isoWeek: {
    params: [`temporal`],
    args: [`date`],
    run: ([v], env) => {
      const d = localDay(`isoWeek`, v, env)
      return isoWeek(d.y, d.m, d.d).week
    },
    returns: always(`number`),
  },
  hours: { params: [`number`], args: [`minutes`], run: ([m]) => num(number(`hours`, m) / 60), returns: always(`number`) },
  formatDuration: { params: [`number`, `string`], args: [`hours`, `format`], run: formatDuration, returns: always(`string`) },
  size: { params: [`sized`], args: [`value`], run: ([v]) => size(v), returns: always(`number`) },
  // VAPP-195: the value, or the default when it is missing (null).
  orElse: {
    params: [`any`, `any`],
    args: [`value`, `default`],
    run: ([v, d]) => (v === null || v === undefined ? (d ?? null) : v),
    returns: (types) => {
      const a = requiredType(types[0]!)
      return a === `null` ? requiredType(types[1]!) : a
    },
  },
}

export { EXPRESSION_CALL_NAMES }

/** The A2UI form in the bind table, on the wall clock in UTC (a renderer
 *  passes its clock and zone through `ResolveOptions`, src/dynamic.ts). */
export const EXPRESSION_BIND_FUNCTIONS: Readonly<Record<string, (args: Record<string, unknown>) => unknown>> = Object.fromEntries(
  EXPRESSION_CALL_NAMES.map((name) => [name, (args: Record<string, unknown>) => callExpressionFunction(name, args)?.value]),
)

const MACROS = new Set([`map`, `filter`, `all`, `exists`])

function fullEnv(env: ExpressionEnv): Required<Pick<ExpressionEnv, `now`>> & Zone {
  return { now: Math.floor(env.now ?? Date.now()), offset: env.offset ?? (() => 0) }
}

function arityError(name: string, def: FunctionDef, n: number): string | null {
  const min = def.variadic ? 1 : (def.min ?? def.params.length)
  const max = def.variadic ? Infinity : def.params.length
  if (n >= min && n <= max) return null
  if (def.variadic) return `${name} takes at least 1 argument`
  if (min === max) return `${name} takes ${min} argument${min === 1 ? `` : `s`}`
  return `${name} takes ${min} or ${max} arguments`
}

/** The A2UI function form: `{call: name, args: {…}}` with its arguments
 *  resolved. `undefined` = not an expression function; `{value: undefined}`
 *  = no value (an error, like every bind-time function). */
export function callExpressionFunction(name: string, args: Record<string, unknown>, env: ExpressionEnv = {}): { value: unknown } | undefined {
  // VAPP-195: a failed call shows its `default` (absent = no value).
  const fallback = { value: args.default }
  if (name === `expr`) {
    if (typeof args.expr !== `string`) return fallback
    const result = evaluateExpression(args.expr, isMap(args.vars) ? args.vars : {}, env)
    return `value` in result ? { value: result.value } : fallback
  }
  if (name === `size` || !Object.prototype.hasOwnProperty.call(EXPRESSION_FUNCTIONS, name)) return undefined
  const def = EXPRESSION_FUNCTIONS[name]!
  let positional: unknown[]
  if (def.variadic) positional = Array.isArray(args.values) ? args.values : []
  else {
    positional = def.args.map((k) => args[k])
    while (positional.length > (def.min ?? def.params.length) && positional[positional.length - 1] === undefined) positional.pop()
  }
  if (arityError(name, def, positional.length)) return fallback
  try {
    return { value: def.run(positional, fullEnv(env)) }
  } catch (e) {
    if (e instanceof ExprError) return fallback
    throw e
  }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

export type ExprNode =
  | { k: `lit`; v: unknown; at: number }
  | { k: `id`; name: string; at: number }
  | { k: `list`; items: ExprNode[]; at: number }
  | { k: `field`; of: ExprNode; name: string; at: number }
  | { k: `index`; of: ExprNode; index: ExprNode; at: number }
  | { k: `unary`; op: string; arg: ExprNode; at: number }
  | { k: `bin`; op: string; a: ExprNode; b: ExprNode; at: number }
  | { k: `cond`; test: ExprNode; then: ExprNode; else: ExprNode; at: number }
  | { k: `call`; fn: string; args: ExprNode[]; at: number }
  | { k: `macro`; fn: string; of: ExprNode; v: string; body: ExprNode; at: number }
  | { k: `has`; of: Extract<ExprNode, { k: `field` }>; at: number }

interface Token {
  t: `num` | `str` | `id` | `op` | `end`
  v: string | number
  at: number
  text: string
}

const OPS2 = [`==`, `!=`, `<=`, `>=`, `&&`, `||`]
const OPS1 = `+-*/%<>!()[],.?:`
const isDigit = (c: string | undefined) => c !== undefined && c >= `0` && c <= `9`
const isIdStart = (c: string | undefined) => c !== undefined && ((c >= `a` && c <= `z`) || (c >= `A` && c <= `Z`) || c === `_`)
const isIdPart = (c: string | undefined) => isIdStart(c) || isDigit(c)

function tokenize(cs: string[]): Token[] {
  const out: Token[] = []
  let i = 0
  while (i < cs.length) {
    const c = cs[i]!
    if (c === ` ` || c === `\t` || c === `\n` || c === `\r`) {
      i++
      continue
    }
    const at = i
    if (isDigit(c)) {
      while (isDigit(cs[i])) i++
      if (cs[i] === `.` && isDigit(cs[i + 1])) {
        i++
        while (isDigit(cs[i])) i++
      }
      if ((cs[i] === `e` || cs[i] === `E`) && (isDigit(cs[i + 1]) || ((cs[i + 1] === `+` || cs[i + 1] === `-`) && isDigit(cs[i + 2])))) {
        i += 2
        while (isDigit(cs[i])) i++
      }
      const text = cs.slice(at, i).join(``)
      out.push({ t: `num`, v: Number(text), at, text })
      continue
    }
    if (c === `'` || c === `"`) {
      i++
      let s = ``
      for (;;) {
        const d = cs[i]
        if (d === undefined || d === `\n`) fail(`at ${at}: unterminated string`)
        if (d === c) break
        if (d === `\\`) {
          const e = cs[i + 1]
          const map: Record<string, string> = { "\\": `\\`, "'": `'`, '"': `"`, n: `\n`, t: `\t`, r: `\r` }
          if (e === undefined || map[e] === undefined) fail(`at ${i}: unknown escape \\${e ?? ``}`)
          s += map[e!]
          i += 2
          continue
        }
        s += d
        i++
      }
      i++
      out.push({ t: `str`, v: s, at, text: cs.slice(at, i).join(``) })
      continue
    }
    if (isIdStart(c)) {
      while (isIdPart(cs[i])) i++
      const text = cs.slice(at, i).join(``)
      out.push({ t: `id`, v: text, at, text })
      continue
    }
    const two = c + (cs[i + 1] ?? ``)
    if (OPS2.includes(two)) {
      out.push({ t: `op`, v: two, at, text: two })
      i += 2
      continue
    }
    if (OPS1.includes(c)) {
      out.push({ t: `op`, v: c, at, text: c })
      i++
      continue
    }
    fail(`at ${at}: unexpected ${c}`)
  }
  out.push({ t: `end`, v: ``, at: cs.length, text: `end` })
  return out
}

/** `a.b.c` as a name (a chain of plain names), else null. */
function dottedName(node: ExprNode): string | null {
  if (node.k === `id`) return node.name
  if (node.k !== `field`) return null
  const of = dottedName(node.of)
  return of === null ? null : `${of}.${node.name}`
}

/** VAPP-195: a value's path as written (`a.b[0].c`, `m['k']`): names,
 *  fields and literal indexes; null for anything else. */
export function pathText(node: ExprNode): string | null {
  if (node.k === `id`) return node.name
  if (node.k === `field`) {
    const of = pathText(node.of)
    return of === null ? null : `${of}.${node.name}`
  }
  if (node.k === `index`) {
    const of = pathText(node.of)
    const key = node.index.k === `lit` ? node.index.v : undefined
    if (of === null) return null
    if (typeof key === `number`) return `${of}[${numText(key)}]`
    if (typeof key === `string`) return `${of}['${key}']`
  }
  return null
}

const rootAt = (node: ExprNode): number => (node.k === `field` ? rootAt(node.of) : node.at)

class Parser {
  private i = 0
  private depth = 0
  private inBody = 0
  constructor(private readonly tokens: Token[]) {}

  private peek(): Token {
    return this.tokens[this.i]!
  }
  private isOp(v: string): boolean {
    const t = this.peek()
    return t.t === `op` && t.v === v
  }
  private next(): Token {
    return this.tokens[this.i++]!
  }
  private unexpected(): never {
    const t = this.peek()
    return fail(`at ${t.at}: unexpected ${t.text}`)
  }
  private expect(v: string): Token {
    if (!this.isOp(v)) this.unexpected()
    return this.next()
  }
  private enter(at: number) {
    if (++this.depth > EXPRESSION_LIMITS.maxDepth) fail(`at ${at}: expression is nested too deeply`)
  }

  parseAll(): ExprNode {
    const node = this.expr()
    if (this.peek().t !== `end`) this.unexpected()
    return node
  }

  expr(): ExprNode {
    const at = this.peek().at
    this.enter(at)
    const test = this.or()
    let out = test
    if (this.isOp(`?`)) {
      this.next()
      const then = this.expr()
      this.expect(`:`)
      const otherwise = this.expr()
      out = { k: `cond`, test, then, else: otherwise, at }
    }
    this.depth--
    return out
  }

  private binary(ops: string[], operand: () => ExprNode): ExprNode {
    let a = operand()
    for (;;) {
      const t = this.peek()
      if (t.t !== `op` || !ops.includes(t.v as string)) return a
      this.next()
      const b = operand()
      a = { k: `bin`, op: t.v as string, a, b, at: t.at }
    }
  }
  private or(): ExprNode {
    return this.binary([`||`], () => this.and())
  }
  private and(): ExprNode {
    return this.binary([`&&`], () => this.rel())
  }
  private rel(): ExprNode {
    return this.binary([`==`, `!=`, `<`, `<=`, `>`, `>=`], () => this.add())
  }
  private add(): ExprNode {
    return this.binary([`+`, `-`], () => this.mul())
  }
  private mul(): ExprNode {
    return this.binary([`*`, `/`, `%`], () => this.unary())
  }

  private unary(): ExprNode {
    const t = this.peek()
    if (t.t === `op` && (t.v === `!` || t.v === `-`)) {
      this.next()
      this.enter(t.at)
      const arg = this.unary()
      this.depth--
      return { k: `unary`, op: t.v as string, arg, at: t.at }
    }
    return this.postfix()
  }

  private postfix(): ExprNode {
    let node = this.primary()
    for (;;) {
      if (this.isOp(`.`)) {
        this.next()
        const name = this.peek()
        if (name.t !== `id`) this.unexpected()
        this.next()
        if (this.isOp(`(`)) {
          if (!MACROS.has(name.v as string)) {
            const dotted = dottedName(node)
            const effect = dotted === null ? `pure` : functionEffect(`${dotted}.${name.v}`)
            if (effect !== `pure`) fail(`at ${rootAt(node)}: ${effectWords(`${dotted}.${name.v}`, effect)}: an expression is pure, use an action step`)
            fail(`at ${name.at}: unknown method ${name.v}`)
          }
          if (this.inBody > 0) fail(`at ${name.at}: nested comprehensions are not allowed`)
          this.next()
          const v = this.peek()
          if (v.t !== `id` || this.tokens[this.i + 1]?.t !== `op` || this.tokens[this.i + 1]?.v !== `,`) fail(`at ${v.at}: ${name.v} needs a name first`)
          this.next()
          this.next()
          this.inBody++
          const body = this.expr()
          this.inBody--
          this.expect(`)`)
          node = { k: `macro`, fn: name.v as string, of: node, v: v.v as string, body, at: name.at }
        } else node = { k: `field`, of: node, name: name.v as string, at: name.at }
        continue
      }
      if (this.isOp(`[`)) {
        const at = this.next().at
        const index = this.expr()
        this.expect(`]`)
        node = { k: `index`, of: node, index, at }
        continue
      }
      return node
    }
  }

  private list(close: string): ExprNode[] {
    const items: ExprNode[] = []
    if (this.isOp(close)) {
      this.next()
      return items
    }
    for (;;) {
      items.push(this.expr())
      if (this.isOp(`,`)) {
        this.next()
        if (this.isOp(close) && close === `]`) {
          this.next()
          return items
        }
        continue
      }
      this.expect(close)
      return items
    }
  }

  private primary(): ExprNode {
    const t = this.peek()
    if (t.t === `num`) {
      this.next()
      if (!Number.isFinite(t.v as number)) fail(`at ${t.at}: number out of range`)
      return { k: `lit`, v: t.v, at: t.at }
    }
    if (t.t === `str`) {
      this.next()
      return { k: `lit`, v: t.v, at: t.at }
    }
    if (t.t === `id`) {
      this.next()
      const name = t.v as string
      if (name === `true` || name === `false`) return { k: `lit`, v: name === `true`, at: t.at }
      if (name === `null`) return { k: `lit`, v: null, at: t.at }
      // VAPP-195: CEL's presence test, `has(a.b)`.
      if (name === `has` && this.isOp(`(`)) {
        this.next()
        const arg = this.expr()
        this.expect(`)`)
        if (arg.k !== `field`) fail(`at ${t.at}: has needs a field: has(a.b)`)
        return { k: `has`, of: arg as Extract<ExprNode, { k: `field` }>, at: t.at }
      }
      if (this.isOp(`(`)) {
        if (!Object.prototype.hasOwnProperty.call(EXPRESSION_FUNCTIONS, name)) {
          const effect = functionEffect(name)
          if (effect !== `pure`) fail(`at ${t.at}: ${effectWords(name, effect)}: an expression is pure, use an action step`)
          fail(`at ${t.at}: unknown function ${name}`)
        }
        this.next()
        return { k: `call`, fn: name, args: this.list(`)`), at: t.at }
      }
      return { k: `id`, name, at: t.at }
    }
    if (t.t === `op` && t.v === `(`) {
      this.next()
      const inner = this.expr()
      this.expect(`)`)
      return inner
    }
    if (t.t === `op` && t.v === `[`) {
      this.next()
      return { k: `list`, items: this.list(`]`), at: t.at }
    }
    return this.unexpected()
  }
}

/** Parse once (at package validation, or before evaluating): the tree, or
 *  the first syntax / cost error (`at <code point>: <reason>`). */
export function parseExpression(source: string): { ast: ExprNode } | { error: string } {
  try {
    const cs = Array.from(source)
    if (cs.length > EXPRESSION_LIMITS.maxLength) fail(`expression is longer than ${EXPRESSION_LIMITS.maxLength} characters`)
    return { ast: new Parser(tokenize(cs)).parseAll() }
  } catch (e) {
    if (e instanceof ExprError) return { error: e.message }
    throw e
  }
}

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

interface Scope {
  name: string
  value: unknown
  parent: Scope | null
}

class Evaluator {
  steps = 0
  constructor(
    private readonly vars: Record<string, unknown>,
    private readonly env: Required<Pick<ExpressionEnv, `now`>> & Zone,
    private readonly maxSteps: number,
  ) {}

  private step() {
    if (++this.steps > this.maxSteps) fail(`step limit exceeded`)
  }

  ev(node: ExprNode, scope: Scope | null): unknown {
    this.step()
    switch (node.k) {
      case `lit`:
        return node.v
      case `list`:
        return node.items.map((item) => this.ev(item, scope))
      case `id`: {
        for (let s = scope; s; s = s.parent) if (s.name === node.name) return s.value
        if (!Object.prototype.hasOwnProperty.call(this.vars, node.name)) fail(`unknown name ${node.name}`)
        return this.vars[node.name] ?? null
      }
      case `has`: {
        const of = this.ev(node.of.of, scope)
        if (of === null || of === undefined) return false
        if (!isMap(of)) return fail(`no field ${node.of.name} on ${typeName(of)}`)
        return Object.prototype.hasOwnProperty.call(of, node.of.name) && of[node.of.name] !== null && of[node.of.name] !== undefined
      }
      case `field`: {
        const of = this.ev(node.of, scope)
        if (of === null || of === undefined) return null
        if (!isMap(of)) return fail(`no field ${node.name} on ${typeName(of)}`)
        return Object.prototype.hasOwnProperty.call(of, node.name) ? (of[node.name] ?? null) : null
      }
      case `index`: {
        const of = this.ev(node.of, scope)
        const index = this.ev(node.index, scope)
        if (Array.isArray(of)) {
          if (typeof index !== `number`) return fail(`a list index is a number, got ${typeName(index)}`)
          if (!Number.isInteger(index)) fail(`a list index is a whole number, got ${numText(index)}`)
          if (index < 0 || index >= of.length) fail(`index ${numText(index)} out of range`)
          return of[index] ?? null
        }
        if (isMap(of)) {
          if (typeof index !== `string`) return fail(`a map key is a string, got ${typeName(index)}`)
          return Object.prototype.hasOwnProperty.call(of, index) ? (of[index] ?? null) : null
        }
        if (of === null || of === undefined) return null
        return fail(`cannot index ${typeName(of)}`)
      }
      case `unary`: {
        const v = this.ev(node.arg, scope)
        if (node.op === `-`) {
          if (typeof v !== `number`) fail(`cannot apply - to ${typeName(v)}`)
          return num(-(v as number))
        }
        if (typeof v !== `boolean`) fail(`cannot apply ! to ${typeName(v)}`)
        return !v
      }
      case `cond`: {
        const test = this.ev(node.test, scope)
        if (typeof test !== `boolean`) fail(`cannot use ${typeName(test)} as a condition`)
        return this.ev(test ? node.then : node.else, scope)
      }
      case `bin`:
        return this.binary(node, scope)
      case `call`: {
        const args = node.args.map((a) => this.ev(a, scope))
        const def = EXPRESSION_FUNCTIONS[node.fn]!
        const arity = arityError(node.fn, def, args.length)
        if (arity) fail(arity)
        return def.run(args, this.env)
      }
      case `macro`: {
        const of = this.ev(node.of, scope)
        if (!Array.isArray(of)) return fail(`${node.fn} needs a list, got ${typeName(of)}`)
        const out: unknown[] = []
        for (const item of of) {
          this.step()
          const v = this.ev(node.body, { name: node.v, value: item ?? null, parent: scope })
          if (node.fn === `map`) {
            out.push(v)
            continue
          }
          if (typeof v !== `boolean`) fail(`${node.fn} needs a bool, got ${typeName(v)}`)
          if (node.fn === `filter`) {
            if (v) out.push(item ?? null)
          } else if (node.fn === `all` && !v) return false
          else if (node.fn === `exists` && v) return true
        }
        if (node.fn === `all`) return true
        if (node.fn === `exists`) return false
        return out
      }
    }
  }

  private binary(node: Extract<ExprNode, { k: `bin` }>, scope: Scope | null): unknown {
    if (node.op === `&&` || node.op === `||`) {
      const a = this.ev(node.a, scope)
      if (typeof a !== `boolean`) fail(`cannot apply ${node.op} to ${typeName(a)}`)
      if (node.op === `&&` ? !a : a) return a
      const b = this.ev(node.b, scope)
      if (typeof b !== `boolean`) fail(`cannot apply ${node.op} to ${typeName(b)}`)
      return b
    }
    const a = this.ev(node.a, scope) ?? null
    const b = this.ev(node.b, scope) ?? null
    switch (node.op) {
      case `==`:
        return deepEqual(a, b)
      case `!=`:
        return !deepEqual(a, b)
      case `<`:
      case `<=`:
      case `>`:
      case `>=`: {
        if (!((typeof a === `number` && typeof b === `number`) || (typeof a === `string` && typeof b === `string`))) fail(`cannot compare ${typeName(a)} and ${typeName(b)}`)
        const c = (a as number | string) < (b as number | string) ? -1 : (a as number | string) > (b as number | string) ? 1 : 0
        return node.op === `<` ? c < 0 : node.op === `<=` ? c <= 0 : node.op === `>` ? c > 0 : c >= 0
      }
      case `+`:
        if (typeof a === `number` && typeof b === `number`) return num(a + b)
        if (typeof a === `string` && typeof b === `string`) return a + b
        if (Array.isArray(a) && Array.isArray(b)) return [...a, ...b]
        return fail(`cannot apply + to ${typeName(a)} and ${typeName(b)}`)
      default: {
        if (typeof a !== `number` || typeof b !== `number`) return fail(`cannot apply ${node.op} to ${typeName(a)} and ${typeName(b)}`)
        if (node.op === `-`) return num(a - b)
        if (node.op === `*`) return num(a * b)
        if (b === 0) fail(node.op === `/` ? `division by zero` : `modulo by zero`)
        return num(node.op === `/` ? a / b : a % b)
      }
    }
  }
}

/** VAPP-194: evaluate a parsed tree and count its steps (an action spends
 *  one budget over all of its expressions). */
export function evaluateCounted(ast: ExprNode, vars: Record<string, unknown>, env: ExpressionEnv = {}): { result: ExpressionResult; steps: number } {
  const ev = new Evaluator(vars, fullEnv(env), env.maxSteps ?? EXPRESSION_LIMITS.maxSteps)
  try {
    return { result: { value: ev.ev(ast, null) ?? null }, steps: ev.steps }
  } catch (e) {
    if (e instanceof ExprError) return { result: { error: e.message }, steps: ev.steps }
    throw e
  }
}

/** Evaluate an expression (a source text or a parsed tree) over `vars`:
 *  `{value}` or `{error}` (a syntax, type, range or cost error). */
export function evaluateExpression(source: string | ExprNode, vars: Record<string, unknown> = {}, env: ExpressionEnv = {}): ExpressionResult {
  let ast: ExprNode
  if (typeof source === `string`) {
    const parsed = parseExpression(source)
    if (`error` in parsed) return parsed
    ast = parsed.ast
  } else ast = source
  try {
    return { value: new Evaluator(vars, fullEnv(env), env.maxSteps ?? EXPRESSION_LIMITS.maxSteps).ev(ast, null) ?? null }
  } catch (e) {
    if (e instanceof ExprError) return { error: e.message }
    throw e
  }
}

// ---------------------------------------------------------------------------
// Type checking (package validation)
// ---------------------------------------------------------------------------

/** VAPP-195 (E2): a value that may be missing is `T?`, spelled `{"?": T}`
 *  (a row never has a field named `?`). */
export const isOptional = (t: ExprType): t is { "?": ExprType } => typeof t === `object` && !Array.isArray(t) && Object.keys(t).length === 1 && Object.prototype.hasOwnProperty.call(t, `?`)
/** `T?` (null and an optional stay as they are). */
export const optionalType = (t: ExprType): ExprType => (isOptional(t) || t === `null` ? t : { "?": t })
/** `T` of a `T?`. */
export const requiredType = (t: ExprType): ExprType => (isOptional(t) ? t[`?`] : t)

/** The name a type prints as in an error: a list `list`, a row `map`, an
 *  optional `T?`. */
export function typeDisplay(t: ExprType): string {
  if (isOptional(t)) return `${typeDisplay(t[`?`])}?`
  return typeof t === `string` ? t : Array.isArray(t) ? `list` : `map`
}

const isRow = (t: ExprType): t is { [field: string]: ExprType } => typeof t === `object` && !Array.isArray(t) && !isOptional(t)
const isTextual = (t: ExprType) => t === `string` || t === `date` || t === `timestamp`
const loose = (t: ExprType) => t === `dyn` || t === `null`

function accepts(param: Param, t: ExprType): boolean {
  if (param === `any` || loose(t)) return true
  if (param === `number`) return t === `number`
  if (param === `string`) return isTextual(t)
  if (param === `temporal`) return isTextual(t)
  return t === `string` || t === `map` || Array.isArray(t) || isRow(t)
}

const PARAM_NAME: Record<Param, string> = { number: `a number`, string: `a string`, temporal: `a date`, sized: `a list, string or map`, any: `a value` }

interface TypeScope {
  name: string
  type: ExprType
  parent: TypeScope | null
}

function literal(node: ExprNode): unknown {
  return node.k === `lit` ? node.v : undefined
}

/** The paths a test proves present when it is true / when it is false:
 *  `has(p)`, `p != null`, `p == null`, through `&&`, `||` and `!`. */
function guards(node: ExprNode): { yes: string[]; no: string[] } {
  if (node.k === `has`) {
    const p = pathText(node.of)
    return { yes: p === null ? [] : [p], no: [] }
  }
  if (node.k === `bin` && (node.op === `==` || node.op === `!=`)) {
    const side = literal(node.a) === null && node.a.k === `lit` ? node.b : literal(node.b) === null && node.b.k === `lit` ? node.a : null
    const p = side ? pathText(side) : null
    if (p === null) return { yes: [], no: [] }
    return node.op === `!=` ? { yes: [p], no: [] } : { yes: [], no: [p] }
  }
  if (node.k === `bin` && node.op === `&&`) {
    const a = guards(node.a)
    const b = guards(node.b)
    return { yes: [...a.yes, ...b.yes], no: [] }
  }
  if (node.k === `bin` && node.op === `||`) {
    const a = guards(node.a)
    const b = guards(node.b)
    return { yes: [], no: [...a.no, ...b.no] }
  }
  if (node.k === `unary` && node.op === `!`) {
    const a = guards(node.arg)
    return { yes: a.no, no: a.yes }
  }
  return { yes: [], no: [] }
}

/** An unguarded use of a value that may be missing. */
function missing(node: ExprNode): never {
  const p = pathText(node)
  return fail(`${p ?? `the value at ${node.at}`} may be missing: guard it with has(), a null check or orElse()`)
}

class Checker {
  /** The paths a guard proved present here (`has(r.f) ? r.f + 1 : 0`). */
  private present: string[] = []
  constructor(private readonly types: Record<string, ExprType>) {}

  /** Type `node` with `paths` proved present. */
  private within(paths: string[], node: ExprNode, scope: TypeScope | null): ExprType {
    const before = this.present
    this.present = [...before, ...paths]
    try {
      return this.tc(node, scope)
    } finally {
      this.present = before
    }
  }

  /** The type of a use that must not be missing. */
  private need(node: ExprNode, scope: TypeScope | null): ExprType {
    const t = this.tc(node, scope)
    if (isOptional(t)) missing(node)
    return t
  }

  tc(node: ExprNode, scope: TypeScope | null): ExprType {
    const t = this.type(node, scope)
    const p = node.k === `id` || node.k === `field` || node.k === `index` ? pathText(node) : null
    return p !== null && this.present.includes(p) ? requiredType(t) : t
  }

  private type(node: ExprNode, scope: TypeScope | null): ExprType {
    switch (node.k) {
      case `lit`:
        return typeName(node.v)
      case `list`: {
        const items = node.items.map((i) => this.tc(i, scope))
        if (items.length === 0) return [`dyn`]
        return [items.every((t) => JSON.stringify(t) === JSON.stringify(items[0])) ? items[0]! : `dyn`]
      }
      case `id`: {
        for (let s = scope; s; s = s.parent) if (s.name === node.name) return s.type
        if (!Object.prototype.hasOwnProperty.call(this.types, node.name)) return fail(`unknown name ${node.name}`)
        return this.types[node.name]!
      }
      case `field`:
      case `has`: {
        const field = node.k === `has` ? (node.of as Extract<ExprNode, { k: `field` }>) : node
        const raw = this.tc(field.of, scope)
        const of = requiredType(raw)
        let t: ExprType
        if (of === `dyn` || of === `map` || of === `null`) t = `dyn`
        else if (isRow(of)) {
          if (!Object.prototype.hasOwnProperty.call(of, field.name)) return fail(`unknown field ${field.name}`)
          t = of[field.name]!
        } else return fail(`no field ${field.name} on ${typeDisplay(of)}`)
        if (node.k === `has`) return `bool`
        return isOptional(raw) ? optionalType(t) : t
      }
      case `index`: {
        const raw = this.tc(node.of, scope)
        const of = requiredType(raw)
        const index = this.need(node.index, scope)
        const wrap = (t: ExprType) => (isOptional(raw) ? optionalType(t) : t)
        if (Array.isArray(of)) {
          if (!loose(index) && index !== `number`) fail(`a list index is a number, got ${typeDisplay(index)}`)
          return wrap(of[0]!)
        }
        if (isRow(of) || of === `map`) {
          if (!loose(index) && !isTextual(index)) fail(`a map key is a string, got ${typeDisplay(index)}`)
          const key = literal(node.index)
          if (isRow(of) && typeof key === `string`) {
            if (!Object.prototype.hasOwnProperty.call(of, key)) fail(`unknown field ${key}`)
            return wrap(of[key]!)
          }
          return wrap(`dyn`)
        }
        if (loose(of)) return wrap(`dyn`)
        return fail(`cannot index ${typeDisplay(of)}`)
      }
      case `unary`: {
        const t = this.need(node.arg, scope)
        const want = node.op === `-` ? `number` : `bool`
        if (!loose(t) && t !== want) fail(`cannot apply ${node.op} to ${typeDisplay(t)}`)
        return want
      }
      case `cond`: {
        const test = this.need(node.test, scope)
        if (!loose(test) && test !== `bool`) fail(`cannot use ${typeDisplay(test)} as a condition`)
        const g = guards(node.test)
        const a = this.within(g.yes, node.then, scope)
        const b = this.within(g.no, node.else, scope)
        if (JSON.stringify(a) === JSON.stringify(b)) return a
        if (a === `null`) return optionalType(b)
        if (b === `null`) return optionalType(a)
        if (JSON.stringify(requiredType(a)) === JSON.stringify(requiredType(b))) return optionalType(requiredType(a))
        return isOptional(a) || isOptional(b) ? optionalType(`dyn`) : `dyn`
      }
      case `bin`:
        return this.binary(node, scope)
      case `call`:
        return this.call(node, scope)
      case `macro`: {
        const of = this.need(node.of, scope)
        if (!Array.isArray(of) && !loose(of)) fail(`${node.fn} needs a list, got ${typeDisplay(of)}`)
        const item = Array.isArray(of) ? of[0]! : `dyn`
        const body = this.tc(node.body, { name: node.v, type: item, parent: scope })
        if (node.fn === `map`) return [body]
        if (isOptional(body)) missing(node.body)
        if (!loose(body) && body !== `bool`) fail(`${node.fn} needs a bool, got ${typeDisplay(body)}`)
        return node.fn === `filter` ? (Array.isArray(of) ? of : [`dyn`]) : `bool`
      }
    }
  }

  private binary(node: Extract<ExprNode, { k: `bin` }>, scope: TypeScope | null): ExprType {
    if (node.op === `&&` || node.op === `||`) {
      const a = this.need(node.a, scope)
      const g = guards(node.a)
      const b = this.within(node.op === `&&` ? g.yes : g.no, node.b, scope)
      if (isOptional(b)) missing(node.b)
      for (const t of [a, b]) if (!loose(t) && t !== `bool`) fail(`cannot apply ${node.op} to ${typeDisplay(t)}`)
      return `bool`
    }
    if (node.op === `==` || node.op === `!=`) {
      // The null check: either side may be missing.
      const a = requiredType(this.tc(node.a, scope))
      const b = requiredType(this.tc(node.b, scope))
      const comparable = loose(a) || loose(b) || (isTextual(a) && isTextual(b)) || typeDisplay(a) === typeDisplay(b)
      if (!comparable) fail(`cannot compare ${typeDisplay(a)} and ${typeDisplay(b)}`)
      return `bool`
    }
    const a = this.need(node.a, scope)
    const b = this.need(node.b, scope)
    const da = typeDisplay(a)
    const db = typeDisplay(b)
    switch (node.op) {
      case `<`:
      case `<=`:
      case `>`:
      case `>=`: {
        const ok = (t: ExprType) => loose(t) || t === `number` || isTextual(t)
        if (!ok(a) || !ok(b) || (a === `number` && isTextual(b)) || (b === `number` && isTextual(a))) fail(`cannot compare ${da} and ${db}`)
        return `bool`
      }
      case `+`:
        if (loose(a) && loose(b)) return `dyn`
        if ((a === `number` || loose(a)) && (b === `number` || loose(b))) return `number`
        if ((isTextual(a) || loose(a)) && (isTextual(b) || loose(b))) return `string`
        if ((Array.isArray(a) || loose(a)) && (Array.isArray(b) || loose(b))) return Array.isArray(a) && Array.isArray(b) && JSON.stringify(a) === JSON.stringify(b) ? a : [`dyn`]
        return fail(`cannot apply + to ${da} and ${db}`)
      default:
        if (!((a === `number` || loose(a)) && (b === `number` || loose(b)))) fail(`cannot apply ${node.op} to ${da} and ${db}`)
        if ((node.op === `/` || node.op === `%`) && literal(node.b) === 0) fail(node.op === `/` ? `division by zero` : `modulo by zero`)
        return `number`
    }
  }

  private call(node: Extract<ExprNode, { k: `call` }>, scope: TypeScope | null): ExprType {
    const def = EXPRESSION_FUNCTIONS[node.fn]!
    const arity = arityError(node.fn, def, node.args.length)
    if (arity) fail(arity)
    // orElse takes a value that may be missing; every other argument is used.
    const types = node.args.map((a, i) => (node.fn === `orElse` && i === 0 ? this.tc(a, scope) : this.need(a, scope)))
    types.forEach((t, i) => {
      const param = def.params[Math.min(i, def.params.length - 1)]!
      if (!accepts(param, requiredType(t))) fail(`${node.fn} needs ${PARAM_NAME[param]}, got ${typeDisplay(t)}`)
    })
    if (node.fn === `orElse`) {
      const a = requiredType(types[0]!)
      const b = types[1]!
      if (!loose(a) && !loose(b) && !(isTextual(a) && isTextual(b)) && typeDisplay(a) !== typeDisplay(b)) fail(`orElse needs a ${typeDisplay(a)} default, got ${typeDisplay(b)}`)
    }
    // The literal arguments a signature fixes: units, digits, formats, a whole n.
    const lit = (i: number) => (node.args[i] ? literal(node.args[i]!) : undefined)
    const unitAt = node.fn === `dateAdd` || node.fn === `dateDiff` ? 2 : node.fn === `startOf` || node.fn === `endOf` ? 1 : -1
    const unit = unitAt >= 0 ? lit(unitAt) : undefined
    if (typeof unit === `string`) {
      const units = unitAt === 2 ? DATE_UNITS : CALENDAR_UNITS
      if (!units.includes(unit)) fail(`unknown unit ${unit}`)
      if (node.fn === `dateAdd` && types[0] === `date` && UNIT_MS[unit] !== undefined) fail(`unit ${unit} needs a timestamp`)
    }
    if (node.fn === `dateAdd` && typeof lit(1) === `number` && !Number.isInteger(lit(1))) fail(`dateAdd needs a whole number, got ${numText(lit(1) as number)}`)
    if (node.fn === `round` && node.args.length > 1) {
      const d = lit(1)
      if (typeof d === `number` && (!Number.isInteger(d) || d < 0 || d > 10)) fail(`round needs 0 to 10 digits`)
    }
    if (node.fn === `div` && lit(1) === 0) fail(`division by zero`)
    if (node.fn === `mod` && lit(1) === 0) fail(`modulo by zero`)
    const format = node.fn === `formatDuration` ? lit(1) : undefined
    if (typeof format === `string` && !DURATION_FORMATS.includes(format)) fail(`unknown duration format ${format}`)
    return def.returns(types)
  }
}

/** Type-check an expression against the names it may read (a kind's
 *  fields, a binding's value): `{type}` or `{error}` (the parse error, or
 *  the first type error). */
export function checkExpression(source: string, types: Record<string, ExprType>): { type: ExprType } | { error: string } {
  const parsed = parseExpression(source)
  if (`error` in parsed) return parsed
  try {
    return { type: new Checker(types).tc(parsed.ast, null) }
  } catch (e) {
    if (e instanceof ExprError) return { error: e.message }
    throw e
  }
}
