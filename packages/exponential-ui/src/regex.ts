// VAPP-289 (docs/round-9-contract.md §9.3): the A2UI 1.0 regex dialect. ONE
// hand-written parser + safety checker + backtracking matcher (no platform
// RegExp anywhere), mirrored by the Rust core (eui-core src/regex.rs).
// fixtures/regex.json is the spec: its `$comment` fixes the reasons, the
// error offsets and the step count both cores share.

import limitsJson from "../catalog/limits.json" with { type: "json" }

export interface RegexOutcome { valid: boolean }

const LIMITS = limitsJson.regex
const ISSUES = limitsJson.issues

// ---------------------------------------------------------------- sets

/** A code point set: sorted, merged, inclusive ranges [lo0, hi0, lo1, hi1, …]. */
type CpSet = number[]

const MAX_CP = 0x10ffff

function normalize(ranges: [number, number][]): CpSet {
  const sorted = [...ranges].sort((a, b) => a[0] - b[0])
  const out: CpSet = []
  for (const [lo, hi] of sorted) {
    if (out.length > 0 && lo <= out[out.length - 1]! + 1) out[out.length - 1] = Math.max(out[out.length - 1]!, hi)
    else out.push(lo, hi)
  }
  return out
}

function complement(set: CpSet): CpSet {
  const out: CpSet = []
  let next = 0
  for (let i = 0; i < set.length; i += 2) {
    if (set[i]! > next) out.push(next, set[i]! - 1)
    next = set[i + 1]! + 1
  }
  if (next <= MAX_CP) out.push(next, MAX_CP)
  return out
}

const pairs = (set: CpSet): [number, number][] => {
  const out: [number, number][] = []
  for (let i = 0; i < set.length; i += 2) out.push([set[i]!, set[i + 1]!])
  return out
}
const union = (sets: CpSet[]): CpSet => normalize(sets.flatMap(pairs))

function has(set: CpSet, c: number): boolean {
  let lo = 0
  let hi = set.length / 2 - 1
  while (lo <= hi) {
    const mid = (lo + hi) >> 1
    if (c < set[2 * mid]!) hi = mid - 1
    else if (c > set[2 * mid + 1]!) lo = mid + 1
    else return true
  }
  return false
}

function intersects(a: CpSet, b: CpSet): boolean {
  let i = 0
  let j = 0
  while (i < a.length && j < b.length) {
    if (a[i + 1]! < b[j]!) i += 2
    else if (b[j + 1]! < a[i]!) j += 2
    else return true
  }
  return false
}

const DIGIT: CpSet = [0x30, 0x39]
const WORD: CpSet = normalize([[0x30, 0x39], [0x41, 0x5a], [0x5f, 0x5f], [0x61, 0x7a]])
const SPACE: CpSet = normalize([[0x09, 0x0d], [0x20, 0x20], [0xa0, 0xa0], [0x1680, 0x1680], [0x2000, 0x200a], [0x2028, 0x2029], [0x202f, 0x202f], [0x205f, 0x205f], [0x3000, 0x3000], [0xfeff, 0xfeff]])
const DOT: CpSet = complement(normalize([[0x0a, 0x0a], [0x0d, 0x0d], [0x2028, 0x2029]]))
const CLASS_ESCAPES: Record<string, CpSet> = { d: DIGIT, D: complement(DIGIT), w: WORD, W: complement(WORD), s: SPACE, S: complement(SPACE) }
const isWordCp = (c: number | undefined): boolean => c !== undefined && has(WORD, c)

// ---------------------------------------------------------------- AST

type QuantNode = { t: `quant`; atom: Node; min: number; max: number; lazy: boolean }
type Node =
  | { t: `set`; set: CpSet }
  | { t: `bol` }
  | { t: `eol` }
  | { t: `wordb`; neg: boolean }
  | { t: `look`; neg: boolean; child: Node }
  | { t: `group`; child: Node }
  | QuantNode
  | { t: `concat`; items: Node[] }
  | { t: `alt`; branches: Node[] }

class SyntaxFail {
  constructor(readonly reason: string, readonly at: number) {}
}

const SYNTAX = new Set([...`^$\\.*+?()[]{}|/`].map((c) => c.codePointAt(0)!))
const cp = (c: string): number => c.codePointAt(0)!
const isDigit = (c: number | undefined): boolean => c !== undefined && c >= 0x30 && c <= 0x39
const hexVal = (c: number | undefined): number => {
  if (c === undefined) return -1
  if (c >= 0x30 && c <= 0x39) return c - 0x30
  if (c >= 0x41 && c <= 0x46) return c - 0x37
  if (c >= 0x61 && c <= 0x66) return c - 0x57
  return -1
}

/** A quantifier bound: decimal digits. Values saturate at 2^32 − 1; the
 *  order check compares the exact digit strings. */
const SATURATE = 0xffffffff
const stripZeros = (digits: string): string => {
  let k = 0
  while (k < digits.length - 1 && digits[k] === `0`) k++
  return digits.slice(k)
}
const boundValue = (digits: string): number => {
  const d = stripZeros(digits)
  return d.length > 10 ? SATURATE : Math.min(Number(d), SATURATE)
}
const boundGreater = (a: string, b: string): boolean => {
  const x = stripZeros(a)
  const y = stripZeros(b)
  return x.length !== y.length ? x.length > y.length : x > y
}

function codePoints(s: string): number[] {
  const out: number[] = []
  for (let i = 0; i < s.length; ) {
    const c = s.codePointAt(i)!
    out.push(c)
    i += c > 0xffff ? 2 : 1
  }
  return out
}

function parse(p: number[]): Node {
  const n = p.length
  let i = 0

  // `{` at j as `{n}`, `{n,}` or `{n,m}` → its bounds and end, else null.
  const braces = (j: number): { min: string; max: string | null; end: number } | null => {
    let k = j + 1
    let min = ``
    while (isDigit(p[k])) min += String.fromCodePoint(p[k++]!)
    if (min === ``) return null
    if (p[k] === cp(`}`)) return { min, max: min, end: k + 1 }
    if (p[k] !== cp(`,`)) return null
    k++
    let max = ``
    while (isDigit(p[k])) max += String.fromCodePoint(p[k++]!)
    if (p[k] !== cp(`}`)) return null
    return { min, max: max === `` ? null : max, end: k + 1 }
  }
  const quantifierAhead = (): boolean => {
    const c = p[i]
    return c === cp(`*`) || c === cp(`+`) || c === cp(`?`) || (c === cp(`{`) && braces(i) !== null)
  }

  const disjunction = (): Node => {
    const branches: Node[] = [alternative()]
    while (p[i] === cp(`|`)) {
      i++
      branches.push(alternative())
    }
    return branches.length === 1 ? branches[0]! : { t: `alt`, branches }
  }
  const alternative = (): Node => {
    const items: Node[] = []
    while (i < n && p[i] !== cp(`|`) && p[i] !== cp(`)`)) items.push(term())
    return { t: `concat`, items }
  }
  const closeGroup = (open: number) => {
    if (p[i] !== cp(`)`)) throw new SyntaxFail(`unmatched (`, open)
    i++
  }

  const term = (): Node => {
    const c = p[i]
    let assertion: Node | null = null
    if (c === cp(`^`)) {
      i++
      assertion = { t: `bol` }
    } else if (c === cp(`$`)) {
      i++
      assertion = { t: `eol` }
    } else if (c === cp(`\\`) && (p[i + 1] === cp(`b`) || p[i + 1] === cp(`B`))) {
      assertion = { t: `wordb`, neg: p[i + 1] === cp(`B`) }
      i += 2
    } else if (c === cp(`(`) && p[i + 1] === cp(`?`) && (p[i + 2] === cp(`=`) || p[i + 2] === cp(`!`))) {
      const open = i
      const neg = p[i + 2] === cp(`!`)
      i += 3
      const child = disjunction()
      closeGroup(open)
      assertion = { t: `look`, neg, child }
    }
    if (assertion) {
      if (quantifierAhead()) throw new SyntaxFail(`quantifier on assertion`, i)
      return assertion
    }
    const atom = parseAtom()
    const q = p[i]
    let min: number
    let max: number
    if (q === cp(`*`)) [min, max] = [0, Infinity]
    else if (q === cp(`+`)) [min, max] = [1, Infinity]
    else if (q === cp(`?`)) [min, max] = [0, 1]
    else if (q === cp(`{`) && braces(i)) {
      const b = braces(i)!
      if (b.max !== null && boundGreater(b.min, b.max)) throw new SyntaxFail(`quantifier range out of order`, i)
      min = boundValue(b.min)
      max = b.max === null ? Infinity : boundValue(b.max)
      i = b.end - 1
    } else return atom
    i++
    let lazy = false
    if (p[i] === cp(`?`)) {
      lazy = true
      i++
    }
    return { t: `quant`, atom, min, max, lazy }
  }

  const parseAtom = (): Node => {
    const at = i
    const c = p[i]!
    if (c === cp(`.`)) {
      i++
      return { t: `set`, set: DOT }
    }
    if (c === cp(`(`)) {
      if (p[i + 1] === cp(`?`)) {
        if (p[i + 2] === cp(`:`)) i += 3
        else if (p[i + 2] === cp(`<`)) throw new SyntaxFail(p[i + 3] === cp(`=`) || p[i + 3] === cp(`!`) ? `lookbehind` : `named group`, at)
        else throw new SyntaxFail(`inline flags`, at)
      } else i++
      const child = disjunction()
      closeGroup(at)
      return { t: `group`, child }
    }
    if (c === cp(`[`)) return parseClass()
    if (c === cp(`]`)) throw new SyntaxFail(`unmatched ]`, at)
    if (c === cp(`}`)) throw new SyntaxFail(`lone }`, at)
    if (c === cp(`{`)) throw new SyntaxFail(braces(i) ? `nothing to repeat` : `lone {`, at)
    if (c === cp(`*`) || c === cp(`+`) || c === cp(`?`)) throw new SyntaxFail(`nothing to repeat`, at)
    if (c === cp(`\\`)) {
      const e = p[i + 1]
      if (e === undefined) throw new SyntaxFail(`invalid escape`, at)
      const ch = String.fromCodePoint(e)
      if (ch in CLASS_ESCAPES) {
        i += 2
        return { t: `set`, set: CLASS_ESCAPES[ch]! }
      }
      if ((e >= cp(`1`) && e <= cp(`9`)) || e === cp(`k`)) throw new SyntaxFail(`backreference`, at)
      return single(charEscape(false))
    }
    i++
    return single(c)
  }
  const single = (c: number): Node => ({ t: `set`, set: [c, c] })

  // `\` at i (not a class escape, not \b \B, not a backreference) → its code point.
  const charEscape = (inClass: boolean): number => {
    const at = i
    const e = p[i + 1]
    if (e === undefined) throw new SyntaxFail(`invalid escape`, at)
    const simple: Record<string, number> = { t: 9, n: 10, v: 11, f: 12, r: 13 }
    const ch = String.fromCodePoint(e)
    if (ch in simple) {
      i += 2
      return simple[ch]!
    }
    if (ch === `0`) {
      if (isDigit(p[i + 2])) throw new SyntaxFail(`octal escape`, at)
      i += 2
      return 0
    }
    if (ch === `c`) throw new SyntaxFail(`control escape`, at)
    if (ch === `p` || ch === `P`) throw new SyntaxFail(`property escape`, at)
    if (ch === `x`) {
      const h1 = hexVal(p[i + 2])
      const h2 = hexVal(p[i + 3])
      if (h1 < 0 || h2 < 0) throw new SyntaxFail(`invalid escape`, at)
      i += 4
      return h1 * 16 + h2
    }
    if (ch === `u`) return unicodeEscape()
    if (SYNTAX.has(e) || (inClass && ch === `-`)) {
      i += 2
      return e
    }
    throw new SyntaxFail(`invalid escape`, at)
  }
  const hex4 = (j: number): number => {
    let v = 0
    for (let k = 0; k < 4; k++) {
      const h = hexVal(p[j + k])
      if (h < 0) return -1
      v = v * 16 + h
    }
    return v
  }
  const unicodeEscape = (): number => {
    const at = i
    if (p[i + 2] === cp(`{`)) {
      let k = i + 3
      let v = 0
      let digits = 0
      while (hexVal(p[k]) >= 0) {
        v = Math.min(v * 16 + hexVal(p[k]), MAX_CP + 1)
        digits++
        k++
      }
      if (digits === 0 || p[k] !== cp(`}`)) throw new SyntaxFail(`invalid escape`, at)
      if (v > MAX_CP) throw new SyntaxFail(`code point out of range`, at)
      i = k + 1
      return v
    }
    const v = hex4(i + 2)
    if (v < 0) throw new SyntaxFail(`invalid escape`, at)
    i += 6
    if (v >= 0xd800 && v <= 0xdbff && p[i] === cp(`\\`) && p[i + 1] === cp(`u`)) {
      const t = hex4(i + 2)
      if (t >= 0xdc00 && t <= 0xdfff) {
        i += 6
        return 0x10000 + ((v - 0xd800) << 10) + (t - 0xdc00)
      }
    }
    return v
  }

  type ClassAtom = { cp: number } | { set: CpSet }
  const classAtom = (): ClassAtom => {
    const at = i
    if (p[i] !== cp(`\\`)) return { cp: p[i++]! }
    const e = p[i + 1]
    if (e === undefined) throw new SyntaxFail(`invalid escape`, at)
    const ch = String.fromCodePoint(e)
    if (ch in CLASS_ESCAPES) {
      i += 2
      return { set: CLASS_ESCAPES[ch]! }
    }
    if (ch === `b`) {
      i += 2
      return { cp: 8 }
    }
    if (e >= cp(`1`) && e <= cp(`9`)) throw new SyntaxFail(`octal escape`, at)
    return { cp: charEscape(true) }
  }
  const parseClass = (): Node => {
    const open = i
    i++
    let neg = false
    if (p[i] === cp(`^`)) {
      neg = true
      i++
    }
    const ranges: [number, number][] = []
    for (;;) {
      if (i >= n) throw new SyntaxFail(`unmatched [`, open)
      if (p[i] === cp(`]`)) {
        i++
        break
      }
      const start = i
      const a = classAtom()
      if (p[i] === cp(`-`) && i + 1 < n && p[i + 1] !== cp(`]`)) {
        i++
        const b = classAtom()
        if (!(`cp` in a) || !(`cp` in b)) throw new SyntaxFail(`class escape in range`, start)
        if (a.cp > b.cp) throw new SyntaxFail(`range out of order`, start)
        ranges.push([a.cp, b.cp])
      } else if (`cp` in a) ranges.push([a.cp, a.cp])
      else ranges.push(...pairs(a.set))
    }
    const set = normalize(ranges)
    return { t: `set`, set: neg ? complement(set) : set }
  }

  const root = disjunction()
  if (i < n) throw new SyntaxFail(`unmatched )`, i)
  return root
}

// ---------------------------------------------------------------- safety (R2–R4)

const children = (n: Node): Node[] => {
  switch (n.t) {
    case `look`:
    case `group`:
      return [n.child]
    case `quant`:
      return [n.atom]
    case `concat`:
      return n.items
    case `alt`:
      return n.branches
    default:
      return []
  }
}
const isZeroWidth = (n: Node): boolean => n.t === `bol` || n.t === `eol` || n.t === `wordb` || n.t === `look`
const isUnbounded = (n: Node): n is QuantNode => n.t === `quant` && n.max === Infinity

function chars(n: Node): CpSet {
  if (n.t === `set`) return n.set
  if (isZeroWidth(n)) return []
  return union(children(n).map(chars))
}
function nullable(n: Node): boolean {
  switch (n.t) {
    case `set`:
      return false
    case `quant`:
      return n.min === 0 || nullable(n.atom)
    case `group`:
      return nullable(n.child)
    case `concat`:
      return n.items.every(nullable)
    case `alt`:
      return n.branches.some(nullable)
    default:
      return true
  }
}
function first(n: Node): CpSet {
  switch (n.t) {
    case `set`:
      return n.set
    case `quant`:
      return first(n.atom)
    case `group`:
      return first(n.child)
    case `alt`:
      return union(n.branches.map(first))
    case `concat`: {
      const sets: CpSet[] = []
      for (const item of n.items) {
        sets.push(first(item))
        if (!nullable(item)) break
      }
      return union(sets)
    }
    default:
      return []
  }
}
function unwrap(n: Node): Node {
  for (;;) {
    if (n.t === `group`) n = n.child
    else if (n.t === `concat` && n.items.length === 1) n = n.items[0]!
    else if (n.t === `alt` && n.branches.length === 1) n = n.branches[0]!
    else return n
  }
}
const containsUnbounded = (n: Node): boolean => isUnbounded(n) || children(n).some(containsUnbounded)

const rule2 = (n: Node): boolean => n.t === `quant` && n.max > 1 && containsUnbounded(n.atom)
function rule3(n: Node): boolean {
  if (!isUnbounded(n)) return false
  const u = unwrap(n.atom)
  if (u.t !== `alt`) return false
  const firsts = u.branches.map(first)
  for (let i = 0; i < firsts.length; i++) for (let j = i + 1; j < firsts.length; j++) if (intersects(firsts[i]!, firsts[j]!)) return true
  return false
}
function rule4(n: Node): boolean {
  if (n.t !== `concat`) return false
  const items = n.items
  for (let i = 0; i < items.length; i++) {
    if (!isUnbounded(unwrap(items[i]!))) continue
    for (let j = i + 1; j < items.length; j++) {
      if (isUnbounded(unwrap(items[j]!)) && intersects(chars(items[i]!), chars(items[j]!))) return true
      if (!nullable(items[j]!)) break
    }
  }
  return false
}
const anyNode = (n: Node, f: (n: Node) => boolean): boolean => f(n) || children(n).some((c) => anyNode(c, f))

/** The lowest rule the pattern breaks, or null. */
function safety(root: Node): 2 | 3 | 4 | null {
  if (anyNode(root, rule2)) return 2
  if (anyNode(root, rule3)) return 3
  if (anyNode(root, rule4)) return 4
  return null
}

// ---------------------------------------------------------------- matcher

type Cont =
  | null
  | { k: `seq`; items: Node[]; i: number; next: Cont }
  | { k: `rep`; q: QuantNode; min: number; max: number; x: number; next: Cont }
type Goal = { m: true; node: Node; pos: number; cont: Cont } | { m: false; pos: number; cont: Cont }

class TooExpensive {}

interface Ctx { v: number[]; steps: number; maxSteps: number }

/** ECMAScript RepeatMatcher(min, max, greedy, x, c): the next goal; the
 *  other choice goes on the backtrack stack. */
function repeat(q: QuantNode, min: number, max: number, x: number, next: Cont, stack: Goal[]): Goal {
  if (max === 0) return { m: false, pos: x, cont: next }
  const tryAtom: Goal = { m: true, node: q.atom, pos: x, cont: { k: `rep`, q, min, max, x, next } }
  if (min !== 0) return tryAtom
  if (q.lazy) {
    stack.push(tryAtom)
    return { m: false, pos: x, cont: next }
  }
  stack.push({ m: false, pos: x, cont: next })
  return tryAtom
}

/** Does `root` match at `start` (the continuation = success)? Depth-first
 *  in ECMAScript order with an explicit backtrack stack; a lookahead runs
 *  its own (atomic) sub-match. */
function run(root: Node, start: number, ctx: Ctx): boolean {
  const { v } = ctx
  const stack: Goal[] = []
  let g: Goal | undefined = { m: true, node: root, pos: start, cont: null }
  while (g) {
    if (g.m) {
      if (++ctx.steps > ctx.maxSteps) throw new TooExpensive()
      const { node, pos, cont }: { node: Node; pos: number; cont: Cont } = g
      switch (node.t) {
        case `set`:
          g = pos < v.length && has(node.set, v[pos]!) ? { m: false, pos: pos + 1, cont } : stack.pop()
          break
        case `bol`:
          g = pos === 0 ? { m: false, pos, cont } : stack.pop()
          break
        case `eol`:
          g = pos === v.length ? { m: false, pos, cont } : stack.pop()
          break
        case `wordb`:
          g = (isWordCp(v[pos - 1]) !== isWordCp(v[pos])) !== node.neg ? { m: false, pos, cont } : stack.pop()
          break
        case `look`:
          g = run(node.child, pos, ctx) !== node.neg ? { m: false, pos, cont } : stack.pop()
          break
        case `group`:
          g = { m: true, node: node.child, pos, cont }
          break
        case `concat`:
          g = { m: false, pos, cont: { k: `seq`, items: node.items, i: 0, next: cont } }
          break
        case `alt`:
          for (let b = node.branches.length - 1; b >= 1; b--) stack.push({ m: true, node: node.branches[b]!, pos, cont })
          g = { m: true, node: node.branches[0]!, pos, cont }
          break
        case `quant`:
          g = repeat(node, node.min, node.max, pos, cont, stack)
          break
      }
    } else {
      const { pos, cont }: { pos: number; cont: Cont } = g
      if (cont === null) return true
      if (cont.k === `seq`) {
        g = cont.i === cont.items.length ? { m: false, pos, cont: cont.next } : { m: true, node: cont.items[cont.i]!, pos, cont: { ...cont, i: cont.i + 1 } }
      } else if (cont.min === 0 && pos === cont.x) g = stack.pop()
      else g = repeat(cont.q, cont.min === 0 ? 0 : cont.min - 1, cont.max === Infinity ? Infinity : cont.max - 1, pos, cont.next, stack)
    }
  }
  return false
}

// ---------------------------------------------------------------- entry

type Checked = { root: Node } | { error: string }

function compile(pattern: unknown): Checked {
  if (typeof pattern !== `string`) return { error: syntaxMessage(`pattern is not a string`, 0) }
  const p = codePoints(pattern)
  if (p.length > LIMITS.maxPatternCodePoints) return { error: ISSUES.regexPatternLength }
  let root: Node
  try {
    root = parse(p)
  } catch (e) {
    if (e instanceof SyntaxFail) return { error: syntaxMessage(e.reason, e.at) }
    throw e
  }
  const rule = safety(root)
  if (rule !== null) return { error: ISSUES.regexRule.replace(`{rule}`, String(rule)) }
  return { root }
}
const syntaxMessage = (reason: string, at: number): string => ISSUES.regexSyntax.replace(`{reason}`, reason).replace(`{offset}`, String(at))

/** §9.3 step 4: the value as text, or null = `{valid: false}`. */
function valueText(value: unknown): string | null {
  if (value === null || value === undefined) return ``
  if (typeof value === `string`) return value
  if (typeof value === `number` || typeof value === `boolean`) return String(value)
  return null
}

function search(root: Node, v: number[]): { valid: boolean; steps: number } | { error: string } {
  const ctx: Ctx = { v, steps: 0, maxSteps: LIMITS.maxSteps }
  try {
    for (let s = 0; s <= v.length; s++) if (run(root, s, ctx)) return { valid: true, steps: ctx.steps }
    return { valid: false, steps: ctx.steps }
  } catch (e) {
    if (e instanceof TooExpensive) return { error: ISSUES.regexTooExpensive }
    throw e
  }
}

/** `regex(value, pattern)` (§9.3): `{result}` or an EXPRESSION_ERROR message. */
export function regexCheck(value: unknown, pattern: unknown): { result: RegexOutcome } | { error: string } {
  const r = regexStepCount(value, pattern)
  return `error` in r ? r : { result: { valid: r.valid } }
}

/** @internal regexCheck plus the steps the match took (fixtures/regex.json
 *  `steps`; 0 when no match ran). */
export function regexStepCount(value: unknown, pattern: unknown): { valid: boolean; steps: number } | { error: string } {
  const c = compile(pattern)
  if (`error` in c) return c
  const text = valueText(value)
  if (text === null) return { valid: false, steps: 0 }
  const v = codePoints(text)
  if (v.length > LIMITS.maxValueCodePoints) return { valid: false, steps: 0 }
  return search(c.root, v)
}
