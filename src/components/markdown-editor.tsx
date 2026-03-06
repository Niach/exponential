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
import { Underline } from "@tiptap/extension-underline"
import { Link } from "@tiptap/extension-link"
import { Placeholder } from "@tiptap/extension-placeholder"
import { Markdown } from "tiptap-markdown"
import {
  Bold,
  Italic,
  Strikethrough,
  Underline as UnderlineIcon,
  Link as LinkIcon,
  Quote,
  RemoveFormatting,
  Code,
  List,
  ListOrdered,
  Heading1,
  Heading2,
  Heading3,
} from "lucide-react"

export interface MarkdownEditorRef {
  setMarkdown: (md: string) => void
  getMarkdown: () => string
}

interface MarkdownEditorProps {
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

  const toggleLink = useCallback(() => {
    if (!editor) return
    if (editor.isActive(`link`)) {
      editor.chain().focus().unsetLink().run()
      return
    }
    const url = window.prompt(`URL`)
    if (url) {
      editor.chain().focus().extendMarkRange(`link`).setLink({ href: url }).run()
    }
  }, [editor])

  if (!editor || !visible) return null

  const btn = (
    active: boolean,
    onClick: () => void,
    icon: React.ReactNode,
    title: string
  ) => (
    <button
      type="button"
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
      className={active ? `is-active` : ``}
      title={title}
    >
      {icon}
    </button>
  )

  return (
    <div
      ref={toolbarRef}
      className="bubble-toolbar"
      style={{ top: position.top, left: position.left }}
    >
      {btn(
        editor.isActive(`heading`, { level: 1 }),
        () => editor.chain().focus().toggleHeading({ level: 1 }).run(),
        <Heading1 className="size-3.5" />,
        `Heading 1`
      )}
      {btn(
        editor.isActive(`heading`, { level: 2 }),
        () => editor.chain().focus().toggleHeading({ level: 2 }).run(),
        <Heading2 className="size-3.5" />,
        `Heading 2`
      )}
      {btn(
        editor.isActive(`heading`, { level: 3 }),
        () => editor.chain().focus().toggleHeading({ level: 3 }).run(),
        <Heading3 className="size-3.5" />,
        `Heading 3`
      )}
      <div className="bubble-separator" />
      {btn(
        editor.isActive(`bold`),
        () => editor.chain().focus().toggleBold().run(),
        <Bold className="size-3.5" />,
        `Bold`
      )}
      {btn(
        editor.isActive(`italic`),
        () => editor.chain().focus().toggleItalic().run(),
        <Italic className="size-3.5" />,
        `Italic`
      )}
      {btn(
        editor.isActive(`underline`),
        () => editor.chain().focus().toggleUnderline().run(),
        <UnderlineIcon className="size-3.5" />,
        `Underline`
      )}
      {btn(
        editor.isActive(`strike`),
        () => editor.chain().focus().toggleStrike().run(),
        <Strikethrough className="size-3.5" />,
        `Strikethrough`
      )}
      {btn(
        editor.isActive(`code`),
        () => editor.chain().focus().toggleCode().run(),
        <Code className="size-3.5" />,
        `Code`
      )}
      <div className="bubble-separator" />
      {btn(editor.isActive(`link`), toggleLink, <LinkIcon className="size-3.5" />, `Link`)}
      {btn(
        editor.isActive(`blockquote`),
        () => editor.chain().focus().toggleBlockquote().run(),
        <Quote className="size-3.5" />,
        `Quote`
      )}
      <div className="bubble-separator" />
      {btn(
        editor.isActive(`bulletList`),
        () => editor.chain().focus().toggleBulletList().run(),
        <List className="size-3.5" />,
        `Bullet list`
      )}
      {btn(
        editor.isActive(`orderedList`),
        () => editor.chain().focus().toggleOrderedList().run(),
        <ListOrdered className="size-3.5" />,
        `Numbered list`
      )}
      <div className="bubble-separator" />
      {btn(
        false,
        () => editor.chain().focus().unsetAllMarks().clearNodes().run(),
        <RemoveFormatting className="size-3.5" />,
        `Clear formatting`
      )}
    </div>
  )
}

export const MarkdownEditor = forwardRef<MarkdownEditorRef, MarkdownEditorProps>(
  ({ markdown, onChange, onBlur, placeholder, autoFocus }, ref) => {
    const onChangeRef = useRef(onChange)
    onChangeRef.current = onChange

    const editor = useEditor({
      extensions: [
        StarterKit.configure({
          heading: { levels: [1, 2, 3] },
        }),
        Underline,
        Link.configure({
          openOnClick: false,
          HTMLAttributes: { class: `editor-link` },
        }),
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
      immediatelyRender: false,
      onUpdate: ({ editor: e }) => {
        onChangeRef.current(getEditorMarkdown(e))
      },
      onBlur: () => {
        onBlur?.()
      },
      editorProps: {
        attributes: {
          class: `tiptap-content`,
        },
        handleDOMEvents: {
          contextmenu: (_view, event) => {
            event.preventDefault()
            return true
          },
        },
      },
    })

    useImperativeHandle(ref, () => ({
      setMarkdown: (md: string) => {
        editor?.commands.setContent(md)
      },
      getMarkdown: () => {
        return getEditorMarkdown(editor)
      },
    }))

    useEffect(() => {
      if (autoFocus && editor) {
        editor.commands.focus(`end`)
      }
    }, [autoFocus, editor])

    return (
      <div className="tiptap-wrapper">
        <BubbleToolbar editor={editor} />
        <EditorContent editor={editor} />
      </div>
    )
  }
)

MarkdownEditor.displayName = `MarkdownEditor`
