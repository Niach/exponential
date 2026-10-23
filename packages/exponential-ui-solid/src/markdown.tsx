// FEED-128: the built-in markdown painter on Solid (React's markdown.tsx):
// the web layer's parser (`parseMarkdown`) → the same elements and classes
// through the theme's `Markdown/<part>` recipes. A host with a real
// renderer passes it as `host.Markdown` and this one steps aside. Every
// href passes the URL policy (denied = the label as text), every image the
// media policy; a paragraph that is one image is a BLOCK image.

import { For, Show, createMemo, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { linkHref, mediaRequestOf, parseMarkdown, type MarkdownBlock, type MarkdownInline, type MarkdownList } from "@exponential-at/ui/web"
import type { HostPlugin } from "./host"
import { createMediaSource } from "./media"
import { css } from "./style"

export { MAX_LINK_NESTING, MAX_LINK_DEST, MAX_BLOCK_NESTING, linkAt, wholeImage, parseList } from "@exponential-at/ui/web"

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
    if (typeof n === `string`) return n
    switch (n.type) {
      case `code`:
        return <code class="xui-Markdown-code">{n.text}</code>
      case `strong`:
        return <strong>{inline(n.children, host)}</strong>
      case `em`:
        return <em>{inline(n.children, host)}</em>
      case `del`:
        return <del>{n.text}</del>
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
  return list.ordered ? <ol>{items}</ol> : <ul>{items}</ul>
}

function block(b: MarkdownBlock, host: HostPlugin): JSX.Element {
  switch (b.type) {
    case `paragraph`:
      return <p class="xui-Markdown-paragraph">{inline(b.children, host)}</p>
    case `image`:
      return <MarkdownImage host={host} src={b.src} alt={b.alt} />
    case `codeBlock`:
      return (
        <pre class="xui-Markdown-codeBlock" data-lang={b.lang}>
          <code>{b.text}</code>
        </pre>
      )
    case `heading`:
      return (
        <Dynamic component={`h${Math.min(b.level, 6)}`} class="xui-Markdown-heading" data-level={b.level}>
          {inline(b.children, host)}
        </Dynamic>
      )
    case `quote`:
      return <blockquote class="xui-Markdown-quote">{blocks(b.children, host)}</blockquote>
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

function blocks(tree: readonly MarkdownBlock[], host: HostPlugin): JSX.Element[] {
  return tree.map((b) => block(b, host))
}

/** The markdown tree of `text` as Solid elements. */
export function renderMarkdown(text: string, host: HostPlugin = {}): JSX.Element[] {
  return blocks(parseMarkdown(text), host)
}

export function BuiltinMarkdown(props: { text: string; class?: string; host?: HostPlugin }): JSX.Element {
  // Parsed once per text (and host).
  const tree = createMemo(() => renderMarkdown(props.text, props.host))
  return <div class={props.class ? `xui-md ${props.class}` : `xui-md`}>{tree()}</div>
}
