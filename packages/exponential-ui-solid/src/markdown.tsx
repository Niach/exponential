// FEED-128: the built-in markdown painter on Solid (React's markdown.tsx):
// the web layer's parser (`parseMarkdown`) → the same elements and classes
// through the theme's `Markdown/<part>` recipes. A host with a real
// renderer passes it as `host.Markdown` and this one steps aside. Every
// href passes the URL policy (denied = the label as text), every image the
// media policy; a paragraph that is one image is a BLOCK image. #VAPP-258:
// a fenced block whose fence names a catalog language paints CodeBlock's
// tokens (`markdownCodeLines`) in the `CodeBlock/token {kind}` recipe's
// colours (the surface's scope on each span; base-css keeps it to the
// colour), the box as before. #VAPP-258 (catalog/host.json `markdown`): a
// host text link with a `chip` paints the catalog Chip's look inline; a
// block image takes the host's `imageSize` aspect and calls `onImagePress`;
// a one-link paragraph the host's `attachment` classifies is the Video /
// AudioPlayer native or the FileUpload file row (React's markdown.tsx).

import { For, Show, createMemo, useContext, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { linkedMarkdown, linkHref, markdownCodeLines, mediaRequestOf, parseMarkdown, partClass, propAttribute, type MarkdownBlock, type MarkdownChip, type MarkdownInline, type MarkdownList } from "@exponential_app/eui/web"
import type { UiNode } from "@exponential_app/eui"
import { SurfaceContext } from "./context"
import type { HostPlugin } from "./host"
import { IconGlyph } from "./icons"
import { createMediaSource } from "./media"
import { NodeView } from "./node-view"
import { BuiltinIcon } from "./natives/shared"
import { css } from "./style"

export { MAX_LINK_NESTING, MAX_LINK_DEST, MAX_BLOCK_NESTING, linkAt, wholeImage, parseList } from "@exponential_app/eui/web"

/** A block image through the media policy and loader (alt text while it
 *  loads or when it fails); a DENIED src is a paragraph of its alt text. */
function MarkdownImage(props: { host: HostPlugin; src: string; alt: string; width?: number; height?: number }): JSX.Element {
  const media = createMediaSource(() => props.host, () => props.src)
  // #VAPP-258: a host-sized image keeps its aspect at its natural size,
  // capped by the width and the 160px max height (markdownImageBox: the
  // height cap is a width cap at that aspect).
  const sized = () => (props.width !== undefined && props.height !== undefined ? { width: `${Math.min(props.width, (160 * props.width) / props.height)}px`, "max-width": `100%`, height: `auto`, "aspect-ratio": `${props.width} / ${props.height}` } : undefined)
  const press = () => props.host.onImagePress
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== `Enter` && e.key !== ` `) return
    e.preventDefault()
    press()?.(props.src)
  }
  return (
    <Show
      when={mediaRequestOf(props.host, props.src) !== null}
      fallback={props.alt ? <p class="xui-Markdown-paragraph">{props.alt}</p> : null}
    >
      <div
        class="xui-Markdown-image"
        role={press() ? `button` : `img`}
        tabIndex={press() ? 0 : undefined}
        data-pressable={press() ? `` : undefined}
        data-sized={sized() ? `` : undefined}
        style={sized()}
        onClick={press() ? () => press()!(props.src) : undefined}
        onKeyDown={press() ? onKeyDown : undefined}
        aria-label={props.alt}
      >
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

/** #VAPP-258: a host's chip, the catalog Chip's look inline (Chip/root +
 *  Chip/label under the theme scope, the default recipe props). */
function MarkdownChipLink(props: { host: HostPlugin; href: string; chip: MarkdownChip; scope: string }): JSX.Element {
  const onClick = (e: MouseEvent) => {
    e.stopPropagation()
    if (!props.host.openUrl) return
    e.preventDefault()
    props.host.openUrl(props.href)
  }
  const recipe = { [propAttribute(true, `shape`)]: `pill`, [propAttribute(true, `selected`)]: `false`, [propAttribute(true, `tone`)]: `neutral` }
  return (
    <a class={`xui-md-chip ${props.scope} ${partClass(`Chip`, `root`)}`.replace(/  +/g, ` `)} data-xui-part="Chip/root" data-chip={props.chip.kind} {...recipe} href={props.href} target="_blank" rel="noreferrer" onClick={onClick}>
      {props.chip.icon ? <IconGlyph icons={props.host.icons} name={props.chip.icon} aria-hidden="true" /> : null}
      <span class={`${props.scope} xui-Text-root ${partClass(`Chip`, `label`)}`.replace(/  +/g, ` `)} data-xui-part="Chip/label" {...recipe} data-r-variant="caption">
        {props.chip.label}
      </span>
    </a>
  )
}

/** #VAPP-258: a one-link paragraph the host classified: the Video /
 *  AudioPlayer native (inside a surface) or the FileUpload file row; outside
 *  a surface a video or audio is that row too. */
function MarkdownAttachment(props: { host: HostPlugin; b: Extract<MarkdownBlock, { type: `attachment` }>; scope: string; id: string }): JSX.Element {
  const inSurface = useContext(SurfaceContext) !== undefined
  if (inSurface && props.b.kind !== `file`) {
    const node: UiNode = props.b.kind === `video` ? { id: props.id, component: `Video`, props: { src: props.b.href }, children: [] } : { id: props.id, component: `AudioPlayer`, props: { src: props.b.href, title: props.b.name }, children: [] }
    return (
      <div class="xui-Markdown-attachment" data-attachment={props.b.kind}>
        <NodeView node={node} />
      </div>
    )
  }
  const part = (name: string) => ({ class: `${props.scope} ${partClass(`FileUpload`, name)}`.trim(), "data-xui-part": `FileUpload/${name}` })
  const onClick = (e: MouseEvent) => {
    if (!props.host.openUrl) return
    e.preventDefault()
    props.host.openUrl(props.b.href)
  }
  return (
    <div class="xui-Markdown-attachment" data-attachment={props.b.kind}>
      <div {...part(`file`)}>
        <span {...part(`fileIcon`)}>
          <BuiltinIcon slot="FileUpload.file" />
        </span>
        <span {...part(`fileName`)}>
          <a href={props.b.href} target="_blank" rel="noreferrer" onClick={onClick}>
            {props.b.name}
          </a>
        </span>
      </div>
    </div>
  )
}

function inline(nodes: readonly MarkdownInline[], host: HostPlugin, scope = ``): JSX.Element[] {
  return nodes.map((n): JSX.Element => {
    // #VAPP-258: a hard line break is a newline in the run.
    if (typeof n === `string`) return n.includes(`\n`) ? n.split(`\n`).flatMap((line, i) => (i ? [<br />, line] : [line])) : n
    switch (n.type) {
      case `code`:
        return <code class="xui-Markdown-code">{n.text}</code>
      case `strong`:
        return <strong>{inline(n.children, host, scope)}</strong>
      case `em`:
        return <em>{inline(n.children, host, scope)}</em>
      case `del`:
        return <del>{n.children ? inline(n.children, host, scope) : n.text}</del>
      case `link`: {
        const href = linkHref(host, n.dest)
        if (href && n.chip) return <MarkdownChipLink host={host} href={href} chip={n.chip} scope={scope} />
        const label = inline(n.children, host, scope)
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

function renderList(list: MarkdownList, host: HostPlugin, scope: string): JSX.Element {
  const items = list.items.map((item) => (
    <li class="xui-Markdown-listItem" data-task={item.task === undefined ? undefined : item.task ? `done` : `todo`}>
      {item.task === undefined ? null : <input type="checkbox" checked={item.task} attr:checked={item.task ? `` : undefined} readOnly />}
      {item.task === undefined ? null : ` `}
      {inline(item.content, host, scope)}
      {item.children.map((c) => renderList(c, host, scope))}
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

function block(b: MarkdownBlock, host: HostPlugin, scope: string, id: string, key: number): JSX.Element {
  switch (b.type) {
    case `paragraph`:
      return <p class="xui-Markdown-paragraph">{inline(b.children, host, scope)}</p>
    case `image`:
      return <MarkdownImage host={host} src={b.src} alt={b.alt} width={b.width} height={b.height} />
    case `attachment`:
      return <MarkdownAttachment host={host} b={b} scope={scope} id={`${id}.attachment.${key}`} />
    case `codeBlock`:
      return (
        <pre class="xui-Markdown-codeBlock" data-lang={b.lang}>
          <code>{codeRuns(b, scope)}</code>
        </pre>
      )
    case `heading`:
      return (
        <Dynamic component={`h${Math.min(b.level, 6)}`} class="xui-Markdown-heading" data-level={b.level}>
          {inline(b.children, host, scope)}
        </Dynamic>
      )
    case `quote`:
      return <blockquote class="xui-Markdown-quote">{blocks(b.children, host, scope, id)}</blockquote>
    case `list`:
      return renderList(b.list, host, scope)
    case `table`:
      return (
        <table class="xui-Markdown-table">
          <thead>
            <tr>
              <For each={b.head}>{(h, c) => <th style={css(b.align[c()] ? { textAlign: b.align[c()]! } : undefined)}>{inline(h, host, scope)}</th>}</For>
            </tr>
          </thead>
          <tbody>
            <For each={b.rows}>
              {(r) => (
                <tr>
                  <For each={r}>{(cell, c) => <td style={css(b.align[c()] ? { textAlign: b.align[c()]! } : undefined)}>{inline(cell, host, scope)}</td>}</For>
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

function blocks(tree: readonly MarkdownBlock[], host: HostPlugin, scope: string, id: string): JSX.Element[] {
  return tree.map((b, k) => block(b, host, scope, id, k))
}

/** The markdown tree of `text` as Solid elements (`scope` = the compiled
 *  theme's scope class: the code token colours, the chip and file row
 *  recipes; `id` = the Markdown node's, its attachments' node ids). */
export function renderMarkdown(text: string, host: HostPlugin = {}, scope = ``, id = `md`): JSX.Element[] {
  return blocks(linkedMarkdown(host, parseMarkdown(text, host.markdownOptions)), host, scope, id)
}

export function BuiltinMarkdown(props: { text: string; class?: string; host?: HostPlugin; id?: string }): JSX.Element {
  const ctx = useContext(SurfaceContext)
  // Parsed once per text (and host, and theme). #VAPP-258 step 5: the host's
  // options (catalog/host.json `markdown.options`); the blocks render keyed
  // by object, so a streamed text's finished blocks (the parser's memo keeps
  // their objects) keep their DOM while the tail re-renders.
  const options = () => props.host?.markdownOptions
  const tree = createMemo(() => {
    const o = options()
    return linkedMarkdown(props.host ?? {}, parseMarkdown(props.text, { breaks: o?.breaks, streaming: o?.streaming === true }))
  })
  const scope = () => ctx?.compiled.scope ?? ``
  return (
    <div class={props.class ? `xui-md ${props.class}` : `xui-md`} data-selectable={options()?.selectable === false ? `false` : undefined}>
      {(() => {
        // A theme change (its scope) re-renders every block.
        const s = scope()
        return <For each={tree()}>{(b, k) => block(b, props.host ?? {}, s, props.id ?? `md`, k())}</For>
      })()}
    </div>
  )
}
