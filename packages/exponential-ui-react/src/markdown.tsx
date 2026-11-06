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
// #VAPP-258 (catalog/host.json `markdown`): a host text link with a `chip`
// paints the catalog Chip's look inline (`.xui-md-chip`, the Chip/root and
// Chip/label recipes); a block image takes the host's `imageSize` aspect
// and calls `onImagePress`; a one-link paragraph the host's `attachment`
// classifies is the Video / AudioPlayer native or the FileUpload file row.

import { Fragment, createElement, memo, useContext, useMemo, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react"
import { linkedMarkdown, linkHref, loadedImageSize, markdownCodeLines, markdownImageCap, markdownImageSizeOf, markdownImageStyle, mediaRequestOf, rememberImageSize, parseMarkdown, partClass, propAttribute, type MarkdownBlock, type MarkdownChip, type MarkdownInline, type MarkdownList } from "@exponential_app/eui/web"
import type { UiNode } from "@exponential_app/eui"
import { SurfaceContext } from "./context"
import type { HostPlugin } from "./host"
import { IconGlyph } from "./icons"
import { useMediaSource } from "./media"
import { NodeView } from "./node-view"
import { BuiltinIcon } from "./natives/shared"

export { MAX_LINK_NESTING, MAX_LINK_DEST, MAX_BLOCK_NESTING, linkAt, wholeImage, parseList } from "@exponential_app/eui/web"

/** A block image through the media policy and loader in an `imageHeight`
 *  (160px unless the host's `imageMaxHeight` says otherwise, ×5) box (the
 *  alt text there while it loads or when it fails; an unknown size takes the
 *  decoded aspect once loaded). A src the policy
 *  DENIES is a paragraph of its alt text (nothing without one). */
function MarkdownImage({ host, src, alt, width, height, scope = `` }: { host: HostPlugin; src: string; alt: string; width?: number; height?: number; scope?: string }) {
  const media = useMediaSource(host, src)
  // #VAPP-258 (markdown-host.json `loaded`): a size the host does not know
  // is the decoded one once the picture loads (one relayout to its real
  // aspect, remembered for the next mount); the placeholder box until then.
  const [loaded, setLoaded] = useState<{ src: string; size?: { width: number; height: number } }>()
  if (mediaRequestOf(host, src) === null) return alt ? <p className="xui-Markdown-paragraph">{alt}</p> : null
  // #VAPP-258: a host-sized image keeps its aspect, capped by the width and
  // the host's `imageMaxHeight` (markdownImageStyle; null = no cap, the
  // width at its aspect up to the viewport's height).
  const size = markdownImageSizeOf(width !== undefined && height !== undefined ? { width, height } : null, loaded?.src === src && loaded.size ? loaded.size : loadedImageSize(src))
  const css = markdownImageStyle(size?.width, size?.height, markdownImageCap(host.markdownOptions))
  const sized = css ? Object.fromEntries(Object.entries(css).map(([k, v]) => [k.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase()), v])) : undefined
  const press = host.onImagePress
  const pressable = press
    ? {
        role: `button`,
        tabIndex: 0,
        "data-pressable": ``,
        onClick: () => press(src),
        onKeyDown: (e: KeyboardEvent<HTMLDivElement>) => {
          if (e.key !== `Enter` && e.key !== ` `) return
          e.preventDefault()
          press(src)
        },
      }
    : { role: `img` }
  return (
    <div className={`${scope} ${partClass(`Markdown`, `image`)}`.trim()} data-xui-part="Markdown/image" data-sized={sized ? `` : undefined} style={sized} aria-label={alt} {...pressable}>
      {media.url && !media.error ? <img alt="" src={media.url} onLoad={(e) => setLoaded({ src, size: rememberImageSize(src, e.currentTarget.naturalWidth, e.currentTarget.naturalHeight) })} /> : <span className="xui-Markdown-imageAlt">{alt}</span>}
    </div>
  )
}

/** #VAPP-258: a host's chip, the catalog Chip's look inline (Chip/root +
 *  Chip/label under the theme scope, the default recipe props). */
function MarkdownChipLink({ host, href, chip, scope }: { host: HostPlugin; href: string; chip: MarkdownChip; scope: string }) {
  const onClick = host.openUrl
    ? (e: MouseEvent<HTMLAnchorElement>) => {
        e.preventDefault()
        e.stopPropagation()
        host.openUrl!(href)
      }
    : (e: MouseEvent<HTMLAnchorElement>) => e.stopPropagation()
  const recipe = { [propAttribute(true, `shape`)]: `pill`, [propAttribute(true, `selected`)]: `false`, [propAttribute(true, `tone`)]: `neutral` }
  return (
    <a className={`xui-md-chip ${scope} ${partClass(`Chip`, `root`)}`.replace(/  +/g, ` `)} data-xui-part="Chip/root" data-chip={chip.kind} {...recipe} href={href} target="_blank" rel="noreferrer" onClick={onClick}>
      {chip.icon ? <IconGlyph icons={host.icons} name={chip.icon} aria-hidden="true" /> : null}
      <span className={`${scope} xui-Text-root ${partClass(`Chip`, `label`)}`.replace(/  +/g, ` `)} data-xui-part="Chip/label" {...recipe} data-r-variant="caption">{chip.label}</span>
    </a>
  )
}

/** #VAPP-258: a one-link paragraph the host classified: the Video /
 *  AudioPlayer native (inside a surface) or the FileUpload file row; outside
 *  a surface a video or audio is that row too. */
function MarkdownAttachment({ host, b, scope, id }: { host: HostPlugin; b: Extract<MarkdownBlock, { type: `attachment` }>; scope: string; id: string }) {
  const inSurface = useContext(SurfaceContext) !== null
  if (inSurface && b.kind !== `file`) {
    const node: UiNode = b.kind === `video` ? { id, component: `Video`, props: { src: b.href }, children: [] } : { id, component: `AudioPlayer`, props: { src: b.href, title: b.name }, children: [] }
    return <div className="xui-Markdown-attachment" data-attachment={b.kind}><NodeView node={node} /></div>
  }
  const part = (name: string) => ({ className: `${scope} ${partClass(`FileUpload`, name)}`.trim(), "data-xui-part": `FileUpload/${name}` })
  const onClick = host.openUrl
    ? (e: MouseEvent<HTMLAnchorElement>) => {
        e.preventDefault()
        host.openUrl!(b.href)
      }
    : undefined
  return (
    <div className="xui-Markdown-attachment" data-attachment={b.kind}>
      <div {...part(`file`)}>
        <span {...part(`fileIcon`)}>
          <BuiltinIcon slot="FileUpload.file" />
        </span>
        <span {...part(`fileName`)}>
          <a href={b.href} target="_blank" rel="noreferrer" onClick={onClick}>{b.name}</a>
        </span>
      </div>
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

function inline(nodes: readonly MarkdownInline[], host: HostPlugin, scope = ``): ReactNode[] {
  return nodes.map((n, k) => {
    // #VAPP-258: a hard line break is a newline in the run.
    if (typeof n === `string`) return n.includes(`\n`) ? <Fragment key={k}>{n.split(`\n`).flatMap((line, i) => (i ? [<br key={i} />, line] : [line]))}</Fragment> : n
    switch (n.type) {
      case `code`:
        return <code key={k} className="xui-Markdown-code">{n.text}</code>
      case `strong`:
        return <strong key={k}>{inline(n.children, host, scope)}</strong>
      case `em`:
        return <em key={k}>{inline(n.children, host, scope)}</em>
      case `del`:
        return <del key={k}>{n.children ? inline(n.children, host, scope) : n.text}</del>
      case `link`: {
        const href = linkHref(host, n.dest)
        if (href && n.chip) return <MarkdownChipLink key={k} host={host} href={href} chip={n.chip} scope={scope} />
        const label = inline(n.children, host, scope)
        return href ? <MarkdownLink key={k} host={host} href={href}>{label}</MarkdownLink> : <span key={k} className="xui-Markdown-link" data-denied="">{label}</span>
      }
    }
  })
}

function renderList(list: MarkdownList, host: HostPlugin, key: number, scope: string): ReactNode {
  const items = list.items.map((item, n) => (
    <li key={n} className="xui-Markdown-listItem" data-task={item.task === undefined ? undefined : item.task ? `done` : `todo`}>
      {item.task === undefined ? null : <input type="checkbox" checked={item.task} readOnly />}
      {item.task === undefined ? null : ` `}
      {inline(item.content, host, scope)}
      {item.children.map((c, ci) => <Fragment key={`c${ci}`}>{renderList(c, host, ci, scope)}</Fragment>)}
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

function block(b: MarkdownBlock, host: HostPlugin, key: number, scope: string, id: string): ReactNode {
  switch (b.type) {
    case `paragraph`:
      return <p key={key} className="xui-Markdown-paragraph">{inline(b.children, host, scope)}</p>
    case `image`:
      return <MarkdownImage key={key} host={host} src={b.src} alt={b.alt} width={b.width} height={b.height} scope={scope} />
    case `attachment`:
      return <MarkdownAttachment key={key} host={host} b={b} scope={scope} id={`${id}.attachment.${key}`} />
    case `codeBlock`:
      return <pre key={key} className="xui-Markdown-codeBlock" data-lang={b.lang}><code>{codeRuns(b, scope)}</code></pre>
    case `heading`:
      return createElement(`h${Math.min(b.level, 6)}`, { key, className: `xui-Markdown-heading`, "data-level": b.level }, ...inline(b.children, host, scope))
    case `quote`:
      return <blockquote key={key} className="xui-Markdown-quote">{blocks(b.children, host, scope, id)}</blockquote>
    case `list`:
      return renderList(b.list, host, key, scope)
    case `table`:
      return (
        <table key={key} className="xui-Markdown-table">
          <thead><tr>{b.head.map((h, c) => <th key={c} style={b.align[c] ? { textAlign: b.align[c] } : undefined}>{inline(h, host, scope)}</th>)}</tr></thead>
          <tbody>{b.rows.map((r, ri) => <tr key={ri}>{r.map((cell, c) => <td key={c} style={b.align[c] ? { textAlign: b.align[c] } : undefined}>{inline(cell, host, scope)}</td>)}</tr>)}</tbody>
        </table>
      )
    case `rule`:
      return <hr key={key} />
  }
}

function blocks(tree: readonly MarkdownBlock[], host: HostPlugin, scope: string, id: string): ReactNode[] {
  return tree.map((b, k) => <Fragment key={k}>{block(b, host, k, scope, id)}</Fragment>)
}

/** #VAPP-258 step 5: one top-level block, re-rendered only when its block
 *  object changes: a streamed text's finished blocks keep their objects
 *  (the parser's memo), so an append renders the tail alone. */
const TopBlock = memo(function TopBlock({ b, host, k, scope, id }: { b: MarkdownBlock; host: HostPlugin; k: number; scope: string; id: string }) {
  return <>{block(b, host, k, scope, id)}</>
})

/** `scope` = the compiled theme's scope class (the code token colours, the
 *  chip and file row recipes); `id` = the Markdown node's (its attachments'
 *  node ids). */
export function renderMarkdown(text: string, host: HostPlugin = {}, scope = ``, id = `md`): ReactNode[] {
  return blocks(linkedMarkdown(host, parseMarkdown(text, host.markdownOptions)), host, scope, id)
}

const NO_HOST: HostPlugin = {}

export function BuiltinMarkdown({ text, className, host = NO_HOST, id = `md` }: { text: string; className?: string; host?: HostPlugin; id?: string }) {
  // Parsed once per text, not on every parent render; the host's text links
  // read live, like a `hostLinks` Text's. #VAPP-258 step 5: the host's
  // options (catalog/host.json `markdown.options`): hard breaks and a
  // streamed prefix parse here, `selectable: false` turns selection off.
  const options = host.markdownOptions
  const breaks = options?.breaks
  const streaming = options?.streaming === true
  const tree = useMemo(() => parseMarkdown(text, { breaks, streaming }), [text, breaks, streaming])
  const scope = useContext(SurfaceContext)?.compiled.scope ?? ``
  const linked = linkedMarkdown(host, tree)
  return (
    <div className={className ? `xui-md ${className}` : `xui-md`} data-selectable={options?.selectable === false ? `false` : undefined}>
      {linked.map((b, k) => <TopBlock key={k} b={b} host={host} k={k} scope={scope} id={id} />)}
    </div>
  )
}
