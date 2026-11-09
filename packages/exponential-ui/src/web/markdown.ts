// VAPP-87 (FEED-128: hoisted from the React renderer's markdown.tsx): the
// built-in markdown PARSER, framework-free: a small GFM subset (headings,
// paragraphs, bold/italic/code/strike, links, images, nested lists, task
// items, block quotes, fenced code, tables, rules) → a tree a renderer
// paints through the theme's `Markdown/<part>` classes. No HTML
// passthrough: tags are text. VAPP-103: link and image destinations parse
// balanced parentheses (CommonMark); the tree carries RAW destinations, the
// renderer passes every href through the URL policy (`linkHref`, denied =
// the label as text) and every image through the media policy. A paragraph
// that is one image is a BLOCK image; #FEED-208: an image directly in a
// top-level paragraph's text splits it around a block image, anywhere else
// it is its alt text, as on every renderer. Every pass is linear in its input.

// From the root package (external to this entry, which `urls.ts` imports
// anyway): the tokenizer is never bundled twice.
import { markdownCodeLanguage, tokenizeCode, type CodeToken } from "@exponential_app/eui"
import { CHIP_KINDS, validChip, type MarkdownChip } from "./markdown-chip"

export { CHIP_KINDS, validChip, type MarkdownChip }

/** Link labels nest at most this deep (×4): a link inside 32 enclosing link
 *  labels is its text. */
export const MAX_LINK_NESTING = 32
/** A link / image destination longer than this (UTF-16 units) is no link. */
export const MAX_LINK_DEST = 8192

const TITLE = /\s+(?:"[^"]*"|'[^']*')/y

/** One inline run's link geometry, precomputed in ONE linear pass so every
 *  `[` resolves in O(1): matching `]` / `)` (escapes honoured), the next
 *  whitespace, `>`, `<`-or-newline and the running paren balance. Parsing a
 *  run is linear however many unclosed `[` / `(` it holds. */
class LinkScanner {
  private readonly close: Int32Array
  private readonly balance: Int32Array
  private readonly nextWs: Int32Array
  private readonly nextGt: Int32Array
  private readonly nextLtNl: Int32Array
  private readonly escaped: Uint8Array
  constructor(private readonly s: string) {
    const n = s.length
    this.close = new Int32Array(n).fill(-1)
    this.balance = new Int32Array(n + 1)
    this.escaped = new Uint8Array(n)
    const brackets: number[] = []
    const parens: number[] = []
    let esc = false
    let bal = 0
    for (let i = 0; i < n; i++) {
      this.balance[i] = bal
      if (esc) {
        this.escaped[i] = 1
        esc = false
        continue
      }
      const c = s[i]
      if (c === `\\`) esc = true
      else if (c === `[`) brackets.push(i)
      else if (c === `]`) {
        const o = brackets.pop()
        if (o !== undefined) this.close[o] = i
      } else if (c === `(`) {
        parens.push(i)
        bal++
      } else if (c === `)`) {
        const o = parens.pop()
        if (o !== undefined) this.close[o] = i
        bal--
      }
    }
    this.balance[n] = bal
    this.nextWs = new Int32Array(n + 1)
    this.nextGt = new Int32Array(n + 1)
    this.nextLtNl = new Int32Array(n + 1)
    this.nextWs[n] = n
    this.nextGt[n] = -1
    this.nextLtNl[n] = n
    for (let i = n - 1; i >= 0; i--) {
      const c = s[i]!
      this.nextWs[i] = !this.escaped[i] && /\s/.test(c) ? i : this.nextWs[i + 1]!
      this.nextGt[i] = c === `>` ? i : this.nextGt[i + 1]!
      this.nextLtNl[i] = c === `<` || c === `\n` ? i : this.nextLtNl[i + 1]!
    }
  }

  /** `[label](dest)` / `![alt](dest)` starting at `s[at]` (the `[`): the label
   *  (brackets balanced), the destination (`<…>` or balanced parentheses, no
   *  whitespace) and the end index. An optional `"title"` is skipped. */
  at(at: number): { label: string; dest: string; end: number } | null {
    const s = this.s
    if (s[at] !== `[` || this.escaped[at]) return null
    const i = this.close[at]!
    if (i < 0 || s[i + 1] !== `(`) return null
    let j = i + 2
    let dest: string
    if (s[j] === `<`) {
      const close = this.nextGt[j]!
      if (close < 0 || this.nextLtNl[j + 1]! < close || close - j - 1 > MAX_LINK_DEST) return null
      dest = s.slice(j + 1, close)
      j = close + 1
    } else {
      // The dest ends at the `)` closing the link's `(`, or earlier at
      // whitespace with its parentheses balanced.
      const k = this.close[i + 1]!
      const w = this.nextWs[j]!
      let stop: number
      if (k >= 0 && k < w) stop = k
      else if (this.balance[w] === this.balance[j]) stop = w
      else return null
      if (stop - j > MAX_LINK_DEST) return null
      dest = s.slice(j, stop).replace(/\\([()])/g, `$1`)
      j = stop
    }
    TITLE.lastIndex = j
    if (TITLE.exec(s)) j = TITLE.lastIndex
    while (s[j] === ` `) j++
    if (s[j] !== `)`) return null
    return { label: s.slice(at + 1, i), dest, end: j + 1 }
  }
}

/** `[label](dest)` / `![alt](dest)` starting at `s[at]` (the `[`): the label
 *  (brackets balanced), the destination (`<…>` or balanced parentheses, no
 *  whitespace) and the end index. An optional `"title"` is skipped. */
export function linkAt(s: string, at: number): { label: string; dest: string; end: number } | null {
  return s[at] === `[` ? new LinkScanner(s).at(at) : null
}

/** A paragraph that is exactly one `![alt](src)` → its alt + src (a BLOCK
 *  image, as every renderer paints it; an image inside a top-level
 *  paragraph's text splits it, `splitImages`). */
export function wholeImage(text: string): { alt: string; src: string } | null {
  if (text[0] !== `!`) return null
  const l = linkAt(text, 1)
  return l && l.end === text.length ? { alt: l.label, src: l.dest } : null
}

/** An inline run's tree: text, code, strong/em (nested), strike, a link
 *  (raw `dest`, label nested). */
export type MarkdownInline =
  | string
  | { type: `code`; text: string }
  | { type: `strong`; children: MarkdownInline[] }
  | { type: `em`; children: MarkdownInline[] }
  /** `children` only when a host's text links reach into it. */
  | { type: `del`; text: string; children?: MarkdownInline[] }
  /** `chip` only on a host's text link (catalog/host.json `markdown`). */
  | { type: `link`; dest: string; children: MarkdownInline[]; chip?: MarkdownChip }

/** #VAPP-258: a host text link painted as the catalog Chip's look, inline. */

/** #VAPP-258: what a host's `attachment(href)` classifies a link as. */
export interface MarkdownAttachment {
  kind: `video` | `audio` | `file`
  name: string
}

export type MarkdownBlock =
  | { type: `paragraph`; children: MarkdownInline[] }
  /** A paragraph that is exactly one image; `width` / `height` = the host's
   *  `imageSize` answer (its box keeps that aspect ratio). */
  | { type: `image`; alt: string; src: string; width?: number; height?: number }
  /** #VAPP-258: a paragraph that is exactly one link the host's
   *  `attachment` classified (`href` = the policed href). */
  | { type: `attachment`; kind: MarkdownAttachment[`kind`]; name: string; href: string }
  | { type: `codeBlock`; lang?: string; text: string }
  | { type: `heading`; level: number; children: MarkdownInline[] }
  | { type: `quote`; children: MarkdownBlock[] }
  | { type: `list`; list: MarkdownList }
  | { type: `table`; align: (`start` | `center` | `end` | undefined)[]; head: MarkdownInline[][]; rows: MarkdownInline[][][] }
  | { type: `rule` }

// #VAPP-258 (markdown-render.json): `***x***` is both; a strong span may
// hold an em one (`**a *b* c**`); an underscore span never opens or closes
// inside a word (`snake_case`).
const SIMPLE = /(`[^`]+`)|(\*\*\*([^*\s](?:[^*]*[^*\s])?)\*\*\*)|(\*\*([^\s](?:.*?[^\s])?)\*\*)|((?<![A-Za-z0-9])__([^_\s](?:[^_]*[^_\s])?)__(?![A-Za-z0-9]))|(\*([^*\s][^*]*)\*)|((?<![A-Za-z0-9])_([^_\s][^_]*)_(?![A-Za-z0-9]))|(~~([^~]+)~~)/

/** One inline run, linear in its length: the link geometry is precomputed
 *  (LinkScanner), the simple spans come off one global regex that never
 *  rescans, and link labels recurse at most MAX_LINK_NESTING deep. */
export function parseInline(text: string, depth = 0): MarkdownInline[] {
  return inlineRun(escapePunctuation(text), depth)
}

/** #FEED-208 (fixtures/markdown-render.json `image_split_*`): a top-level
 *  paragraph with images directly in its text = the text before each image
 *  (a paragraph), the image (a block image), the text after it. ASCII
 *  spaces, tabs and line breaks at a split edge trim; an empty side drops. */
function splitImages(text: string): MarkdownBlock[] {
  const marked: (MarkdownInline | ImageMark)[] = []
  inlineRun(escapePunctuation(text), 0, marked)
  if (!marked.some((n) => typeof n !== `string` && n.type === `image`)) return [{ type: `paragraph`, children: marked as MarkdownInline[] }]
  const out: MarkdownBlock[] = []
  let run: MarkdownInline[] = []
  const edge = (trimEnd: boolean) => {
    // Trim the run's leading edge (always) and its trailing one (before an image or at the end).
    while (run.length && typeof run[0] === `string`) {
      const t = (run[0] as string).replace(/^[ \t\n]+/, ``)
      if (t) { run[0] = t; break }
      run.shift()
    }
    while (trimEnd && run.length && typeof run[run.length - 1] === `string`) {
      const t = (run[run.length - 1] as string).replace(/[ \t\n]+$/, ``)
      if (t) { run[run.length - 1] = t; break }
      run.pop()
    }
    if (run.length) out.push({ type: `paragraph`, children: run })
    run = []
  }
  for (const n of marked) {
    if (typeof n !== `string` && n.type === `image`) {
      edge(true)
      out.push({ type: `image`, alt: n.alt, src: n.src })
    } else run.push(n as MarkdownInline)
  }
  edge(true)
  return out
}

// #VAPP-258: a backslash-escaped ASCII punctuation character is literal: it
// rides the parse as a private-use code point (so it never opens a span or
// a link) and decodes back to itself in text, or to `\x` inside code.
const PUA = 0xe000
const ESCAPE = /\\([!-\/:-@[-`{-~])/g
const ESCAPED = /[\ue000-\ue07f]/g
function escapePunctuation(text: string): string {
  return text.includes(`\\`) ? text.replace(ESCAPE, (_, c: string) => String.fromCharCode(PUA + c.charCodeAt(0))) : text
}
function unescapeCode(text: string): string {
  return text.replace(ESCAPED, (c) => `\\${String.fromCharCode(c.charCodeAt(0) - PUA)}`)
}
const ENTITIES: Record<string, string> = { amp: `&`, lt: `<`, gt: `>`, quot: `"`, apos: `'`, nbsp: `\u00a0` }
const ENTITY = /&(?:#(\d{1,7})|#[xX]([0-9a-fA-F]{1,6})|([A-Za-z]+));/g
/** Escapes and the common entities decoded (text, never code). */
function decodeText(text: string): string {
  const unescaped = text.replace(ESCAPED, (c) => String.fromCharCode(c.charCodeAt(0) - PUA))
  if (!unescaped.includes(`&`)) return unescaped
  return unescaped.replace(ENTITY, (whole, dec: string | undefined, hex: string | undefined, name: string | undefined) => {
    if (name) return ENTITIES[name] ?? whole
    const code = dec ? Number.parseInt(dec, 10) : Number.parseInt(hex!, 16)
    return code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : whole
  })
}

/** #FEED-208: a top-level paragraph's run marks each image directly in its
 *  text (`images`) so the paragraph splits around it (`splitImages`). */
type ImageMark = { type: `image`; alt: string; src: string }

function inlineRun(text: string, depth: number, images?: (MarkdownInline | ImageMark)[]): MarkdownInline[] {
  const out: MarkdownInline[] = images as MarkdownInline[] | undefined ?? []
  const scan = depth < MAX_LINK_NESTING && text.includes(`[`) ? new LinkScanner(text) : null
  const simple = new RegExp(SIMPLE.source, `g`)
  let pos = 0
  let m: RegExpExecArray | null = null
  let searched = false
  while (pos < text.length) {
    // The first simple span at or after `pos` (still valid while `pos` has
    // not passed it).
    if (!searched || (m && m.index < pos)) {
      simple.lastIndex = pos
      m = simple.exec(text)
      searched = true
    }
    const limit = m ? m.index : text.length
    // The first `[` / `![` before the next simple span that opens a link.
    let link: { at: number; image: boolean; label: string; dest: string; end: number } | null = null
    if (scan) {
      for (let p = text.indexOf(`[`, pos); p >= 0 && p < limit; p = text.indexOf(`[`, p + 1)) {
        const l = scan.at(p)
        if (!l) continue
        const image = p > pos && text[p - 1] === `!`
        link = { at: image ? p - 1 : p, image, ...l }
        break
      }
    }
    if (link) {
      if (link.at > pos) out.push(decodeText(text.slice(pos, link.at)))
      // #FEED-208: an image directly in a top-level paragraph's text splits
      // it (`images`); anywhere else (emphasis, a link label, a heading, a
      // list item, a cell, a quote) it paints its alt text (×5).
      if (link.image && images) images.push({ type: `image`, alt: decodeText(link.label), src: decodeText(link.dest) })
      else if (link.image) out.push(decodeText(link.label))
      else out.push({ type: `link`, dest: decodeText(link.dest), children: inlineRun(link.label, depth + 1) })
      pos = link.end
      continue
    }
    if (!m) {
      out.push(decodeText(text.slice(pos)))
      break
    }
    if (m.index > pos) out.push(decodeText(text.slice(pos, m.index)))
    if (m[1]) out.push({ type: `code`, text: unescapeCode(m[1].slice(1, -1)) })
    else if (m[2]) out.push({ type: `strong`, children: [{ type: `em`, children: inlineRun(m[3]!, depth) }] })
    else if (m[4]) out.push({ type: `strong`, children: inlineRun(m[5]!, depth) })
    else if (m[6]) out.push({ type: `strong`, children: inlineRun(m[7]!, depth) })
    else if (m[8]) out.push({ type: `em`, children: inlineRun(m[9]!, depth) })
    else if (m[10]) out.push({ type: `em`, children: inlineRun(m[11]!, depth) })
    else if (m[12]) out.push({ type: `del`, text: decodeText(m[13]!), children: inlineRun(m[13]!, depth) })
    pos = m.index + m[0].length
  }
  return out
}

export const LIST_ITEM = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/
/** An item line with its continuation lines (`breaks: hard` joins them with newlines). */
const LIST_ITEM_BODY = /^([ \t]*)([-*+]|\d+[.)])[ \t]+([\s\S]*)$/

export interface MarkdownListItem {
  /** The item's raw text (its inline markdown). */
  text: string
  /** `text` parsed. */
  content: MarkdownInline[]
  /** A task item: done (`[x]`) or not (`[ ]`). */
  task?: boolean
  children: MarkdownList[]
}
export interface MarkdownList {
  indent: number
  ordered: boolean
  /** An ordered list's first number (its source marker's), else 1. */
  start: number
  items: MarkdownListItem[]
}

/** Consecutive list item lines → a list tree: an item whose marker is
 *  indented 2+ columns past its list's nests under the previous item. */
export function parseList(lines: readonly string[]): MarkdownList {
  const indentOf = (ws: string) => ws.replace(/\t/g, `    `).length
  // VAPP-107 (property-tested): no lines, or a line that is no list item,
  // is an empty list / a skipped line, never a throw.
  const first = lines.length ? LIST_ITEM_BODY.exec(lines[0]!) : null
  if (!first) return { indent: 0, ordered: false, start: 1, items: [] }
  const startOf = (marker: string) => (/\d/.test(marker) ? Number.parseInt(marker, 10) : 1)
  const root: MarkdownList = { indent: indentOf(first[1]!), ordered: /\d/.test(first[2]!), start: startOf(first[2]!), items: [] }
  const stack: MarkdownList[] = [root]
  for (const line of lines) {
    const m = LIST_ITEM_BODY.exec(line)
    if (!m) continue
    const indent = indentOf(m[1]!)
    while (stack.length > 1 && indent < stack[stack.length - 1]!.indent) stack.pop()
    let list = stack[stack.length - 1]!
    const parent = list.items[list.items.length - 1]
    if (parent && indent >= list.indent + 2 && stack.length < MAX_BLOCK_NESTING) {
      const child: MarkdownList = { indent, ordered: /\d/.test(m[2]!), start: startOf(m[2]!), items: [] }
      parent.children.push(child)
      stack.push(child)
      list = child
    }
    // An item's trailing blanks are no text (as on the natives).
    const body = m[3]!.replace(/[ \t]+$/, ``)
    const task = /^\[([ xX])\]\s+([\s\S]*)$/.exec(body)
    list.items.push(task ? { text: task[2]!, content: parseInline(task[2]!), task: task[1] !== ` `, children: [] } : { text: body, content: parseInline(body), children: [] })
  }
  return root
}

const isTableRow = (line: string) => /^\s*\|.*\|\s*$/.test(line)
const isTableRule = (line: string) => /^\s*\|(\s*:?-+:?\s*\|)+\s*$/.test(line)
const cells = (line: string) => line.trim().replace(/^\|/, ``).replace(/\|$/, ``).split(/(?<!\\)\|/).map((c) => c.replace(/\\\|/g, `|`).trim())

/** Block quotes and lists nest at most this deep (deeper = text). */
export const MAX_BLOCK_NESTING = 32

/** A line that opens a block of its own (never a paragraph continuation). */
function startsBlock(line: string): boolean {
  return /^(`{3,}|~{3,})/.test(line) || /^#{1,6}\s/.test(line) || /^\s*>/.test(line) || isTableRow(line) || /^\s*([-*_])(\s*\1){2,}\s*$/.test(line)
}

/** Markdown text → its block tree. */
/** #VAPP-258 (fixtures/markdown-render.json): a fenced block's lines as
 *  the catalog's ONE tokenizer (`tokenizeCode`, CodeBlock's tokens) gives
 *  them when the fence names a highlighted language
 *  (`markdownCodeLanguage`), else null = the text as it is. A painter
 *  colours each token but `plain` through `CodeBlock/token {kind}`, colour
 *  only (base-css), so the block's box and height never change. */
export function markdownCodeLines(block: { lang?: string; text: string }): CodeToken[][] | null {
  const language = markdownCodeLanguage(block.lang)
  return language ? tokenizeCode(block.text, language) : null
}

export function parseMarkdown(text: string, options: MarkdownOptions = {}): MarkdownBlock[] {
  const hard = options.breaks === `hard`
  return options.streaming ? parseStreaming(text, hard) : parseBlocks(text, 0, hard)
}

/** #VAPP-258 (catalog/host.json `markdown.options`): a host's render
 *  options. Unset = the default (soft breaks, the document density, the
 *  renderer's own selectability, a finished text). */
export interface MarkdownOptions {
  breaks?: `soft` | `hard`
  density?: `document` | `chat`
  selectable?: boolean
  streaming?: boolean
  /** A block image's height cap in px; `null` = none (the available width
   *  at the image's aspect, at most the viewport's height); unset = 160. */
  imageMaxHeight?: number | null
}

/** From this many blocks on, gpui and Compose lay out only visible blocks. */
// The four `catalog/host.json` `markdown` constants the parser needs, inlined
// so a renderer bundle never carries the whole host contract (37 KB of
// prose); markdown-host-constants.test.ts locks them to the JSON.
export const MARKDOWN_LAZY_BLOCKS: number = 24

const FENCE = /^(`{3,}|~{3,})([\w+#.-]*)\s*$/
const fenceClose = (open: string) => new RegExp(`^${open[0] === `~` ? `~` : `\``}{${open.length},}\\s*$`)

/** The fence still open after `text`'s lines from `from` on (`from` at a
 *  line start outside a fence), its last COMPLETE blank line outside a
 *  fence (the stable boundary: the index after it, else `from`) and the
 *  line index of the open fence's opener. */
function scanFences(text: string, from: number): { boundary: number; open: boolean; openerLine: number; lines: number } {
  let close: RegExp | null = null
  let boundary = from
  let openerLine = -1
  let n = 0
  for (let at = from; ; n++) {
    const nl = text.indexOf(`\n`, at)
    const line = text.slice(at, nl < 0 ? text.length : nl)
    if (close) {
      if (close.test(line)) close = null
    } else {
      const f = FENCE.exec(line)
      if (f) {
        close = fenceClose(f[1]!)
        openerLine = n
      } else if (nl >= 0 && line.trim() === ``) boundary = nl + 1
    }
    if (nl < 0) break
    at = nl + 1
  }
  return { boundary, open: close !== null, openerLine, lines: n + 1 }
}

/** The per-text memo of streamed parses: the stable blocks before a
 *  boundary, reused while new text extends the old. */
interface StreamMemo {
  text: string
  hard: boolean
  boundary: number
  stable: MarkdownBlock[]
}
const STREAM_MEMOS: StreamMemo[] = []
const STREAM_MEMO_CAP = 16

/** Forget every streamed parse (tests: a cold parse of a prefix). */
export function resetMarkdownStreamMemo(): void {
  STREAM_MEMOS.length = 0
}

/** #VAPP-258 (markdown-render.json `streamingCases`): a growing prefix =
 *  the stable blocks (memoized, reparsed only past the old boundary) + the
 *  tail with its unfinished end held back. */
function parseStreaming(raw: string, hard: boolean): MarkdownBlock[] {
  const text = raw.replace(/\r\n?/g, `\n`)
  let memo: StreamMemo | undefined
  for (const m of STREAM_MEMOS) if (m.hard === hard && text.startsWith(m.text) && (!memo || m.text.length > memo.text.length)) memo = m
  const from = memo?.boundary ?? 0
  const { boundary } = scanFences(text, from)
  const stable = boundary > from ? [...(memo?.stable ?? []), ...parseBlocks(text.slice(from, boundary), 0, hard)] : (memo?.stable ?? [])
  if (memo && memo.text.length <= text.length) STREAM_MEMOS.splice(STREAM_MEMOS.indexOf(memo), 1)
  STREAM_MEMOS.unshift({ text, hard, boundary, stable })
  if (STREAM_MEMOS.length > STREAM_MEMO_CAP) STREAM_MEMOS.length = STREAM_MEMO_CAP
  let tail = text.slice(boundary)
  if (!tail) return stable
  // An open fence holds back its last line while it is empty or only fence
  // characters (a closing fence on its way).
  const fence = scanFences(tail, 0)
  if (fence.open) {
    const nl = tail.lastIndexOf(`\n`)
    if (fence.lines - 1 > fence.openerLine && /^\s*[`~]*\s*$/.test(tail.slice(nl + 1))) tail = tail.slice(0, nl)
  } else {
    // Outside a fence the text's unfinished end waits: a trailing run of
    // delimiters (a closer or opener on its way) and a last line that is
    // only a list marker or heading hashes so far.
    for (let held = tail.replace(TRAILING_HELD, ``); held !== tail; held = tail.replace(TRAILING_HELD, ``)) tail = held
  }
  const blocks = parseBlocks(tail, 0, hard)
  const last = blocks[blocks.length - 1]
  if (last) {
    const held = holdBackBlock(last, true)
    if (held.type === `paragraph` && held.children.length === 0) blocks.pop()
    else blocks[blocks.length - 1] = held
  }
  return stable.length ? [...stable, ...blocks] : blocks
}

// (A line that is only backticks or tildes stays: a fence opening or closing.)
const TRAILING_HELD = /(?:[*_]+|(?<!(?:^|\n)[ \t]*[`~]*)[`~]+|(?:^|\n)[ \t]*(?:[-+*]|\d{1,9}[.)]?|#{1,6}[ \t]*))$/

const UNFINISHED_LINK = /!?\[([^[\]]*)(?:\](?:\([^()\s]*)?)?$/
// A MAXIMAL run of `*` / `_` / backticks (or of 2+ `~`), not after an
// ASCII letter, digit or delimiter, followed by a non-space or the end.
const HELD_DELIMITERS = /(?<![A-Za-z0-9*_`~])(?:[*_`]+(?![*_`])|~~+(?!~))(?=\S|$)/g

/** The streaming tail's plain text with what would flicker held back: an
 *  unfinished link at its end → its label, and every opening delimiter
 *  run dropped (catalog/host.json `markdown.options.streaming`). */
function holdBackInlines(nodes: readonly MarkdownInline[]): MarkdownInline[] {
  const out = [...nodes]
  const last = out[out.length - 1]
  if (typeof last === `string`) out[out.length - 1] = last.replace(UNFINISHED_LINK, `$1`)
  return out.map((n) => (typeof n === `string` ? n.replace(HELD_DELIMITERS, ``) : n)).filter((n) => n !== ``)
}

function holdBackList(list: MarkdownList): MarkdownList {
  const items = [...list.items]
  const item = items[items.length - 1]
  if (!item) return list
  const child = item.children[item.children.length - 1]
  items[items.length - 1] = child ? { ...item, children: [...item.children.slice(0, -1), holdBackList(child)] } : { ...item, content: holdBackInlines(item.content) }
  return { ...list, items }
}

function holdBackAll(list: MarkdownList): MarkdownList {
  return { ...list, items: list.items.map((item) => ({ ...item, content: holdBackInlines(item.content), children: item.children.map(holdBackAll) })) }
}

/** The tail's last block held back (`whole` = a quote's every inline, as
 *  the natives' one quote block). */
function holdBackBlock(b: MarkdownBlock, last: boolean): MarkdownBlock {
  switch (b.type) {
    case `paragraph`:
    case `heading`:
      return { ...b, children: holdBackInlines(b.children) }
    case `quote`:
      return { ...b, children: b.children.map((c) => holdBackBlock(c, false)) }
    case `list`:
      return { ...b, list: last ? holdBackList(b.list) : holdBackAll(b.list) }
    default:
      return b
  }
}

function parseBlocks(text: string, depth: number, hard = false): MarkdownBlock[] {
  const lines = text.replace(/\r\n?/g, `\n`).split(`\n`)
  const blocks: MarkdownBlock[] = []
  let i = 0
  const para: string[] = []
  const flush = () => {
    if (para.length === 0) return
    // A line ending in a backslash or two spaces breaks hard (a newline in
    // the run); any other line break is a space.
    // #VAPP-258: `breaks: hard` = every line break is one.
    const text = para.reduce((all, line, k) => {
      if (k === 0) return line
      const forced = /(\\| {2,})$/.exec(all)
      return forced ? `${all.slice(0, forced.index)}\n${line}` : hard ? `${all}\n${line}` : `${all} ${line}`
    }, ``).replace(/ +$/, ``)
    const image = wholeImage(text)
    if (image) blocks.push({ type: `image`, alt: image.alt, src: image.src })
    else if (depth === 0) blocks.push(...splitImages(text))
    else blocks.push({ type: `paragraph`, children: parseInline(text) })
    para.length = 0
  }
  while (i < lines.length) {
    const line = lines[i]!
    const fence = FENCE.exec(line)
    if (fence) {
      flush()
      const body: string[] = []
      const close = fenceClose(fence[1]!)
      i++
      while (i < lines.length && !close.test(lines[i]!)) body.push(lines[i++]!)
      i++
      blocks.push({ type: `codeBlock`, lang: fence[2] || undefined, text: body.join(`\n`) })
      continue
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line)
    if (heading) {
      flush()
      blocks.push({ type: `heading`, level: heading[1]!.length, children: parseInline(heading[2]!.replace(/[ \t]+$/, ``)) })
      i++
      continue
    }
    if (depth < MAX_BLOCK_NESTING && /^\s*>/.test(line)) {
      flush()
      const body: string[] = []
      while (i < lines.length && /^\s*>/.test(lines[i]!)) body.push(lines[i++]!.replace(/^\s*>\s?/, ``))
      blocks.push({ type: `quote`, children: parseBlocks(body.join(`\n`), depth + 1, hard) })
      continue
    }
    if (LIST_ITEM.test(line)) {
      flush()
      const body: string[] = []
      // A non-blank line under an item that starts no block continues it.
      while (i < lines.length) {
        const next = lines[i]!
        if (LIST_ITEM.test(next)) body.push(next)
        else if (next.trim() !== `` && !startsBlock(next)) body[body.length - 1] += `${hard ? `\n` : ` `}${next.trim()}`
        else break
        i++
      }
      blocks.push({ type: `list`, list: parseList(body) })
      continue
    }
    if (isTableRow(line) && i + 1 < lines.length && isTableRule(lines[i + 1]!)) {
      flush()
      const head = cells(line)
      const align = cells(lines[i + 1]!).map((c) => (c.startsWith(`:`) && c.endsWith(`:`) ? (`center` as const) : c.endsWith(`:`) ? (`end` as const) : c.startsWith(`:`) ? (`start` as const) : undefined))
      i += 2
      const rows: string[][] = []
      while (i < lines.length && isTableRow(lines[i]!)) rows.push(cells(lines[i++]!))
      blocks.push({ type: `table`, align, head: head.map((h) => parseInline(h)), rows: rows.map((r) => r.map((c) => parseInline(c))) })
      continue
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flush()
      blocks.push({ type: `rule` })
      i++
      continue
    }
    if (line.trim() === ``) {
      flush()
      i++
      continue
    }
    para.push(line.replace(/^\s+/, ``))
    i++
  }
  flush()
  return blocks
}

/** One piece of a run a host's `textLinks` answered for (urls.ts `textLinkSegments`). */
type TextLinkAsk = (text: string) => readonly { text: string; href?: string; chip?: MarkdownChip }[] | null

const BARE_URL = /(?<![A-Za-z0-9_])https?:\/\/[^ \t\n]*/gi

/** A text run's inlines: a bare url stays text, the pieces around it ask. */
function linkRun(run: string, ask: TextLinkAsk): MarkdownInline[] {
  const out: MarkdownInline[] = []
  const piece = (text: string) => {
    const segments = text ? ask(text) : null
    if (!segments) return void (text && out.push(text))
    for (const segment of segments) out.push(segment.href ? { type: `link`, dest: segment.href, children: [segment.text], ...(segment.chip ? { chip: segment.chip } : {}) } : segment.text)
  }
  let at = 0
  for (const url of run.matchAll(BARE_URL)) {
    piece(run.slice(at, url.index))
    out.push(url[0])
    at = url.index + url[0].length
  }
  piece(run.slice(at))
  return out
}

function linkInlines(nodes: readonly MarkdownInline[], ask: TextLinkAsk): MarkdownInline[] {
  const out: MarkdownInline[] = []
  let run = ``
  const flush = () => {
    if (run) out.push(...linkRun(run, ask))
    run = ``
  }
  for (const n of nodes) {
    if (typeof n === `string`) {
      run += n
      continue
    }
    flush()
    if (n.type === `strong` || n.type === `em`) out.push({ ...n, children: linkInlines(n.children, ask) })
    else if (n.type === `del`) out.push({ ...n, children: linkRun(n.text, ask) })
    else out.push(n)
  }
  flush()
  return out
}

function linkList(list: MarkdownList, ask: TextLinkAsk): MarkdownList {
  return { ...list, items: list.items.map((item) => ({ ...item, content: linkInlines(item.content, ask), children: item.children.map((c) => linkList(c, ask)) })) }
}

/** `fixtures/markdown-text-links.json`: a Markdown's text runs through a
 *  host's text links (the hook's answer already policed): what it links is a
 *  plain `link`, painted like any other. Code, link labels and bare urls never
 *  ask. */
export function linkMarkdownText(tree: readonly MarkdownBlock[], ask: TextLinkAsk): MarkdownBlock[] {
  return tree.map((b) => {
    switch (b.type) {
      case `paragraph`:
      case `heading`:
        return { ...b, children: linkInlines(b.children, ask) }
      case `quote`:
        return { ...b, children: linkMarkdownText(b.children, ask) }
      case `list`:
        return { ...b, list: linkList(b.list, ask) }
      case `table`:
        return { ...b, head: b.head.map((h) => linkInlines(h, ask)), rows: b.rows.map((r) => r.map((c) => linkInlines(c, ask))) }
      default:
        return b
    }
  })
}

/** catalog/host.json `markdown`: the chip and attachment kinds a host may
 *  answer, and a block image box's max height. */
export const ATTACHMENT_KINDS: readonly string[] = [`video`, `audio`, `file`]
export const MARKDOWN_IMAGE_MAX_HEIGHT: number = 160

/** #VAPP-258: a host's chip as painted: a known kind and a non-empty label
 *  (an icon only when non-empty), else none. */

const validSize = (size: unknown): { width: number; height: number } | null => {
  if (!size || typeof size !== `object`) return null
  const { width, height } = size as Record<string, unknown>
  return typeof width === `number` && typeof height === `number` && Number.isFinite(width) && Number.isFinite(height) && width > 0 && height > 0 ? { width, height } : null
}

/** #VAPP-258 (catalog/host.json `markdown.options.imageMaxHeight`): the
 *  height cap a host's options ask for: a positive number, `null` = none,
 *  unset (or invalid) = the catalog's `imageMaxHeight`. */
export function markdownImageCap(options?: MarkdownOptions): number | null {
  const v = options?.imageMaxHeight
  if (v === null) return null
  return typeof v === `number` && Number.isFinite(v) && v > 0 ? v : MARKDOWN_IMAGE_MAX_HEIGHT
}

/** The aspect (height / width) an unsized image takes without a cap: 4:3. */
export const MARKDOWN_UNSIZED_ASPECT = 3 / 4

/** #VAPP-258 (fixtures/markdown-host.json `imageBoxes`): a block image's
 *  box in `available` px. With a `maxHeight` cap: the host's size at its
 *  natural size, scaled down to fit the width and the cap; no (valid) size
 *  = the fixed box (available × cap). With `null` (no cap): the available
 *  width at the size's aspect (4:3 unsized), scaled down to the `viewport`
 *  height when it is taller (no viewport = no limit). */
export function markdownImageBox(size: { width: number; height: number } | null | undefined, available: number, maxHeight: number | null = MARKDOWN_IMAGE_MAX_HEIGHT, viewport?: number | null): { width: number; height: number } {
  const s = validSize(size)
  if (maxHeight === null) {
    const ratio = s ? s.height / s.width : MARKDOWN_UNSIZED_ASPECT
    const height = available * ratio
    if (typeof viewport === `number` && Number.isFinite(viewport) && viewport > 0 && height > viewport) return { width: viewport / ratio, height: viewport }
    return { width: available, height }
  }
  if (!s) return { width: available, height: maxHeight }
  const scale = Math.min(1, available / s.width, maxHeight / s.height)
  return { width: s.width * scale, height: s.height * scale }
}

/** #VAPP-258 (fixtures/markdown-host.json `imageBoxes` `loaded`): the size
 *  a block image's box uses: the host's, else the size it decoded at (an
 *  unknown size takes the picture's real aspect once loaded: no gap under
 *  it), else none (the placeholder box). */
export function markdownImageSizeOf(size: { width: number; height: number } | null | undefined, loaded?: { width: number; height: number } | null): { width: number; height: number } | null {
  return validSize(size) ?? validSize(loaded)
}

const LOADED_IMAGE_SIZES = new Map<string, { width: number; height: number }>()

/** The decoded size of a block image src this page loaded before (the
 *  media cache a remount or a re-measure asks first, so a known picture
 *  never flashes the placeholder again). */
export function loadedImageSize(src: string): { width: number; height: number } | undefined {
  return LOADED_IMAGE_SIZES.get(src)
}

/** Remember the size `src` decoded at (an invalid size is ignored); bounded. */
export function rememberImageSize(src: string, width: number, height: number): { width: number; height: number } | undefined {
  const size = validSize({ width, height })
  if (!size) return undefined
  if (LOADED_IMAGE_SIZES.size >= 512 && !LOADED_IMAGE_SIZES.has(src)) LOADED_IMAGE_SIZES.delete(LOADED_IMAGE_SIZES.keys().next().value!)
  LOADED_IMAGE_SIZES.set(src, size)
  return size
}

/** #VAPP-258: the web renderers' box style of a block image (CSS property
 *  names, kebab case) under `cap` ([markdownImageBox] in CSS: the viewport
 *  is `100vh`); undefined = the stylesheet's fixed 160px box. */
export function markdownImageStyle(width: number | undefined, height: number | undefined, cap: number | null): Record<string, string> | undefined {
  const sized = width !== undefined && height !== undefined && width > 0 && height > 0
  if (cap === null) {
    if (sized) return { width: `100%`, "max-width": `calc(100vh * ${width} / ${height})`, height: `auto`, "aspect-ratio": `${width} / ${height}` }
    return { width: `100%`, "max-width": `calc(100vh * 4 / 3)`, height: `auto`, "aspect-ratio": `4 / 3` }
  }
  if (sized) return { width: `${Math.min(width, (cap * width) / height)}px`, "max-width": `100%`, height: `auto`, "aspect-ratio": `${width} / ${height}` }
  return cap === MARKDOWN_IMAGE_MAX_HEIGHT ? undefined : { height: `${cap}px` }
}

/** The hooks `hostMarkdown` applies (urls.ts `hostedMarkdown` builds them
 *  from a host plugin): `link` = the URL policy (null = denied). */
export interface MarkdownHooks {
  link: (href: string) => string | undefined
  textLinks?: TextLinkAsk
  imageSize?: (src: string) => { width: number; height: number } | null | undefined
  attachment?: (href: string) => MarkdownAttachment | null | undefined
}

/** #VAPP-258 (catalog/host.json `markdown`, fixtures/markdown-host.json):
 *  a tree through a host's hooks. A top-level paragraph that is exactly one
 *  markdown link the host's `attachment` classifies (its href allowed) is an
 *  attachment block; a block image the host's `imageSize` knows carries the
 *  size; then the text links (chips included). No hooks = the tree. */
export function hostMarkdown(tree: readonly MarkdownBlock[], hooks: MarkdownHooks): MarkdownBlock[] {
  const out = tree.map((b): MarkdownBlock => {
    if (b.type === `paragraph` && hooks.attachment && b.children.length === 1) {
      const only = b.children[0]!
      if (typeof only !== `string` && only.type === `link`) {
        const href = hooks.link(only.dest)
        const answer = href ? hooks.attachment(only.dest) : null
        if (href && answer && ATTACHMENT_KINDS.includes(answer.kind) && typeof answer.name === `string` && answer.name) return { type: `attachment`, kind: answer.kind, name: answer.name, href }
      }
    }
    if (b.type === `image` && hooks.imageSize) {
      const size = validSize(hooks.imageSize(b.src))
      if (size) return { ...b, width: size.width, height: size.height }
    }
    return b
  })
  return hooks.textLinks ? linkMarkdownText(out, hooks.textLinks) : out
}
