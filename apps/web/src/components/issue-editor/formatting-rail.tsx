import { useEffect, useReducer, useRef, useState } from "react"
import type { Editor } from "@tiptap/react"
import { NodeSelection } from "@tiptap/pm/state"
import { conceptIcon } from "@/lib/icons.generated"
import { EmojiPickerPopover } from "@/components/emoji-picker"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { acceptedImageContentTypes } from "@/lib/storage/issue-attachments"
import {
  partitionUploadFiles,
  type MarkdownEditorImageUploadConfig,
} from "@/components/issue-editor/markdown-editor"
import { cn } from "@/lib/utils"

// EXP-568 — the ONE formatting rail behind both editor chromes: the desktop
// selection bubble (selection-rail.tsx) and the phone keyboard bar
// (mobile-formatting-bar.tsx). It replaces the always-on static toolbar.
//
// EXP-587: the INSERT controls (emoji · image · attach) are a separate piece,
// `EditorInsertControls`. The phone keyboard bar still carries them (it is
// up whenever the keyboard is, caret or selection), but the desktop bubble
// only exists over a SELECTION, where inserting replaced the selected text —
// so on desktop they live in the static `EditorInsertBar` under the editor
// instead, Linear-style.
//
// The rail has three MODES and the host owns the mode state (the desktop
// bubble has to keep itself visible while a non-main mode is up):
//   main → the everyday row
//   text → paragraph/headings + the character marks (behind the "Aa" button)
//   link → the inline URL editor
// Mode content REPLACES the row entirely rather than expanding it; a phone
// keyboard bar has no room for a second line.

export type FormattingRailMode = `main` | `text` | `link`

const EmojiIcon = conceptIcon(`editor-emoji`)
const ImageIcon = conceptIcon(`editor-image`)
const AttachIcon = conceptIcon(`ui-attach`)
const IssueRefIcon = conceptIcon(`editor-issue-ref`)
const LinkIcon = conceptIcon(`editor-link`)
const TextFormatIcon = conceptIcon(`editor-text-format`)
const TextIcon = conceptIcon(`editor-text`)
const ListIcon = conceptIcon(`editor-list`)
const ListOrderedIcon = conceptIcon(`editor-list-ordered`)
const ListTodoIcon = conceptIcon(`editor-list-todo`)
const QuoteIcon = conceptIcon(`editor-quote`)
const CodeIcon = conceptIcon(`editor-code`)
const BoldIcon = conceptIcon(`editor-bold`)
const ItalicIcon = conceptIcon(`editor-italic`)
const StrikethroughIcon = conceptIcon(`editor-strikethrough`)
const ClearFormattingIcon = conceptIcon(`editor-clear-formatting`)
const Heading1Icon = conceptIcon(`editor-heading-1`)
const Heading2Icon = conceptIcon(`editor-heading-2`)
const Heading3Icon = conceptIcon(`editor-heading-3`)
const BackIcon = conceptIcon(`ui-back`)
const CheckIcon = conceptIcon(`ui-check`)
const CloseIcon = conceptIcon(`ui-close`)
const KeyboardDownIcon = conceptIcon(`ui-chevron-down`)
const DeleteIcon = conceptIcon(`ui-delete`)

/**
 * Does this device get the rail's Delete table action?
 *
 * EXP-727 gated it on the mobile BREAKPOINT, but the table chrome it stands
 * in for (table-controls.tsx) is HOVER-only: a tablet in landscape is wide
 * enough to be called `desktop` here and still has no pointer to reveal the
 * grips with, leaving no way to delete a table at all. Any touch device gets
 * the button — no hover, or the width breakpoint says mobile.
 */
export function showsTableDelete(platform: `desktop` | `mobile`): boolean {
  if (platform === `mobile`) return true
  if (typeof window === `undefined` || typeof window.matchMedia !== `function`)
    return false
  return window.matchMedia(`(hover: none)`).matches
}

/**
 * The text to insert for the `#` button. The issue-ref autocomplete only
 * triggers at a TOKEN start (`/(?:^|\s)#…/` in lib/editor-autocomplete.ts), so
 * a `#` typed straight after a word would insert a dead character instead of
 * opening the picker — prepend a space in that case.
 */
export function issueRefInsertionText(charBefore: string | undefined) {
  if (!charBefore || /\s/.test(charBefore)) return `#`
  return ` #`
}

/**
 * Re-render on every editor transaction so the `is-active` states track the
 * caret. `useEditor` deliberately does NOT re-render its host on transactions
 * (tiptap v3's `shouldRerenderOnTransaction` defaults to false), and the rail
 * is the one surface that has to mirror editor state live. Guarded on `.on`
 * so tests can drive the rail with a lightweight editor stand-in.
 */
function useEditorTransactions(editor: Editor) {
  const [, bump] = useReducer((n: number) => n + 1, 0)
  useEffect(() => {
    if (typeof editor?.on !== `function`) return
    editor.on(`transaction`, bump)
    return () => {
      editor.off(`transaction`, bump)
    }
  }, [editor])
}

/** A rail button. `tabIndex={-1}` + the mousedown preventDefault keep the tab
 *  order and the editor selection intact (EXP-10) — ported verbatim from the
 *  static toolbar it replaces. */
function RailButton({
  active,
  destructive,
  label,
  onClick,
  children,
}: {
  active?: boolean
  /** Red glyph — the rail's one destructive action (Delete table, EXP-727). */
  destructive?: boolean
  label: string
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      tabIndex={-1}
      onMouseDown={(event) => event.preventDefault()}
      onClick={onClick}
      className={cn(active && `is-active`, destructive && `is-destructive`)}
      title={label}
      aria-label={label}
    >
      {children}
    </button>
  )
}

/** The same shape as RailButton, for a Radix trigger that must receive the
 *  library's own props (`asChild`). */
function RailTriggerButton({
  active,
  label,
  children,
  ...rest
}: {
  active?: boolean
  label: string
  children: React.ReactNode
} & React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      tabIndex={-1}
      {...rest}
      onMouseDown={(event) => {
        event.preventDefault()
        rest.onMouseDown?.(event)
      }}
      className={active ? `is-active` : ``}
      title={label}
      aria-label={label}
    >
      {children}
    </button>
  )
}

function RailSeparator() {
  return <div className="rail-separator" aria-hidden />
}

interface FormattingRailProps {
  editor: Editor
  imageUpload?: MarkdownEditorImageUploadConfig
  platform: `desktop` | `mobile`
  mode: FormattingRailMode
  onModeChange: (mode: FormattingRailMode) => void
  /** Fired while a rail-owned overlay (emoji popover, a dropdown) is open, so
   *  the host can keep the rail alive across the focus loss it causes. */
  onOverlayOpenChange?: (open: boolean) => void
  /** Mobile only: the keyboard-down button. */
  onDismissKeyboard?: () => void
}

export function FormattingRail({
  editor,
  imageUpload,
  platform,
  mode,
  onModeChange,
  onOverlayOpenChange,
  onDismissKeyboard,
}: FormattingRailProps) {
  useEditorTransactions(editor)

  const isActive = (name: string, attrs?: Record<string, unknown>) => {
    try {
      return editor.isActive(name, attrs)
    } catch {
      return false
    }
  }

  const insertPlainText = (text: string) => insertPlainTextAt(editor, text)

  const insertIssueRef = () => {
    const { $from } = editor.state.selection
    const charBefore =
      $from.parent.isTextblock && $from.parentOffset > 0
        ? $from.parent.textBetween(
            $from.parentOffset - 1,
            $from.parentOffset,
            undefined,
            `￼`
          )
        : undefined
    insertPlainText(issueRefInsertionText(charBefore))
  }

  const bulletActive = isActive(`bulletList`)
  const orderedActive = isActive(`orderedList`)
  const taskActive = isActive(`taskList`)
  const ActiveListIcon = orderedActive
    ? ListOrderedIcon
    : taskActive
      ? ListTodoIcon
      : ListIcon

  const listControl = (
    <DropdownMenu onOpenChange={onOverlayOpenChange}>
      <DropdownMenuTrigger asChild>
        <RailTriggerButton
          label="Lists"
          active={bulletActive || orderedActive || taskActive}
        >
          <ActiveListIcon className="size-3.5" />
        </RailTriggerButton>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" side={platform === `mobile` ? `top` : `bottom`}>
        <DropdownMenuItem
          onSelect={() => editor.chain().focus().toggleBulletList().run()}
        >
          <ListIcon className="size-4" />
          List
          {bulletActive && <CheckIcon className="ml-auto size-4" />}
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={() => editor.chain().focus().toggleOrderedList().run()}
        >
          <ListOrderedIcon className="size-4" />
          Numbered
          {orderedActive && <CheckIcon className="ml-auto size-4" />}
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={() => editor.chain().focus().toggleTaskList().run()}
        >
          <ListTodoIcon className="size-4" />
          Checklist
          {taskActive && <CheckIcon className="ml-auto size-4" />}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )

  const mainContent = (
    <>
      {platform === `mobile` && (
        <EditorInsertControls
          editor={editor}
          imageUpload={imageUpload}
          platform="mobile"
          onOverlayOpenChange={onOverlayOpenChange}
        />
      )}
      <RailButton label="Insert issue reference" onClick={insertIssueRef}>
        <IssueRefIcon className="size-3.5" />
      </RailButton>
      <RailButton
        label="Link"
        active={isActive(`link`)}
        onClick={() => onModeChange(`link`)}
      >
        <LinkIcon className="size-3.5" />
      </RailButton>
      <RailSeparator />
      <RailButton label="Text formatting" onClick={() => onModeChange(`text`)}>
        <TextFormatIcon className="size-3.5" />
      </RailButton>
      {listControl}
      <RailButton
        label="Quote"
        active={isActive(`blockquote`)}
        onClick={() => editor.chain().focus().toggleBlockquote().run()}
      >
        <QuoteIcon className="size-3.5" />
      </RailButton>
      <RailButton
        label="Code"
        active={isActive(`code`)}
        onClick={() => editor.chain().focus().toggleCode().run()}
      >
        <CodeIcon className="size-3.5" />
      </RailButton>
      {/* EXP-727: a touch device has no hover chrome for tables
          (table-controls.tsx is pointer-only) and a long-press inside a cell
          is the browser's own selection, so the ONE touch table action rides
          the keyboard bar while the caret sits in a table. Row/column edits
          stay on the hover chrome. */}
      {showsTableDelete(platform) && isActive(`table`) && (
        <>
          <RailSeparator />
          <RailButton
            label="Delete table"
            destructive
            onClick={() => editor.chain().focus().deleteTable().run()}
          >
            <DeleteIcon className="size-3.5" />
          </RailButton>
        </>
      )}
      <div className="flex-1" aria-hidden />
      {platform === `mobile` && (
        <RailButton
          label="Hide keyboard"
          onClick={() => onDismissKeyboard?.()}
        >
          <KeyboardDownIcon className="size-3.5" />
        </RailButton>
      )}
    </>
  )

  // ── text ──────────────────────────────────────────────────────────────────

  const headingActive = (level: 1 | 2 | 3) => isActive(`heading`, { level })
  const paragraphActive =
    isActive(`paragraph`) &&
    !headingActive(1) &&
    !headingActive(2) &&
    !headingActive(3) &&
    !bulletActive &&
    !orderedActive &&
    !taskActive

  const textContent = (
    <>
      <RailButton label="Back" onClick={() => onModeChange(`main`)}>
        <BackIcon className="size-3.5" />
      </RailButton>
      <RailButton
        label="Text"
        active={paragraphActive}
        onClick={() => editor.chain().focus().setParagraph().run()}
      >
        <TextIcon className="size-3.5" />
      </RailButton>
      <RailButton
        label="Heading 1"
        active={headingActive(1)}
        onClick={() =>
          editor.chain().focus().toggleHeading({ level: 1 }).run()
        }
      >
        <Heading1Icon className="size-3.5" />
      </RailButton>
      <RailButton
        label="Heading 2"
        active={headingActive(2)}
        onClick={() =>
          editor.chain().focus().toggleHeading({ level: 2 }).run()
        }
      >
        <Heading2Icon className="size-3.5" />
      </RailButton>
      <RailButton
        label="Heading 3"
        active={headingActive(3)}
        onClick={() =>
          editor.chain().focus().toggleHeading({ level: 3 }).run()
        }
      >
        <Heading3Icon className="size-3.5" />
      </RailButton>
      <RailSeparator />
      <RailButton
        label="Bold"
        active={isActive(`bold`)}
        onClick={() => editor.chain().focus().toggleBold().run()}
      >
        <BoldIcon className="size-3.5" />
      </RailButton>
      <RailButton
        label="Italic"
        active={isActive(`italic`)}
        onClick={() => editor.chain().focus().toggleItalic().run()}
      >
        <ItalicIcon className="size-3.5" />
      </RailButton>
      <RailButton
        label="Strikethrough"
        active={isActive(`strike`)}
        onClick={() => editor.chain().focus().toggleStrike().run()}
      >
        <StrikethroughIcon className="size-3.5" />
      </RailButton>
      <RailButton
        label="Clear formatting"
        onClick={() =>
          editor.chain().focus().unsetAllMarks().clearNodes().run()
        }
      >
        <ClearFormattingIcon className="size-3.5" />
      </RailButton>
      <div className="flex-1" aria-hidden />
    </>
  )

  // ── link ──────────────────────────────────────────────────────────────────

  const linkContent = (
    <LinkEditor
      editor={editor}
      onDone={() => onModeChange(`main`)}
    />
  )

  return (
    <div
      // Keyed on the mode: the swap is a content REPLACEMENT, so React
      // remounts the row and tw-animate-css re-runs the enter animation.
      key={mode}
      className={cn(
        `flex w-full items-center gap-px`,
        `animate-in fade-in zoom-in-95 duration-fast ease-standard motion-reduce:animate-none`
      )}
    >
      {mode === `main` && mainContent}
      {mode === `text` && textContent}
      {mode === `link` && linkContent}
    </div>
  )
}

/** A selected image (NodeSelection) would be REPLACED by inserted text —
 *  insert after it instead, mirroring MarkdownEditor's insertImage. */
function insertPlainTextAt(editor: Editor, text: string) {
  const { selection } = editor.state
  editor
    .chain()
    .focus()
    .command(({ tr }) => {
      if (selection instanceof NodeSelection) {
        tr.insertText(text, selection.to, selection.to)
      } else {
        tr.insertText(text)
      }
      return true
    })
    .run()
}

interface EditorInsertControlsProps {
  editor: Editor
  imageUpload?: MarkdownEditorImageUploadConfig
  platform: `desktop` | `mobile`
  /** Fired while the emoji popover / the phone's image-or-file menu is open,
   *  so a host that dies on blur (the keyboard bar) can hold itself up. */
  onOverlayOpenChange?: (open: boolean) => void
}

/**
 * EXP-587: emoji · image · attach. The file pickers have no keyboard shortcut
 * to fall back on, so this is their only entry; the hidden `<input type=file>`
 * elements ride along. Rendered inside the phone rail and, on desktop, by
 * [`EditorInsertBar`].
 */
export function EditorInsertControls({
  editor,
  imageUpload,
  platform,
  onOverlayOpenChange,
}: EditorInsertControlsProps) {
  const imageInputRef = useRef<HTMLInputElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)

  const routeFiles = (fileList: FileList | null) => {
    if (!imageUpload) return
    const { images, others } = partitionUploadFiles(fileList)
    if (images.length > 0) void imageUpload.onFiles(images)
    if (others.length > 0) void imageUpload.onOtherFiles?.(others)
  }

  const hiddenInputs = imageUpload?.enabled ? (
    <>
      <input
        ref={imageInputRef}
        type="file"
        accept={acceptedImageContentTypes.join(`,`)}
        multiple
        hidden
        onChange={(event) => {
          routeFiles(event.target.files)
          event.target.value = ``
        }}
      />
      {imageUpload.onOtherFiles && (
        <input
          ref={fileInputRef}
          type="file"
          multiple
          hidden
          onChange={(event) => {
            routeFiles(event.target.files)
            event.target.value = ``
          }}
        />
      )}
    </>
  ) : null

  const emojiControl = (
    <EmojiPickerPopover
      side={platform === `mobile` ? `top` : undefined}
      onOpenChange={onOverlayOpenChange}
      onPick={(unicode) => insertPlainTextAt(editor, unicode)}
    >
      <RailTriggerButton label="Insert emoji">
        <EmojiIcon className="size-3.5" />
      </RailTriggerButton>
    </EmojiPickerPopover>
  )

  const attachControls = !imageUpload?.enabled ? null : platform ===
    `desktop` ? (
    <>
      <RailButton
        label="Insert image"
        onClick={() => imageInputRef.current?.click()}
      >
        <ImageIcon className="size-3.5" />
      </RailButton>
      {imageUpload.onOtherFiles && (
        <RailButton
          label="Attach file"
          onClick={() => fileInputRef.current?.click()}
        >
          <AttachIcon className="size-3.5" />
        </RailButton>
      )}
    </>
  ) : imageUpload.onOtherFiles ? (
    // One glyph on a phone rail — the two destinations become menu items.
    <DropdownMenu onOpenChange={onOverlayOpenChange}>
      <DropdownMenuTrigger asChild>
        <RailTriggerButton label="Insert image or file">
          <ImageIcon className="size-3.5" />
        </RailTriggerButton>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" side="top">
        <DropdownMenuItem onSelect={() => imageInputRef.current?.click()}>
          <ImageIcon className="size-4" />
          Image
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => fileInputRef.current?.click()}>
          <AttachIcon className="size-4" />
          File
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  ) : (
    <RailButton
      label="Insert image"
      onClick={() => imageInputRef.current?.click()}
    >
      <ImageIcon className="size-3.5" />
    </RailButton>
  )

  return (
    <>
      {hiddenInputs}
      {emojiControl}
      {attachControls}
    </>
  )
}

/**
 * EXP-587: the static Linear-style strip under a desktop editor. Shares the
 * rail's button recipe (`.formatting-rail`) but is not a rail: it never
 * floats, never hides, and needs no mode — it is the one place the insert
 * controls live on desktop.
 */
export function EditorInsertBar({
  editor,
  imageUpload,
}: {
  editor: Editor
  imageUpload?: MarkdownEditorImageUploadConfig
}) {
  return (
    <div
      data-editor-insert-bar=""
      className="formatting-rail editor-insert-bar flex items-center gap-px"
    >
      <EditorInsertControls
        editor={editor}
        imageUpload={imageUpload}
        platform="desktop"
      />
    </div>
  )
}

/** The inline URL editor — the whole rail while `mode === 'link'`. */
function LinkEditor({
  editor,
  onDone,
}: {
  editor: Editor
  onDone: () => void
}) {
  const inputRef = useRef<HTMLInputElement>(null)
  const [url, setUrl] = useState(() => {
    const href = editor.getAttributes(`link`).href
    return typeof href === `string` ? href : ``
  })

  useEffect(() => {
    const frame = requestAnimationFrame(() => inputRef.current?.focus())
    return () => cancelAnimationFrame(frame)
  }, [])

  const apply = () => {
    const href = url.trim()
    if (href) {
      editor.chain().focus().extendMarkRange(`link`).setLink({ href }).run()
    } else {
      editor.chain().focus().extendMarkRange(`link`).unsetLink().run()
    }
    onDone()
  }

  return (
    // The dialog shells whitelist Escape aimed at this layer so it closes the
    // link editor instead of the dialog (dialog-shell.tsx).
    <div className="flex w-full items-center gap-1" data-editor-link-edit="">
      <input
        ref={inputRef}
        className="rail-link-input"
        value={url}
        placeholder="https://…"
        aria-label="Link URL"
        onMouseDown={(event) => event.stopPropagation()}
        onChange={(event) => setUrl(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === `Enter`) {
            event.preventDefault()
            apply()
          } else if (event.key === `Escape`) {
            event.preventDefault()
            onDone()
          }
        }}
      />
      <RailButton label="Apply link" onClick={apply}>
        <CheckIcon className="size-3.5" />
      </RailButton>
      <RailButton label="Cancel link" onClick={onDone}>
        <CloseIcon className="size-3.5" />
      </RailButton>
    </div>
  )
}
