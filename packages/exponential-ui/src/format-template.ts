// VAPP-289 (round 9 §6.1): the `formatString` template. `${<pointer>}` reads
// the data, `${fn(name: value, …)}` calls a function: both become ordinary
// dynamic values the caller's own evaluator resolves (bindings against the
// scope, calls against the catalog the enclosing formatString resolved to:
// `${…}` has no syntax for a catalog). The Rust core's `format_template.rs`
// mirrors it; fixtures/a2ui10-functions.json `evaluations` locks both.

import { INDEX_FUNCTION } from "./functions"
import { isIdentifier, isXidContinue } from "./ids"
import { MAX_DEPTH } from "./limits"
import type { DataBinding, FunctionCall } from "./expr"

/** A literal run, a binding, a call, or `null` = an expression that does not parse. */
export type TemplatePart = string | DataBinding | FunctionCall | null

const isSpace = (c: string | undefined): boolean => c === ` ` || c === `\t` || c === `\n` || c === `\r`

/** Is a `${…}` body a call: `name(` after optional spaces? */
function callName(body: string): { name: string; open: number } | null {
  let i = 0
  while (isSpace(body[i])) i++
  const start = i
  if (body[i] === `@`) i++
  i = nameEnd(body, i)
  const name = body.slice(start, i)
  while (isSpace(body[i])) i++
  return body[i] === `(` && (name === INDEX_FUNCTION || isIdentifier(name)) ? { name, open: i } : null
}

function nameEnd(text: string, from: number): number {
  let i = from
  while (i < text.length) {
    const cp = text.codePointAt(i)!
    if (!isXidContinue(cp)) break
    i += cp > 0xffff ? 2 : 1
  }
  return i
}

/** The index of the `}` closing a body that starts at `from` (nested braces
 *  and quoted strings skipped), or -1. */
function closing(text: string, from: number): number {
  let depth = 1
  for (let i = from; i < text.length; i++) {
    const c = text[i]
    if (c === `{`) depth++
    else if (c === `}` && --depth === 0) return i
    else if (c === `'` || c === `"`) {
      for (i++; i < text.length && text[i] !== c; i++) if (text[i] === `\\`) i++
    }
  }
  return -1
}

/** The parts of a formatString template. `\${` is a literal `${`; an
 *  unclosed `${` is literal text. `catalogId` = what the enclosing call
 *  resolved to (every call but `@index` names it). */
export function parseTemplate(text: string, catalogId: string): TemplatePart[] {
  const parts: TemplatePart[] = []
  let literal = ``
  const flush = () => {
    if (literal) parts.push(literal)
    literal = ``
  }
  let i = 0
  while (i < text.length) {
    if (text.startsWith(`\\\${`, i)) {
      literal += `\${`
      i += 3
      continue
    }
    if (!text.startsWith(`\${`, i)) {
      literal += text[i++]
      continue
    }
    const call = callName(text.slice(i + 2))
    const end = call ? closing(text, i + 2) : text.indexOf(`}`, i + 2)
    if (end < 0) {
      literal += text.slice(i)
      break
    }
    const body = text.slice(i + 2, end)
    flush()
    parts.push(call ? new Parser(body, catalogId).whole() : { "@path": body.trim() })
    i = end + 1
  }
  flush()
  return parts
}

class Malformed extends Error {}

class Parser {
  private at = 0
  constructor(
    private readonly text: string,
    private readonly catalogId: string,
    private readonly depth = 1
  ) {}

  whole(): FunctionCall | null {
    try {
      const out = this.call()
      this.space()
      return this.at === this.text.length ? out : null
    } catch (e) {
      if (e instanceof Malformed) return null
      throw e
    }
  }

  private space(): void {
    while (isSpace(this.text[this.at])) this.at++
  }

  private call(): FunctionCall {
    if (this.depth > MAX_DEPTH) throw new Malformed()
    const head = callName(this.text.slice(this.at))
    if (!head) throw new Malformed()
    this.at += head.open + 1
    const args: Record<string, unknown> = {}
    this.space()
    while (this.text[this.at] !== `)`) {
      const end = nameEnd(this.text, this.at)
      const name = this.text.slice(this.at, end)
      if (!isIdentifier(name)) throw new Malformed()
      this.at = end
      this.space()
      if (this.text[this.at++] !== `:`) throw new Malformed()
      args[name] = this.value()
      this.space()
      if (this.text[this.at] === `,`) {
        this.at++
        this.space()
      } else if (this.text[this.at] !== `)`) throw new Malformed()
    }
    this.at++
    return head.name === INDEX_FUNCTION ? { "@call": head.name, args } : { "@call": head.name, catalogId: this.catalogId, args }
  }

  /** A quoted string, a number, true / false / null, `${…}`, a call, a pointer. */
  private value(): unknown {
    this.space()
    const rest = this.text.slice(this.at)
    if (rest.startsWith(`\${`)) {
      const end = closing(this.text, this.at + 2)
      if (end < 0) throw new Malformed()
      const body = this.text.slice(this.at + 2, end)
      this.at = end + 1
      if (!callName(body)) {
        if (body.trim() === ``) throw new Malformed()
        return { "@path": body.trim() }
      }
      const nested = new Parser(body, this.catalogId, this.depth + 1).whole()
      if (!nested) throw new Malformed()
      return nested
    }
    if (rest[0] === `'` || rest[0] === `"`) return this.string(rest[0])
    const number = /^-?[0-9]+(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/.exec(rest)
    if (number) {
      this.at += number[0].length
      return Number(number[0])
    }
    if (callName(rest)) {
      const nested = new Parser(this.text, this.catalogId, this.depth + 1)
      nested.at = this.at
      const out = nested.call()
      this.at = nested.at
      return out
    }
    let end = this.at
    while (end < this.text.length) {
      const next = nameEnd(this.text, end)
      if (next > end) end = next
      else if (`/.-~`.includes(this.text[end]!)) end++
      else break
    }
    const token = this.text.slice(this.at, end)
    if (token === ``) throw new Malformed()
    this.at = end
    if (token === `true`) return true
    if (token === `false`) return false
    if (token === `null`) return null
    return { "@path": token }
  }

  private string(quote: string): string {
    let out = ``
    for (this.at++; this.at < this.text.length; this.at++) {
      const c = this.text[this.at]!
      if (c === quote) {
        this.at++
        return out
      }
      if (c !== `\\`) out += c
      else {
        const next = this.text[++this.at]
        if (next === undefined) break
        out += next === `n` ? `\n` : next === `t` ? `\t` : next === `r` ? `\r` : next
      }
    }
    throw new Malformed()
  }
}

/** A template part that does not parse (§6.8). */
export const MALFORMED_TEMPLATE = `formatString: malformed expression`
