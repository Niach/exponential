import { useEffect, useReducer, useRef, useState } from "react"
import type { Editor } from "@tiptap/react"
import { NodeSelection } from "@tiptap/pm/state"
import { FocusScope } from "radix-ui/internal"
import {
  Button,
  conceptIcon,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  Input,
} from "@exp/ui"
import { EmojiPickerPopover } from "@/components/emoji-picker"
import {
  acceptedImageContentTypes,
  acceptedVideoUploadContentTypes,
} from "@/lib/storage/issue-attachments"
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
 * Re-render on every editor transaction so the `aria-pressed` states track the
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

// EXP-960: a rail button IS the ghost icon button (`Button variant="ghost"
// size="icon-sm"`, the styleguide's ghost-icon-button entry): the 32px box,
// the 16px glyph at 70% foreground, the row wash on hover. The toggled state
// rides `aria-pressed`, which the ghost variant paints; the one destructive
// glyph (Delete table, EXP-727) is a colour on top. The rail used to carry
// its own 28px CSS button system in styles.css; this is what replaced it.
const RAIL_BUTTON_CLASS = `text-foreground/70`

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
  /** A toggle's state. Leave undefined on a plain action button — a button
   *  with no `aria-pressed` is an action, not a toggle. */
  active?: boolean
  /** Red glyph — the rail's one destructive action (Delete table, EXP-727). */
  destructive?: boolean
  label: string
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      tabIndex={-1}
      onMouseDown={(event) => event.preventDefault()}
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        RAIL_BUTTON_CLASS,
        destructive && `text-destructive hover:text-destructive`
      )}
      title={label}
      aria-label={label}
    >
      {children}
    </Button>
  )
}

/** The same shape as RailButton, for a Radix trigger that must receive the
 *  library's own props (`asChild`), including its ref. */
function RailTriggerButton({
  active,
  label,
  children,
  className,
  ...rest
}: {
  active?: boolean
  label: string
  children: React.ReactNode
} & React.ComponentProps<`button`>) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      tabIndex={-1}
      {...rest}
      onMouseDown={(event) => {
        event.preventDefault()
        rest.onMouseDown?.(event)
      }}
      aria-pressed={active}
      className={cn(RAIL_BUTTON_CLASS, className)}
      title={label}
      aria-label={label}
    >
      {children}
    </Button>
  )
}

function RailSeparator() {
  return <div className="mx-0.5 h-4 w-px shrink-0 bg-border" aria-hidden />
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
          <ActiveListIcon />
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
        <IssueRefIcon />
      </RailButton>
      <RailButton
        label="Link"
        active={isActive(`link`)}
        onClick={() => onModeChange(`link`)}
      >
        <LinkIcon />
      </RailButton>
      <RailSeparator />
      <RailButton label="Text formatting" onClick={() => onModeChange(`text`)}>
        <TextFormatIcon />
      </RailButton>
      {listControl}
      <RailButton
        label="Quote"
        active={isActive(`blockquote`)}
        onClick={() => editor.chain().focus().toggleBlockquote().run()}
      >
        <QuoteIcon />
      </RailButton>
      <RailButton
        label="Code"
        active={isActive(`code`)}
        onClick={() => editor.chain().focus().toggleCode().run()}
      >
        <CodeIcon />
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
            <DeleteIcon />
          </RailButton>
        </>
      )}
      <div className="flex-1" aria-hidden />
      {platform === `mobile` && (
        <RailButton
          label="Hide keyboard"
          onClick={() => onDismissKeyboard?.()}
        >
          <KeyboardDownIcon />
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
        <BackIcon />
      </RailButton>
      <RailButton
        label="Text"
        active={paragraphActive}
        onClick={() => editor.chain().focus().setParagraph().run()}
      >
        <TextIcon />
      </RailButton>
      <RailButton
        label="Heading 1"
        active={headingActive(1)}
        onClick={() =>
          editor.chain().focus().toggleHeading({ level: 1 }).run()
        }
      >
        <Heading1Icon />
      </RailButton>
      <RailButton
        label="Heading 2"
        active={headingActive(2)}
        onClick={() =>
          editor.chain().focus().toggleHeading({ level: 2 }).run()
        }
      >
        <Heading2Icon />
      </RailButton>
      <RailButton
        label="Heading 3"
        active={headingActive(3)}
        onClick={() =>
          editor.chain().focus().toggleHeading({ level: 3 }).run()
        }
      >
        <Heading3Icon />
      </RailButton>
      <RailSeparator />
      <RailButton
        label="Bold"
        active={isActive(`bold`)}
        onClick={() => editor.chain().focus().toggleBold().run()}
      >
        <BoldIcon />
      </RailButton>
      <RailButton
        label="Italic"
        active={isActive(`italic`)}
        onClick={() => editor.chain().focus().toggleItalic().run()}
      >
        <ItalicIcon />
      </RailButton>
      <RailButton
        label="Strikethrough"
        active={isActive(`strike`)}
        onClick={() => editor.chain().focus().toggleStrike().run()}
      >
        <StrikethroughIcon />
      </RailButton>
      <RailButton
        label="Clear formatting"
        onClick={() =>
          editor.chain().focus().unsetAllMarks().clearNodes().run()
        }
      >
        <ClearFormattingIcon />
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
    const { images, media, others } = partitionUploadFiles(fileList)
    if (images.length > 0) void imageUpload.onFiles(images)
    // EXP-824: clips embed as media blocks; a host without that flow keeps
    // them as plain files (the create dialog embeds them post-create).
    if (imageUpload.onMediaFiles) {
      if (media.length > 0) void imageUpload.onMediaFiles(media)
      if (others.length > 0) void imageUpload.onOtherFiles?.(others)
    } else if (media.length + others.length > 0) {
      void imageUpload.onOtherFiles?.([...media, ...others])
    }
  }

  const hiddenInputs = imageUpload?.enabled ? (
    <>
      <input
        ref={imageInputRef}
        type="file"
        // EXP-824: the picker offers clips beside images — a video lands as
        // an inline media block, so it belongs on the same button.
        accept={[
          ...acceptedImageContentTypes,
          ...acceptedVideoUploadContentTypes,
          `audio/*`,
        ].join(`,`)}
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
        <EmojiIcon />
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
        <ImageIcon />
      </RailButton>
      {imageUpload.onOtherFiles && (
        <RailButton
          label="Attach file"
          onClick={() => fileInputRef.current?.click()}
        >
          <AttachIcon />
        </RailButton>
      )}
    </>
  ) : imageUpload.onOtherFiles ? (
    // One glyph on a phone rail — the two destinations become menu items.
    <DropdownMenu onOpenChange={onOverlayOpenChange}>
      <DropdownMenuTrigger asChild>
        <RailTriggerButton label="Insert image or file">
          <ImageIcon />
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
      <ImageIcon />
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
 * EXP-587: the static Linear-style strip under a desktop editor. The same
 * rail buttons, but not a rail: it never floats, never hides, and needs no
 * mode — it is the one place the insert controls live on desktop.
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
      className="editor-insert-bar flex items-center gap-px"
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

  const apply = () => {
    const href = url.trim()
    if (href) {
      editor.chain().focus().extendMarkRange(`link`).setLink({ href }).run()
    } else {
      editor.chain().focus().extendMarkRange(`link`).unsetLink().run()
    }
    onDone()
  }

  const cancel = () => {
    // Hand focus (and the selection under it) back to the editor ourselves:
    // the scope below is told NOT to on unmount, see there.
    editor.commands.focus()
    onDone()
  }

  return (
    // The dialog shells whitelist Escape aimed at this layer so it closes the
    // link editor instead of the dialog (dialog-shell.tsx).
    //
    // EXP-967: both rails portal to document.body, OUTSIDE the create/edit
    // dialog's Radix focus trap, which watches `focusin` document-wide and
    // pulled focus straight back to the editor the moment this field took it
    // (the typed URL then replaced the selection). Radix's own escape hatch is
    // a nested FocusScope: mounting one pauses every scope above it on the
    // stack for as long as it lives, and unmounting resumes them. Untrapped,
    // so Tab still leaves the field; the mount autofocus IS the field focus
    // the old requestAnimationFrame did.
    <FocusScope.Root
      className="flex w-full items-center gap-1"
      data-editor-link-edit=""
      onMountAutoFocus={(event) => {
        event.preventDefault()
        inputRef.current?.focus()
      }}
      // Apply and Cancel refocus the editor themselves (with the selection
      // restored); the scope's own return-focus would otherwise fire a tick
      // later, after a closing dialog has already moved focus elsewhere.
      onUnmountAutoFocus={(event) => event.preventDefault()}
    >
      <Input
        ref={inputRef}
        // The SearchField's `sm` rung, shrunk to the width of a URL: the rail
        // is a 32px strip, so the stock 36px field would not fit in it.
        className="h-7 w-48 max-w-[60vw] px-2 text-xs md:text-xs"
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
            cancel()
          }
        }}
      />
      <RailButton label="Apply link" onClick={apply}>
        <CheckIcon />
      </RailButton>
      <RailButton label="Cancel link" onClick={cancel}>
        <CloseIcon />
      </RailButton>
    </FocusScope.Root>
  )
}
