// Round 1 (docs/round-1-contract.md §3): CodeBlock's BUILT-IN tokenizer, the
// reference every renderer mirrors (catalog/code.json `$comment` lists the
// rules in order; fixtures/code-tokens.json locks the output). Deliberately
// small: one forward scan, no regex backtracking, no nesting beyond markup
// tags, so the Rust core, Swift and Kotlin port it line by line.

import codeJson from "../catalog/code.json" with { type: "json" }

export interface CodeToken {
  kind: string
  text: string
}

interface LanguageSpec {
  plain?: boolean
  ansi?: boolean
  lineKinds?: Record<string, string>
  lineComment?: string[]
  blockComment?: [string, string][]
  strings?: string[]
  multiline?: string[]
  keyStrings?: boolean
  markup?: boolean
  numbers?: boolean
  identExtra?: string
  keywords?: string[]
  caseInsensitive?: boolean
  keyIdents?: boolean
  typeNames?: string[]
  types?: string
  operators?: string
  punctuation?: string
}

export const CODE_LANGUAGES: Record<string, LanguageSpec> = codeJson.languages as unknown as Record<string, LanguageSpec>
export const CODE_LANGUAGE_NAMES: readonly string[] = Object.keys(CODE_LANGUAGES)

const isDigit = (c: string | undefined) => c !== undefined && c >= `0` && c <= `9`
const isAlpha = (c: string | undefined) => c !== undefined && ((c >= `a` && c <= `z`) || (c >= `A` && c <= `Z`))
const isSpace = (c: string | undefined) => c === ` ` || c === `\t`

/** The next char after `j` that is not a space or tab. */
function nextNonSpace(code: string, j: number): string | undefined {
  let k = j
  while (isSpace(code[k])) k++
  return code[k]
}

/** The source as flat tokens (text may span lines). `lineStart` = whether
 *  index 0 sits at the start of a line (an ansi run that begins mid-line
 *  does not). */
function scan(code: string, spec: LanguageSpec, lineStart = true): CodeToken[] {
  const out: CodeToken[] = []
  const n = code.length
  const operators = spec.operators ?? codeJson.defaults.operators
  const punctuation = spec.punctuation ?? codeJson.defaults.punctuation
  const extra = spec.identExtra ?? ``
  const identStart = (c: string | undefined) => isAlpha(c) || c === `_` || c === `$` || (c !== undefined && extra.includes(c))
  const identPart = (c: string | undefined) => identStart(c) || isDigit(c)
  const keywords = new Set(spec.keywords ?? [])
  const typeNames = new Set(spec.typeNames ?? [])
  const push = (kind: string, text: string) => out.push({ kind, text })
  const lastSignificant = () => {
    for (let k = out.length - 1; k >= 0; k--) if (out[k].text.trim() !== ``) return out[k]
    return undefined
  }
  if (spec.plain) return code ? [{ kind: `plain`, text: code }] : []
  let i = 0
  let inTag = false
  while (i < n) {
    const c = code[i]
    if (c === `\n`) {
      push(`plain`, c)
      i++
      continue
    }
    // (1) line kinds
    if (spec.lineKinds && (i === 0 ? lineStart : code[i - 1] === `\n` || isSpaceRunToLineStart(code, i, lineStart))) {
      // EXP-1324: the LONGEST listed prefix wins (`FAILED` over `FAIL`).
      let prefix: string | undefined
      for (const p of Object.keys(spec.lineKinds)) if (code.startsWith(p, i) && (!prefix || p.length > prefix.length)) prefix = p
      if (prefix && !isSpace(c)) {
        let j = i
        while (j < n && code[j] !== `\n`) j++
        push(spec.lineKinds[prefix], code.slice(i, j))
        i = j
        continue
      }
    }
    // (2) line comments
    const line = !inTag ? (spec.lineComment ?? []).find((p) => code.startsWith(p, i)) : undefined
    if (line) {
      let j = i
      while (j < n && code[j] !== `\n`) j++
      push(`comment`, code.slice(i, j))
      i = j
      continue
    }
    // (3) block comments
    const block = !inTag ? (spec.blockComment ?? []).find(([open]) => code.startsWith(open, i)) : undefined
    if (block) {
      const end = code.indexOf(block[1], i + block[0].length)
      const j = end < 0 ? n : end + block[1].length
      push(`comment`, code.slice(i, j))
      i = j
      continue
    }
    // (4) strings
    if ((spec.strings ?? []).includes(c) && (!spec.markup || inTag)) {
      const multi = (spec.multiline ?? []).includes(c)
      let j = i + 1
      while (j < n) {
        if (code[j] === `\\`) {
          j += 2
          continue
        }
        if (code[j] === c) {
          j++
          break
        }
        if (code[j] === `\n` && !multi) break
        j++
      }
      j = Math.min(j, n)
      const kind = spec.keyStrings && nextNonSpace(code, j) === `:` ? `property` : `string`
      push(kind, code.slice(i, j))
      i = j
      continue
    }
    // (5) markup
    if (spec.markup) {
      if (!inTag) {
        if (c === `<` && (isAlpha(code[i + 1]) || code[i + 1] === `/` || code[i + 1] === `!`)) {
          const open = code[i + 1] === `/` ? `</` : `<`
          push(`punctuation`, open)
          let j = i + open.length
          const start = j
          while (j < n && (isAlpha(code[j]) || isDigit(code[j]) || code[j] === `-` || code[j] === `:` || code[j] === `!`)) j++
          if (j > start) push(`tag`, code.slice(start, j))
          inTag = true
          i = j
          continue
        }
        let j = i
        while (j < n && code[j] !== `<` && code[j] !== `\n`) j++
        if (j === i) j = i + 1
        push(`plain`, code.slice(i, j))
        i = j
        continue
      }
      if (c === `>` || code.startsWith(`/>`, i)) {
        const close = c === `>` ? `>` : `/>`
        push(`punctuation`, close)
        inTag = false
        i += close.length
        continue
      }
      if (c === `=`) {
        push(`operator`, c)
        i++
        continue
      }
      if (isAlpha(c) || c === `_` || c === `:` || c === `@`) {
        let j = i
        while (j < n && (isAlpha(code[j]) || isDigit(code[j]) || `_:@.-`.includes(code[j]))) j++
        push(`attribute`, code.slice(i, j))
        i = j
        continue
      }
      if (isSpace(c)) {
        let j = i
        while (isSpace(code[j])) j++
        push(`plain`, code.slice(i, j))
        i = j
        continue
      }
      push(`plain`, c)
      i++
      continue
    }
    // (6) numbers
    if (spec.numbers !== false && (isDigit(c) || (c === `.` && isDigit(code[i + 1])))) {
      let j = i + 1
      while (j < n && (isDigit(code[j]) || isAlpha(code[j]) || code[j] === `_` || code[j] === `.`)) j++
      push(`number`, code.slice(i, j))
      i = j
      continue
    }
    // (7) identifiers
    if (identStart(c)) {
      let j = i + 1
      while (j < n && identPart(code[j])) j++
      const word = code.slice(i, j)
      const key = spec.caseInsensitive ? word.toLowerCase() : word
      const next = nextNonSpace(code, j)
      const after = (() => {
        let k = j
        while (isSpace(code[k])) k++
        return code[k + 1]
      })()
      const prev = lastSignificant()
      let kind = `plain`
      if (keywords.has(key)) kind = `keyword`
      else if (spec.keyIdents && next === `:` && after !== `:`) kind = `property`
      else if (next === `(`) kind = `function`
      else if (prev && prev.kind === `punctuation` && prev.text.endsWith(`.`)) kind = `property`
      else if (typeNames.has(word) || (spec.types === `capitalized` && word[0] >= `A` && word[0] <= `Z`)) kind = `type`
      push(kind, word)
      i = j
      continue
    }
    // (8) spaces
    if (isSpace(c)) {
      let j = i
      while (isSpace(code[j])) j++
      push(`plain`, code.slice(i, j))
      i = j
      continue
    }
    // (9) operators
    if (operators.includes(c)) {
      let j = i
      while (j < n && operators.includes(code[j])) j++
      push(`operator`, code.slice(i, j))
      i = j
      continue
    }
    // (10) punctuation, (11) anything else
    push(punctuation.includes(c) ? `punctuation` : `plain`, c)
    i++
  }
  return out
}

/** True when only spaces/tabs separate `i` from the start of its line
 *  (`lineStart` = whether the text's index 0 starts a line). */
function isSpaceRunToLineStart(code: string, i: number, lineStart = true): boolean {
  let k = i - 1
  while (k >= 0 && isSpace(code[k])) k--
  return k < 0 ? lineStart : code[k] === `\n`
}

/** EXP-1324 `ansi`: one run of the escape-free text, with the SGR colour
 *  (a codeToken kind) it was printed in, or null. */
export interface AnsiRun {
  kind: string | null
  text: string
}

const isCsiParam = (c: number) => c >= 0x30 && c <= 0x3f
const isCsiIntermediate = (c: number) => c >= 0x20 && c <= 0x2f
const isCsiFinal = (c: number) => c >= 0x40 && c <= 0x7e

/** The SGR colour after applying `params` (left to right) to `kind`:
 *  31/91 error, 32/92 success, 33/93 warning, 2/90 comment, 0/39/empty
 *  reset; any other parameter leaves the colour as it was. */
function sgrKind(params: string, kind: string | null): string | null {
  let out = kind
  for (const raw of params.split(`;`)) {
    const p = raw === `` ? 0 : Number(raw)
    if (p === 0 || p === 39) out = null
    else if (p === 31 || p === 91) out = `error`
    else if (p === 32 || p === 92) out = `success`
    else if (p === 33 || p === 93) out = `warning`
    else if (p === 2 || p === 90) out = `comment`
  }
  return out
}

/** The source with every ANSI CSI escape removed, split into runs by SGR
 *  colour (catalog/code.json `$comment`, `ansi`). Adjacent runs of one
 *  colour merge; an incomplete escape stays text. */
export function ansiRuns(source: string): AnsiRun[] {
  const runs: AnsiRun[] = []
  let kind: string | null = null
  let text = ``
  const flush = () => {
    if (text === ``) return
    const last = runs.at(-1)
    if (last && last.kind === kind) last.text += text
    else runs.push({ kind, text })
    text = ``
  }
  let i = 0
  const n = source.length
  while (i < n) {
    if (source.charCodeAt(i) === 0x1b && source[i + 1] === `[`) {
      let j = i + 2
      while (j < n && isCsiParam(source.charCodeAt(j))) j++
      const params = source.slice(i + 2, j)
      while (j < n && isCsiIntermediate(source.charCodeAt(j))) j++
      if (j < n && isCsiFinal(source.charCodeAt(j))) {
        if (source[j] === `m`) {
          const next = sgrKind(params, kind)
          if (next !== kind) {
            flush()
            kind = next
          }
        }
        i = j + 1
        continue
      }
    }
    text += source[i]
    i++
  }
  flush()
  return runs
}

/** `ansi`: coloured runs verbatim, uncoloured ones through the scanner. */
function scanAnsi(code: string, spec: LanguageSpec): CodeToken[] {
  const out: CodeToken[] = []
  let atLine = true
  for (const run of ansiRuns(code)) {
    if (run.kind) out.push({ kind: run.kind, text: run.text })
    else out.push(...scan(run.text, spec, atLine))
    atLine = isSpaceRunToLineStart(run.text, run.text.length, atLine)
  }
  return out
}

/** The text a CodeBlock shows and copies: the source with CRLF → LF and,
 *  for `ansi`, every escape removed. */
export function codeText(source: string, language: string): string {
  const code = source.replace(/\r\n?/g, `\n`)
  return CODE_LANGUAGES[language]?.ansi ? ansiRuns(code).map((r) => r.text).join(``) : code
}

/** The source tokenized per LINE: each line's tokens concatenate to exactly
 *  its text (no `\n`), adjacent tokens of one kind merged; an empty line is
 *  an empty list. An unknown language tokenizes as `plain`. */
export function tokenizeCode(source: string, language: string): CodeToken[][] {
  const code = source.replace(/\r\n?/g, `\n`)
  const spec = CODE_LANGUAGES[language] ?? CODE_LANGUAGES.plain
  const lines: CodeToken[][] = [[]]
  for (const token of spec.ansi ? scanAnsi(code, spec) : scan(code, spec)) {
    const pieces = token.text.split(`\n`)
    pieces.forEach((piece, index) => {
      if (index > 0) lines.push([])
      if (piece === ``) return
      const line = lines.at(-1)!
      const last = line.at(-1)
      if (last && last.kind === token.kind) last.text += piece
      else line.push({ kind: token.kind, text: piece })
    })
  }
  return lines
}
