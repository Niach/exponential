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

import { Fragment, createElement, type ReactNode } from "react"
import type { HostPlugin } from "./host"
import { useMediaSource } from "./media"
import { linkHref, mediaRequestOf } from "./urls"

type Inline = ReactNode

/** `[label](dest)` / `![alt](dest)` starting at `s[at]` (the `[`): the label
 *  (brackets balanced), the destination (`<…>` or balanced parentheses, no
 *  whitespace) and the end index. An optional `"title"` is skipped. */
export function linkAt(s: string, at: number): { label: string; dest: string; end: number } | null {
  if (s[at] !== `[`) return null
  let depth = 0
  let i = at
  for (; i < s.length; i++) {
    const c = s[i]
    if (c === `\\`) {
      i++
      continue
    }
    if (c === `[`) depth++
    else if (c === `]` && --depth === 0) break
  }
  if (i >= s.length || s[i + 1] !== `(`) return null
  const label = s.slice(at + 1, i)
  let j = i + 2
  let dest = ``
  if (s[j] === `<`) {
    const close = s.indexOf(`>`, j)
    if (close < 0 || /[\n<]/.test(s.slice(j + 1, close))) return null
    dest = s.slice(j + 1, close)
    j = close + 1
  } else {
    let parens = 0
    const start = j
    for (; j < s.length; j++) {
      const c = s[j]!
      if (c === `\\` && j + 1 < s.length) {
        j++
        continue
      }
      if (/\s/.test(c)) break
      if (c === `(`) parens++
      else if (c === `)`) {
        if (parens === 0) break
        parens--
      }
    }
    if (parens !== 0) return null
    dest = s.slice(start, j).replace(/\\([()])/g, `$1`)
  }
  const title = /^\s+(?:"[^"]*"|'[^']*')/.exec(s.slice(j))
  if (title) j += title[0].length
  while (s[j] === ` `) j++
  if (s[j] !== `)`) return null
  return { label, dest, end: j + 1 }
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

function inline(text: string, host: HostPlugin, key = 0): Inline[] {
  const out: Inline[] = []
  let rest = text
  let i = key
  while (rest.length) {
    const m = SIMPLE.exec(rest)
    const limit = m ? m.index : rest.length
    // The first `[` / `![` before the next simple span that opens a link.
    let link: { at: number; image: boolean; label: string; dest: string; end: number } | null = null
    for (let p = rest.indexOf(`[`); p >= 0 && p < limit; p = rest.indexOf(`[`, p + 1)) {
      const l = linkAt(rest, p)
      if (!l) continue
      const image = p > 0 && rest[p - 1] === `!`
      link = { at: image ? p - 1 : p, image, ...l }
      break
    }
    if (link) {
      if (link.at > 0) out.push(rest.slice(0, link.at))
      const k = i++
      // An image inside running text paints its alt text (×4).
      if (link.image) out.push(link.label)
      else {
        const href = linkHref(host, link.dest)
        const label = inline(link.label, host, k * 100)
        out.push(href ? <a key={k} className="xui-Markdown-link" href={href} target="_blank" rel="noreferrer">{label}</a> : <span key={k} className="xui-Markdown-link" data-denied="">{label}</span>)
      }
      rest = rest.slice(link.end)
      continue
    }
    if (!m) {
      out.push(rest)
      break
    }
    if (m.index > 0) out.push(rest.slice(0, m.index))
    const k = i++
    if (m[1]) out.push(<code key={k} className="xui-Markdown-code">{m[1].slice(1, -1)}</code>)
    else if (m[2]) out.push(<strong key={k}>{inline(m[3]!, host, k * 100)}</strong>)
    else if (m[4]) out.push(<strong key={k}>{inline(m[5]!, host, k * 100)}</strong>)
    else if (m[6]) out.push(<em key={k}>{inline(m[7]!, host, k * 100)}</em>)
    else if (m[8]) out.push(<em key={k}>{inline(m[9]!, host, k * 100)}</em>)
    else if (m[10]) out.push(<del key={k}>{m[11]}</del>)
    rest = rest.slice(m.index + m[0].length)
  }
  return out
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
    if (parent && indent >= list.indent + 2) {
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

export function renderMarkdown(text: string, host: HostPlugin = {}): ReactNode[] {
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
    if (/^\s*>/.test(line)) {
      flush()
      const body: string[] = []
      while (i < lines.length && /^\s*>/.test(lines[i])) body.push(lines[i++].replace(/^\s*>\s?/, ``))
      blocks.push(<blockquote key={key++} className="xui-Markdown-quote">{renderMarkdown(body.join(`\n`), host)}</blockquote>)
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
  return <div className={className ? `xui-md ${className}` : `xui-md`}>{renderMarkdown(text, host)}</div>
}
