import { useEffect, useRef } from "react"
import MarkdownIt from "markdown-it"
import { cn } from "@exp/ui"
import { attachmentIdFromUrl, signedAttachmentUrl, useMcpActions } from "./actions"

// EXP-1183 — issue descriptions, comments and run reports are plain GFM
// (CLAUDE.md "Markdown"). The views render it read-only through markdown-it
// with raw HTML OFF (every tag in the source is escaped), painted by the
// `.exp-markdown` rules in styles.css — the web editor's `.tiptap-content`
// typography. Two departures, both forced by the sandbox: an image is an
// authenticated `/api/attachments` URL, so it renders as a placeholder the
// component swaps for the attachment's signed URL (`exponential_attachments_get`
// through the host); links go through the host (`onOpenLink`), never a
// navigation.

const md = new MarkdownIt({ html: false, linkify: true, breaks: false })

md.renderer.rules.image = (tokens, index) => {
  const alt = md.utils.escapeHtml(tokens[index].content || `image`)
  const id = attachmentIdFromUrl(tokens[index].attrGet(`src`))
  return id
    ? `<span class="exp-markdown-image" data-attachment-id="${id}" data-alt="${alt}">${alt}</span>`
    : `<span class="exp-markdown-image">${alt}</span>`
}

const defaultLinkOpen =
  md.renderer.rules.link_open ??
  ((tokens, index, options, _env, self) =>
    self.renderToken(tokens, index, options))
md.renderer.rules.link_open = (tokens, index, options, env, self) => {
  const href = tokens[index].attrGet(`href`) ?? ``
  // Relative app links have no origin inside the sandbox.
  if (href.startsWith(`/`) && typeof env?.origin === `string`) {
    tokens[index].attrSet(`href`, `${env.origin}${href}`)
  }
  return defaultLinkOpen(tokens, index, options, env, self)
}

const TASK = /<li>(<p>)?\[( |x|X)\]\s/g

/** GFM → HTML: escaped source, task-list items as disabled checkboxes. */
export function renderMarkdown(source: string, origin?: string): string {
  return md
    .render(source, { origin })
    .replace(
      TASK,
      (_match, paragraph: string | undefined, mark: string) =>
        `<li class="exp-task" data-checked="${mark !== ` `}">${paragraph ?? ``}<input type="checkbox" disabled${mark !== ` ` ? ` checked` : ``} /> `
    )
    .replace(/<table>/g, `<div class="exp-table"><table>`)
    .replace(/<\/table>/g, `</table></div>`)
}

export function Markdown({
  source,
  origin,
  onOpenLink,
  className,
}: {
  source: string
  /** The app origin relative links resolve against. */
  origin?: string
  onOpenLink?: (url: string) => void
  className?: string
}) {
  const ref = useRef<HTMLDivElement>(null)
  const { call } = useMcpActions()
  // Attachment placeholders become pictures once their signed URL is back.
  useEffect(() => {
    const root = ref.current
    if (!root) return
    let live = true
    for (const node of root.querySelectorAll<HTMLElement>(`[data-attachment-id]`)) {
      const id = node.dataset.attachmentId
      if (!id) continue
      void signedAttachmentUrl(call, id).then((url) => {
        if (!live || !url) return
        const img = document.createElement(`img`)
        img.src = url
        img.alt = node.dataset.alt ?? ``
        img.loading = `lazy`
        node.replaceWith(img)
      })
    }
    return () => {
      live = false
    }
  }, [source, call])
  return (
    <div
      ref={ref}
      className={cn(`exp-markdown`, className)}
      onClick={(event) => {
        const anchor = (event.target as HTMLElement).closest(`a`)
        if (!anchor) return
        event.preventDefault()
        const href = anchor.getAttribute(`href`)
        if (href && /^https?:/i.test(href)) onOpenLink?.(href)
      }}
      dangerouslySetInnerHTML={{ __html: renderMarkdown(source, origin) }}
    />
  )
}
