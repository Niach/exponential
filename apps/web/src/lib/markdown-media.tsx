import { useRef, useState } from "react"
import { Node, mergeAttributes } from "@tiptap/core"
import type { Node as ProseMirrorNode } from "@tiptap/pm/model"
import type { MarkdownSerializerState } from "prosemirror-markdown"
import {
  NodeViewWrapper,
  ReactNodeViewRenderer,
  type ReactNodeViewProps,
} from "@tiptap/react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Download, Eye, Ellipsis, Trash2 } from "lucide-react"
import { attachmentCollection } from "@/lib/collections"
import {
  attachmentIdFromSrc,
  srcWithWidth,
  stripQuery,
  widthParamFromSrc,
} from "@/lib/markdown-image"
import {
  buildAttachmentPosterUrl,
  isAudioContentType,
  isInlineMediaContentType,
  isVideoContentType,
} from "@/lib/storage/issue-attachments"
import { AttachmentMediaPlayer } from "@/components/attachment-media-player"
import { ImagePreviewDialog } from "@/components/image-preview-dialog"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { cn } from "@/lib/utils"

// EXP-824 — inline video/audio in descriptions and comments.
//
// THE CONTRACT (shared by all four clients): a media embed is a PLAIN LINK
// on a paragraph of its own — `[clip.mp4](/api/attachments/{id})`, optionally
// `?w=480` exactly like images — NEVER the image form. Only a paragraph that
// consists solely of one link to an attachment URL becomes a media block; a
// link inside running text stays an ordinary link. The renderer upgrades the
// block to a `<video>` when the referenced attachment row's content type is
// `video/*`, to `<audio>` for `audio/*`, and back to a plain link for any
// other type; until the row has synced it shows a neutral placeholder.
//
// PARSE STRATEGY: tiptap-markdown renders markdown → HTML → DOM, then lets
// each extension patch the DOM (`parse.updateDOM`) before ProseMirror reads
// it. A `<p>` whose only child is an `<a>` pointing at an attachment URL is
// swapped for `<div data-media-link>`, which is the ONLY thing this node's
// `parseHTML` matches — so the Link mark never sees it and links in running
// text are untouched. Serialization writes the link and closes the block
// (the EXP-271 image lesson), so the fixtures round-trip byte-for-byte.

export const mediaLinkNodeName = `mediaLink`

const minResizeWidth = 160
const fallbackMaxResizeWidth = 4000

function isWhitespaceText(node: ChildNode) {
  return node.nodeType === 3 && /^\s*$/.test(node.textContent ?? ``)
}

/**
 * Whether a link href names one of OUR attachments: the relative
 * `/api/attachments/{id}` form, or an absolute URL on the app's own origin.
 * A foreign host's `/api/attachments/…` path is never ours (the server and
 * iOS apply the same rule), so a pasted link to it stays a plain link
 * instead of becoming a media block that can never resolve.
 */
export function isOwnAttachmentHref(href: string, origin: string): boolean {
  if (!attachmentIdFromSrc(href)) return false
  try {
    return new URL(href, origin).origin === new URL(origin).origin
  } catch {
    return false
  }
}

/**
 * Lifts every standalone attachment link paragraph into the media node's
 * DOM form. Exported for the round-trip test; runs inside tiptap-markdown's
 * parse pipeline on every setContent/paste.
 */
export function liftStandaloneAttachmentLinks(
  root: HTMLElement,
  origin: string = root.ownerDocument.defaultView?.location.origin ??
    window.location.origin
) {
  root.querySelectorAll(`p`).forEach((paragraph) => {
    // Nested containers (list items, quotes, table cells) keep their links —
    // a block player has no sane place inside them.
    if (paragraph.parentElement?.closest(`li, blockquote, td, th`)) return
    const children = Array.from(paragraph.childNodes).filter(
      (node) => !isWhitespaceText(node)
    )
    if (children.length !== 1) return
    const anchor = children[0]
    if (!(anchor instanceof Element) || anchor.tagName !== `A`) return
    // Plain-text label only: `[![img](…)](…)` and `[`code`](…)` stay links.
    if (anchor.childElementCount > 0) return
    const href = anchor.getAttribute(`href`) ?? ``
    if (!isOwnAttachmentHref(href, origin)) return
    const block = paragraph.ownerDocument.createElement(`div`)
    block.setAttribute(`data-media-link`, ``)
    block.setAttribute(`data-src`, href)
    block.setAttribute(`data-label`, anchor.textContent ?? ``)
    paragraph.replaceWith(block)
  })
}

function serializeMediaLink(
  state: MarkdownSerializerState,
  node: ProseMirrorNode
) {
  const label = state.esc(String(node.attrs.label ?? ``))
  const src = String(node.attrs.src ?? ``).replace(/[()]/g, `\\$&`)
  state.write(`[${label}](${src})`)
  state.closeBlock(node)
}

interface ResizeDrag {
  edge: `left` | `right`
  startX: number
  startWidth: number
  naturalWidth: number | null
  latestWidth: number | null
}

function MarkdownMediaNodeView({
  deleteNode,
  editor,
  node,
  selected,
  updateAttributes,
}: ReactNodeViewProps) {
  const label = typeof node.attrs.label === `string` ? node.attrs.label : ``
  const src = typeof node.attrs.src === `string` ? node.attrs.src : ``
  const attachmentId = attachmentIdFromSrc(src)
  const { data: attachments } = useLiveQuery(
    (query) =>
      attachmentId
        ? query
            .from({ a: attachmentCollection })
            .where(({ a }) => eq(a.id, attachmentId))
        : undefined,
    [attachmentId]
  )
  const attachment = attachments?.[0]
  const persistedWidth = widthParamFromSrc(src)
  const fullSizeSrc = stripQuery(src)
  const displayLabel = label || attachment?.filename || `attachment`

  const frameRef = useRef<HTMLDivElement | null>(null)
  const dragRef = useRef<ResizeDrag | null>(null)
  const [dragWidth, setDragWidth] = useState<number | null>(null)
  const [lightboxOpen, setLightboxOpen] = useState(false)

  const isVideo = attachment ? isVideoContentType(attachment.contentType) : false
  const isAudio = attachment ? isAudioContentType(attachment.contentType) : false
  const isMedia = attachment
    ? isInlineMediaContentType(attachment.contentType)
    : false
  const renderWidth = dragWidth ?? persistedWidth

  const beginResize =
    (edge: `left` | `right`) => (event: React.PointerEvent<HTMLDivElement>) => {
      if (!editor.isEditable || !frameRef.current) return
      event.preventDefault()
      event.stopPropagation()
      dragRef.current = {
        edge,
        startX: event.clientX,
        startWidth: frameRef.current.getBoundingClientRect().width,
        naturalWidth:
          typeof attachment?.width === `number` ? attachment.width : null,
        latestWidth: null,
      }
      event.currentTarget.setPointerCapture(event.pointerId)
    }

  const moveResize = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current
    if (!drag) return
    event.preventDefault()
    const delta = event.clientX - drag.startX
    const raw =
      drag.edge === `right` ? drag.startWidth + delta : drag.startWidth - delta
    const max = drag.naturalWidth ?? fallbackMaxResizeWidth
    const next = Math.round(Math.min(max, Math.max(minResizeWidth, raw)))
    drag.latestWidth = next
    setDragWidth(next)
  }

  const endResize = () => {
    const drag = dragRef.current
    if (!drag) return
    dragRef.current = null
    setDragWidth(null)
    if (drag.latestWidth === null) return
    const atFullWidth =
      drag.naturalWidth !== null && drag.latestWidth >= drag.naturalWidth
    const nextSrc = srcWithWidth(src, atFullWidth ? null : drag.latestWidth)
    if (nextSrc !== src) updateAttributes({ src: nextSrc })
  }

  const handleDownload = async () => {
    try {
      const url = new URL(fullSizeSrc, window.location.origin)
      url.searchParams.set(`download`, `1`)
      const anchor = document.createElement(`a`)
      anchor.href = url.toString()
      anchor.download = attachment?.filename || label || `media`
      anchor.click()
    } catch {
      // Nothing sensible to download.
    }
  }

  const body = !attachment ? (
    <div className="editor-media-placeholder" contentEditable={false}>
      <span className="truncate">{displayLabel}</span>
    </div>
  ) : isMedia ? (
    <AttachmentMediaPlayer
      attachment={attachment}
      displayWidth={isVideo ? renderWidth : null}
      onExpand={isVideo && !editor.isEditable ? () => setLightboxOpen(true) : undefined}
      frameProps={{
        // EXP-421 parity: while editing, the frame is the drag handle that
        // hands the block move to ProseMirror.
        draggable: editor.isEditable ? true : undefined,
        ...(editor.isEditable ? { "data-drag-handle": `` } : {}),
      }}
    />
  ) : (
    // A link to a non-media attachment (a PDF, say): plain link, as written.
    <p className="editor-media-fallback" contentEditable={false}>
      <a
        href={fullSizeSrc}
        className="editor-link"
        target="_blank"
        rel="noreferrer"
      >
        {displayLabel}
      </a>
    </p>
  )

  return (
    <NodeViewWrapper
      ref={frameRef}
      className={cn(
        `editor-media-node`,
        isAudio && `is-audio`,
        selected && `is-selected`,
        dragWidth !== null && `is-resizing`
      )}
      style={isVideo && renderWidth ? { width: `${renderWidth}px` } : undefined}
      data-selected={selected ? `true` : `false`}
    >
      {editor.isEditable ? (
        <div contentEditable={false}>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant="ghost"
                size="icon-xs"
                className="editor-image-menu-trigger"
                aria-label={`Media options for ${displayLabel}`}
              >
                <Ellipsis className="size-3.5" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-56">
              {isVideo ? (
                <DropdownMenuItem onSelect={() => setLightboxOpen(true)}>
                  <Eye />
                  View video
                </DropdownMenuItem>
              ) : null}
              <DropdownMenuItem onSelect={() => void handleDownload()}>
                <Download />
                Download
              </DropdownMenuItem>
              <DropdownMenuItem
                variant="destructive"
                onSelect={() => {
                  deleteNode()
                  editor.chain().focus().run()
                }}
              >
                <Trash2 />
                Remove from description
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ) : null}
      {editor.isEditable && isVideo ? (
        <>
          <div
            className="editor-image-handle is-left"
            contentEditable={false}
            aria-hidden="true"
            onPointerDown={beginResize(`left`)}
            onPointerMove={moveResize}
            onPointerUp={endResize}
            onPointerCancel={endResize}
          />
          <div
            className="editor-image-handle is-right"
            contentEditable={false}
            aria-hidden="true"
            onPointerDown={beginResize(`right`)}
            onPointerMove={moveResize}
            onPointerUp={endResize}
            onPointerCancel={endResize}
          />
        </>
      ) : null}
      {body}
      {attachment && isVideo ? (
        <ImagePreviewDialog
          open={lightboxOpen}
          onOpenChange={setLightboxOpen}
          src={attachment.url}
          poster={
            attachment.posterStorageKey
              ? buildAttachmentPosterUrl(attachment.id)
              : undefined
          }
          kind="video"
          label={displayLabel}
        />
      ) : null}
    </NodeViewWrapper>
  )
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    mediaLink: {
      /** Inserts a media block at the selection. */
      setMediaLink: (attrs: { label: string; src: string }) => ReturnType
    }
  }
}

export const MarkdownMedia = Node.create({
  name: mediaLinkNodeName,
  group: `block`,
  atom: true,
  draggable: true,
  selectable: true,

  addAttributes() {
    return {
      label: { default: `` },
      src: { default: `` },
    }
  },

  parseHTML() {
    return [
      {
        tag: `div[data-media-link]`,
        getAttrs: (element) => ({
          label: element.getAttribute(`data-label`) ?? ``,
          src: element.getAttribute(`data-src`) ?? ``,
        }),
      },
    ]
  },

  renderHTML({ node, HTMLAttributes }) {
    return [
      `div`,
      mergeAttributes(HTMLAttributes, {
        "data-media-link": ``,
        "data-label": String(node.attrs.label ?? ``),
        "data-src": String(node.attrs.src ?? ``),
        class: `editor-media-node`,
      }),
    ]
  },

  addCommands() {
    return {
      setMediaLink:
        (attrs) =>
        ({ commands }) =>
          commands.insertContent({ type: this.name, attrs }),
    }
  },

  addStorage() {
    return {
      markdown: {
        serialize: serializeMediaLink,
        parse: {
          updateDOM(element: HTMLElement) {
            liftStandaloneAttachmentLinks(element)
          },
        },
      },
    }
  },

  addNodeView() {
    return ReactNodeViewRenderer(MarkdownMediaNodeView)
  },
})
