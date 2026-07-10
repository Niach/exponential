import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react"
import { createPortal } from "react-dom"
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
import { MarkdownParagraph } from "@/components/issue-editor/markdown-paragraph"
import { IssueRefExtension } from "@/lib/issue-ref-extension"
import { MentionPillExtension } from "@/lib/mention-pill-extension"
import {
  EditorAutocompleteExtension,
  type EditorAutocompleteActive,
} from "@/lib/editor-autocomplete"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { useMentions } from "@/components/mention-provider"
import {
  IssueCandidateRow,
  UserCandidateRow,
} from "@/components/autocomplete-rows"
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

// ── Pieces of the static toolbar above the editor ──

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
      // Keep the whole formatting toolbar out of the tab order — Tab from the
      // title must land in the editor content, not cycle these buttons
      // (EXP-10). They stay mouse/toolbar-accessible; the underlying actions
      // all have keyboard shortcuts inside the editor.
      tabIndex={-1}
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
 *  toolbar so focus stays within it. */
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
          tabIndex={-1}
          onMouseDown={(e) => e.preventDefault()}
          onClick={apply}
          title="Apply link"
        >
          <Check className="size-3.5" />
        </button>
        {editor.isActive(`link`) ? (
          <button
            type="button"
            tabIndex={-1}
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

/** The common formatting controls of the toolbar. */
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
      <div className="toolbar-separator" />
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
      <div className="toolbar-separator" />
      <LinkControl editor={editor} />
      <ToolbarButton
        active={editor.isActive(`blockquote`)}
        onClick={() => editor.chain().focus().toggleBlockquote().run()}
        title="Quote"
      >
        <Quote className="size-3.5" />
      </ToolbarButton>
      <div className="toolbar-separator" />
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
      <div className="toolbar-separator" />
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
      <div className="toolbar-separator" />
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
 *  button). */
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

    // Optional workspace contexts (null outside a workspace layout) that
    // resolve `#IDENTIFIER` tokens to issues and `@email` tokens to members
    // for the pill decorations + the caret autocomplete. Held in refs so the
    // extensions (created once) always read fresh data.
    const issueRefs = useIssueRefs()
    const issueRefsRef = useRef(issueRefs)
    issueRefsRef.current = issueRefs
    const mentions = useMentions()
    const mentionsRef = useRef(mentions)
    mentionsRef.current = mentions

    // In-progress `@`/`#` token at the caret (reported by the autocomplete
    // extension) driving the floating candidate menu below.
    const [autocomplete, setAutocomplete] =
      useState<EditorAutocompleteActive | null>(null)
    const [activeIndex, setActiveIndex] = useState(0)
    const keyHandlerRef = useRef<(event: KeyboardEvent) => boolean>(() => false)
    const menuRef = useRef<HTMLDivElement | null>(null)

    const editor = useEditor({
      extensions: [
        StarterKit.configure({
          heading: { levels: [1, 2, 3] },
          // Replaced below by CodeBlockLowlight for syntax highlighting.
          codeBlock: false,
          // Replaced below by MarkdownParagraph so intentional blank lines
          // round-trip through GFM (EXP-7).
          paragraph: false,
        }),
        MarkdownParagraph,
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
        IssueRefExtension.configure({
          getResolved: (identifier) =>
            issueRefsRef.current?.resolve(identifier) ?? null,
          onOpen: (identifier) => issueRefsRef.current?.open(identifier),
        }),
        MentionPillExtension.configure({
          getResolved: (email) => mentionsRef.current?.resolve(email) ?? null,
        }),
        EditorAutocompleteExtension.configure({
          onStateChange: (active) => {
            setAutocomplete(active)
            setActiveIndex(0)
          },
          onKeyDown: (event) => keyHandlerRef.current(event),
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

    // Re-run the issue-ref/mention decorations when resolution data changes
    // (issues/members sync in live) — a no-op transaction recomputes plugin
    // decorations without touching the document (onUpdate only fires on doc
    // changes).
    useEffect(() => {
      if (!editor || editor.isDestroyed) return
      editor.view.dispatch(editor.state.tr)
    }, [editor, issueRefs, mentions])

    // ── @mention / #issue autocomplete menu ──

    const mentionCandidates =
      autocomplete?.kind === `mention` && mentions
        ? mentions.search(autocomplete.query)
        : []
    const issueCandidates =
      autocomplete?.kind === `issueRef` && issueRefs
        ? issueRefs.search(autocomplete.query, { limit: 6 })
        : []
    const candidateCount =
      autocomplete?.kind === `mention`
        ? mentionCandidates.length
        : issueCandidates.length

    // Replace the in-progress `@query`/`#query` token with the canonical
    // plain-text interchange form (`@<email>` / `#<IDENTIFIER>`). insertText
    // keeps it plain text — never a custom node — so the markdown round-trip
    // stays untouched.
    const insertToken = (token: string) => {
      const range = autocomplete
      if (!range || !editor) return
      editor
        .chain()
        .focus()
        .command(({ tr }) => {
          tr.insertText(`${token} `, range.from, range.to)
          return true
        })
        .run()
    }

    const insertActive = (index: number) => {
      if (autocomplete?.kind === `mention` && mentionCandidates[index]) {
        insertToken(`@${mentionCandidates[index].email}`)
      } else if (autocomplete?.kind === `issueRef` && issueCandidates[index]) {
        insertToken(`#${issueCandidates[index].identifier}`)
      }
    }

    keyHandlerRef.current = (event) => {
      if (!autocomplete || candidateCount === 0) return false
      if (event.key === `ArrowDown`) {
        setActiveIndex((i) => (i + 1) % candidateCount)
        return true
      }
      if (event.key === `ArrowUp`) {
        setActiveIndex((i) => (i - 1 + candidateCount) % candidateCount)
        return true
      }
      if (event.key === `Enter` || event.key === `Tab`) {
        insertActive(activeIndex)
        return true
      }
      if (event.key === `Escape`) {
        setAutocomplete(null)
        return true
      }
      return false
    }

    // Anchor the menu at the trigger char in VIEWPORT coordinates and portal
    // it to document.body with position:fixed — inside the create-issue
    // dialog the editor sits in an overflow-y-auto scroll region that used to
    // clip the popup and inflate scrollHeight (EXP-54). Recomputed per
    // keystroke — every doc change re-reports the token with fresh positions.
    // Clamped to the viewport horizontally; flips above the caret when there
    // is no room below.
    const menuStyle = (() => {
      if (!editor || !autocomplete) return null
      if (candidateCount === 0) return null
      try {
        const coords = editor.view.coordsAtPos(autocomplete.from)
        const menuWidth = 288 // w-72
        const viewportPad = 8
        const left = Math.max(
          viewportPad,
          Math.min(coords.left, window.innerWidth - menuWidth - viewportPad)
        )
        const spaceBelow = window.innerHeight - coords.bottom - viewportPad
        const spaceAbove = coords.top - viewportPad
        // Above the dialog (shadcn DialogContent is z-50).
        const base = { left, zIndex: 60 }
        if (spaceBelow < 200 && spaceAbove > spaceBelow) {
          return {
            ...base,
            bottom: window.innerHeight - coords.top + 4,
            maxHeight: Math.max(48, Math.min(spaceAbove - 4, 320)),
          }
        }
        return {
          ...base,
          top: coords.bottom + 4,
          maxHeight: Math.max(48, Math.min(spaceBelow - 4, 320)),
        }
      } catch {
        return null
      }
    })()

    // A fixed-position popup detaches from the caret the moment any ancestor
    // scroll region moves (dialog body, page, sheet) — close it instead of
    // chasing the caret. Scrolling inside the menu itself stays allowed.
    const menuOpen = Boolean(editable && autocomplete && menuStyle)
    useEffect(() => {
      if (!menuOpen) return
      const close = (event: Event) => {
        if (
          event.target instanceof Node &&
          menuRef.current?.contains(event.target)
        ) {
          return
        }
        setAutocomplete(null)
      }
      window.addEventListener(`scroll`, close, true)
      window.addEventListener(`resize`, close)
      return () => {
        window.removeEventListener(`scroll`, close, true)
        window.removeEventListener(`resize`, close)
      }
    }, [menuOpen])

    return (
      <div className="tiptap-wrapper">
        {editable ? (
          <StaticToolbar editor={editor} imageUpload={imageUpload} />
        ) : null}
        <EditorContent editor={editor} />
        {editable && autocomplete && menuStyle
          ? createPortal(
              <div
                ref={menuRef}
                // Radix modal dialogs set pointer-events:none on <body> while
                // open; this portal lives outside the DialogContent subtree,
                // so it must re-enable pointer events itself or every click
                // falls through to the dialog beneath (EXP-54). The data
                // attribute lets dialog hosts whitelist interactions here in
                // their onInteractOutside guards.
                data-editor-autocomplete=""
                className="pointer-events-auto fixed w-72 overflow-y-auto rounded-md border bg-popover shadow-md"
                style={menuStyle}
              >
                {autocomplete.kind === `mention` &&
                  mentionCandidates.map((user, i) => (
                    <UserCandidateRow
                      key={user.id}
                      user={user}
                      active={i === activeIndex}
                      onSelect={() => insertActive(i)}
                      onHover={() => setActiveIndex(i)}
                    />
                  ))}
                {autocomplete.kind === `issueRef` &&
                  issueCandidates.map((issue, i) => (
                    <IssueCandidateRow
                      key={issue.id}
                      issue={issue}
                      active={i === activeIndex}
                      onSelect={() => insertActive(i)}
                      onHover={() => setActiveIndex(i)}
                    />
                  ))}
              </div>,
              document.body
            )
          : null}
      </div>
    )
  }
)

MarkdownEditor.displayName = `MarkdownEditor`
