import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react"
import { type Editor, useEditor, EditorContent } from "@tiptap/react"
import { StarterKit } from "@tiptap/starter-kit"
import { Link } from "@tiptap/extension-link"
import { Placeholder } from "@tiptap/extension-placeholder"
import { TaskList } from "@tiptap/extension-task-list"
import { TaskItem } from "@tiptap/extension-task-item"
import { CodeBlockLowlight } from "@tiptap/extension-code-block-lowlight"
import { createLowlight, common } from "lowlight"
import { Markdown } from "tiptap-markdown"

// `common` covers ~35 popular languages incl. ts/tsx/js/jsx/json/bash/css/html/
// python/rust/go — enough for plan code blocks without pulling all 200 grammars.
const lowlight = createLowlight(common)
import {
  Bold,
  Italic,
  Strikethrough,
  Link as LinkIcon,
  Unlink,
  Check,
  Image as ImageIcon,
  Quote,
  RemoveFormatting,
  Code,
  List,
  ListOrdered,
  ListChecks,
  Heading1,
  Heading2,
  Heading3,
} from "lucide-react"
import { MarkdownImage } from "@/lib/markdown-image"
import { acceptedImageContentTypes } from "@/lib/storage/issue-attachments"
import { cn } from "@/lib/utils"

export interface MarkdownEditorImageUploadConfig {
  disabledReason?: string
  enabled: boolean
  onFiles: (files: File[]) => Promise<void>
  uploading?: boolean
}

export interface MarkdownEditorRef {
  focus: () => void
  setMarkdown: (md: string) => void
  getMarkdown: () => string
  insertImage: (image: { alt?: string; src: string }) => void
}

interface MarkdownEditorProps {
  editable?: boolean
  imageUpload?: MarkdownEditorImageUploadConfig
  markdown: string
  onChange: (markdown: string) => void
  onBlur?: () => void
  placeholder?: string
  autoFocus?: boolean
}

type MarkdownEditorInstance = Editor & {
  storage: Editor[`storage`] & {
    markdown: {
      getMarkdown: () => string
    }
  }
}

function hasMarkdownStorage(
  editor: Editor | null
): editor is MarkdownEditorInstance {
  return Boolean(
    editor &&
    `markdown` in editor.storage &&
    typeof (editor.storage as MarkdownEditorInstance[`storage`]).markdown
      .getMarkdown === `function`
  )
}

function getEditorMarkdown(editor: Editor | null) {
  return hasMarkdownStorage(editor) ? editor.storage.markdown.getMarkdown() : ``
}

function getImageFiles(fileList: FileList | null | undefined) {
  return Array.from(fileList ?? []).filter((file) =>
    file.type.startsWith(`image/`)
  )
}

// ── Shared toolbar pieces (used by both the selection bubble and the static
//    toolbar above the editor) ──

function ToolbarButton({
  active,
  onClick,
  title,
  children,
}: {
  active?: boolean
  onClick: () => void
  title: string
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
      className={active ? `is-active` : ``}
      title={title}
    >
      {children}
    </button>
  )
}

/** Inline link editor — replaces the old `window.prompt`. Rendered inside the
 *  toolbar so focus stays within it (the bubble's blur-to-hide check passes). */
function LinkControl({ editor }: { editor: Editor }) {
  const [editing, setEditing] = useState(false)
  const [url, setUrl] = useState(``)
  const inputRef = useRef<HTMLInputElement>(null)

  const open = useCallback(() => {
    const href = editor.getAttributes(`link`).href
    setUrl(typeof href === `string` ? href : ``)
    setEditing(true)
    requestAnimationFrame(() => inputRef.current?.focus())
  }, [editor])

  const apply = useCallback(() => {
    const href = url.trim()
    if (href) {
      editor.chain().focus().extendMarkRange(`link`).setLink({ href }).run()
    } else {
      editor.chain().focus().extendMarkRange(`link`).unsetLink().run()
    }
    setEditing(false)
  }, [editor, url])

  const remove = useCallback(() => {
    editor.chain().focus().extendMarkRange(`link`).unsetLink().run()
    setEditing(false)
  }, [editor])

  if (editing) {
    return (
      <span className="toolbar-link-edit">
        <input
          ref={inputRef}
          className="toolbar-link-input"
          value={url}
          placeholder="https://…"
          onMouseDown={(e) => e.stopPropagation()}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === `Enter`) {
              e.preventDefault()
              apply()
            } else if (e.key === `Escape`) {
              e.preventDefault()
              setEditing(false)
            }
          }}
          onBlur={() => {
            // Commit on blur so clicking back into the editor keeps the link.
            setTimeout(() => setEditing(false), 120)
          }}
        />
        <button
          type="button"
          onMouseDown={(e) => e.preventDefault()}
          onClick={apply}
          title="Apply link"
        >
          <Check className="size-3.5" />
        </button>
        {editor.isActive(`link`) ? (
          <button
            type="button"
            onMouseDown={(e) => e.preventDefault()}
            onClick={remove}
            title="Remove link"
          >
            <Unlink className="size-3.5" />
          </button>
        ) : null}
      </span>
    )
  }

  return (
    <ToolbarButton active={editor.isActive(`link`)} onClick={open} title="Link">
      <LinkIcon className="size-3.5" />
    </ToolbarButton>
  )
}

/** The common formatting controls, shared by both toolbars. */
function ToolbarActions({ editor }: { editor: Editor }) {
  return (
    <>
      <ToolbarButton
        active={editor.isActive(`heading`, { level: 1 })}
        onClick={() => editor.chain().focus().toggleHeading({ level: 1 }).run()}
        title="Heading 1"
      >
        <Heading1 className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive(`heading`, { level: 2 })}
        onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
        title="Heading 2"
      >
        <Heading2 className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive(`heading`, { level: 3 })}
        onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
        title="Heading 3"
      >
        <Heading3 className="size-3.5" />
      </ToolbarButton>
      <div className="bubble-separator" />
      <ToolbarButton
        active={editor.isActive(`bold`)}
        onClick={() => editor.chain().focus().toggleBold().run()}
        title="Bold"
      >
        <Bold className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive(`italic`)}
        onClick={() => editor.chain().focus().toggleItalic().run()}
        title="Italic"
      >
        <Italic className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive(`strike`)}
        onClick={() => editor.chain().focus().toggleStrike().run()}
        title="Strikethrough"
      >
        <Strikethrough className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive(`code`)}
        onClick={() => editor.chain().focus().toggleCode().run()}
        title="Code"
      >
        <Code className="size-3.5" />
      </ToolbarButton>
      <div className="bubble-separator" />
      <LinkControl editor={editor} />
      <ToolbarButton
        active={editor.isActive(`blockquote`)}
        onClick={() => editor.chain().focus().toggleBlockquote().run()}
        title="Quote"
      >
        <Quote className="size-3.5" />
      </ToolbarButton>
      <div className="bubble-separator" />
      <ToolbarButton
        active={editor.isActive(`bulletList`)}
        onClick={() => editor.chain().focus().toggleBulletList().run()}
        title="Bullet list"
      >
        <List className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive(`orderedList`)}
        onClick={() => editor.chain().focus().toggleOrderedList().run()}
        title="Numbered list"
      >
        <ListOrdered className="size-3.5" />
      </ToolbarButton>
      <ToolbarButton
        active={editor.isActive(`taskList`)}
        onClick={() => editor.chain().focus().toggleTaskList().run()}
        title="Task list"
      >
        <ListChecks className="size-3.5" />
      </ToolbarButton>
      <div className="bubble-separator" />
      <ToolbarButton
        onClick={() =>
          editor.chain().focus().unsetAllMarks().clearNodes().run()
        }
        title="Clear formatting"
      >
        <RemoveFormatting className="size-3.5" />
      </ToolbarButton>
    </>
  )
}

/** Image button (static toolbar only) — opens a file picker routed through the
 *  same upload path as paste/drop. */
function ImageControl({
  imageUpload,
}: {
  imageUpload?: MarkdownEditorImageUploadConfig
}) {
  const inputRef = useRef<HTMLInputElement>(null)
  if (!imageUpload?.enabled) return null
  return (
    <>
      <div className="bubble-separator" />
      <input
        ref={inputRef}
        type="file"
        accept={acceptedImageContentTypes.join(`,`)}
        multiple
        hidden
        onChange={(event) => {
          const files = getImageFiles(event.target.files)
          if (files.length > 0) void imageUpload.onFiles(files)
          event.target.value = ``
        }}
      />
      <ToolbarButton
        onClick={() => inputRef.current?.click()}
        title="Insert image"
      >
        <ImageIcon className="size-3.5" />
      </ToolbarButton>
    </>
  )
}

/** Always-visible toolbar above the editor (discoverability; houses the image
 *  button). The selection bubble still appears for quick inline formatting. */
function StaticToolbar({
  editor,
  imageUpload,
}: {
  editor: Editor | null
  imageUpload?: MarkdownEditorImageUploadConfig
}) {
  if (!editor) return null
  return (
    <div className="static-toolbar">
      <ToolbarActions editor={editor} />
      <ImageControl imageUpload={imageUpload} />
    </div>
  )
}

function BubbleToolbar({ editor }: { editor: Editor | null }) {
  const toolbarRef = useRef<HTMLDivElement>(null)
  const [visible, setVisible] = useState(false)
  const [position, setPosition] = useState({ top: 0, left: 0 })

  const updatePosition = useCallback(() => {
    if (!editor) return

    const { from, to, empty } = editor.state.selection
    if (empty) {
      setVisible(false)
      return
    }

    const editorEl = editor.view.dom.closest(`.tiptap-wrapper`)
    if (!editorEl) return

    const start = editor.view.coordsAtPos(from)
    const end = editor.view.coordsAtPos(to)
    const wrapperRect = editorEl.getBoundingClientRect()

    const selectionCenterX = (start.left + end.right) / 2
    const toolbarWidth = toolbarRef.current?.offsetWidth ?? 320

    setPosition({
      top: start.top - wrapperRect.top - 44,
      left: Math.max(
        4,
        Math.min(
          selectionCenterX - wrapperRect.left - toolbarWidth / 2,
          wrapperRect.width - toolbarWidth - 4
        )
      ),
    })
    setVisible(true)
  }, [editor])

  useEffect(() => {
    if (!editor) return

    editor.on(`selectionUpdate`, updatePosition)
    editor.on(`blur`, () => {
      setTimeout(() => {
        if (!toolbarRef.current?.contains(document.activeElement)) {
          setVisible(false)
        }
      }, 150)
    })

    return () => {
      editor.off(`selectionUpdate`, updatePosition)
    }
  }, [editor, updatePosition])

  if (!editor || !visible) return null

  return (
    <div
      ref={toolbarRef}
      className="bubble-toolbar"
      style={{ top: position.top, left: position.left }}
    >
      <ToolbarActions editor={editor} />
    </div>
  )
}

export const MarkdownEditor = forwardRef<
  MarkdownEditorRef,
  MarkdownEditorProps
>(
  (
    {
      markdown,
      onChange,
      onBlur,
      placeholder,
      autoFocus,
      imageUpload,
      editable = true,
    },
    ref
  ) => {
    const onChangeRef = useRef(onChange)
    onChangeRef.current = onChange
    const imageUploadRef = useRef(imageUpload)
    imageUploadRef.current = imageUpload

    const editor = useEditor({
      extensions: [
        StarterKit.configure({
          heading: { levels: [1, 2, 3] },
          // Replaced below by CodeBlockLowlight for syntax highlighting.
          codeBlock: false,
        }),
        CodeBlockLowlight.configure({
          lowlight,
          defaultLanguage: `plaintext`,
        }),
        Link.configure({
          openOnClick: false,
          HTMLAttributes: { class: `editor-link` },
        }),
        TaskList,
        TaskItem.configure({ nested: true }),
        MarkdownImage,
        Placeholder.configure({
          placeholder: placeholder ?? `Add description...`,
        }),
        Markdown.configure({
          html: false,
          transformPastedText: true,
          transformCopiedText: true,
        }),
      ],
      content: markdown,
      editable,
      immediatelyRender: false,
      onUpdate: ({ editor: e }) => {
        onChangeRef.current(getEditorMarkdown(e))
      },
      onBlur: () => {
        onBlur?.()
      },
      editorProps: {
        attributes: {
          class: cn(`tiptap-content`, !editable && `cursor-default`),
          "aria-label": `Issue description`,
          "aria-readonly": String(!editable),
        },
        handleDOMEvents: {
          contextmenu: (_view, event) => {
            event.preventDefault()
            return true
          },
        },
        handlePaste: (_view, event) => {
          const files = getImageFiles(event.clipboardData?.files)

          if (!editable || files.length === 0 || !imageUploadRef.current) {
            return false
          }

          event.preventDefault()
          void imageUploadRef.current.onFiles(files)
          return true
        },
        handleDrop: (_view, event) => {
          const files = getImageFiles(event.dataTransfer?.files)

          if (!editable || files.length === 0 || !imageUploadRef.current) {
            return false
          }

          event.preventDefault()
          void imageUploadRef.current.onFiles(files)
          return true
        },
      },
    })

    useImperativeHandle(ref, () => ({
      focus: () => {
        editor?.commands.focus(`end`)
      },
      setMarkdown: (md: string) => {
        editor?.commands.setContent(md)
      },
      getMarkdown: () => {
        return getEditorMarkdown(editor)
      },
      insertImage: ({ alt, src }) => {
        editor?.chain().focus().setImage({ alt, src }).run()
      },
    }))

    useEffect(() => {
      if (autoFocus && editor) {
        editor.commands.focus(`end`)
      }
    }, [autoFocus, editor])

    useEffect(() => {
      editor?.setEditable(editable)
    }, [editable, editor])

    return (
      <div className="tiptap-wrapper">
        {editable ? (
          <StaticToolbar editor={editor} imageUpload={imageUpload} />
        ) : null}
        {editable ? <BubbleToolbar editor={editor} /> : null}
        <EditorContent editor={editor} />
      </div>
    )
  }
)

MarkdownEditor.displayName = `MarkdownEditor`
