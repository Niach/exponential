// VAPP-87 (FEED-128: hoisted from the React renderer's markdown.tsx): the
// built-in markdown PARSER, framework-free: a small GFM subset (headings,
// paragraphs, bold/italic/code/strike, links, images, nested lists, task
// items, block quotes, fenced code, tables, rules) → a tree a renderer
// paints through the theme's `Markdown/<part>` classes. No HTML
// passthrough: tags are text. VAPP-103: link and image destinations parse
// balanced parentheses (CommonMark); the tree carries RAW destinations, the
// renderer passes every href through the URL policy (`linkHref`, denied =
// the label as text) and every image through the media policy. A paragraph
// that is one image is a BLOCK image; an image inside running text is its
// alt text, as on every renderer. Every pass is linear in its input.

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
 *  image, as every renderer paints it; an image inside running text is its
 *  alt text). */
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
  | { type: `del`; text: string }
  | { type: `link`; dest: string; children: MarkdownInline[] }

export type MarkdownBlock =
  | { type: `paragraph`; children: MarkdownInline[] }
  /** A paragraph that is exactly one image. */
  | { type: `image`; alt: string; src: string }
  | { type: `codeBlock`; lang?: string; text: string }
  | { type: `heading`; level: number; children: MarkdownInline[] }
  | { type: `quote`; children: MarkdownBlock[] }
  | { type: `list`; list: MarkdownList }
  | { type: `table`; align: (`start` | `center` | `end` | undefined)[]; head: MarkdownInline[][]; rows: MarkdownInline[][][] }
  | { type: `rule` }

const SIMPLE = /(`[^`]+`)|(\*\*([^*]+)\*\*)|(__([^_]+)__)|(\*([^*\s][^*]*)\*)|(_([^_\s][^_]*)_)|(~~([^~]+)~~)/

/** One inline run, linear in its length: the link geometry is precomputed
 *  (LinkScanner), the simple spans come off one global regex that never
 *  rescans, and link labels recurse at most MAX_LINK_NESTING deep. */
export function parseInline(text: string, depth = 0): MarkdownInline[] {
  const out: MarkdownInline[] = []
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
      if (link.at > pos) out.push(text.slice(pos, link.at))
      // An image inside running text paints its alt text (×4).
      if (link.image) out.push(link.label)
      else out.push({ type: `link`, dest: link.dest, children: parseInline(link.label, depth + 1) })
      pos = link.end
      continue
    }
    if (!m) {
      out.push(text.slice(pos))
      break
    }
    if (m.index > pos) out.push(text.slice(pos, m.index))
    if (m[1]) out.push({ type: `code`, text: m[1].slice(1, -1) })
    else if (m[2]) out.push({ type: `strong`, children: parseInline(m[3]!, depth) })
    else if (m[4]) out.push({ type: `strong`, children: parseInline(m[5]!, depth) })
    else if (m[6]) out.push({ type: `em`, children: parseInline(m[7]!, depth) })
    else if (m[8]) out.push({ type: `em`, children: parseInline(m[9]!, depth) })
    else if (m[10]) out.push({ type: `del`, text: m[11]! })
    pos = m.index + m[0].length
  }
  return out
}

export const LIST_ITEM = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/

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
  items: MarkdownListItem[]
}

/** Consecutive list item lines → a list tree: an item whose marker is
 *  indented 2+ columns past its list's nests under the previous item. */
export function parseList(lines: readonly string[]): MarkdownList {
  const indentOf = (ws: string) => ws.replace(/\t/g, `    `).length
  // VAPP-107 (property-tested): no lines, or a line that is no list item,
  // is an empty list / a skipped line, never a throw.
  const first = lines.length ? LIST_ITEM.exec(lines[0]!) : null
  if (!first) return { indent: 0, ordered: false, items: [] }
  const root: MarkdownList = { indent: indentOf(first[1]!), ordered: /\d/.test(first[2]!), items: [] }
  const stack: MarkdownList[] = [root]
  for (const line of lines) {
    const m = LIST_ITEM.exec(line)
    if (!m) continue
    const indent = indentOf(m[1]!)
    while (stack.length > 1 && indent < stack[stack.length - 1]!.indent) stack.pop()
    let list = stack[stack.length - 1]!
    const parent = list.items[list.items.length - 1]
    if (parent && indent >= list.indent + 2 && stack.length < MAX_BLOCK_NESTING) {
      const child: MarkdownList = { indent, ordered: /\d/.test(m[2]!), items: [] }
      parent.children.push(child)
      stack.push(child)
      list = child
    }
    const task = /^\[([ xX])\]\s+(.*)$/.exec(m[3]!)
    list.items.push(task ? { text: task[2]!, content: parseInline(task[2]!), task: task[1] !== ` `, children: [] } : { text: m[3]!, content: parseInline(m[3]!), children: [] })
  }
  return root
}

const isTableRow = (line: string) => /^\s*\|.*\|\s*$/.test(line)
const isTableRule = (line: string) => /^\s*\|(\s*:?-+:?\s*\|)+\s*$/.test(line)
const cells = (line: string) => line.trim().replace(/^\|/, ``).replace(/\|$/, ``).split(/(?<!\\)\|/).map((c) => c.replace(/\\\|/g, `|`).trim())

/** Block quotes and lists nest at most this deep (deeper = text). */
export const MAX_BLOCK_NESTING = 32

/** Markdown text → its block tree. */
export function parseMarkdown(text: string): MarkdownBlock[] {
  return parseBlocks(text, 0)
}

function parseBlocks(text: string, depth: number): MarkdownBlock[] {
  const lines = text.replace(/\r\n?/g, `\n`).split(`\n`)
  const blocks: MarkdownBlock[] = []
  let i = 0
  const para: string[] = []
  const flush = () => {
    if (para.length === 0) return
    const text = para.join(` `)
    const image = wholeImage(text)
    if (image) blocks.push({ type: `image`, alt: image.alt, src: image.src })
    else blocks.push({ type: `paragraph`, children: parseInline(text) })
    para.length = 0
  }
  while (i < lines.length) {
    const line = lines[i]!
    const fence = /^```(\w*)\s*$/.exec(line)
    if (fence) {
      flush()
      const body: string[] = []
      i++
      while (i < lines.length && !/^```\s*$/.test(lines[i]!)) body.push(lines[i++]!)
      i++
      blocks.push({ type: `codeBlock`, lang: fence[1] || undefined, text: body.join(`\n`) })
      continue
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line)
    if (heading) {
      flush()
      blocks.push({ type: `heading`, level: heading[1]!.length, children: parseInline(heading[2]!) })
      i++
      continue
    }
    if (depth < MAX_BLOCK_NESTING && /^\s*>/.test(line)) {
      flush()
      const body: string[] = []
      while (i < lines.length && /^\s*>/.test(lines[i]!)) body.push(lines[i++]!.replace(/^\s*>\s?/, ``))
      blocks.push({ type: `quote`, children: parseBlocks(body.join(`\n`), depth + 1) })
      continue
    }
    if (LIST_ITEM.test(line)) {
      flush()
      const body: string[] = []
      while (i < lines.length && LIST_ITEM.test(lines[i]!)) body.push(lines[i++]!)
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
    para.push(line.trim())
    i++
  }
  flush()
  return blocks
}
