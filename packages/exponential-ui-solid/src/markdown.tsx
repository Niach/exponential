// FEED-128: the built-in markdown painter on Solid (React's markdown.tsx):
// the web layer's parser (`parseMarkdown`) → the same elements and classes
// through the theme's `Markdown/<part>` recipes. A host with a real
// renderer passes it as `host.Markdown` and this one steps aside. Every
// href passes the URL policy (denied = the label as text), every image the
// media policy; a paragraph that is one image is a BLOCK image. #VAPP-258:
// a fenced block whose fence names a catalog language paints CodeBlock's
// tokens (`markdownCodeLines`) in the `CodeBlock/token {kind}` recipe's
// colours (the surface's scope on each span; base-css keeps it to the
// colour), the box as before.

import { For, Show, createMemo, useContext, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { linkedMarkdown, linkHref, markdownCodeLines, mediaRequestOf, parseMarkdown, partClass, propAttribute, type MarkdownBlock, type MarkdownInline, type MarkdownList } from "@exponential_app/eui/web"
import { SurfaceContext } from "./context"
import type { HostPlugin } from "./host"
import { createMediaSource } from "./media"
import { css } from "./style"

export { MAX_LINK_NESTING, MAX_LINK_DEST, MAX_BLOCK_NESTING, linkAt, wholeImage, parseList } from "@exponential_app/eui/web"

/** A block image through the media policy and loader (alt text while it
 *  loads or when it fails); a DENIED src is a paragraph of its alt text. */
function MarkdownImage(props: { host: HostPlugin; src: string; alt: string }): JSX.Element {
  const media = createMediaSource(() => props.host, () => props.src)
  return (
    <Show
      when={mediaRequestOf(props.host, props.src) !== null}
      fallback={props.alt ? <p class="xui-Markdown-paragraph">{props.alt}</p> : null}
    >
      <div class="xui-Markdown-image" role="img" aria-label={props.alt}>
        {media().url && !media().error ? <img alt="" src={media().url} /> : <span class="xui-Markdown-imageAlt">{props.alt}</span>}
      </div>
    </Show>
  )
}

/** A policed markdown link: a host with `openUrl` opens it. */
function MarkdownLink(props: { host: HostPlugin; href: string; children: JSX.Element }): JSX.Element {
  const onClick = (e: MouseEvent) => {
    if (!props.host.openUrl) return
    e.preventDefault()
    props.host.openUrl(props.href)
  }
  return (
    <a class="xui-Markdown-link" href={props.href} target="_blank" rel="noreferrer" onClick={onClick}>
      {props.children}
    </a>
  )
}

function inline(nodes: readonly MarkdownInline[], host: HostPlugin): JSX.Element[] {
  return nodes.map((n): JSX.Element => {
    // #VAPP-258: a hard line break is a newline in the run.
    if (typeof n === `string`) return n.includes(`\n`) ? n.split(`\n`).flatMap((line, i) => (i ? [<br />, line] : [line])) : n
    switch (n.type) {
      case `code`:
        return <code class="xui-Markdown-code">{n.text}</code>
      case `strong`:
        return <strong>{inline(n.children, host)}</strong>
      case `em`:
        return <em>{inline(n.children, host)}</em>
      case `del`:
        return <del>{n.children ? inline(n.children, host) : n.text}</del>
      case `link`: {
        const href = linkHref(host, n.dest)
        const label = inline(n.children, host)
        return href ? (
          <MarkdownLink host={host} href={href}>
            {label}
          </MarkdownLink>
        ) : (
          <span class="xui-Markdown-link" data-denied="">
            {label}
          </span>
        )
      }
    }
    return null
  })
}

function renderList(list: MarkdownList, host: HostPlugin): JSX.Element {
  const items = list.items.map((item) => (
    <li class="xui-Markdown-listItem" data-task={item.task === undefined ? undefined : item.task ? `done` : `todo`}>
      {item.task === undefined ? null : <input type="checkbox" checked={item.task} attr:checked={item.task ? `` : undefined} readOnly />}
      {item.task === undefined ? null : ` `}
      {inline(item.content, host)}
      {item.children.map((c) => renderList(c, host))}
    </li>
  ))
  return list.ordered ? <ol start={list.start === 1 ? undefined : list.start}>{items}</ol> : <ul>{items}</ul>
}

/** A fenced block's text: CodeBlock's token spans (`CodeBlock/token`
 *  under the theme `scope`, `kind` as its recipe prop) when the fence names
 *  a highlighted language, else the text as it is. */
function codeRuns(b: { lang?: string; text: string }, scope: string): JSX.Element {
  const lines = markdownCodeLines(b)
  if (!lines) return b.text
  const token = `${scope} ${partClass(`CodeBlock`, `token`)}`.trim()
  const kind = propAttribute(false, `kind`)
  return lines.flatMap((tokens, i): JSX.Element[] => [
    i ? `\n` : null,
    ...tokens.map((t) => (t.kind === `plain` ? t.text : <span class={token} {...{ [kind]: t.kind }} data-token={t.kind}>{t.text}</span>)),
  ])
}

function block(b: MarkdownBlock, host: HostPlugin, scope: string): JSX.Element {
  switch (b.type) {
    case `paragraph`:
      return <p class="xui-Markdown-paragraph">{inline(b.children, host)}</p>
    case `image`:
      return <MarkdownImage host={host} src={b.src} alt={b.alt} />
    case `codeBlock`:
      return (
        <pre class="xui-Markdown-codeBlock" data-lang={b.lang}>
          <code>{codeRuns(b, scope)}</code>
        </pre>
      )
    case `heading`:
      return (
        <Dynamic component={`h${Math.min(b.level, 6)}`} class="xui-Markdown-heading" data-level={b.level}>
          {inline(b.children, host)}
        </Dynamic>
      )
    case `quote`:
      return <blockquote class="xui-Markdown-quote">{blocks(b.children, host, scope)}</blockquote>
    case `list`:
      return renderList(b.list, host)
    case `table`:
      return (
        <table class="xui-Markdown-table">
          <thead>
            <tr>
              <For each={b.head}>{(h, c) => <th style={css(b.align[c()] ? { textAlign: b.align[c()]! } : undefined)}>{inline(h, host)}</th>}</For>
            </tr>
          </thead>
          <tbody>
            <For each={b.rows}>
              {(r) => (
                <tr>
                  <For each={r}>{(cell, c) => <td style={css(b.align[c()] ? { textAlign: b.align[c()]! } : undefined)}>{inline(cell, host)}</td>}</For>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      )
    case `rule`:
      return <hr />
  }
  return null
}

function blocks(tree: readonly MarkdownBlock[], host: HostPlugin, scope: string): JSX.Element[] {
  return tree.map((b) => block(b, host, scope))
}

/** The markdown tree of `text` as Solid elements (`scope` = the compiled
 *  theme's scope class: the code token colours). */
export function renderMarkdown(text: string, host: HostPlugin = {}, scope = ``): JSX.Element[] {
  return blocks(linkedMarkdown(host, parseMarkdown(text)), host, scope)
}

export function BuiltinMarkdown(props: { text: string; class?: string; host?: HostPlugin }): JSX.Element {
  const ctx = useContext(SurfaceContext)
  // Parsed once per text (and host, and theme).
  const tree = createMemo(() => renderMarkdown(props.text, props.host, ctx?.compiled.scope ?? ``))
  return <div class={props.class ? `xui-md ${props.class}` : `xui-md`}>{tree()}</div>
}
