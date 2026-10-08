// VAPP-87: the built-in markdown painter — a small GFM subset (headings,
// paragraphs, bold/italic/code, links, images, lists, block quotes, fenced
// code, tables) rendered through the theme's `Markdown/<part>` recipes. A
// host with a real renderer (the app's TipTap view) passes it as
// `host.Markdown` and this one steps aside. No HTML passthrough: tags are
// text.

import { Fragment, createElement, type ReactNode } from "react"

type Inline = ReactNode

function inline(text: string, key = 0, link?: (href: string) => string): Inline[] {
  const out: Inline[] = []
  let rest = text
  let i = key
  const re = /(`[^`]+`)|(!\[([^\]]*)\]\(([^)\s]+)\))|(\[([^\]]+)\]\(([^)\s]+)\))|(\*\*([^*]+)\*\*)|(__([^_]+)__)|(\*([^*\s][^*]*)\*)|(_([^_\s][^_]*)_)|(~~([^~]+)~~)/
  while (rest.length) {
    const m = re.exec(rest)
    if (!m) {
      out.push(rest)
      break
    }
    if (m.index > 0) out.push(rest.slice(0, m.index))
    const k = i++
    if (m[1]) out.push(<code key={k} className="xui-Markdown-code">{m[1].slice(1, -1)}</code>)
    else if (m[2]) out.push(<img key={k} alt={m[3]} src={link ? link(m[4]) : m[4]} />)
    else if (m[5]) out.push(<a key={k} className="xui-Markdown-link" href={link ? link(m[7]) : m[7]} target="_blank" rel="noreferrer">{inline(m[6], k * 100, link)}</a>)
    else if (m[8]) out.push(<strong key={k}>{inline(m[9], k * 100, link)}</strong>)
    else if (m[10]) out.push(<strong key={k}>{inline(m[11], k * 100, link)}</strong>)
    else if (m[12]) out.push(<em key={k}>{inline(m[13], k * 100, link)}</em>)
    else if (m[14]) out.push(<em key={k}>{inline(m[15], k * 100, link)}</em>)
    else if (m[16]) out.push(<del key={k}>{m[17]}</del>)
    rest = rest.slice(m.index + m[0].length)
  }
  return out
}

const isTableRow = (line: string) => /^\s*\|.*\|\s*$/.test(line)
const isTableRule = (line: string) => /^\s*\|(\s*:?-+:?\s*\|)+\s*$/.test(line)
const cells = (line: string) => line.trim().replace(/^\|/, ``).replace(/\|$/, ``).split(/(?<!\\)\|/).map((c) => c.replace(/\\\|/g, `|`).trim())

export function renderMarkdown(text: string, link?: (href: string) => string): ReactNode[] {
  const lines = text.replace(/\r\n?/g, `\n`).split(`\n`)
  const blocks: ReactNode[] = []
  let i = 0
  let key = 0
  const para: string[] = []
  const flush = () => {
    if (para.length === 0) return
    blocks.push(<p key={key++} className="xui-Markdown-paragraph">{inline(para.join(` `), key * 1000, link)}</p>)
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
      blocks.push(createElement(`h${Math.min(level, 6)}`, { key: key++, className: `xui-Markdown-heading`, "data-level": level }, ...inline(heading[2], key * 1000, link)))
      i++
      continue
    }
    if (/^\s*>/.test(line)) {
      flush()
      const body: string[] = []
      while (i < lines.length && /^\s*>/.test(lines[i])) body.push(lines[i++].replace(/^\s*>\s?/, ``))
      blocks.push(<blockquote key={key++} className="xui-Markdown-quote">{renderMarkdown(body.join(`\n`), link)}</blockquote>)
      continue
    }
    const list = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/.exec(line)
    if (list) {
      flush()
      const ordered = /\d/.test(list[2])
      const items: ReactNode[] = []
      while (i < lines.length) {
        const m = /^(\s*)([-*+]|\d+[.)])\s+(.*)$/.exec(lines[i])
        if (!m) break
        const task = /^\[([ xX])\]\s+(.*)$/.exec(m[3])
        items.push(
          <li key={items.length} className="xui-Markdown-listItem" data-task={task ? (task[1] === ` ` ? `todo` : `done`) : undefined}>
            {task ? <input type="checkbox" checked={task[1] !== ` `} readOnly /> : null}
            {task ? ` ` : null}
            {inline(task ? task[2] : m[3], key * 1000 + items.length * 10, link)}
          </li>
        )
        i++
      }
      blocks.push(ordered ? <ol key={key++}>{items}</ol> : <ul key={key++}>{items}</ul>)
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
          <thead><tr>{head.map((h, c) => <th key={c} style={align[c] ? { textAlign: align[c] } : undefined}>{inline(h, c, link)}</th>)}</tr></thead>
          <tbody>{rows.map((r, ri) => <tr key={ri}>{r.map((cell, c) => <td key={c} style={align[c] ? { textAlign: align[c] } : undefined}>{inline(cell, c, link)}</td>)}</tr>)}</tbody>
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

export function BuiltinMarkdown({ text, className, link }: { text: string; className?: string; link?: (href: string) => string }) {
  return <div className={className ? `xui-md ${className}` : `xui-md`}>{renderMarkdown(text, link)}</div>
}
