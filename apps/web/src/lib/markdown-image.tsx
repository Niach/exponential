import Image from "@tiptap/extension-image"
import {
  NodeViewWrapper,
  ReactNodeViewRenderer,
  type ReactNodeViewProps,
} from "@tiptap/react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { X } from "lucide-react"
import { attachmentCollection } from "@/lib/collections"
import { cn } from "@/lib/utils"

/** Pull the attachment id out of a `/api/attachments/{id}` (relative or
 *  absolute) image src so we can look up its probed pixel dimensions. */
function attachmentIdFromSrc(src: string): string | null {
  const match = src.match(/\/api\/attachments\/([^/?#]+)/)
  return match ? match[1] : null
}

function MarkdownImageNodeView({
  deleteNode,
  editor,
  node,
  selected,
}: ReactNodeViewProps) {
  const alt = typeof node.attrs.alt === `string` ? node.attrs.alt : ``
  const src = typeof node.attrs.src === `string` ? node.attrs.src : ``
  const title =
    typeof node.attrs.title === `string` ? node.attrs.title : undefined
  const imageLabel = alt || `attachment`

  // Reserve the intrinsic aspect-ratio before the bytes load (no layout shift
  // on reload). Dimensions come from the synced attachments collection, keyed
  // by the id in the src — the markdown stays `![alt](url)` with no dimensions.
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
  const dims = attachments?.[0]
  const width = typeof dims?.width === `number` ? dims.width : undefined
  const height = typeof dims?.height === `number` ? dims.height : undefined

  return (
    <NodeViewWrapper
      className={cn(`editor-image-node`, selected && `is-selected`)}
      data-selected={selected ? `true` : `false`}
    >
      {editor.isEditable ? (
        <button
          type="button"
          className="editor-image-remove"
          contentEditable={false}
          aria-label={`Remove image ${imageLabel}`}
          onMouseDown={(event) => {
            event.preventDefault()
            event.stopPropagation()
          }}
          onClick={(event) => {
            event.preventDefault()
            event.stopPropagation()
            deleteNode()
            editor.chain().focus().run()
          }}
        >
          <X className="size-3" />
        </button>
      ) : null}
      <img
        src={src}
        alt={alt}
        title={title}
        width={width}
        height={height}
        className="editor-image"
        draggable="false"
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
  addNodeView() {
    return ReactNodeViewRenderer(MarkdownImageNodeView)
  },
}).configure({
  allowBase64: false,
  HTMLAttributes: {
    class: `editor-image`,
  },
})
