import { useRef, useState } from "react"
import Image from "@tiptap/extension-image"
import type { Node as ProseMirrorNode } from "@tiptap/pm/model"
import type { MarkdownSerializerState } from "prosemirror-markdown"
import {
  NodeViewWrapper,
  ReactNodeViewRenderer,
  type ReactNodeViewProps,
} from "@tiptap/react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Copy, Download, Eye, Ellipsis, Trash2 } from "lucide-react"
import { attachmentCollection } from "@/lib/collections"
import { IMAGE_ACTIONS_COPY } from "@exp/domain-contract/image-actions"
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  ImagePreviewDialog,
  copyImage,
  imageMenuProps,
  pictureSource,
} from "@exp/ui"
import { cn } from "@/lib/utils"

/** Pull the attachment id out of a `/api/attachments/{id}` (relative or
 *  absolute) image src so we can look up its probed pixel dimensions. */
export function attachmentIdFromSrc(src: string): string | null {
  const match = src.match(/\/api\/attachments\/([^/?#]+)/)
  return match ? match[1] : null
}

/** The display width persisted as a `?w=<int>` query param on the attachment
 *  src (EXP-52). The markdown stays plain GFM — `![alt](/api/attachments/{id}?w=480)`
 *  — and clients that don't understand the param simply ignore it. */
export function widthParamFromSrc(src: string): number | null {
  const match = src.match(/[?&]w=(\d+)(?:[&#]|$)/)
  if (!match) return null
  const parsed = Number.parseInt(match[1], 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null
}

/** The src stripped of query/hash — the canonical full-size attachment form. */
export function stripQuery(src: string): string {
  return src.split(`#`)[0].split(`?`)[0]
}

/** Rebuild an attachment src carrying (or dropping) the `?w=` width param. */
export function srcWithWidth(src: string, width: number | null): string {
  const base = stripQuery(src)
  return width ? `${base}?w=${width}` : base
}

// Drag clamps: min keeps the image usable/grabbable; max is the natural
// probed width (dragging back to it removes the param so the markdown stays
// canonical-clean). CSS `max-width: 100%` caps display in narrow columns.
const minResizeWidth = 120
const fallbackMaxResizeWidth = 4000

interface ResizeDrag {
  edge: `left` | `right`
  startX: number
  startWidth: number
  /** Natural pixel width (probed dims, else the loaded img element). */
  naturalWidth: number | null
  latestWidth: number | null
}

function MarkdownImageNodeView({
  deleteNode,
  editor,
  node,
  selected,
  updateAttributes,
}: ReactNodeViewProps) {
  const alt = typeof node.attrs.alt === `string` ? node.attrs.alt : ``
  const src = typeof node.attrs.src === `string` ? node.attrs.src : ``
  const title =
    typeof node.attrs.title === `string` ? node.attrs.title : undefined
  const imageLabel = alt || `attachment`

  // Reserve the intrinsic aspect-ratio before the bytes load (no layout shift
  // on reload). Dimensions come from the synced attachments collection, keyed
  // by the id in the src — the markdown carries no dimensions (only the
  // optional `?w=` display width).
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
  const width =
    typeof attachment?.width === `number` ? attachment.width : undefined
  const height =
    typeof attachment?.height === `number` ? attachment.height : undefined

  // The `?w=` contract only applies to our attachment URLs — external image
  // srcs pass through untouched (no resize handles, query preserved).
  const persistedWidth = attachmentId ? widthParamFromSrc(src) : null
  const fullSizeSrc = attachmentId ? stripQuery(src) : src

  // #EXP-1945 (image-actions.json `imageSource`): only our attachments,
  // data and local pictures load; another website's is a link in the
  // picture's place (the Markdown keeps its bytes), anything else its alt.
  const source = pictureSource(src, alt)
  const loads = source.kind === `load`

  const imgRef = useRef<HTMLImageElement | null>(null)
  const dragRef = useRef<ResizeDrag | null>(null)
  const [dragWidth, setDragWidth] = useState<number | null>(null)
  const [lightboxOpen, setLightboxOpen] = useState(false)

  const renderWidth = dragWidth ?? persistedWidth

  const beginResize =
    (edge: `left` | `right`) => (event: React.PointerEvent<HTMLDivElement>) => {
      if (!editor.isEditable) return
      const img = imgRef.current
      if (!img) return
      event.preventDefault()
      event.stopPropagation()
      const probedOrLoaded =
        width ?? (img.naturalWidth > 0 ? img.naturalWidth : null)
      dragRef.current = {
        edge,
        startX: event.clientX,
        startWidth: img.getBoundingClientRect().width,
        naturalWidth: probedOrLoaded,
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
    // No movement — leave the node untouched.
    if (drag.latestWidth === null) return
    // Back at (or beyond) natural width ⇒ drop the param entirely so the
    // markdown stays canonical-clean.
    const atFullWidth =
      drag.naturalWidth !== null && drag.latestWidth >= drag.naturalWidth
    const nextSrc = srcWithWidth(src, atFullWidth ? null : drag.latestWidth)
    if (nextSrc !== src) {
      updateAttributes({ src: nextSrc })
    }
  }

  const absoluteUrl = () =>
    new URL(fullSizeSrc, window.location.origin).toString()

  const handleDownload = async () => {
    try {
      const response = await fetch(absoluteUrl())
      if (!response.ok) throw new Error(`download failed: ${response.status}`)
      const blob = await response.blob()
      const objectUrl = URL.createObjectURL(blob)
      const anchor = document.createElement(`a`)
      anchor.href = objectUrl
      anchor.download = attachment?.filename || alt || `image`
      anchor.click()
      // Revoke after the browser has had time to start the download.
      setTimeout(() => URL.revokeObjectURL(objectUrl), 60_000)
    } catch {
      // Network/permission failure — nothing sensible to download.
    }
  }

  return (
    <NodeViewWrapper
      className={cn(
        `editor-image-node`,
        selected && `is-selected`,
        dragWidth !== null && `is-resizing`
      )}
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
                aria-label={`Image options for ${imageLabel}`}
              >
                <Ellipsis className="size-3.5" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-56">
              {loads ? (
                <>
                  <DropdownMenuItem onSelect={() => setLightboxOpen(true)}>
                    <Eye />
                    View image
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => void handleDownload()}>
                    <Download />
                    Download
                  </DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => void copyImage(fullSizeSrc)}>
                    <Copy />
                    {IMAGE_ACTIONS_COPY.copyImage}
                  </DropdownMenuItem>
                </>
              ) : null}
              {/* No separator above a destructive item (EXP-687). */}
              <DropdownMenuItem
                variant="destructive"
                onSelect={() => {
                  deleteNode()
                  editor.chain().focus().run()
                }}
              >
                <Trash2 />
                {/* EXP-297: removing the node only unlinks the image from the
                    text — the attachment itself now lives until someone
                    deletes it explicitly (the auto-GC is gone). */}
                Remove from description
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ) : null}
      {editor.isEditable && attachmentId ? (
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
      {source.kind === `link` ? (
        <a href={source.href} target="_blank" rel="noreferrer" className="editor-image-link" contentEditable={false}>
          {source.text}
        </a>
      ) : source.kind === `text` ? (
        source.text ? <span className="editor-image-alt">{source.text}</span> : null
      ) : (
      <img
        ref={imgRef}
        {...(editor.isEditable ? {} : imageMenuProps(fullSizeSrc, attachment?.filename))}
        src={src}
        alt={alt}
        title={title}
        width={width}
        height={height}
        style={renderWidth ? { width: `${renderWidth}px` } : undefined}
        className={cn(`editor-image`, !editor.isEditable && `cursor-zoom-in`)}
        // EXP-421: while editing, the image itself is the drag handle —
        // TipTap's node-view dragstart sees `data-drag-handle`, selects the
        // node and hands the drag to ProseMirror, which moves the block on
        // drop (the dropcursor draws the target line). Read mode keeps the
        // lightbox click and no drag.
        draggable={editor.isEditable ? `true` : `false`}
        data-drag-handle={editor.isEditable ? `` : undefined}
        onClick={editor.isEditable ? undefined : () => setLightboxOpen(true)}
      />
      )}
      <ImagePreviewDialog
        open={lightboxOpen}
        onOpenChange={setLightboxOpen}
        src={fullSizeSrc}
        alt={alt}
        label={imageLabel}
        naturalSize={width && height ? { width, height } : null}
      />
    </NodeViewWrapper>
  )
}

export const MarkdownImage = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      alt: {
        default: null,
      },
      src: {
        default: null,
      },
      title: {
        default: null,
      },
    }
  },
  // tiptap-markdown ships prosemirror-markdown's INLINE image serializer for
  // this node, but `@tiptap/extension-image` is block-level (`inline: false`)
  // — so the written `![alt](src)` never closed its block and the following
  // paragraph was glued straight onto it (`![](/api/attachments/x)after`).
  // Every save silently welded the image into the next paragraph; the desktop
  // IDE then rendered the whole line as literal text (EXP-271). Mirror
  // prosemirror-markdown's own `horizontal_rule` (write + closeBlock) so a
  // block image serializes as its own block.
  addStorage() {
    return {
      ...this.parent?.(),
      markdown: {
        serialize(state: MarkdownSerializerState, node: ProseMirrorNode) {
          const alt = state.esc(String(node.attrs.alt ?? ``))
          const src = String(node.attrs.src ?? ``).replace(/[()]/g, `\\$&`)
          const title = node.attrs.title
            ? ` "${String(node.attrs.title).replace(/"/g, `\\"`)}"`
            : ``
          state.write(`![${alt}](${src}${title})`)
          state.closeBlock(node)
        },
        parse: {
          // handled by markdown-it
        },
      },
    }
  },
  addNodeView() {
    return ReactNodeViewRenderer(MarkdownImageNodeView)
  },
}).configure({
  allowBase64: false,
  HTMLAttributes: {
    class: `editor-image`,
  },
})
