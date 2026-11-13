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

import { For, Show, createMemo, createSignal, onCleanup, useContext, type JSX } from "solid-js"
import { Dynamic } from "solid-js/web"
import { MARKDOWN_CELL_TEXT, watchMarkdownTable, linkedMarkdown, linkHref, markdownChipOverlay, markdownPartAttrs, markdownProfileHost, loadedImageSize, markdownCodeLines, markdownImageCap, markdownImageSizeOf, markdownImageStyle, mediaRequestOf, rememberImageSize, parseMarkdown, partClass, propAttribute, type MarkdownBlock, type MarkdownChip, type MarkdownInline, type MarkdownList } from "@exponential_app/eui/web"
import type { MarkdownVariant } from "@exponential_app/eui/web"
import type { UiNode } from "@exponential_app/eui"
import { SurfaceContext } from "./context"
import type { HostPlugin } from "./host"
import { IconGlyph } from "./icons"
import { createMediaSource } from "./media"
import { NodeView } from "./node-view"
import { BuiltinIcon } from "./natives/shared"
import { css } from "./style"

/** #VAPP-258: a `Markdown/<part>` element's recipe attributes (web
 *  markdown-parts.ts): the theme scope class and the painter-supplied recipe
 *  props (`density`, a heading's `level`), so the theme's part rules reach
 *  every part. */
function mdPart(host: HostPlugin, scope: string, name: string, extra?: Record<string, string | number>): Record<string, string> {
  const p = markdownPartAttrs(scope, name, host.markdownOptions?.density, extra, (host as { markdownInline?: MarkdownVariant }).markdownInline)
  return { class: p.class, ...p.attrs }
}

const styleOf = (decls: [string, string][]): Record<string, string> | undefined => (decls.length ? Object.fromEntries(decls) : undefined)

export { MAX_LINK_NESTING, MAX_LINK_DEST, MAX_BLOCK_NESTING, linkAt, wholeImage, parseList } from "@exponential_app/eui/web"

/** A block image through the media policy and loader (alt text while it
 *  loads or when it fails; an unknown size takes the decoded aspect once
 *  loaded); a DENIED src is a paragraph of its alt text. */
function MarkdownImage(props: { host: HostPlugin; src: string; alt: string; width?: number; height?: number; scope?: string }): JSX.Element {
  const media = createMediaSource(() => props.host, () => props.src)
  // #VAPP-258: a host-sized image keeps its aspect, capped by the width and
  // the host's `imageMaxHeight` (markdownImageStyle; null = no cap, the
  // width at its aspect up to the viewport's height).
  // #VAPP-258 (markdown-host.json `loaded`): a size the host does not know
  // is the decoded one once the picture loads (one relayout to its real
  // aspect, remembered for the next mount); the placeholder box until then.
  const [loaded, setLoaded] = createSignal<{ src: string; size?: { width: number; height: number } }>()
  const size = () => {
    const l = loaded()
    return markdownImageSizeOf(props.width !== undefined && props.height !== undefined ? { width: props.width, height: props.height } : null, l?.src === props.src && l.size ? l.size : loadedImageSize(props.src))
  }
  const sized = () => markdownImageStyle(size()?.width, size()?.height, markdownImageCap(props.host.markdownOptions))
  const press = () => props.host.onImagePress
  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key !== `Enter` && e.key !== ` `) return
    e.preventDefault()
    press()?.(props.src)
  }
  return (
    <Show
      when={mediaRequestOf(props.host, props.src) !== null}
      fallback={props.alt ? <p {...mdPart(props.host, props.scope ?? ``, `paragraph`)}>{props.alt}</p> : null}
    >
      <div
        {...mdPart(props.host, props.scope ?? ``, `image`)}
        role={press() ? `button` : `img`}
        tabIndex={press() ? 0 : undefined}
        data-pressable={press() ? `` : undefined}
        data-sized={sized() ? `` : undefined}
        style={sized()}
        onClick={press() ? () => press()!(props.src) : undefined}
        onKeyDown={press() ? onKeyDown : undefined}
        aria-label={props.alt}
      >
        {media().url && !media().error ? <img alt="" src={media().url} onLoad={(e) => setLoaded({ src: props.src, size: rememberImageSize(props.src, e.currentTarget.naturalWidth, e.currentTarget.naturalHeight) })} /> : <span class="xui-Markdown-imageAlt">{props.alt}</span>}
      </div>
    </Show>
  )
}

/** A policed markdown link: a host with `openUrl` opens it. */
function MarkdownLink(props: { host: HostPlugin; href: string; scope: string; children: JSX.Element }): JSX.Element {
  const onClick = (e: MouseEvent) => {
    if (!props.host.openUrl) return
    e.preventDefault()
    props.host.openUrl(props.href)
  }
  return (
    <a {...mdPart(props.host, props.scope, `link`)} href={props.href} target="_blank" rel="noreferrer" onClick={onClick}>
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
  // #VAPP-258: the `Markdown/chip` + `Markdown/chipLabel` parts (their
  // `chip` kind) over the Chip's look: classes + attributes for the sheet,
  // the theme's rules for this kind inline (they win over Chip's).
  const ctx = useContext(SurfaceContext)
  const density = props.host.markdownOptions?.density
  const { "data-xui-part": _part, ...own } = markdownPartAttrs(props.scope, `chip`, density, { chip: props.chip.kind }).attrs
  const { "data-xui-part": _labelPart, ...ownLabel } = markdownPartAttrs(props.scope, `chipLabel`, density, { chip: props.chip.kind }).attrs
  const ownIdent = markdownPartAttrs(props.scope, `chipIdent`, density, { chip: props.chip.kind }).attrs
  return (
    <a class={`xui-md-chip ${props.scope} ${partClass(`Chip`, `root`)} ${partClass(`Markdown`, `chip`)}`.replace(/  +/g, ` `)} data-xui-part="Chip/root" data-chip={props.chip.kind} {...recipe} {...own} style={styleOf(markdownChipOverlay(ctx?.theme, `chip`, props.chip.kind, density))} href={props.href} target="_blank" rel="noreferrer" onClick={onClick}>
      {props.chip.icon ? <IconGlyph icons={props.host.icons} name={props.chip.icon} aria-hidden="true" /> : null}
      {props.chip.ident ? (
        <span class={`${props.scope} ${partClass(`Markdown`, `chipIdent`)}`.trim()} {...ownIdent} style={styleOf(markdownChipOverlay(ctx?.theme, `chipIdent`, props.chip.kind, density))}>
          {props.chip.ident}
        </span>
      ) : null}
      <span class={`${props.scope} xui-Text-root ${partClass(`Chip`, `label`)} ${partClass(`Markdown`, `chipLabel`)}`.replace(/  +/g, ` `)} data-xui-part="Chip/label" {...recipe} {...ownLabel} data-r-variant="caption" style={styleOf(markdownChipOverlay(ctx?.theme, `chipLabel`, props.chip.kind, density))}>
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
        return <code {...mdPart(host, scope, `code`)}>{n.text}</code>
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
          <MarkdownLink host={host} href={href} scope={scope}>
            {label}
          </MarkdownLink>
        ) : (
          <span {...mdPart(host, scope, `link`)} data-denied="">
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
    <li {...mdPart(host, scope, `listItem`)} data-task={item.task === undefined ? undefined : item.task ? `done` : `todo`}>
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
      return <p {...mdPart(host, scope, `paragraph`)}>{inline(b.children, host, scope)}</p>
    case `image`:
      return <MarkdownImage host={host} src={b.src} alt={b.alt} width={b.width} height={b.height} scope={scope} />
    case `attachment`:
      return <MarkdownAttachment host={host} b={b} scope={scope} id={`${id}.attachment.${key}`} />
    case `codeBlock`:
      return (
        <pre {...mdPart(host, scope, `codeBlock`)} data-lang={b.lang}>
          <code>{codeRuns(b, scope)}</code>
        </pre>
      )
    case `heading`:
      return (
        <Dynamic component={`h${Math.min(b.level, 6)}`} {...mdPart(host, scope, `heading`, { level: b.level })} data-level={b.level}>
          {inline(b.children, host, scope)}
        </Dynamic>
      )
    case `quote`:
      return <blockquote {...mdPart(host, scope, `quote`)}>{blocks(b.children, host, scope, id)}</blockquote>
    case `list`:
      return renderList(b.list, host, scope)
    case `table`:
      // #FEED-224: the columns fit by fixtures/table-fit.json (web/table-fit.ts).
      return (
        <div class="xui-Markdown-tableScroll" ref={(el) => onCleanup(watchMarkdownTable(el))}>
          <table {...mdPart(host, scope, `table`)}>
            <thead>
              <tr>
                <For each={b.head}>{(h, c) => <th {...mdPart(host, scope, `headerCell`)} style={css(b.align[c()] ? { textAlign: b.align[c()]! } : undefined)}><div class={MARKDOWN_CELL_TEXT}>{inline(h, host, scope)}</div></th>}</For>
              </tr>
            </thead>
            <tbody>
              <For each={b.rows}>
                {(r) => (
                  <tr>
                    <For each={r}>{(cell, c) => <td {...mdPart(host, scope, `cell`)} style={css(b.align[c()] ? { textAlign: b.align[c()]! } : undefined)}><div class={MARKDOWN_CELL_TEXT}>{inline(cell, host, scope)}</div></td>}</For>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
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

export function BuiltinMarkdown(props: { text: string; class?: string; host?: HostPlugin; id?: string; inline?: MarkdownVariant }): JSX.Element {
  const ctx = useContext(SurfaceContext)
  const host = createMemo(() => markdownProfileHost(props.host ?? {}, props.inline))
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
        return <For each={tree()}>{(b, k) => block(b, host(), s, props.id ?? `md`, k())}</For>
      })()}
    </div>
  )
}
