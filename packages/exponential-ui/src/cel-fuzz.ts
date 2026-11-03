// The CEL differential fuzz (eApps hardening, spec §4.2): a SEEDED generator
// of random expressions, vars, type maps and A2UI calls, answered by the TS
// reference (src/cel.ts). `fixtures/cel-fuzz.json` is its committed corpus:
// `cel-fuzz.test.ts` regenerates it and must get the same bytes (so a TS
// change regenerates the corpus), and the Rust core replays it
// (`eui-core/tests/cel_fuzz.rs`), so every disagreement between the twins
// fails CI. Explore more seeds locally with
// `bun run ./scripts/cel-fuzz.ts --seed <n> --count <n> --out <file>` and
// `CEL_FUZZ_CORPUS=<file> cargo test -p eui-core --test cel_fuzz`; every
// mismatch found becomes an `expressions.json` case and a fix in both.

import { callExpressionFunction, checkExpression, evaluateExpression, parseTimestamp, type ExprType } from "./cel"

/** mulberry32: a tiny deterministic PRNG (the corpus must not depend on the runtime). */
export function rng(seed: number): () => number {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

/** The zones the corpus runs in: a UTC offset (minutes east) per instant. */
export const FUZZ_ZONES: Record<string, { offset: number; transitions: [string, number][] }> = {
  utc: { offset: 0, transitions: [] },
  vienna: { offset: 60, transitions: [[`2026-03-29T01:00:00Z`, 120], [`2026-10-25T01:00:00Z`, 60], [`2027-03-28T01:00:00Z`, 120]] },
  newYork: { offset: -300, transitions: [[`2026-03-08T07:00:00Z`, -240], [`2026-11-01T06:00:00Z`, -300]] },
  kolkata: { offset: 330, transitions: [] },
  kiritimati: { offset: 840, transitions: [] },
  baker: { offset: -720, transitions: [] },
}

export function zoneOffset(z: { offset: number; transitions: [string, number][] }): (ms: number) => number {
  const steps = z.transitions.map(([at, o]) => [parseTimestamp(at)!, o] as const)
  return (ms) => {
    let o = z.offset
    for (const [at, off] of steps) if (ms >= at) o = off
    return o
  }
}

const UNITS = [`second`, `minute`, `hour`, `day`, `week`, `month`, `quarter`, `year`]
const CAL_UNITS = [`day`, `week`, `month`, `quarter`, `year`]
const NOWS = [`2026-11-02T09:30:00Z`, `2026-03-29T00:59:59.999Z`, `2026-03-29T01:00:00Z`, `2026-10-25T00:30:00Z`, `2026-11-01T05:59:00Z`, `2024-02-29T23:59:59Z`, `2027-01-03T23:30:00Z`, `2026-12-31T23:30:00Z`]

type Ty = `number` | `string` | `bool` | `date` | `timestamp` | `list` | `nums` | `rows` | `map` | `null` | `any`

class Gen {
  constructor(private readonly r: () => number) {}

  f = () => this.r()
  int = (lo: number, hi: number) => lo + Math.floor(this.r() * (hi - lo + 1))
  chance = (p: number) => this.r() < p
  pick = <T>(xs: readonly T[]): T => xs[Math.floor(this.r() * xs.length)]!

  // ---- values --------------------------------------------------------------

  number(): number {
    switch (this.int(0, 11)) {
      case 0:
        return 0
      case 1:
        return this.int(-3, 12)
      case 2:
        return this.int(-1000, 1000) / 4
      case 3:
        return this.pick([0.1, 0.2, 0.3, 0.5, 1.5, 2.5, -2.5, 0.49999999999999994, 1e-7, 123.456, 7.5, 151.5])
      case 4:
        return this.pick([1e21, -1e21, 1e300, -1e300, 9007199254740993, 2 ** 53, 1.7976931348623157e308, 5e-324, 1e15, 123456789012])
      case 5:
        return this.int(-5, 5) * 60
      default:
        return Math.round(this.r() * 2000 - 1000) / this.pick([1, 10, 100, 3, 7])
    }
  }

  string(): string {
    return this.pick([``, `a`, `b`, `A`, `ab`, `Zeit`, `straße`, `ÄÖÜ`, `x y`, `😀`, `a😀b`, `￿`, ``, `é`, `é`, `h:mm`, `hh:mm`, `day`, `week`, `days`, `2026-11-02`, `null`, `'q'`, `"dq"`, `back\\slash`, `tab\there`, `line\nbreak`, `10`, `9`])
  }

  date(): string {
    switch (this.int(0, 5)) {
      case 0:
        return this.pick([`2026-11-02`, `2026-03-29`, `2026-10-25`, `2024-02-29`, `2025-12-29`, `2027-01-03`, `2026-01-01`, `2026-12-31`, `2021-01-03`, `2020-12-31`])
      case 1:
        return this.pick([`0001-01-01`, `9999-12-31`, `0000-01-01`, `1970-01-01`, `1900-03-01`])
      case 2:
        return this.pick([`2026-02-30`, `2026-13-01`, `2026-1-01`, `2026-11-2`, `20261102`])
      default: {
        const y = this.int(1999, 2031)
        const m = this.int(1, 12)
        const d = this.int(1, 28)
        return `${y}-${String(m).padStart(2, `0`)}-${String(d).padStart(2, `0`)}`
      }
    }
  }

  timestamp(): string {
    switch (this.int(0, 6)) {
      case 0:
        return this.pick([`2026-03-29T01:00:00Z`, `2026-03-29T00:59:59.999Z`, `2026-10-25T00:30:00Z`, `2026-10-25T01:30:00Z`, `2026-11-01T05:30:00Z`, `2026-03-08T07:00:00Z`, `2027-01-03T23:30:00Z`, `2026-11-02T09:30:00Z`])
      case 1:
        return this.pick([`2026-11-02T09:30`, `2026-11-02T09:30:00+05:30`, `2026-11-02T09:30:00-11:00`, `2026-11-02T09:30:00.1Z`, `2026-11-02T09:30:00.123456Z`, `2026-11-02T24:00:00Z`, `2026-11-02T09:60:00Z`, `2026-11-02T09:30:00+24:00`, `9999-12-31T23:59:59.999Z`, `0001-01-01T00:00:00Z`, `0000-12-31T23:00:00-02:00`, `2026-11-02 09:30:00Z`])
      default: {
        const d = this.date()
        const h = String(this.int(0, 23)).padStart(2, `0`)
        const mi = String(this.pick([0, 15, 30, 59])).padStart(2, `0`)
        const s = String(this.pick([0, 0, 1, 59])).padStart(2, `0`)
        const ms = this.chance(0.3) ? `.${this.int(0, 999)}` : ``
        const off = this.pick([`Z`, `Z`, ``, `+01:00`, `-05:00`, `+14:00`, `-12:00`, `+05:30`])
        return `${d}T${h}:${mi}:${s}${ms}${off}`
      }
    }
  }

  /** A random JSON value; `depth` bounds nesting. */
  value(depth = 2): unknown {
    switch (this.int(0, depth > 0 ? 9 : 6)) {
      case 0:
        return null
      case 1:
        return this.chance(0.5)
      case 2:
      case 3:
        return this.number()
      case 4:
        return this.string()
      case 5:
        return this.date()
      case 6:
        return this.timestamp()
      case 7:
      case 8:
        return Array.from({ length: this.int(0, 4) }, () => this.value(depth - 1))
      default:
        return Object.fromEntries(Array.from({ length: this.int(0, 3) }, () => [this.pick([`a`, `b`, `hours`, `note`, `date`, `x`]), this.value(depth - 1)]))
    }
  }

  // ---- literals as source text ----------------------------------------------

  numLit(): string {
    const n = this.number()
    const text = this.chance(0.1) && Number.isInteger(n) && Math.abs(n) < 1e6 ? `${Math.abs(n)}.0` : String(Math.abs(n))
    // A literal never starts with `-`: the generator spells the sign as a unary.
    const lit = text.replace(`e+`, this.pick([`e`, `e+`, `E`]))
    return n < 0 ? `-${lit}` : lit
  }

  strLit(s: string = this.string()): string {
    const q = this.pick([`'`, `"`])
    let out = q
    for (const c of s) {
      if (c === `\\`) out += `\\\\`
      else if (c === q) out += `\\${q}`
      else if (c === `\n`) out += this.pick([`\\n`, `\\n`])
      else if (c === `\t`) out += this.pick([`\\t`, `\t`])
      else out += c
    }
    return out + q
  }
}

/** One var set: values, and the types the checker reads them with. */
interface VarSet {
  vars: Record<string, unknown>
  types: Record<string, ExprType>
}

function varSet(g: Gen): VarSet {
  const n = () => g.number()
  const vars: Record<string, unknown> = {
    n: n(),
    m: n(),
    k: g.int(-3, 12),
    s: g.string(),
    t: g.string(),
    d: g.date(),
    ts: g.timestamp(),
    b: g.chance(0.5),
    nothing: null,
    xs: Array.from({ length: g.int(0, 6) }, n),
    ws: Array.from({ length: g.int(0, 4) }, () => g.string()),
    ds: Array.from({ length: g.int(0, 4) }, () => g.date()),
    rows: Array.from({ length: g.int(0, 5) }, () => {
      const row: Record<string, unknown> = { hours: n(), date: g.date() }
      if (g.chance(0.6)) row.note = g.chance(0.8) ? g.string() : null
      return row
    }),
    entry: { hours: n(), date: g.date(), note: g.chance(0.5) ? g.string() : null, at: g.timestamp(), tags: [g.string(), g.string()] },
    opt: g.chance(0.5) ? { hours: n() } : { hours: n(), note: g.string() },
    meta: g.value(2),
    any: g.value(2),
    maybe: g.chance(0.5) ? n() : null,
  }
  const types: Record<string, ExprType> = {
    n: `number`,
    m: `number`,
    k: `number`,
    s: `string`,
    t: `string`,
    d: `date`,
    ts: `timestamp`,
    b: `bool`,
    nothing: `null`,
    xs: [`number`],
    ws: [`string`],
    ds: [`date`],
    rows: [{ hours: `number`, date: `date`, note: { "?": `string` } }],
    entry: { hours: `number`, date: `date`, note: { "?": `string` }, at: `timestamp`, tags: [`string`] },
    opt: { hours: `number`, note: { "?": `string` } },
    meta: `map`,
    any: `dyn`,
    maybe: { "?": `number` },
  }
  return { vars, types }
}

/** The expression generator: mostly well-typed, sometimes not (`chaos`),
 *  over the var set's names plus a macro variable inside a body. */
class ExprGen {
  private budget = 0
  constructor(
    private readonly g: Gen,
    private readonly chaos: number,
  ) {}

  run(ty: Ty, budget: number): string {
    this.budget = budget
    return this.e(ty, 0, null)
  }

  private leafy(depth: number) {
    return --this.budget <= 0 || depth > 6
  }

  /** `it` = the macro variable in scope: its name and element type. */
  private e(ty: Ty, depth: number, it: { name: string; ty: Ty } | null): string {
    const g = this.g
    if (g.chance(this.chaos)) ty = g.pick<Ty>([`number`, `string`, `bool`, `date`, `timestamp`, `list`, `map`, `null`, `any`])
    const leaf = this.leafy(depth)
    const sub = (t: Ty) => this.e(t, depth + 1, it)
    const paren = (s: string) => (g.chance(0.7) ? `(${s})` : s)
    if (it && it.ty === ty && g.chance(0.4)) return it.name
    if (it && it.ty === `rows` && g.chance(0.3)) {
      if (ty === `number`) return `${it.name}.hours`
      if (ty === `date`) return `${it.name}.date`
      if (ty === `string`) return g.pick([`orElse(${it.name}.note, ${g.strLit()})`, `${it.name}.note`])
      if (ty === `bool`) return `has(${it.name}.note)`
    }
    switch (ty) {
      case `number`: {
        if (leaf) return g.pick([g.numLit(), `n`, `m`, `k`, `entry.hours`, `opt.hours`, `xs[0]`, `size(xs)`, `orElse(maybe, 0)`, `maybe`])
        return g.pick<() => string>([
          () => g.numLit(),
          () => `${paren(sub(`number`))} ${g.pick([`+`, `-`, `*`, `/`, `%`])} ${paren(sub(`number`))}`,
          () => `-${paren(sub(`number`))}`,
          () => `${g.pick([`mul`, `div`, `mod`])}(${sub(`number`)}, ${sub(`number`)})`,
          () => (g.chance(0.5) ? `round(${sub(`number`)})` : `round(${sub(`number`)}, ${g.pick([`0`, `1`, `2`, `10`, `11`, `-1`, `1.5`, sub(`number`)])})`),
          () => `${g.pick([`floor`, `ceil`, `abs`, `hours`])}(${sub(`number`)})`,
          () => `${g.pick([`min`, `max`])}(${Array.from({ length: g.int(0, 4) }, () => sub(`number`)).join(`, `)})`,
          () => `dateDiff(${sub(g.pick<Ty>([`date`, `timestamp`]))}, ${sub(g.pick<Ty>([`date`, `timestamp`]))}, ${this.unit(UNITS)})`,
          () => `${g.pick([`weekday`, `isoWeek`])}(${sub(g.pick<Ty>([`date`, `timestamp`]))})`,
          () => `size(${sub(g.pick<Ty>([`list`, `string`, `map`, `nums`, `rows`]))})`,
          () => `xs[${this.index()}]`,
          () => `${sub(`nums`)}[${this.index()}]`,
          () => `rows[${this.index()}].hours`,
          () => `${paren(sub(`bool`))} ? ${sub(`number`)} : ${sub(`number`)}`,
          () => `orElse(${g.pick([`maybe`, `rows[0].hours`, `entry['hours']`, `meta.a`, `any`])}, ${sub(`number`)})`,
          () => `entry['hours']`,
        ])()
      }
      case `string`: {
        if (leaf) return g.pick([g.strLit(), `s`, `t`, `ws[0]`, `orElse(entry.note, '')`, `entry.note`, `d`])
        return g.pick<() => string>([
          () => g.strLit(),
          () => `${paren(sub(`string`))} + ${paren(sub(`string`))}`,
          () => `formatDuration(${sub(`number`)}, ${g.pick([`'h:mm'`, `'hh:mm'`, `'h'`, `s`])})`,
          () => `${paren(sub(`bool`))} ? ${sub(`string`)} : ${sub(`string`)}`,
          () => `ws[${this.index()}]`,
          () => `orElse(${g.pick([`entry.note`, `opt.note`, `rows[0].note`, `meta['b']`])}, ${sub(`string`)})`,
        ])()
      }
      case `bool`: {
        if (leaf) return g.pick([`true`, `false`, `b`, `has(entry.note)`, `has(opt.note)`, `has(meta.a)`, `maybe != null`, `maybe == null`])
        return g.pick<() => string>([
          () => {
            const t = g.pick<Ty>([`number`, `string`, `date`, `timestamp`, `number`])
            return `${paren(sub(t))} ${g.pick([`<`, `<=`, `>`, `>=`, `==`, `!=`])} ${paren(sub(g.chance(0.85) ? t : g.pick<Ty>([`number`, `string`, `null`, `list`])))}`
          },
          () => `${paren(sub(`bool`))} ${g.pick([`&&`, `||`])} ${paren(sub(`bool`))}`,
          () => `!${paren(sub(`bool`))}`,
          () => `${paren(sub(g.pick<Ty>([`list`, `map`, `any`, `null`])))} ${g.pick([`==`, `!=`])} ${paren(sub(g.pick<Ty>([`list`, `map`, `any`, `null`])))}`,
          () => this.macro(g.pick([`all`, `exists`]), depth, it),
          () => `has(${g.pick([`entry`, `opt`, `meta`, `any`, `rows[0]`, `nothing`])}.${g.pick([`note`, `hours`, `a`, `b`])})`,
          () => `${g.pick([`maybe`, `opt.note`, `entry.note`, `meta.a`])} ${g.pick([`==`, `!=`])} null`,
          () => `${g.pick([`maybe`, `opt.note`])} != null && ${g.pick([`maybe > 1`, `opt.note == 'a'`, `size(opt.note) > 0`])}`,
        ])()
      }
      case `date`: {
        if (leaf) return g.pick([g.strLit(g.date()), `d`, `entry.date`, `ds[0]`, `today()`])
        return g.pick<() => string>([
          () => g.strLit(g.date()),
          () => `today()`,
          () => `dateAdd(${sub(`date`)}, ${g.pick([sub(`number`), String(g.int(-40, 40)), `1.5`, `100000000`])}, ${this.unit(g.chance(0.85) ? CAL_UNITS : UNITS)})`,
          () => `${g.pick([`startOf`, `endOf`])}(${sub(`date`)}, ${this.unit(g.chance(0.9) ? CAL_UNITS : UNITS)})`,
          () => `${paren(sub(`bool`))} ? ${sub(`date`)} : ${sub(`date`)}`,
          () => `rows[${this.index()}].date`,
        ])()
      }
      case `timestamp`: {
        if (leaf) return g.pick([g.strLit(g.timestamp()), `ts`, `entry.at`, `now()`])
        return g.pick<() => string>([
          () => g.strLit(g.timestamp()),
          () => `now()`,
          () => `dateAdd(${sub(`timestamp`)}, ${g.pick([sub(`number`), String(g.int(-50, 50)), `-1`, `1e10`])}, ${this.unit(UNITS)})`,
          () => `${g.pick([`startOf`, `endOf`])}(${sub(`timestamp`)}, ${this.unit(g.chance(0.9) ? CAL_UNITS : UNITS)})`,
          () => `${paren(sub(`bool`))} ? ${sub(`timestamp`)} : ${sub(`timestamp`)}`,
        ])()
      }
      case `nums`: {
        if (leaf) return g.pick([`xs`, `[]`, `[1, 2, 3]`, `[${g.numLit()}]`])
        return g.pick<() => string>([
          () => `[${Array.from({ length: g.int(0, 4) }, () => sub(`number`)).join(`, `)}${g.chance(0.1) ? `,` : ``}]`,
          () => `${paren(sub(`nums`))} + ${paren(sub(`nums`))}`,
          () => (it ? `xs` : g.chance(0.5) ? this.comp(`map`, `xs`, `number`, `number`, depth) : this.comp(`map`, `rows`, `rows`, `number`, depth)),
          () => (it ? `xs` : this.comp(`filter`, `xs`, `number`, `bool`, depth)),
          () => (it ? `xs` : `rows.map(r, r.hours)`),
        ])()
      }
      case `rows`:
        return g.pick([`rows`, it ? `rows` : `rows.filter(r, r.hours > ${g.numLit()})`, `[entry]`])
      case `list`:
        if (leaf) return g.pick([`xs`, `ws`, `ds`, `rows`, `[]`, `entry.tags`])
        return g.pick<() => string>([
          () => `[${Array.from({ length: g.int(0, 4) }, () => sub(g.pick<Ty>([`number`, `string`, `bool`, `null`, `date`]))).join(`, `)}]`,
          () => sub(`nums`),
          () => {
            if (it) return `ws`
            const [src, elem] = g.pick<[string, Ty]>([[`ws`, `string`], [`ds`, `date`], [`rows`, `rows`], [`xs`, `number`], [`any`, `any`], [`meta`, `any`]])
            return this.comp(g.pick([`map`, `filter`]), src, elem, g.pick<Ty>([`string`, `number`, `bool`, `date`]), depth)
          },
          () => `${paren(sub(`list`))} + ${paren(sub(`list`))}`,
        ])()
      case `map`:
        return g.pick([`meta`, `entry`, `opt`, `rows[0]`, `any`, `meta.a`, `entry['tags']`])
      case `null`:
        return g.pick([`null`, `nothing`, `meta.zz`, `entry.missing`, `opt.note`])
      case `any`:
        return g.pick([`any`, `meta`, `meta.a`, `meta['b']`, `any[0]`, `any.x`, `nothing.x`, `nothing[0]`, `any.a.b`])
    }
  }

  /** A comprehension over `src` (elements typed `elem`); the variable may
   *  shadow a var (`n`). */
  private comp(fn: string, src: string, elem: Ty, body: Ty, depth: number): string {
    const name = this.g.pick([`x`, `r`, `it`, `n`])
    return `${src}.${fn}(${name}, ${this.e(fn === `filter` ? `bool` : body, depth + 1, { name, ty: elem })})`
  }

  private macro(fn: string, depth: number, it: { name: string; ty: Ty } | null): string {
    const g = this.g
    if (it) return `b`
    const [src, elem] = g.pick<[string, Ty]>([
      [`xs`, `number`],
      [`rows`, `rows`],
      [`ws`, `string`],
      [`ds`, `date`],
      [`[]`, `any`],
      [`any`, `any`],
      [`[1, 'a', null]`, `any`],
    ])
    const name = g.pick([`x`, `x`, `n`])
    return `${src}.${fn}(${name}, ${this.e(`bool`, depth + 1, { name, ty: elem })})`
  }

  private unit(units: readonly string[]): string {
    const g = this.g
    if (g.chance(0.05)) return g.pick([`'days'`, `'Day'`, `s`, `''`, `1`])
    return `'${g.pick(units)}'`
  }

  private index(): string {
    if (this.g.chance(0.15)) return this.g.numLit()
    return this.g.pick([`0`, `1`, `2`, `-1`, `1.5`, `k`, `100`, `1e21`, `'a'`, `0.0`, `size(xs) - 1`])
  }
}

/** Break a source text a little: drop, duplicate or insert one character. */
function mutate(g: Gen, src: string): string {
  const cs = Array.from(src)
  const i = g.int(0, cs.length)
  switch (g.int(0, 3)) {
    case 0:
      cs.splice(i, 1)
      break
    case 1:
      cs.splice(i, 0, cs[i] ?? `)`)
      break
    case 2:
      cs.splice(i, 0, g.pick([`(`, `)`, `[`, `]`, `.`, `,`, `?`, `:`, `!`, `-`, `'`, `\\`, `#`, `é`, `1`, ` `, `&`, `|`, `=`]))
      break
    default:
      cs.splice(i, 1, g.pick([`+`, `*`, `==`, `.map(x, x)`, `[0]`, `.a`]))
  }
  return cs.join(``)
}

const SOUP = [`(`, `)`, `[`, `]`, `.`, `,`, `?`, `:`, `!`, `-`, `+`, `*`, `/`, `%`, `<`, `<=`, `==`, `!=`, `&&`, `||`, `=`, `&`, `|`, `x`, `xs`, `n`, `s`, `entry`, `.hours`, `map`, `has`, `size`, `min`, `orElse`, `true`, `null`, `1`, `0`, `1.5`, `1e3`, `1E-2`, `.5`, `5.`, `1e`, `1e400`, `0x10`, `007`, `'a'`, `"😀"`, `'\\q'`, `'\\n'`, `'`, `é`, `😀`, `#`, `$`, ` `, `\t`, `\n`, `in`, `int(`, `.map(x, x)`, `[0]`]

/** A random token sequence: parse errors and their positions. */
function soup(g: Gen): string {
  return Array.from({ length: g.int(1, 12) }, () => g.pick(SOUP)).join(g.pick([``, ` `]))
}

/** Expressions at the caps: length, nesting, steps. */
function capCase(g: Gen): string {
  switch (g.int(0, 7)) {
    case 0: {
      const n = g.int(29, 34)
      return `${`(`.repeat(n)}1${`)`.repeat(n)}`
    }
    case 1: {
      const n = g.int(29, 34)
      return `${`-`.repeat(n)}1`
    }
    case 2: {
      const n = g.int(29, 34)
      return `${`!`.repeat(n)}true`
    }
    case 3: {
      const n = g.int(14, 18)
      return `${`true ? `.repeat(n)}1${` : 0`.repeat(n)}`
    }
    case 4:
      return `1 + ${Array.from({ length: g.int(495, 502) }, () => `1`).join(`+`)}`.slice(0, g.int(995, 1005))
    case 5:
      return `'${`é`.repeat(g.int(996, 1000))}'`
    case 6: {
      const n = g.int(28, 33)
      return `${`[`.repeat(n)}1${`]`.repeat(n)}`
    }
    default: {
      const n = g.int(10, 17)
      return `${`min(`.repeat(n)}1${`)`.repeat(n)}`
    }
  }
}

export interface FuzzCase {
  expr: string
  /** The var set it reads (`varSets[set]`). */
  set: number
  now: string
  zone: string
  maxSteps?: number
  eval: { value: unknown } | { error: string }
  check: { type: ExprType } | { error: string }
}

export interface FuzzCall {
  call: string
  args: Record<string, unknown>
  now: string
  zone: string
  /** `outside` = not an expression function; `none` = no value. */
  result: { value: unknown } | { none: true } | { outside: true }
}

export interface FuzzCorpus {
  $comment: string
  seed: number
  zones: typeof FUZZ_ZONES
  varSets: VarSet[]
  cases: FuzzCase[]
  calls: FuzzCall[]
}

const CALL_FNS = [`mul`, `div`, `mod`, `round`, `floor`, `ceil`, `abs`, `min`, `max`, `today`, `now`, `dateAdd`, `dateDiff`, `startOf`, `endOf`, `weekday`, `isoWeek`, `hours`, `formatDuration`, `orElse`, `expr`, `size`, `percent`]
const CALL_ARGS = [`a`, `b`, `value`, `digits`, `values`, `date`, `n`, `unit`, `minutes`, `hours`, `format`, `default`]

/** JSON's view of a value (what the Rust twin reads back): -0 is 0. */
const json = (v: unknown): unknown => (v === undefined ? undefined : JSON.parse(JSON.stringify(v)))

export function generateCorpus(seed: number, count: number, callCount = Math.floor(count / 5)): FuzzCorpus {
  const g = new Gen(rng(seed))
  const varSets = Array.from({ length: 12 }, () => varSet(g))
  const offsets = Object.fromEntries(Object.entries(FUZZ_ZONES).map(([k, z]) => [k, zoneOffset(z)]))
  const cases: FuzzCase[] = []
  for (let i = 0; i < count; i++) {
    const set = g.int(0, varSets.length - 1)
    const roll = g.f()
    let expr: string
    if (roll < 0.04) expr = capCase(g)
    else if (roll < 0.08) expr = soup(g)
    else {
      const ty = g.pick<Ty>([`number`, `number`, `number`, `string`, `bool`, `bool`, `date`, `timestamp`, `nums`, `list`, `map`, `any`])
      expr = new ExprGen(g, roll < 0.25 ? 0.15 : roll < 0.3 ? 0.5 : 0).run(ty, g.int(1, 14))
      if (g.chance(0.06)) expr = mutate(g, expr)
    }
    const now = g.pick(NOWS)
    const zone = g.pick(Object.keys(FUZZ_ZONES))
    const maxSteps = g.chance(0.12) ? g.int(1, 40) : undefined
    const env = { now: parseTimestamp(now)!, offset: offsets[zone]!, ...(maxSteps === undefined ? {} : { maxSteps }) }
    const ev = evaluateExpression(expr, varSets[set]!.vars, env)
    const ck = checkExpression(expr, varSets[set]!.types)
    cases.push({ expr, set, now, zone, ...(maxSteps === undefined ? {} : { maxSteps }), eval: json(ev) as FuzzCase[`eval`], check: json(ck) as FuzzCase[`check`] })
  }
  const calls: FuzzCall[] = []
  for (let i = 0; i < callCount; i++) {
    const call = g.pick(CALL_FNS)
    const args: Record<string, unknown> = {}
    for (const k of CALL_ARGS) if (g.chance(0.35)) args[k] = g.chance(0.5) ? g.value(1) : k === `unit` ? g.pick(UNITS) : k === `values` ? Array.from({ length: g.int(0, 3) }, () => g.number()) : g.value(0)
    if (call === `expr`) {
      const set = varSets[g.int(0, varSets.length - 1)]!
      args.expr = g.chance(0.9) ? new ExprGen(g, 0.1).run(g.pick<Ty>([`number`, `string`, `bool`, `date`]), g.int(1, 8)) : g.value(0)
      if (g.chance(0.8)) args.vars = set.vars
    }
    const now = g.pick(NOWS)
    const zone = g.pick(Object.keys(FUZZ_ZONES))
    const got = callExpressionFunction(call, json(args) as Record<string, unknown>, { now: parseTimestamp(now)!, offset: offsets[zone]! })
    const result: FuzzCall[`result`] = got === undefined ? { outside: true } : got.value === undefined ? { none: true } : { value: json(got.value) }
    calls.push({ call, args: json(args) as Record<string, unknown>, now, zone, result })
  }
  return {
    $comment: `The CEL differential fuzz corpus (eApps, spec §4.2), GENERATED by src/cel-fuzz.ts from \`seed\` (never edit by hand: \`bun run ./scripts/cel-fuzz.ts --write\`). Each case is a random expression over var set \`varSets[set]\` at \`now\` in zone \`zones[zone]\` (a UTC offset per instant, like expressions.json), with an optional \`maxSteps\`; \`eval\` = the TS reference's value | error, \`check\` = its type | error over the set's \`types\`. Each call is the A2UI form \`{call, args}\` → value | none | outside (not an expression function). The TS test regenerates the corpus byte for byte; the Rust core (eui-core tests/cel_fuzz.rs) replays it, so a disagreement between the twins fails CI. A mismatch found here becomes an expressions.json case and a fix in both twins.`,
    seed,
    zones: FUZZ_ZONES,
    varSets: json(varSets) as VarSet[],
    cases,
    calls,
  }
}

/** The committed corpus's parameters. */
export const CORPUS_SEED = 191
export const CORPUS_COUNT = 1200
