// VAPP-87: the built-in markdown painter — a small GFM subset (headings,
// paragraphs, bold/italic/code, links, images, nested lists, block quotes,
// fenced code, tables) rendered through the theme's `Markdown/<part>`
// recipes. A host with a real renderer (the app's TipTap view) passes it as
// `host.Markdown` and this one steps aside. No HTML passthrough: tags are
// text. VAPP-103: link and image destinations parse balanced parentheses
// (CommonMark); every href passes the URL policy (denied = the label as
// text), every image the media policy. A paragraph that is one image is a
// BLOCK image (denied = a paragraph of its alt text); an image inside
// running text is its alt text, as on every renderer.

import { Fragment, createElement, useMemo, type MouseEvent, type ReactNode } from "react"
import type { HostPlugin } from "./host"
import { useMediaSource } from "./media"
import { linkHref, mediaRequestOf } from "./urls"

type Inline = ReactNode

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

/** A block image through the media policy and loader in an `imageHeight`
 *  (160px, ×4) box (loading or loaded, so the box never jumps;
 *  the alt text there while it loads or when it fails). A src the policy
 *  DENIES is a paragraph of its alt text (nothing without one). */
function MarkdownImage({ host, src, alt }: { host: HostPlugin; src: string; alt: string }) {
  const media = useMediaSource(host, src)
  if (mediaRequestOf(host, src) === null) return alt ? <p className="xui-Markdown-paragraph">{alt}</p> : null
  return (
    <div className="xui-Markdown-image" role="img" aria-label={alt}>
      {media.url && !media.error ? <img alt="" src={media.url} /> : <span className="xui-Markdown-imageAlt">{alt}</span>}
    </div>
  )
}

const SIMPLE = /(`[^`]+`)|(\*\*([^*]+)\*\*)|(__([^_]+)__)|(\*([^*\s][^*]*)\*)|(_([^_\s][^_]*)_)|(~~([^~]+)~~)/

/** One inline run, linear in its length: the link geometry is precomputed
 *  (LinkScanner), the simple spans come off one global regex that never
 *  rescans, and link labels recurse at most MAX_LINK_NESTING deep. */
function inline(text: string, host: HostPlugin, key = 0, depth = 0): Inline[] {
  const out: Inline[] = []
  const scan = depth < MAX_LINK_NESTING && text.includes(`[`) ? new LinkScanner(text) : null
  const simple = new RegExp(SIMPLE.source, `g`)
  let pos = 0
  let i = key
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
      const k = i++
      // An image inside running text paints its alt text (×4).
      if (link.image) out.push(link.label)
      else {
        const href = linkHref(host, link.dest)
        const label = inline(link.label, host, k * 100, depth + 1)
        out.push(href ? <MarkdownLink key={k} host={host} href={href}>{label}</MarkdownLink> : <span key={k} className="xui-Markdown-link" data-denied="">{label}</span>)
      }
      pos = link.end
      continue
    }
    if (!m) {
      out.push(text.slice(pos))
      break
    }
    if (m.index > pos) out.push(text.slice(pos, m.index))
    const k = i++
    if (m[1]) out.push(<code key={k} className="xui-Markdown-code">{m[1].slice(1, -1)}</code>)
    else if (m[2]) out.push(<strong key={k}>{inline(m[3]!, host, k * 100, depth)}</strong>)
    else if (m[4]) out.push(<strong key={k}>{inline(m[5]!, host, k * 100, depth)}</strong>)
    else if (m[6]) out.push(<em key={k}>{inline(m[7]!, host, k * 100, depth)}</em>)
    else if (m[8]) out.push(<em key={k}>{inline(m[9]!, host, k * 100, depth)}</em>)
    else if (m[10]) out.push(<del key={k}>{m[11]}</del>)
    pos = m.index + m[0].length
  }
  return out
}

/** A policed markdown link: a host with `openUrl` opens it (a plain click
 *  goes through the host, never a bare new tab). */
function MarkdownLink({ host, href, children }: { host: HostPlugin; href: string; children: ReactNode }) {
  const onClick = host.openUrl
    ? (e: MouseEvent<HTMLAnchorElement>) => {
        e.preventDefault()
        host.openUrl!(href)
      }
    : undefined
  return <a className="xui-Markdown-link" href={href} target="_blank" rel="noreferrer" onClick={onClick}>{children}</a>
}

const LIST_ITEM = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/

interface ListItem {
  text: string
  task?: boolean
  children: List[]
}
interface List {
  indent: number
  ordered: boolean
  items: ListItem[]
}

/** Consecutive list item lines → a list tree: an item whose marker is
 *  indented 2+ columns past its list's nests under the previous item. */
export function parseList(lines: readonly string[]): List {
  const indentOf = (ws: string) => ws.replace(/\t/g, `    `).length
  const first = LIST_ITEM.exec(lines[0]!)!
  const root: List = { indent: indentOf(first[1]!), ordered: /\d/.test(first[2]!), items: [] }
  const stack: List[] = [root]
  for (const line of lines) {
    const m = LIST_ITEM.exec(line)!
    const indent = indentOf(m[1]!)
    while (stack.length > 1 && indent < stack[stack.length - 1]!.indent) stack.pop()
    let list = stack[stack.length - 1]!
    const parent = list.items[list.items.length - 1]
    if (parent && indent >= list.indent + 2 && stack.length < MAX_BLOCK_NESTING) {
      const child: List = { indent, ordered: /\d/.test(m[2]!), items: [] }
      parent.children.push(child)
      stack.push(child)
      list = child
    }
    const task = /^\[([ xX])\]\s+(.*)$/.exec(m[3]!)
    list.items.push(task ? { text: task[2]!, task: task[1] !== ` `, children: [] } : { text: m[3]!, children: [] })
  }
  return root
}

function renderList(list: List, host: HostPlugin, key: number): ReactNode {
  const items = list.items.map((item, n) => (
    <li key={n} className="xui-Markdown-listItem" data-task={item.task === undefined ? undefined : item.task ? `done` : `todo`}>
      {item.task === undefined ? null : <input type="checkbox" checked={item.task} readOnly />}
      {item.task === undefined ? null : ` `}
      {inline(item.text, host, key * 1000 + n * 10)}
      {item.children.map((c, ci) => <Fragment key={`c${ci}`}>{renderList(c, host, key * 1000 + n * 10 + ci + 1)}</Fragment>)}
    </li>
  ))
  return list.ordered ? <ol key={key}>{items}</ol> : <ul key={key}>{items}</ul>
}

const isTableRow = (line: string) => /^\s*\|.*\|\s*$/.test(line)
const isTableRule = (line: string) => /^\s*\|(\s*:?-+:?\s*\|)+\s*$/.test(line)
const cells = (line: string) => line.trim().replace(/^\|/, ``).replace(/\|$/, ``).split(/(?<!\\)\|/).map((c) => c.replace(/\\\|/g, `|`).trim())

/** Block quotes and lists nest at most this deep (deeper = text). */
export const MAX_BLOCK_NESTING = 32

export function renderMarkdown(text: string, host: HostPlugin = {}): ReactNode[] {
  return renderBlocks(text, host, 0)
}

function renderBlocks(text: string, host: HostPlugin, depth: number): ReactNode[] {
  const lines = text.replace(/\r\n?/g, `\n`).split(`\n`)
  const blocks: ReactNode[] = []
  let i = 0
  let key = 0
  const para: string[] = []
  const flush = () => {
    if (para.length === 0) return
    const text = para.join(` `)
    const image = wholeImage(text)
    if (image) blocks.push(<MarkdownImage key={key++} host={host} src={image.src} alt={image.alt} />)
    else blocks.push(<p key={key++} className="xui-Markdown-paragraph">{inline(text, host, key * 1000)}</p>)
    para.length = 0
  }
  while (i < lines.length) {
    const line = lines[i]
    const fence = /^```(\w*)\s*$/.exec(line)
    if (fence) {
      flush()
      const body: string[] = []
      i++
      while (i < lines.length && !/^```\s*$/.test(lines[i])) body.push(lines[i++])
      i++
      blocks.push(<pre key={key++} className="xui-Markdown-codeBlock" data-lang={fence[1] || undefined}><code>{body.join(`\n`)}</code></pre>)
      continue
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line)
    if (heading) {
      flush()
      const level = heading[1].length
      blocks.push(createElement(`h${Math.min(level, 6)}`, { key: key++, className: `xui-Markdown-heading`, "data-level": level }, ...inline(heading[2]!, host, key * 1000)))
      i++
      continue
    }
    if (depth < MAX_BLOCK_NESTING && /^\s*>/.test(line)) {
      flush()
      const body: string[] = []
      while (i < lines.length && /^\s*>/.test(lines[i])) body.push(lines[i++].replace(/^\s*>\s?/, ``))
      blocks.push(<blockquote key={key++} className="xui-Markdown-quote">{renderBlocks(body.join(`\n`), host, depth + 1)}</blockquote>)
      continue
    }
    if (LIST_ITEM.test(line)) {
      flush()
      const body: string[] = []
      while (i < lines.length && LIST_ITEM.test(lines[i]!)) body.push(lines[i++]!)
      blocks.push(renderList(parseList(body), host, key++))
      continue
    }
    if (isTableRow(line) && i + 1 < lines.length && isTableRule(lines[i + 1])) {
      flush()
      const head = cells(line)
      const align = cells(lines[i + 1]).map((c) => (c.startsWith(`:`) && c.endsWith(`:`) ? `center` : c.endsWith(`:`) ? `end` : c.startsWith(`:`) ? `start` : undefined))
      i += 2
      const rows: string[][] = []
      while (i < lines.length && isTableRow(lines[i])) rows.push(cells(lines[i++]))
      blocks.push(
        <table key={key++} className="xui-Markdown-table">
          <thead><tr>{head.map((h, c) => <th key={c} style={align[c] ? { textAlign: align[c] } : undefined}>{inline(h, host, c)}</th>)}</tr></thead>
          <tbody>{rows.map((r, ri) => <tr key={ri}>{r.map((cell, c) => <td key={c} style={align[c] ? { textAlign: align[c] } : undefined}>{inline(cell, host, c)}</td>)}</tr>)}</tbody>
        </table>
      )
      continue
    }
    if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flush()
      blocks.push(<hr key={key++} />)
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
  return blocks.map((b, k) => <Fragment key={k}>{b}</Fragment>)
}

export function BuiltinMarkdown({ text, className, host }: { text: string; className?: string; host?: HostPlugin }) {
  // Parsed once per text (and host), not on every parent render.
  const blocks = useMemo(() => renderMarkdown(text, host), [text, host])
  return <div className={className ? `xui-md ${className}` : `xui-md`}>{blocks}</div>
}
