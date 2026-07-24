import { useRef, useState } from "react"
import Image from "@tiptap/extension-image"
import {
  NodeViewWrapper,
  ReactNodeViewRenderer,
  type ReactNodeViewProps,
} from "@tiptap/react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  Copy,
  Download,
  Eye,
  Link as LinkIcon,
  MoreHorizontal,
  Trash2,
} from "lucide-react"
import { attachmentCollection } from "@/lib/collections"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { cn } from "@/lib/utils"

/** Pull the attachment id out of a `/api/attachments/{id}` (relative or
 *  absolute) image src so we can look up its probed pixel dimensions. */
function attachmentIdFromSrc(src: string): string | null {
  const match = src.match(/\/api\/attachments\/([^/?#]+)/)
  return match ? match[1] : null
}

/** The display width persisted as a `?w=<int>` query param on the attachment
 *  src (EXP-52). The markdown stays plain GFM — `![alt](/api/attachments/{id}?w=480)`
 *  — and clients that don't understand the param simply ignore it. */
function widthParamFromSrc(src: string): number | null {
  const match = src.match(/[?&]w=(\d+)(?:[&#]|$)/)
  if (!match) return null
  const parsed = Number.parseInt(match[1], 10)
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null
}

/** The src stripped of query/hash — the canonical full-size attachment form. */
function stripQuery(src: string): string {
  return src.split(`#`)[0].split(`?`)[0]
}

/** Rebuild an attachment src carrying (or dropping) the `?w=` width param. */
function srcWithWidth(src: string, width: number | null): string {
  const base = stripQuery(src)
  return width ? `${base}?w=${width}` : base
}

// Drag clamps: min keeps the image usable/grabbable; max is the natural
// probed width (dragging back to it removes the param so the markdown stays
// canonical-clean). CSS `max-width: 100%` caps display in narrow columns.
const minResizeWidth = 120
const fallbackMaxResizeWidth = 4000

/** Re-encode to PNG via canvas — webp/jpeg ClipboardItems are widely
 *  rejected, PNG is the interoperable clipboard image format. */
async function blobToPngBlob(blob: Blob): Promise<Blob> {
  if (blob.type === `image/png`) return blob
  const bitmap = await createImageBitmap(blob)
  try {
    const canvas = document.createElement(`canvas`)
    canvas.width = bitmap.width
    canvas.height = bitmap.height
    const context = canvas.getContext(`2d`)
    if (!context) throw new Error(`canvas 2d context unavailable`)
    context.drawImage(bitmap, 0, 0)
    return await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob(
        (encoded) =>
          encoded ? resolve(encoded) : reject(new Error(`png encode failed`)),
        `image/png`
      )
    })
  } finally {
    bitmap.close()
  }
}

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

  const handleCopyImage = async () => {
    const url = absoluteUrl()
    try {
      if (typeof ClipboardItem === `undefined` || !navigator.clipboard?.write) {
        throw new Error(`image clipboard unsupported`)
      }
      // Pass a promise so Safari accepts the write within the user gesture.
      const png = fetch(url)
        .then((response) => {
          if (!response.ok) throw new Error(`fetch failed: ${response.status}`)
          return response.blob()
        })
        .then(blobToPngBlob)
      await navigator.clipboard.write([new ClipboardItem({ "image/png": png })])
    } catch {
      // Graceful fallback: at least put the image link on the clipboard.
      await navigator.clipboard.writeText(url).catch(() => {})
    }
  }

  const handleCopyLink = async () => {
    await navigator.clipboard.writeText(absoluteUrl()).catch(() => {})
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
                <MoreHorizontal className="size-3.5" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-44">
              <DropdownMenuItem onSelect={() => setLightboxOpen(true)}>
                <Eye />
                View image
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => void handleDownload()}>
                <Download />
                Download
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => void handleCopyImage()}>
                <Copy />
                Copy image
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={() => void handleCopyLink()}>
                <LinkIcon />
                Copy link
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                variant="destructive"
                onSelect={() => {
                  deleteNode()
                  editor.chain().focus().run()
                }}
              >
                <Trash2 />
                Delete
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
      <img
        ref={imgRef}
        src={src}
        alt={alt}
        title={title}
        width={width}
        height={height}
        style={renderWidth ? { width: `${renderWidth}px` } : undefined}
        className={cn(`editor-image`, !editor.isEditable && `cursor-zoom-in`)}
        draggable="false"
        onClick={editor.isEditable ? undefined : () => setLightboxOpen(true)}
      />
      <Dialog open={lightboxOpen} onOpenChange={setLightboxOpen}>
        <DialogContent
          className="w-auto max-w-[min(96vw,80rem)] p-2 max-sm:content-center max-sm:justify-items-center sm:max-w-[min(96vw,80rem)]"
          aria-describedby={undefined}
        >
          <DialogTitle className="sr-only">{imageLabel}</DialogTitle>
          <img
            src={fullSizeSrc}
            alt={alt}
            className="max-h-[85vh] w-auto max-w-full rounded-md object-contain"
          />
        </DialogContent>
      </Dialog>
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
  addNodeView() {
    return ReactNodeViewRenderer(MarkdownImageNodeView)
  },
}).configure({
  allowBase64: false,
  HTMLAttributes: {
    class: `editor-image`,
  },
})
