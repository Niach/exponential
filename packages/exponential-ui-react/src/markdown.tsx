// VAPP-87: the built-in markdown painter. The parser (a small GFM subset →
// a block/inline tree) is the web layer's (`@exponential_app/eui/web`
// markdown.ts, FEED-128); this paints the tree through the theme's
// `Markdown/<part>` recipes. A host with a real renderer (the app's TipTap
// view) passes it as `host.Markdown` and this one steps aside. VAPP-103:
// every href passes the URL policy (denied = the label as text), every
// image the media policy. A paragraph that is one image is a BLOCK image
// (denied = a paragraph of its alt text); an image inside running text is
// its alt text, as on every renderer. #VAPP-258: a fenced block whose fence
// names a catalog language paints CodeBlock's tokens (`markdownCodeLines`)
// in the `CodeBlock/token {kind}` recipe's colours (the surface's scope on
// each span; base-css keeps it to the colour), the box as before.

import { Fragment, createElement, useContext, useMemo, type MouseEvent, type ReactNode } from "react"
import { linkedMarkdown, linkHref, markdownCodeLines, mediaRequestOf, parseMarkdown, partClass, propAttribute, type MarkdownBlock, type MarkdownInline, type MarkdownList } from "@exponential_app/eui/web"
import { SurfaceContext } from "./context"
import type { HostPlugin } from "./host"
import { useMediaSource } from "./media"

export { MAX_LINK_NESTING, MAX_LINK_DEST, MAX_BLOCK_NESTING, linkAt, wholeImage, parseList } from "@exponential_app/eui/web"

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

function inline(nodes: readonly MarkdownInline[], host: HostPlugin): ReactNode[] {
  return nodes.map((n, k) => {
    // #VAPP-258: a hard line break is a newline in the run.
    if (typeof n === `string`) return n.includes(`\n`) ? <Fragment key={k}>{n.split(`\n`).flatMap((line, i) => (i ? [<br key={i} />, line] : [line]))}</Fragment> : n
    switch (n.type) {
      case `code`:
        return <code key={k} className="xui-Markdown-code">{n.text}</code>
      case `strong`:
        return <strong key={k}>{inline(n.children, host)}</strong>
      case `em`:
        return <em key={k}>{inline(n.children, host)}</em>
      case `del`:
        return <del key={k}>{n.children ? inline(n.children, host) : n.text}</del>
      case `link`: {
        const href = linkHref(host, n.dest)
        const label = inline(n.children, host)
        return href ? <MarkdownLink key={k} host={host} href={href}>{label}</MarkdownLink> : <span key={k} className="xui-Markdown-link" data-denied="">{label}</span>
      }
    }
  })
}

function renderList(list: MarkdownList, host: HostPlugin, key: number): ReactNode {
  const items = list.items.map((item, n) => (
    <li key={n} className="xui-Markdown-listItem" data-task={item.task === undefined ? undefined : item.task ? `done` : `todo`}>
      {item.task === undefined ? null : <input type="checkbox" checked={item.task} readOnly />}
      {item.task === undefined ? null : ` `}
      {inline(item.content, host)}
      {item.children.map((c, ci) => <Fragment key={`c${ci}`}>{renderList(c, host, ci)}</Fragment>)}
    </li>
  ))
  return list.ordered ? <ol key={key} start={list.start === 1 ? undefined : list.start}>{items}</ol> : <ul key={key}>{items}</ul>
}

/** A fenced block's text: CodeBlock's token spans (`CodeBlock/token`
 *  under the theme `scope`, `kind` as its recipe prop) when the fence names
 *  a highlighted language, else the text as it is. */
function codeRuns(b: { lang?: string; text: string }, scope: string): ReactNode {
  const lines = markdownCodeLines(b)
  if (!lines) return b.text
  const token = `${scope} ${partClass(`CodeBlock`, `token`)}`.trim()
  const kind = propAttribute(false, `kind`)
  return lines.flatMap((tokens, i) => [
    i ? `\n` : null,
    ...tokens.map((t, j) => (t.kind === `plain` ? t.text : <span key={`${i}.${j}`} className={token} {...{ [kind]: t.kind }} data-token={t.kind}>{t.text}</span>)),
  ])
}

function block(b: MarkdownBlock, host: HostPlugin, key: number, scope: string): ReactNode {
  switch (b.type) {
    case `paragraph`:
      return <p key={key} className="xui-Markdown-paragraph">{inline(b.children, host)}</p>
    case `image`:
      return <MarkdownImage key={key} host={host} src={b.src} alt={b.alt} />
    case `codeBlock`:
      return <pre key={key} className="xui-Markdown-codeBlock" data-lang={b.lang}><code>{codeRuns(b, scope)}</code></pre>
    case `heading`:
      return createElement(`h${Math.min(b.level, 6)}`, { key, className: `xui-Markdown-heading`, "data-level": b.level }, ...inline(b.children, host))
    case `quote`:
      return <blockquote key={key} className="xui-Markdown-quote">{blocks(b.children, host, scope)}</blockquote>
    case `list`:
      return renderList(b.list, host, key)
    case `table`:
      return (
        <table key={key} className="xui-Markdown-table">
          <thead><tr>{b.head.map((h, c) => <th key={c} style={b.align[c] ? { textAlign: b.align[c] } : undefined}>{inline(h, host)}</th>)}</tr></thead>
          <tbody>{b.rows.map((r, ri) => <tr key={ri}>{r.map((cell, c) => <td key={c} style={b.align[c] ? { textAlign: b.align[c] } : undefined}>{inline(cell, host)}</td>)}</tr>)}</tbody>
        </table>
      )
    case `rule`:
      return <hr key={key} />
  }
}

function blocks(tree: readonly MarkdownBlock[], host: HostPlugin, scope: string): ReactNode[] {
  return tree.map((b, k) => <Fragment key={k}>{block(b, host, k, scope)}</Fragment>)
}

/** `scope` = the compiled theme's scope class (the code token colours). */
export function renderMarkdown(text: string, host: HostPlugin = {}, scope = ``): ReactNode[] {
  return blocks(linkedMarkdown(host, parseMarkdown(text)), host, scope)
}

const NO_HOST: HostPlugin = {}

export function BuiltinMarkdown({ text, className, host = NO_HOST }: { text: string; className?: string; host?: HostPlugin }) {
  // Parsed once per text, not on every parent render; the host's text links
  // read live, like a `hostLinks` Text's.
  const tree = useMemo(() => parseMarkdown(text), [text])
  const scope = useContext(SurfaceContext)?.compiled.scope ?? ``
  return <div className={className ? `xui-md ${className}` : `xui-md`}>{blocks(linkedMarkdown(host, tree), host, scope)}</div>
}
