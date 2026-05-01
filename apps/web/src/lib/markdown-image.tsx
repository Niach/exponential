import Image from "@tiptap/extension-image"
import {
  NodeViewWrapper,
  ReactNodeViewRenderer,
  type ReactNodeViewProps,
} from "@tiptap/react"
import { X } from "lucide-react"
import { cn } from "@/lib/utils"

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
