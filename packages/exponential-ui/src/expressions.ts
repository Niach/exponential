// VAPP-191 (eApps Q2, spec docs/spec/apps.md §4.2/§4.3): the EXPRESSION
// language = a subset of CEL (Common Expression Language) with one number
// type (every JSON number is a CEL double, so `hours * 60` needs no
// conversion), JSON values only, maths, date/time and duration functions,
// a type checker over declared variable types (a package's record kinds)
// and cost caps (length, nesting, no nested comprehensions, a step limit).
// Pure: no Date object, no locale, no zone database; the clock and the team
// zone's UTC offset come in through `ExpressionClock`. The Rust core's
// `expressions` module mirrors it line by line and fixtures/expressions.json
// locks both (eval, check, cost and call-form cases; Swift and Kotlin
// replay it through the FFI). The A2UI function form (`{"call": "mul",
// "args": {"a", "b"}}`) of every function with a `call` spelling runs the
// same code (`EXPRESSION_CALL_FUNCTIONS`, in the bind table).

import { daysInMonth, formatDay, isoWeek as isoWeekOf, parseDay, weekdayOf, type CalendarDay } from "./dates"
import { parseDateValue, type ZoneOffset } from "./format"
import spec from "../catalog/expressions.json" with { type: "json" }

/** The cost caps: source characters (code points), parenthesis/bracket/call
 *  nesting, evaluation steps (one per node evaluated, comprehension bodies
 *  once per item). */
export const EXPRESSION_LIMITS: { readonly maxLength: number; readonly maxDepth: number; readonly maxSteps: number } = spec.limits

/** One function's fixed signature (`fixtures/expressions.json` `functions`):
 *  `params` = a type, a union `a|b`, a trailing `?` (optional) or `...`
 *  (variadic; a single list argument spreads); `unit` / `format` = a string
 *  from `units` / `formats`; `call` = the A2UI call-form argument names
 *  (absent = CEL only). */
export interface ExpressionSignature {
  params: string[]
  returns: string
  call?: string[]
  units?: string[]
  formats?: string[]
}
export const EXPRESSION_FUNCTIONS: Readonly<Record<string, ExpressionSignature>> = spec.functions
/** The receiver methods (`x.size()`, `s.contains(t)`…). */
export const EXPRESSION_METHODS: Readonly<Record<string, ExpressionSignature & { receiver: string }>> = spec.methods
/** `l.map(x, e)` `l.filter(x, p)` `l.all(x, p)` `l.exists(x, p)`: never nested. */
export const EXPRESSION_COMPREHENSIONS: readonly string[] = spec.comprehensions
/** CEL's reserved words: never a name. */
export const EXPRESSION_RESERVED: readonly string[] = spec.reserved

export type ExprValue = null | boolean | number | string | ExprValue[] | { [key: string]: ExprValue }

/** Where `today()` and `now()` stand: epoch ms and the team zone's offset. */
export interface ExpressionClock {
  now?: number
  zone?: ZoneOffset
}

/** An evaluation: the value, or why it has none (`column N: …`). */
export type ExpressionResult = { value: ExprValue } | { error: string }

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

type Token = { t: `num` | `str` | `id` | `op` | `eof`; v: string; n?: number; at: number }

class Failure {
  constructor(readonly message: string) {}
}
const fail = (at: number, message: string): never => {
  throw new Failure(`column ${at + 1}: ${message}`)
}

const OPS2 = [`==`, `!=`, `<=`, `>=`, `&&`, `||`]
const OPS1 = `+-*/%<>!?:.,()[]{}`
const isDigit = (c: string | undefined) => c !== undefined && c >= `0` && c <= `9`
const isIdStart = (c: string | undefined) => c !== undefined && ((c >= `a` && c <= `z`) || (c >= `A` && c <= `Z`) || c === `_`)
const isIdChar = (c: string | undefined) => isIdStart(c) || isDigit(c)
const isSpace = (c: string | undefined) => c === ` ` || c === `\t` || c === `\n` || c === `\r`

function lex(chars: string[]): Token[] {
  const out: Token[] = []
  let i = 0
  while (i < chars.length) {
    const c = chars[i]!
    if (isSpace(c)) {
      i++
      continue
    }
    const at = i
    if (isDigit(c)) {
      while (isDigit(chars[i])) i++
      if (chars[i] === `.` && isDigit(chars[i + 1])) {
        i++
        while (isDigit(chars[i])) i++
      }
      if ((chars[i] === `e` || chars[i] === `E`) && (isDigit(chars[i + 1]) || ((chars[i + 1] === `+` || chars[i + 1] === `-`) && isDigit(chars[i + 2])))) {
        i += 2
        while (isDigit(chars[i])) i++
      }
      if (isIdChar(chars[i]) || chars[i] === `.` && isDigit(chars[i + 1])) fail(at, `invalid number`)
      const text = chars.slice(at, i).join(``)
      const n = Number(text)
      if (!Number.isFinite(n)) fail(at, `invalid number`)
      out.push({ t: `num`, v: text, n, at })
      continue
    }
    if (isIdStart(c)) {
      while (isIdChar(chars[i])) i++
      out.push({ t: `id`, v: chars.slice(at, i).join(``), at })
      continue
    }
    if (c === `"` || c === `'`) {
      i++
      let s = ``
      for (;;) {
        const d = chars[i]
        if (d === undefined || d === `\n`) fail(at, `unterminated string`)
        if (d === c) break
        if (d === `\\`) {
          const e = chars[i + 1]
          const simple: Record<string, string> = { "\\": `\\`, '"': `"`, "'": `'`, n: `\n`, r: `\r`, t: `\t` }
          if (e !== undefined && Object.prototype.hasOwnProperty.call(simple, e)) {
            s += simple[e]
            i += 2
            continue
          }
          if (e === `u`) {
            const hex = chars.slice(i + 2, i + 6).join(``)
            const code = /^[0-9a-fA-F]{4}$/.test(hex) ? parseInt(hex, 16) : -1
            if (code < 0 || (code >= 0xd800 && code <= 0xdfff)) fail(i, `invalid escape`)
            s += String.fromCodePoint(code)
            i += 6
            continue
          }
          fail(i, `invalid escape`)
        }
        s += d
        i++
      }
      i++
      out.push({ t: `str`, v: s, at })
      continue
    }
    const two = c + (chars[i + 1] ?? ``)
    if (OPS2.includes(two)) {
      out.push({ t: `op`, v: two, at })
      i += 2
      continue
    }
    if (OPS1.includes(c)) {
      out.push({ t: `op`, v: c, at })
      i++
      continue
    }
    fail(at, `unexpected character "${c}"`)
  }
  out.push({ t: `eof`, v: ``, at: chars.length })
  return out
}

// ---------------------------------------------------------------------------
// Parser (CEL precedence: ?: < || < && < relations/in < + - < * / % < unary < member)
// ---------------------------------------------------------------------------

export type ExprNode =
  | { k: `lit`; at: number; v: ExprValue }
  | { k: `id`; at: number; name: string }
  | { k: `list`; at: number; items: ExprNode[] }
  | { k: `map`; at: number; entries: [ExprNode, ExprNode][] }
  | { k: `field`; at: number; target: ExprNode; name: string }
  | { k: `index`; at: number; target: ExprNode; index: ExprNode }
  | { k: `unary`; at: number; op: string; arg: ExprNode }
  | { k: `binary`; at: number; op: string; left: ExprNode; right: ExprNode }
  | { k: `cond`; at: number; test: ExprNode; then: ExprNode; else: ExprNode }
  | { k: `call`; at: number; name: string; target?: ExprNode; args: ExprNode[] }
  | { k: `has`; at: number; arg: ExprNode & { k: `field` } }
  | { k: `comp`; at: number; op: string; target: ExprNode; v: string; body: ExprNode }

const RELATIONS = [`==`, `!=`, `<`, `<=`, `>`, `>=`]

class Parser {
  private i = 0
  private depth = 0
  private comps = 0
  constructor(private readonly toks: Token[]) {}

  private peek(): Token {
    return this.toks[this.i]!
  }
  private isOp(v: string): boolean {
    const t = this.peek()
    return t.t === `op` && t.v === v
  }
  private eat(v: string): Token {
    const t = this.peek()
    if (t.t !== `op` || t.v !== v) fail(t.at, `expected "${v}"`)
    this.i++
    return t
  }

  parse(): ExprNode {
    const node = this.expr()
    const t = this.peek()
    if (t.t !== `eof`) fail(t.at, `unexpected ${describe(t)}`)
    return node
  }

  private expr(): ExprNode {
    if (++this.depth > EXPRESSION_LIMITS.maxDepth) fail(this.peek().at, `nests deeper than ${EXPRESSION_LIMITS.maxDepth}`)
    const test = this.or()
    let node = test
    if (this.isOp(`?`)) {
      const at = this.eat(`?`).at
      const then = this.or()
      this.eat(`:`)
      const otherwise = this.expr()
      node = { k: `cond`, at, test, then, else: otherwise }
    }
    this.depth--
    return node
  }
  private or(): ExprNode {
    let left = this.and()
    while (this.isOp(`||`)) {
      const at = this.eat(`||`).at
      left = { k: `binary`, at, op: `||`, left, right: this.and() }
    }
    return left
  }
  private and(): ExprNode {
    let left = this.relation()
    while (this.isOp(`&&`)) {
      const at = this.eat(`&&`).at
      left = { k: `binary`, at, op: `&&`, left, right: this.relation() }
    }
    return left
  }
  private relation(): ExprNode {
    let left = this.addition()
    for (;;) {
      const t = this.peek()
      const op = t.t === `op` && RELATIONS.includes(t.v) ? t.v : t.t === `id` && t.v === `in` ? `in` : null
      if (!op) return left
      this.i++
      left = { k: `binary`, at: t.at, op, left, right: this.addition() }
    }
  }
  private addition(): ExprNode {
    let left = this.multiplication()
    while (this.isOp(`+`) || this.isOp(`-`)) {
      const t = this.peek()
      this.i++
      left = { k: `binary`, at: t.at, op: t.v, left, right: this.multiplication() }
    }
    return left
  }
  private multiplication(): ExprNode {
    let left = this.unary()
    while (this.isOp(`*`) || this.isOp(`/`) || this.isOp(`%`)) {
      const t = this.peek()
      this.i++
      left = { k: `binary`, at: t.at, op: t.v, left, right: this.unary() }
    }
    return left
  }
  private unary(): ExprNode {
    const ops: Token[] = []
    while (this.isOp(`!`) || this.isOp(`-`)) ops.push(this.toks[this.i++]!)
    let node = this.member()
    for (let j = ops.length - 1; j >= 0; j--) node = { k: `unary`, at: ops[j]!.at, op: ops[j]!.v, arg: node }
    return node
  }
  private args(): ExprNode[] {
    this.eat(`(`)
    const out: ExprNode[] = []
    if (!this.isOp(`)`)) {
      out.push(this.expr())
      while (this.isOp(`,`)) {
        this.i++
        out.push(this.expr())
      }
    }
    this.eat(`)`)
    return out
  }
  private member(): ExprNode {
    let node = this.primary()
    for (;;) {
      if (this.isOp(`.`)) {
        this.i++
        const name = this.peek()
        if (name.t !== `id`) fail(name.at, `expected a field name`)
        this.i++
        if (!this.isOp(`(`)) {
          node = { k: `field`, at: name.at, target: node, name: name.v }
          continue
        }
        if (EXPRESSION_COMPREHENSIONS.includes(name.v)) {
          if (this.comps > 0) fail(name.at, `comprehensions do not nest`)
          this.comps++
          const args = this.args()
          this.comps--
          const v = args[0]
          if (args.length !== 2 || v?.k !== `id`) fail(name.at, `${name.v} takes a variable name and an expression`)
          node = { k: `comp`, at: name.at, op: name.v, target: node, v: (v as { name: string }).name, body: args[1]! }
          continue
        }
        node = { k: `call`, at: name.at, name: name.v, target: node, args: this.args() }
        continue
      }
      if (this.isOp(`[`)) {
        const at = this.eat(`[`).at
        const index = this.expr()
        this.eat(`]`)
        node = { k: `index`, at, target: node, index }
        continue
      }
      return node
    }
  }
  private primary(): ExprNode {
    const t = this.peek()
    if (t.t === `num`) {
      this.i++
      return { k: `lit`, at: t.at, v: t.n! }
    }
    if (t.t === `str`) {
      this.i++
      return { k: `lit`, at: t.at, v: t.v }
    }
    if (t.t === `id`) {
      if (t.v === `true` || t.v === `false`) {
        this.i++
        return { k: `lit`, at: t.at, v: t.v === `true` }
      }
      if (t.v === `null`) {
        this.i++
        return { k: `lit`, at: t.at, v: null }
      }
      if (t.v === `in` || EXPRESSION_RESERVED.includes(t.v)) fail(t.at, `"${t.v}" is reserved`)
      this.i++
      if (!this.isOp(`(`)) return { k: `id`, at: t.at, name: t.v }
      const args = this.args()
      if (t.v === `has`) {
        const arg = args[0]
        if (args.length !== 1 || arg?.k !== `field`) fail(t.at, `has takes a field selection like has(a.b)`)
        return { k: `has`, at: t.at, arg: arg as ExprNode & { k: `field` } }
      }
      return { k: `call`, at: t.at, name: t.v, args }
    }
    if (t.t === `op` && t.v === `(`) {
      this.i++
      const node = this.expr()
      this.eat(`)`)
      return node
    }
    if (t.t === `op` && t.v === `[`) {
      this.i++
      const items: ExprNode[] = []
      while (!this.isOp(`]`)) {
        items.push(this.expr())
        if (!this.isOp(`,`)) break
        this.i++
      }
      this.eat(`]`)
      return { k: `list`, at: t.at, items }
    }
    if (t.t === `op` && t.v === `{`) {
      this.i++
      const entries: [ExprNode, ExprNode][] = []
      while (!this.isOp(`}`)) {
        const key = this.expr()
        this.eat(`:`)
        entries.push([key, this.expr()])
        if (!this.isOp(`,`)) break
        this.i++
      }
      this.eat(`}`)
      return { k: `map`, at: t.at, entries }
    }
    return fail(t.at, t.t === `eof` ? `expected an expression` : `unexpected ${describe(t)}`)
  }
}

function describe(t: Token): string {
  if (t.t === `eof`) return `end`
  if (t.t === `str`) return `string`
  return `"${t.v}"`
}

/** The parsed expression, or the first problem with it (`column N: …`;
 *  over `maxLength` characters: no column). */
export function parseExpression(source: string): { ast: ExprNode } | { error: string } {
  const chars = [...source]
  if (chars.length > EXPRESSION_LIMITS.maxLength) return { error: `longer than ${EXPRESSION_LIMITS.maxLength} characters` }
  try {
    return { ast: new Parser(lex(chars)).parse() }
  } catch (e) {
    if (e instanceof Failure) return { error: e.message }
    throw e
  }
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

const isMap = (v: ExprValue): v is { [key: string]: ExprValue } => typeof v === `object` && v !== null && !Array.isArray(v)

export function typeOfValue(v: ExprValue): string {
  if (v === null) return `null`
  if (Array.isArray(v)) return `list`
  if (typeof v === `object`) return `map`
  if (typeof v === `boolean`) return `bool`
  return typeof v
}

/** Any JS value as an expression value: undefined → null, non-finite
 *  numbers → null, objects copied with own keys only. */
export function toExprValue(v: unknown): ExprValue {
  if (v === undefined || v === null) return null
  if (typeof v === `number`) return Number.isFinite(v) ? v : null
  if (typeof v === `string` || typeof v === `boolean`) return v
  if (Array.isArray(v)) return v.map(toExprValue)
  if (typeof v === `object`) {
    const out: { [key: string]: ExprValue } = {}
    for (const [k, x] of Object.entries(v as object)) out[k] = toExprValue(x)
    return out
  }
  return null
}

export function exprEqual(a: ExprValue, b: ExprValue): boolean {
  if (a === b) return true
  if (Array.isArray(a)) return Array.isArray(b) && a.length === b.length && a.every((x, i) => exprEqual(x, b[i]!))
  if (isMap(a) && isMap(b)) {
    const ka = Object.keys(a)
    return ka.length === Object.keys(b).length && ka.every((k) => Object.prototype.hasOwnProperty.call(b, k) && exprEqual(a[k]!, b[k]!))
  }
  return false
}

/** Strings compare by code point (= the Rust core's byte order). */
function compareText(a: string, b: string): number {
  const x = [...a]
  const y = [...b]
  for (let i = 0; i < Math.min(x.length, y.length); i++) {
    const d = x[i]!.codePointAt(0)! - y[i]!.codePointAt(0)!
    if (d !== 0) return d
  }
  return x.length - y.length
}

/** A decimal number spelled in a string (`" 7.5 "`, `-1e3`), else null. */
export function decimalText(text: string): number | null {
  const t = text.replace(/^[ \t\n\r]+|[ \t\n\r]+$/g, ``)
  if (!/^-?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?$/.test(t)) return null
  const n = Number(t)
  return Number.isFinite(n) ? n : null
}

/** JS `Math.round` (half rounds up, also for negatives). */
const jsRound = (v: number) => Math.floor(v + 0.5)

/** Number → text the way JS `String(n)` spells it. */
const numberText = (n: number) => String(n)

// ---------------------------------------------------------------------------
// Dates (spec §4.3: a date is a calendar day; instants read in the team zone)
// ---------------------------------------------------------------------------

const MS_DAY = 86_400_000
const DATE_UNITS = [`day`, `week`, `month`, `quarter`, `year`]

/** Days since 1970-01-01 (proleptic Gregorian). */
export function dayNumber(day: CalendarDay): number {
  const y = day.m <= 2 ? day.y - 1 : day.y
  const era = Math.floor(y / 400)
  const yoe = y - era * 400
  const mp = (day.m + 9) % 12
  const doy = Math.floor((153 * mp + 2) / 5) + day.d - 1
  const doe = yoe * 365 + Math.floor(yoe / 4) - Math.floor(yoe / 100) + doy
  return era * 146097 + doe - 719468
}

/** The calendar day `n` days after 1970-01-01. */
export function dayFromNumber(n: number): CalendarDay {
  const z = n + 719468
  const era = Math.floor(z / 146097)
  const doe = z - era * 146097
  const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146096)) / 365)
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100))
  const mp = Math.floor((5 * doy + 2) / 153)
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1
  const m = mp < 10 ? mp + 3 : mp - 9
  return { y: era * 400 + yoe + (m <= 2 ? 1 : 0), m, d }
}

const offsetAt = (clock: ExpressionClock, ms: number) => (clock.zone ? clock.zone(ms) : 0)
const clockNow = (clock: ExpressionClock) => clock.now ?? Date.now()

/** The local calendar day of an instant (epoch ms) in the clock's zone. */
function localDay(ms: number, clock: ExpressionClock): CalendarDay {
  return dayFromNumber(Math.floor((ms + offsetAt(clock, ms) * 60_000) / MS_DAY))
}

const pad = (n: number, w: number) => String(n).padStart(w, `0`)

function checkedDay(day: CalendarDay, at: number): string {
  if (day.y < 0 || day.y > 9999) fail(at, `date out of range`)
  return formatDay(day)
}

/** A date argument: `YYYY-MM-DD`, or an ISO date-time (its day in the team zone). */
function dayArg(v: ExprValue, name: string, at: number, clock: ExpressionClock): CalendarDay {
  if (typeof v !== `string`) return fail(at, `${name} needs a date, got ${typeOfValue(v)}`)
  if (v.length === 10) {
    const day = parseDay(v)
    if (day) return day
  } else if (parseDay(v)) {
    const parsed = parseDateValue(v)
    if (parsed && !parsed.dateOnly) return localDay(parsed.ms, clock)
  }
  return fail(at, `"${v}" is not a date`)
}

/** An instant for dateDiff's hour/minute/second: a date-time, a date (its
 *  local midnight) or a time of day `HH:MM[:SS]` (seconds into the day). */
function instantArg(v: ExprValue, at: number, clock: ExpressionClock): { ms: number; time: boolean } {
  if (typeof v === `string`) {
    const time = /^(\d{2}):(\d{2})(?::(\d{2}))?$/.exec(v)
    if (time) {
      const [h, m, s] = [Number(time[1]), Number(time[2]), Number(time[3] ?? 0)]
      if (h < 24 && m < 60 && s < 60) return { ms: (h * 3600 + m * 60 + s) * 1000, time: true }
    } else if (v.length === 10) {
      const day = parseDay(v)
      if (day) {
        const utc = dayNumber(day) * MS_DAY
        return { ms: utc - offsetAt(clock, utc) * 60_000, time: false }
      }
    } else if (parseDay(v)) {
      const parsed = parseDateValue(v)
      if (parsed && !parsed.dateOnly) return { ms: parsed.ms, time: false }
    }
    return fail(at, `"${v}" is not a date or a time`)
  }
  return fail(at, `dateDiff needs dates or times, got ${typeOfValue(v)}`)
}

function unitArg(v: ExprValue, name: string, at: number): string {
  const units = EXPRESSION_FUNCTIONS[name]!.units!
  if (typeof v !== `string` || !units.includes(v)) fail(at, `unknown unit ${typeof v === `string` ? `"${v}"` : typeOfValue(v)} (${units.join(`, `)})`)
  return v as string
}

function addMonths(day: CalendarDay, n: number): CalendarDay {
  const total = day.y * 12 + (day.m - 1) + n
  const y = Math.floor(total / 12)
  const m = total - y * 12 + 1
  return { y, m, d: Math.min(day.d, daysInMonth(y, m)) }
}

function startOf(day: CalendarDay, unit: string): CalendarDay {
  if (unit === `week`) return dayFromNumber(dayNumber(day) - ((weekdayOf(day.y, day.m, day.d) + 6) % 7))
  if (unit === `month`) return { y: day.y, m: day.m, d: 1 }
  if (unit === `quarter`) return { y: day.y, m: Math.floor((day.m - 1) / 3) * 3 + 1, d: 1 }
  if (unit === `year`) return { y: day.y, m: 1, d: 1 }
  return day
}

function endOf(day: CalendarDay, unit: string): CalendarDay {
  if (unit === `week`) return dayFromNumber(dayNumber(startOf(day, `week`)) + 6)
  if (unit === `month`) return { y: day.y, m: day.m, d: daysInMonth(day.y, day.m) }
  if (unit === `quarter`) {
    const m = Math.floor((day.m - 1) / 3) * 3 + 3
    return { y: day.y, m, d: daysInMonth(day.y, m) }
  }
  if (unit === `year`) return { y: day.y, m: 12, d: 31 }
  return day
}

function monthsBetween(a: CalendarDay, b: CalendarDay): number {
  let m = b.y * 12 + b.m - (a.y * 12 + a.m)
  if (m > 0 && b.d < a.d) m--
  if (m < 0 && b.d > a.d) m++
  return m
}

const trunc = (n: number) => Math.trunc(n) || 0

function formatDuration(hours: number, format: string): string {
  const seconds = format.endsWith(`:ss`)
  const total = jsRound(Math.abs(hours) * (seconds ? 3600 : 60))
  const sign = hours < 0 && total > 0 ? `-` : ``
  const width = format.startsWith(`hh`) ? 2 : 1
  if (!seconds) return `${sign}${pad(Math.floor(total / 60), width)}:${pad(total % 60, 2)}`
  return `${sign}${pad(Math.floor(total / 3600), width)}:${pad(Math.floor((total % 3600) / 60), 2)}:${pad(total % 60, 2)}`
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

function num(v: ExprValue, name: string, at: number): number {
  if (typeof v !== `number`) fail(at, `${name} needs a number, got ${typeOfValue(v)}`)
  return v as number
}
function text(v: ExprValue, name: string, at: number): string {
  if (typeof v !== `string`) fail(at, `${name} needs a string, got ${typeOfValue(v)}`)
  return v as string
}
function whole(v: ExprValue, name: string, at: number): number {
  const n = num(v, name, at)
  if (!Number.isInteger(n)) fail(at, `${name} needs a whole number, got ${numberText(n)}`)
  return n
}
const finite = (n: number, at: number): number => (Number.isFinite(n) ? n : fail(at, `number out of range`))

function arity(name: string, sig: ExpressionSignature, got: number, at: number): void {
  const required = sig.params.filter((p) => !p.endsWith(`?`) && !p.endsWith(`...`)).length
  const variadic = sig.params.some((p) => p.endsWith(`...`))
  const max = variadic ? Infinity : sig.params.length
  const min = variadic ? Math.max(required, 1) : required
  if (got >= min && got <= max) return
  const s = (n: number) => `${n} argument${n === 1 ? `` : `s`}`
  const want = variadic ? `at least ${s(min)}` : min === max ? s(min) : `${min} to ${s(max)}`
  fail(at, `${name} takes ${want}, got ${got}`)
}

function spread(args: ExprValue[], name: string, at: number): number[] {
  const list = args.length === 1 && Array.isArray(args[0]) ? args[0] : args
  if (list.length === 0) fail(at, `${name} needs at least one number`)
  return list.map((v) => num(v, name, at))
}

type Impl = (args: ExprValue[], at: number, clock: ExpressionClock) => ExprValue

const FUNCTIONS: Record<string, Impl> = {
  size: ([v], at) => {
    if (typeof v === `string`) return [...v].length
    if (Array.isArray(v)) return v.length
    if (v !== undefined && isMap(v)) return Object.keys(v).length
    return fail(at, `size needs a string, a list or a map, got ${typeOfValue(v!)}`)
  },
  string: ([v], at) => {
    if (v === null) return `null`
    if (typeof v === `boolean`) return String(v)
    if (typeof v === `number`) return numberText(v)
    if (typeof v === `string`) return v
    return fail(at, `string cannot convert a ${typeOfValue(v!)}`)
  },
  double: ([v], at) => {
    if (typeof v === `number`) return v
    if (typeof v === `string`) return decimalText(v) ?? fail(at, `"${v}" is not a number`)
    return fail(at, `double needs a number or a string, got ${typeOfValue(v!)}`)
  },
  int: (args, at, clock) => trunc(FUNCTIONS.double!(args, at, clock) as number),
  mul: ([a, b], at) => finite(num(a!, `mul`, at) * num(b!, `mul`, at), at),
  div: ([a, b], at) => {
    const d = num(b!, `div`, at)
    if (d === 0) fail(at, `division by zero`)
    return finite(num(a!, `div`, at) / d, at)
  },
  mod: ([a, b], at) => {
    const d = num(b!, `mod`, at)
    if (d === 0) fail(at, `modulo by zero`)
    return finite(num(a!, `mod`, at) % d, at) || 0
  },
  round: ([v, digits], at) => {
    const n = num(v!, `round`, at)
    if (digits === undefined || digits === null) return jsRound(n) || 0
    const d = whole(digits, `round`, at)
    if (d < 0 || d > 10) fail(at, `round takes 0 to 10 digits, got ${numberText(d)}`)
    const f = 10 ** d
    return jsRound(n * f) / f || 0
  },
  floor: ([v], at) => Math.floor(num(v!, `floor`, at)) || 0,
  ceil: ([v], at) => Math.ceil(num(v!, `ceil`, at)) || 0,
  abs: ([v], at) => Math.abs(num(v!, `abs`, at)),
  min: (args, at) => Math.min(...spread(args, `min`, at)),
  max: (args, at) => Math.max(...spread(args, `max`, at)),
  today: (_, _at, clock) => formatDay(localDay(clockNow(clock), clock)),
  now: (_, _at, clock) => {
    const ms = Math.floor(clockNow(clock) / 1000) * 1000
    const offset = offsetAt(clock, ms)
    const local = ms + offset * 60_000
    const day = dayFromNumber(Math.floor(local / MS_DAY))
    const secs = Math.floor((local - Math.floor(local / MS_DAY) * MS_DAY) / 1000)
    const o = Math.abs(offset)
    return `${formatDay(day)}T${pad(Math.floor(secs / 3600), 2)}:${pad(Math.floor((secs % 3600) / 60), 2)}:${pad(secs % 60, 2)}${offset < 0 ? `-` : `+`}${pad(Math.floor(o / 60), 2)}:${pad(o % 60, 2)}`
  },
  dateAdd: ([d, n, unit], at, clock) => {
    const day = dayArg(d!, `dateAdd`, at, clock)
    const k = whole(n!, `dateAdd`, at)
    const u = unitArg(unit!, `dateAdd`, at)
    if (u === `day` || u === `week`) return checkedDay(dayFromNumber(dayNumber(day) + k * (u === `week` ? 7 : 1)), at)
    return checkedDay(addMonths(day, k * (u === `quarter` ? 3 : u === `year` ? 12 : 1)), at)
  },
  dateDiff: ([a, b, unit], at, clock) => {
    const u = unitArg(unit!, `dateDiff`, at)
    if (!DATE_UNITS.includes(u)) {
      const x = instantArg(a!, at, clock)
      const y = instantArg(b!, at, clock)
      if (x.time !== y.time) fail(at, `dateDiff compares two times or two dates`)
      return trunc((y.ms - x.ms) / (u === `hour` ? 3_600_000 : u === `minute` ? 60_000 : 1000))
    }
    const x = dayArg(a!, `dateDiff`, at, clock)
    const y = dayArg(b!, `dateDiff`, at, clock)
    if (u === `day`) return dayNumber(y) - dayNumber(x)
    if (u === `week`) return trunc((dayNumber(y) - dayNumber(x)) / 7)
    const months = monthsBetween(x, y)
    return u === `month` ? months : trunc(months / (u === `quarter` ? 3 : 12))
  },
  startOf: ([d, unit], at, clock) => formatDay(startOf(dayArg(d!, `startOf`, at, clock), unitArg(unit!, `startOf`, at))),
  endOf: ([d, unit], at, clock) => formatDay(endOf(dayArg(d!, `endOf`, at, clock), unitArg(unit!, `endOf`, at))),
  weekday: ([d], at, clock) => {
    const day = dayArg(d!, `weekday`, at, clock)
    return weekdayOf(day.y, day.m, day.d) || 7
  },
  isoWeek: ([d], at, clock) => {
    const day = dayArg(d!, `isoWeek`, at, clock)
    return isoWeekOf(day.y, day.m, day.d).week
  },
  hours: ([m], at) => num(m!, `hours`, at) / 60,
  formatDuration: ([h, format], at) => {
    const hours = num(h!, `formatDuration`, at)
    const formats = EXPRESSION_FUNCTIONS.formatDuration!.formats!
    const f = format === undefined || format === null ? formats[0]! : text(format, `formatDuration`, at)
    if (!formats.includes(f)) fail(at, `unknown duration format "${f}" (${formats.join(`, `)})`)
    return formatDuration(hours, f)
  },
}

const METHODS: Record<string, (target: ExprValue, args: ExprValue[], at: number) => ExprValue> = {
  size: (t, _, at) => FUNCTIONS.size!([t], at, {}),
  contains: (t, [s], at) => text(t, `contains`, at).includes(text(s!, `contains`, at)),
  startsWith: (t, [s], at) => text(t, `startsWith`, at).startsWith(text(s!, `startsWith`, at)),
  endsWith: (t, [s], at) => text(t, `endsWith`, at).endsWith(text(s!, `endsWith`, at)),
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

interface State {
  steps: number
  clock: ExpressionClock
}

function field(target: ExprValue, name: string, at: number): ExprValue {
  if (isMap(target)) return Object.prototype.hasOwnProperty.call(target, name) ? target[name]! : null
  return fail(at, `no field "${name}" on ${typeOfValue(target)}`)
}

function evaluate(n: ExprNode, scope: ReadonlyMap<string, ExprValue>, st: State): ExprValue {
  if (++st.steps > EXPRESSION_LIMITS.maxSteps) fail(n.at, `step limit of ${EXPRESSION_LIMITS.maxSteps} exceeded`)
  switch (n.k) {
    case `lit`:
      return n.v
    case `id`:
      return scope.get(n.name) ?? null
    case `list`:
      return n.items.map((x) => evaluate(x, scope, st))
    case `map`: {
      const out: { [key: string]: ExprValue } = {}
      for (const [k, v] of n.entries) {
        const key = evaluate(k, scope, st)
        if (typeof key !== `string`) fail(k.at, `a map key must be a string, got ${typeOfValue(key)}`)
        if (Object.prototype.hasOwnProperty.call(out, key as string)) fail(k.at, `duplicate key "${key}"`)
        Object.defineProperty(out, key as string, { value: evaluate(v, scope, st), writable: true, enumerable: true, configurable: true })
      }
      return out
    }
    case `field`:
      return field(evaluate(n.target, scope, st), n.name, n.at)
    case `has`: {
      const target = evaluate(n.arg.target, scope, st)
      if (!isMap(target)) fail(n.arg.at, `no field "${n.arg.name}" on ${typeOfValue(target)}`)
      return Object.prototype.hasOwnProperty.call(target, n.arg.name) && (target as Record<string, ExprValue>)[n.arg.name] !== null
    }
    case `index`: {
      const target = evaluate(n.target, scope, st)
      const index = evaluate(n.index, scope, st)
      if (Array.isArray(target)) {
        if (typeof index !== `number` || !Number.isInteger(index)) return fail(n.at, `a list index must be a whole number, got ${typeof index === `number` ? numberText(index) : typeOfValue(index)}`)
        if (index < 0 || index >= target.length) fail(n.at, `index ${numberText(index)} out of range (${target.length} items)`)
        return target[index]!
      }
      if (isMap(target)) {
        if (typeof index !== `string`) fail(n.at, `a map key must be a string, got ${typeOfValue(index)}`)
        return Object.prototype.hasOwnProperty.call(target, index as string) ? target[index as string]! : null
      }
      return fail(n.at, `cannot index a ${typeOfValue(target)}`)
    }
    case `unary`: {
      const v = evaluate(n.arg, scope, st)
      if (n.op === `!`) return typeof v === `boolean` ? !v : fail(n.at, `! needs a bool, got ${typeOfValue(v)}`)
      return typeof v === `number` ? -v || 0 : fail(n.at, `- needs a number, got ${typeOfValue(v)}`)
    }
    case `cond`: {
      const test = evaluate(n.test, scope, st)
      if (typeof test !== `boolean`) fail(n.at, `?: needs a bool, got ${typeOfValue(test)}`)
      return evaluate(test ? n.then : n.else, scope, st)
    }
    case `binary`:
      return binary(n, scope, st)
    case `call`: {
      const target = n.target ? evaluate(n.target, scope, st) : undefined
      const args = n.args.map((x) => evaluate(x, scope, st))
      if (n.target) {
        const method = Object.prototype.hasOwnProperty.call(METHODS, n.name) ? METHODS[n.name]! : fail(n.at, `unknown method "${n.name}"`)
        arity(n.name, EXPRESSION_METHODS[n.name]!, args.length, n.at)
        return method(target!, args, n.at)
      }
      const fn = Object.prototype.hasOwnProperty.call(FUNCTIONS, n.name) ? FUNCTIONS[n.name]! : fail(n.at, `unknown function "${n.name}"`)
      arity(n.name, EXPRESSION_FUNCTIONS[n.name]!, args.length, n.at)
      return fn(args, n.at, st.clock)
    }
    case `comp`: {
      const target = evaluate(n.target, scope, st)
      if (!Array.isArray(target)) return fail(n.at, `${n.op} needs a list, got ${typeOfValue(target)}`)
      const inner = new Map(scope)
      const body = (item: ExprValue) => {
        inner.set(n.v, item)
        return evaluate(n.body, inner, st)
      }
      const test = (item: ExprValue) => {
        const v = body(item)
        return typeof v === `boolean` ? v : fail(n.body.at, `${n.op} needs a bool, got ${typeOfValue(v)}`)
      }
      if (n.op === `map`) return target.map(body)
      if (n.op === `filter`) return target.filter(test)
      if (n.op === `all`) {
        for (const item of target) if (!test(item)) return false
        return true
      }
      for (const item of target) if (test(item)) return true
      return false
    }
  }
}

function binary(n: ExprNode & { k: `binary` }, scope: ReadonlyMap<string, ExprValue>, st: State): ExprValue {
  const op = n.op
  if (op === `&&` || op === `||`) {
    const l = evaluate(n.left, scope, st)
    if (typeof l !== `boolean`) fail(n.at, `${op} needs bools, got ${typeOfValue(l)}`)
    if (op === `&&` ? !l : l) return l
    const r = evaluate(n.right, scope, st)
    if (typeof r !== `boolean`) fail(n.at, `${op} needs bools, got ${typeOfValue(r)}`)
    return r
  }
  const l = evaluate(n.left, scope, st)
  const r = evaluate(n.right, scope, st)
  const got = `${typeOfValue(l)} and ${typeOfValue(r)}`
  switch (op) {
    case `==`:
      return exprEqual(l, r)
    case `!=`:
      return !exprEqual(l, r)
    case `in`:
      if (Array.isArray(r)) return r.some((x) => exprEqual(x, l))
      if (isMap(r)) return typeof l === `string` && Object.prototype.hasOwnProperty.call(r, l)
      return fail(n.at, `in needs a list or a map, got ${typeOfValue(r)}`)
    case `<`:
    case `<=`:
    case `>`:
    case `>=`: {
      let c: number
      if (typeof l === `number` && typeof r === `number`) c = l < r ? -1 : l > r ? 1 : 0
      else if (typeof l === `string` && typeof r === `string`) c = compareText(l, r)
      else return fail(n.at, `${op} compares numbers or strings, got ${got}`)
      return op === `<` ? c < 0 : op === `<=` ? c <= 0 : op === `>` ? c > 0 : c >= 0
    }
    case `+`:
      if (typeof l === `number` && typeof r === `number`) return finite(l + r, n.at)
      if (typeof l === `string` && typeof r === `string`) return l + r
      if (Array.isArray(l) && Array.isArray(r)) return [...l, ...r]
      return fail(n.at, `+ needs two numbers, strings or lists, got ${got}`)
    default: {
      if (typeof l !== `number` || typeof r !== `number`) return fail(n.at, `${op} needs numbers, got ${got}`)
      if (op === `-`) return finite(l - r, n.at)
      if (op === `*`) return finite(l * r, n.at)
      if (r === 0) fail(n.at, op === `/` ? `division by zero` : `modulo by zero`)
      return op === `/` ? finite(l / r, n.at) : l % r || 0
    }
  }
}

/** Evaluate `source` over `vars` (any JSON; a missing name reads null). */
export function evalExpression(source: string, vars: Record<string, unknown> = {}, clock: ExpressionClock = {}): ExpressionResult {
  const parsed = parseExpression(source)
  if (`error` in parsed) return parsed
  return evalParsed(parsed.ast, vars, clock)
}

/** Evaluate an already parsed expression. */
export function evalParsed(ast: ExprNode, vars: Record<string, unknown> = {}, clock: ExpressionClock = {}): ExpressionResult {
  const scope = new Map<string, ExprValue>()
  for (const [k, v] of Object.entries(vars)) scope.set(k, toExprValue(v))
  try {
    return { value: evaluate(ast, scope, { steps: 0, clock }) }
  } catch (e) {
    if (e instanceof Failure) return { error: e.message }
    throw e
  }
}

// ---------------------------------------------------------------------------
// Type checker
// ---------------------------------------------------------------------------

/** A static type: bool number string null dyn, list<T>, map, or a record
 *  kind (its declared fields; the row's reserved keys are strings). */
export type ExprType =
  | { k: `bool` | `number` | `string` | `null` | `dyn` | `map` }
  | { k: `list`; of: ExprType }
  | { k: `kind`; name: string; fields: Record<string, ExprType> }

const T = (k: `bool` | `number` | `string` | `null` | `dyn` | `map`): ExprType => ({ k })
const DYN = T(`dyn`)
const listOf = (of: ExprType): ExprType => ({ k: `list`, of })

export function typeName(t: ExprType): string {
  if (t.k === `list`) return `list<${typeName(t.of)}>`
  if (t.k === `kind`) return t.name
  return t.k
}

/** A kind as the checker sees it: `{schema: {properties: {<field>: {type, items?}}}}`. */
export interface CheckKind {
  schema?: { properties?: Record<string, { type?: string; items?: { type?: string } }> }
}

/** The row keys every record carries (catalog/host.json `records.reserved` + `authorId`). */
export const EXPRESSION_ROW_KEYS: readonly string[] = spec.rowKeys

function schemaType(prop: { type?: string; items?: { type?: string } } | undefined): ExprType {
  switch (prop?.type) {
    case `string`:
      return T(`string`)
    case `number`:
    case `integer`:
      return T(`number`)
    case `boolean`:
      return T(`bool`)
    case `array`:
      return listOf(prop.items ? schemaType(prop.items) : DYN)
    default:
      return DYN
  }
}

function kindType(name: string, kind: CheckKind): ExprType {
  const fields: Record<string, ExprType> = {}
  for (const key of EXPRESSION_ROW_KEYS) fields[key] = T(`string`)
  for (const [f, prop] of Object.entries(kind.schema?.properties ?? {})) fields[f] = schemaType(prop)
  return { k: `kind`, name, fields }
}

/** A declared type spelled `bool number string null dyn map list list<T>`
 *  or a kind's name; null = unknown. */
export function parseTypeName(text: string, kinds: Record<string, CheckKind> = {}): ExprType | null {
  if ([`bool`, `number`, `string`, `null`, `dyn`, `map`].includes(text)) return T(text as `dyn`)
  if (text === `list`) return listOf(DYN)
  const list = /^list<(.+)>$/.exec(text)
  if (list) {
    const of = parseTypeName(list[1]!, kinds)
    return of ? listOf(of) : null
  }
  return Object.prototype.hasOwnProperty.call(kinds, text) ? kindType(text, kinds[text]!) : null
}

/** The static type of a JSON value (a template's literal var). */
export function typeOfLiteral(v: unknown): ExprType {
  if (v === null || v === undefined) return T(`null`)
  if (typeof v === `boolean`) return T(`bool`)
  if (typeof v === `number`) return T(`number`)
  if (typeof v === `string`) return T(`string`)
  if (Array.isArray(v)) return listOf(join(v.map(typeOfLiteral)))
  return T(`map`)
}

const same = (a: ExprType, b: ExprType) => typeName(a) === typeName(b)
const family = (t: ExprType) => (t.k === `kind` ? `map` : t.k)

function join(types: ExprType[]): ExprType {
  const real = types.filter((t) => t.k !== `null`)
  if (real.length === 0) return types.length > 0 ? T(`null`) : DYN
  return real.every((t) => same(t, real[0]!)) ? real[0]! : DYN
}

/** Does a value of `t` fit a signature param (`number`, `string|list`, `list<number>`, `unit`…)? */
function fits(t: ExprType, param: string): boolean {
  if (t.k === `dyn` || t.k === `null`) return true
  const base = param.replace(/\?$|\.\.\.$/, ``)
  return base.split(`|`).some((want) => {
    if (want === `dyn`) return true
    if (want === `unit` || want === `format`) return t.k === `string`
    if (want === `map`) return t.k === `map` || t.k === `kind`
    if (want === `list`) return t.k === `list`
    const list = /^list<(.+)>$/.exec(want)
    if (list) return t.k === `list` && fits(t.of, list[1]!)
    return t.k === want
  })
}

class Checker {
  readonly issues: string[] = []
  private issue(at: number, message: string): ExprType {
    this.issues.push(`column ${at + 1}: ${message}`)
    return DYN
  }

  check(n: ExprNode, scope: ReadonlyMap<string, ExprType>): ExprType {
    switch (n.k) {
      case `lit`:
        return typeOfLiteral(n.v)
      case `id`:
        return scope.get(n.name) ?? this.issue(n.at, `unknown name "${n.name}"`)
      case `list`:
        return listOf(join(n.items.map((x) => this.check(x, scope))))
      case `map`:
        for (const [k, v] of n.entries) {
          const key = this.check(k, scope)
          if (!fits(key, `string`)) this.issue(k.at, `a map key must be a string, got ${typeName(key)}`)
          this.check(v, scope)
        }
        return T(`map`)
      case `field`:
        return this.field(this.check(n.target, scope), n.name, n.at)
      case `has`:
        this.field(this.check(n.arg.target, scope), n.arg.name, n.arg.at)
        return T(`bool`)
      case `index`: {
        const target = this.check(n.target, scope)
        const index = this.check(n.index, scope)
        if (target.k === `list`) return fits(index, `number`) ? target.of : this.issue(n.at, `a list index must be a whole number, got ${typeName(index)}`)
        if (target.k === `map` || target.k === `kind` || target.k === `dyn`) return fits(index, `string`) || target.k === `dyn` ? DYN : this.issue(n.at, `a map key must be a string, got ${typeName(index)}`)
        return this.issue(n.at, `cannot index a ${typeName(target)}`)
      }
      case `unary`: {
        const t = this.check(n.arg, scope)
        const want = n.op === `!` ? `bool` : `number`
        return fits(t, want) ? T(want) : this.issue(n.at, `${n.op} needs a ${want}, got ${typeName(t)}`)
      }
      case `cond`: {
        const test = this.check(n.test, scope)
        if (!fits(test, `bool`)) this.issue(n.at, `?: needs a bool, got ${typeName(test)}`)
        return join([this.check(n.then, scope), this.check(n.else, scope)])
      }
      case `binary`:
        return this.binary(n, scope)
      case `call`:
        return this.call(n, scope)
      case `comp`: {
        const target = this.check(n.target, scope)
        if (target.k !== `list` && target.k !== `dyn`) {
          this.issue(n.at, `${n.op} needs a list, got ${typeName(target)}`)
        }
        const inner = new Map(scope)
        inner.set(n.v, target.k === `list` ? target.of : DYN)
        const body = this.check(n.body, inner)
        if (n.op === `map`) return listOf(body)
        if (!fits(body, `bool`)) this.issue(n.body.at, `${n.op} needs a bool, got ${typeName(body)}`)
        return n.op === `filter` ? (target.k === `list` ? target : listOf(DYN)) : T(`bool`)
      }
    }
  }

  private field(target: ExprType, name: string, at: number): ExprType {
    if (target.k === `kind`) return Object.prototype.hasOwnProperty.call(target.fields, name) ? target.fields[name]! : this.issue(at, `${target.name} has no field "${name}"`)
    if (target.k === `map` || target.k === `dyn`) return DYN
    return this.issue(at, `no field "${name}" on ${typeName(target)}`)
  }

  private binary(n: ExprNode & { k: `binary` }, scope: ReadonlyMap<string, ExprType>): ExprType {
    const l = this.check(n.left, scope)
    const r = this.check(n.right, scope)
    const got = `${typeName(l)} and ${typeName(r)}`
    const op = n.op
    if (op === `&&` || op === `||`) {
      for (const t of [l, r]) if (!fits(t, `bool`)) this.issue(n.at, `${op} needs bools, got ${typeName(t)}`)
      return T(`bool`)
    }
    if (op === `==` || op === `!=`) {
      if (l.k !== `dyn` && r.k !== `dyn` && l.k !== `null` && r.k !== `null` && family(l) !== family(r)) this.issue(n.at, `${op} compares ${typeName(l)} with ${typeName(r)}`)
      return T(`bool`)
    }
    if (op === `in`) {
      if (r.k !== `list` && r.k !== `map` && r.k !== `kind` && r.k !== `dyn`) this.issue(n.at, `in needs a list or a map, got ${typeName(r)}`)
      return T(`bool`)
    }
    if (op === `<` || op === `<=` || op === `>` || op === `>=`) {
      const ok = (fits(l, `number`) && fits(r, `number`)) || (fits(l, `string`) && fits(r, `string`))
      if (!ok || l.k === `null` || r.k === `null`) this.issue(n.at, `${op} compares numbers or strings, got ${got}`)
      return T(`bool`)
    }
    if (op === `+`) {
      if (l.k === `null` || r.k === `null`) return this.issue(n.at, `+ needs two numbers, strings or lists, got ${got}`)
      if (l.k === `dyn`) return r.k === `number` || r.k === `string` || r.k === `list` ? r : r.k === `dyn` ? DYN : this.issue(n.at, `+ needs two numbers, strings or lists, got ${got}`)
      if (r.k === `dyn`) return l.k === `number` || l.k === `string` || l.k === `list` ? l : this.issue(n.at, `+ needs two numbers, strings or lists, got ${got}`)
      if ((l.k === `number` || l.k === `string`) && l.k === r.k) return l
      if (l.k === `list` && r.k === `list`) return listOf(join([l.of, r.of]))
      return this.issue(n.at, `+ needs two numbers, strings or lists, got ${got}`)
    }
    if (!fits(l, `number`) || !fits(r, `number`) || l.k === `null` || r.k === `null`) return this.issue(n.at, `${op} needs numbers, got ${got}`)
    return T(`number`)
  }

  private call(n: ExprNode & { k: `call` }, scope: ReadonlyMap<string, ExprType>): ExprType {
    const target = n.target ? this.check(n.target, scope) : undefined
    const args = n.args.map((x) => this.check(x, scope))
    const table: Record<string, ExpressionSignature & { receiver?: string }> = n.target ? EXPRESSION_METHODS : EXPRESSION_FUNCTIONS
    if (!Object.prototype.hasOwnProperty.call(table, n.name)) return this.issue(n.at, `unknown ${n.target ? `method` : `function`} "${n.name}"`)
    const sig = table[n.name]!
    if (target && sig.receiver && !fits(target, sig.receiver)) this.issue(n.at, `${n.name} needs a ${sig.receiver.split(`|`).join(` or `)}, got ${typeName(target)}`)
    try {
      arity(n.name, sig, args.length, n.at)
    } catch (e) {
      if (e instanceof Failure) {
        this.issues.push(e.message)
        return T(sig.returns as `dyn`)
      }
      throw e
    }
    const variadic = sig.params.find((p) => p.endsWith(`...`))
    const listSpread = variadic && args.length === 1 && args[0]!.k === `list`
    args.forEach((t, i) => {
      const param = variadic && i >= sig.params.length - 1 ? variadic : sig.params[i]!
      const want = listSpread ? `list<${param.replace(/\.\.\.$/, ``)}>` : param
      if (!fits(t, want)) this.issue(n.args[i]!.at, `argument ${i + 1} of ${n.name} must be ${want.replace(/\?$|\.\.\.$/, ``)}, got ${typeName(t)}`)
      const arg = n.args[i]!
      if (arg.k === `lit` && typeof arg.v === `string`) {
        if (param === `unit` && !sig.units!.includes(arg.v)) this.issue(arg.at, `unknown unit "${arg.v}" (${sig.units!.join(`, `)})`)
        if (param.replace(/\?$/, ``) === `format` && !sig.formats!.includes(arg.v)) this.issue(arg.at, `unknown duration format "${arg.v}" (${sig.formats!.join(`, `)})`)
      }
    })
    return T(sig.returns as `dyn`)
  }
}

/** The check of an expression against declared variable types: `types` =
 *  name → a type name (`number`, `list<entry>`, a kind's name…), `kinds` =
 *  the package's record kinds. Answers the expression's static type and
 *  every problem (`column N: …`; a parse error alone when it does not
 *  parse; a bad declaration as `"<name>": unknown type "…"`). */
export function checkExpression(source: string, types: Record<string, string> = {}, kinds: Record<string, CheckKind> = {}): { type: string; issues: string[] } {
  const parsed = parseExpression(source)
  if (`error` in parsed) return { type: `dyn`, issues: [parsed.error] }
  const scope = new Map<string, ExprType>()
  const issues: string[] = []
  for (const [name, spelled] of Object.entries(types)) {
    const t = parseTypeName(spelled, kinds)
    if (!t) issues.push(`"${name}": unknown type "${spelled}"`)
    scope.set(name, t ?? DYN)
  }
  return checkParsed(parsed.ast, scope, issues)
}

/** Check a parsed expression over a typed scope (`issues` = earlier ones). */
export function checkParsed(ast: ExprNode, scope: ReadonlyMap<string, ExprType>, issues: string[] = []): { type: string; issues: string[] } {
  const checker = new Checker()
  const type = checker.check(ast, scope)
  return { type: typeName(type), issues: [...issues, ...checker.issues] }
}

// ---------------------------------------------------------------------------
// The A2UI call form
// ---------------------------------------------------------------------------

/** A call-form argument as the CEL function sees it: a number param takes
 *  a decimal string too (form and URL state arrive as strings), a missing
 *  one is null. */
function callArg(v: unknown, param: string): ExprValue {
  const x = toExprValue(v)
  if (typeof x === `string` && param.replace(/\?$|\.\.\.$/, ``) === `number`) return decimalText(x) ?? x
  return x
}

/** One call-form function with a clock: the named args mapped onto the CEL
 *  function's params; undefined when it fails. `expr` = `{source, vars}`. */
export function callExpressionFunction(name: string, args: Record<string, unknown>, clock: ExpressionClock = {}): unknown {
  if (name === `expr`) {
    if (typeof args.source !== `string`) return undefined
    const vars = typeof args.vars === `object` && args.vars !== null && !Array.isArray(args.vars) ? (args.vars as Record<string, unknown>) : {}
    const out = evalExpression(args.source, vars, clock)
    return `value` in out ? out.value : undefined
  }
  const sig = Object.prototype.hasOwnProperty.call(EXPRESSION_FUNCTIONS, name) ? EXPRESSION_FUNCTIONS[name]! : undefined
  if (!sig?.call) return undefined
  const values: ExprValue[] = []
  sig.call.forEach((key, i) => {
    const param = sig.params[i]!
    const v = args[key]
    if (param.endsWith(`...`)) {
      if (Array.isArray(v)) for (const x of v) values.push(callArg(x, param))
    } else if (v !== undefined || !param.endsWith(`?`)) values.push(callArg(v, param))
  })
  while (values.length > 0 && values.at(-1) === null && sig.params[values.length - 1]?.endsWith(`?`)) values.pop()
  try {
    arity(name, sig, values.length, 0)
    return FUNCTIONS[name]!(values, 0, clock)
  } catch (e) {
    if (e instanceof Failure) return undefined
    throw e
  }
}

/** The call-form names: every function with a `call` spelling + `expr`. */
export const EXPRESSION_CALL_NAMES: readonly string[] = [...Object.keys(EXPRESSION_FUNCTIONS).filter((k) => EXPRESSION_FUNCTIONS[k]!.call), `expr`]
/** The names whose value reads the clock (`ResolveOptions.now` / `zone`). */
export const EXPRESSION_CLOCK_NAMES: readonly string[] = [`today`, `now`, `expr`]

/** The bind-table entries (system clock, UTC; `expressionCallFunctions`
 *  binds another clock). */
export function expressionCallFunctions(clock: ExpressionClock = {}): Record<string, (args: Record<string, unknown>) => unknown> {
  const out: Record<string, (args: Record<string, unknown>) => unknown> = {}
  for (const name of EXPRESSION_CALL_NAMES) out[name] = (args) => callExpressionFunction(name, args, clock)
  return out
}
export const EXPRESSION_CALL_FUNCTIONS = expressionCallFunctions()

export { numberText as expressionNumberText }
