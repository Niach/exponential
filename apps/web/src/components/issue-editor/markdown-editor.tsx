import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react"
import { createPortal } from "react-dom"
import { type Editor, useEditor, EditorContent } from "@tiptap/react"
import { NodeSelection, TextSelection } from "@tiptap/pm/state"
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
import { MarkdownImage } from "@/lib/markdown-image"
import {
  MarkdownTableExtensions,
  moveSelectionAfterTable,
  tableTrailingNodeOptions,
} from "@/lib/markdown-table"
import { MarkdownParagraph } from "@/components/issue-editor/markdown-paragraph"
import { ArrowInputRules } from "@/lib/arrow-input-rules"
import { IssueRefExtension } from "@/lib/issue-ref-extension"
import { MentionPillExtension } from "@/lib/mention-pill-extension"
import {
  EditorAutocompleteExtension,
  type EditorAutocompleteActive,
} from "@/lib/editor-autocomplete"
import { useIssueRefs } from "@/components/issue-ref-provider"
import { useMentions } from "@/components/mention-provider"
import {
  EmojiCandidateRow,
  IssueCandidateRow,
  UserCandidateRow,
} from "@/components/autocomplete-rows"
import { EditorSelectionRail } from "@/components/issue-editor/selection-rail"
import { EditorInsertBar } from "@/components/issue-editor/formatting-rail"
import { EditorTableControls } from "@/components/issue-editor/table-controls"
import { EditorMobileFormattingBar } from "@/components/issue-editor/mobile-formatting-bar"
import { useIsMobile } from "@/hooks/use-mobile"
import {
  findEmojiByShortcode,
  pushRecentEmoji,
  searchEmoji,
  useEmojiData,
} from "@/lib/emoji"
import { isAcceptedImageContentType } from "@/lib/storage/issue-attachments"
import {
  releaseKeyboardClearance,
  revealCaretAboveKeyboard,
} from "@/lib/keyboard-caret"
import { cn } from "@/lib/utils"

export interface MarkdownEditorImageUploadConfig {
  disabledReason?: string
  enabled: boolean
  onFiles: (files: File[]) => Promise<void>
  /**
   * Receives pasted/dropped/picked files that are NOT of an accepted inline
   * image type (EXP-297: only png/jpeg/webp/gif/avif may be embedded in
   * markdown — everything else belongs in the issue's Files section). When
   * absent, such files are ignored.
   */
  onOtherFiles?: (files: File[]) => void | Promise<void>
  uploading?: boolean
}

export interface MarkdownEditorRef {
  focus: () => void
  setMarkdown: (md: string) => void
  // Null while the editor instance does not exist yet (creation is deferred
  // by immediatelyRender: false) — an empty string would be
  // indistinguishable from a legitimately empty document.
  getMarkdown: () => string | null
  insertImage: (image: { alt?: string; src: string }) => void
  // Inserts at the very end of the document instead of the caret — the Files
  // section's attach button routes images here (EXP-316).
  appendImage: (image: { alt?: string; src: string }) => void
}

interface MarkdownEditorProps {
  editable?: boolean
  imageUpload?: MarkdownEditorImageUploadConfig
  markdown: string
  onChange: (markdown: string) => void
  onBlur?: () => void
  placeholder?: string
  autoFocus?: boolean
  /**
   * Compact chat presentation (EXP-440): the agent-session feed renders each
   * bubble through this editor, where the document paddings and heading sizes
   * of an issue description would dwarf a one-line narration.
   */
  appearance?: `document` | `chat`
  /**
   * Turn bare URLs into links, and treat a single newline as a hard break —
   * markdown-it options for text that was authored as chat, not as a GFM
   * document. Both are read ONCE, at editor creation (tiptap-markdown builds
   * its markdown-it instance in onBeforeCreate), so they must not change over
   * an instance's life.
   */
  linkify?: boolean
  hardBreaks?: boolean
  /** Accessible name of the editable/readonly region. */
  ariaLabel?: string
  /**
   * Fired when the editable content gains/loses focus (EXP-568: the issue
   * detail page hides its floating mobile bar while the description is being
   * written, so the keyboard rail is the only chrome on screen).
   */
  onFocusChange?: (focused: boolean) => void
  /**
   * Height (px) of a sticky overlay at the TOP of the scrollport the editor
   * lives in. ProseMirror's scroll-into-view treats the region under such an
   * overlay as visible, so the caret could sit hidden behind it; this feeds
   * `scrollThreshold`/`scrollMargin` so edits scroll clear of the band.
   */
  topScrollInset?: number
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

/**
 * Splits an upload batch into inline-embeddable images (the exact 5-type
 * accepted set — the only types the markdown pipeline may reference) and
 * everything else, which routes to the Files-section flow via onOtherFiles.
 */
export function partitionUploadFiles(fileList: FileList | null | undefined) {
  const files = Array.from(fileList ?? [])
  return {
    images: files.filter((file) => isAcceptedImageContentType(file.type)),
    others: files.filter((file) => !isAcceptedImageContentType(file.type)),
  }
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
      topScrollInset,
      appearance = `document`,
      linkify,
      hardBreaks,
      ariaLabel,
      onFocusChange,
    },
    ref
  ) => {
    const isMobile = useIsMobile()
    const onChangeRef = useRef(onChange)
    onChangeRef.current = onChange
    const onFocusChangeRef = useRef(onFocusChange)
    onFocusChangeRef.current = onFocusChange
    const imageUploadRef = useRef(imageUpload)
    imageUploadRef.current = imageUpload
    // useEditor captures editorProps once, so the drop/paste handlers below
    // must not close over `editable` — it flips false→true when team_members
    // syncs in and the permission check flips.
    const editableRef = useRef(editable)
    editableRef.current = editable

    // Optional team contexts (null outside a team layout) that
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
    // The positioning origin for the table hover overlay (EXP-726).
    const wrapperRef = useRef<HTMLDivElement | null>(null)

    // True when `handleUploadedFiles` below would claim this batch — the drop
    // handler needs to know BEFORE it decides to move the caret.
    const willHandleDroppedFiles = (fileList: FileList | null | undefined) => {
      const upload = imageUploadRef.current
      const { images, others } = partitionUploadFiles(fileList)
      if (!editableRef.current || !upload) return false
      return (
        images.length > 0 || (others.length > 0 && Boolean(upload.onOtherFiles))
      )
    }

    // Shared paste/drop handler: inline-image types go through the markdown
    // embed pipeline, everything else to the Files-section flow (when wired).
    const handleUploadedFiles = (
      fileList: FileList | null | undefined,
      event: Event
    ) => {
      const upload = imageUploadRef.current
      const { images, others } = partitionUploadFiles(fileList)
      const hasImages = images.length > 0
      const hasOthers = others.length > 0 && Boolean(upload?.onOtherFiles)

      if (!editableRef.current || !upload || (!hasImages && !hasOthers)) {
        return false
      }

      event.preventDefault()
      if (hasImages) void upload.onFiles(images)
      if (hasOthers) void upload.onOtherFiles?.(others)
      return true
    }

    const editor = useEditor({
      extensions: [
        StarterKit.configure({
          heading: { levels: [1, 2, 3] },
          // EXP-421: the horizontal line marking where a dragged image (or a
          // dropped file) will land — themed, and thick enough to read
          // against a paragraph edge.
          dropcursor: { color: `var(--ring)`, width: 2 },
          // Replaced below by CodeBlockLowlight for syntax highlighting.
          codeBlock: false,
          // Replaced below by MarkdownParagraph so intentional blank lines
          // round-trip through GFM (EXP-7).
          paragraph: false,
          // EXP-726: TrailingNode appends its paragraph on every editor,
          // read-only ones included. MarkdownTable's own trailing-paragraph
          // plugin does the job for tables and skips read-only bodies, which
          // would otherwise render a blank row under a closing table.
          trailingNode: tableTrailingNodeOptions,
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
        // EXP-726 — declared AFTER StarterKit so the in-table Enter/Shift-Enter
        // keymap outranks splitBlock, and BEFORE the ref/mention/autocomplete
        // extensions because tiptap builds its plugins from the REVERSED
        // extension list: the autocomplete, declared last, keeps Tab/Enter for
        // its candidate menu.
        ...MarkdownTableExtensions,
        IssueRefExtension.configure({
          getResolved: (identifier) =>
            issueRefsRef.current?.resolve(identifier) ?? null,
          onOpen: (identifier) => issueRefsRef.current?.open(identifier),
        }),
        MentionPillExtension.configure({
          getResolved: (email) => mentionsRef.current?.resolve(email) ?? null,
        }),
        ArrowInputRules,
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
        // Captured at editor creation — tiptap-markdown builds its markdown-it
        // instance in onBeforeCreate, so later prop changes do not apply.
        Markdown.configure({
          html: false,
          transformPastedText: true,
          transformCopiedText: true,
          linkify: linkify ?? false,
          breaks: hardBreaks ?? false,
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
          "aria-label": ariaLabel ?? `Issue description`,
          "aria-readonly": String(!editable),
        },
        handlePaste: (_view, event) =>
          handleUploadedFiles(event.clipboardData?.files, event),
        // EXP-421: a dropped file belongs where it was dropped, not at
        // whatever the caret happened to be. The upload is async, so instead
        // of tracking a position we move the SELECTION to the drop point now
        // — ProseMirror maps it through every intervening transaction, and
        // the eventual `insertImage` lands there. (Accepted consequence: if
        // the user moves the caret mid-upload, the image follows the caret.)
        // A file-less drop is an internal node drag: leave it entirely to
        // ProseMirror, which moves the block itself.
        handleDrop: (view, event) => {
          if (willHandleDroppedFiles(event.dataTransfer?.files)) {
            const coords = view.posAtCoords({
              left: event.clientX,
              top: event.clientY,
            })
            if (coords) {
              view.dispatch(
                view.state.tr.setSelection(
                  TextSelection.near(view.state.doc.resolve(coords.pos))
                )
              )
              view.focus()
            }
          }
          return handleUploadedFiles(event.dataTransfer?.files, event)
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
        return editor ? getEditorMarkdown(editor) : null
      },
      insertImage: ({ alt, src }) => {
        if (!editor) return
        // A table cell holds ONE paragraph, so a block image inserted at a
        // caret inside one would split the table around it. Park the
        // selection after the table and let the image land below it.
        moveSelectionAfterTable(editor)
        // A drop onto (or just before) an existing image leaves a
        // NodeSelection over it — setImage would REPLACE that node. Insert
        // after it instead, so the dropped image lands beside the existing
        // one. Also covers the image-only doc, whose default selection is
        // already a NodeSelection.
        const { selection } = editor.state
        if (selection instanceof NodeSelection) {
          editor
            .chain()
            .focus()
            .insertContentAt(selection.to, {
              type: `image`,
              attrs: { alt, src },
            })
            .run()
          return
        }
        editor.chain().focus().setImage({ alt, src }).run()
      },
      appendImage: ({ alt, src }) => {
        if (!editor) return
        // `end` lands inside the last cell when the document ends in a table.
        editor.commands.focus(`end`)
        moveSelectionAfterTable(editor)
        editor.chain().focus().setImage({ alt, src }).run()
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

    // Keep the caret clear of a sticky overlay at the scrollport's top.
    // scrollThreshold decides WHETHER ProseMirror scrolls (its default 0
    // treats the overlaid region as visible), scrollMargin how far past it —
    // both need the band height on their top side. setProps merges partials,
    // so the drop/paste handlers stay intact.
    useEffect(() => {
      if (!editor) return
      const inset = topScrollInset ?? 0
      editor.view.setProps({
        scrollThreshold: { top: inset, right: 0, bottom: 0, left: 0 },
        scrollMargin: { top: inset + 8, right: 5, bottom: 5, left: 5 },
      })
    }, [editor, topScrollInset])

    // TipTap reads `content` only at editor creation. Read-only instances
    // (e.g. comment bodies fed live from sync) have no local edits to
    // protect, so re-apply the markdown prop when it diverges; editable
    // instances own their document and sync explicitly via the ref.
    // emitUpdate must stay off: setContent would otherwise re-enter onChange
    // with the editor's re-serialization of the prop, and markdown authored
    // on other clients need not round-trip byte-identically — a host tracking
    // dirty state against the raw synced text would see phantom local edits.
    useEffect(() => {
      if (!editor || editable) return
      if (getEditorMarkdown(editor) === markdown) return
      editor.commands.setContent(markdown, { emitUpdate: false })
    }, [editor, editable, markdown])

    // EXP-198: mobile keyboards overlay the layout viewport without resizing
    // it, so ProseMirror's own scrollIntoView cannot tell that the caret has
    // slipped behind the keyboard. While the editor is focused, chase every
    // caret move (and keyboard show/hide — visualViewport resize) with a
    // visual-viewport-aware reveal.
    useEffect(() => {
      if (!editor) return
      let frame = 0
      const reveal = () => {
        cancelAnimationFrame(frame)
        frame = requestAnimationFrame(() => {
          if (editor.isDestroyed) return
          if (!editor.isFocused) {
            releaseKeyboardClearance()
            return
          }
          revealCaretAboveKeyboard(editor.view)
        })
      }
      const release = () => {
        cancelAnimationFrame(frame)
        releaseKeyboardClearance()
      }
      const onFocus = () => {
        onFocusChangeRef.current?.(true)
        reveal()
      }
      const onBlurred = () => {
        onFocusChangeRef.current?.(false)
        release()
      }
      editor.on(`selectionUpdate`, reveal)
      editor.on(`focus`, onFocus)
      editor.on(`blur`, onBlurred)
      const vv = window.visualViewport
      vv?.addEventListener(`resize`, reveal)
      return () => {
        editor.off(`selectionUpdate`, reveal)
        editor.off(`focus`, onFocus)
        editor.off(`blur`, onBlurred)
        vv?.removeEventListener(`resize`, reveal)
        release()
      }
    }, [editor])

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
    // EXP-551: `:shortcode` candidates from the lazily loaded emoji dataset
    // (the chunk starts loading the first time a `:xx` token appears).
    const emojiData = useEmojiData(autocomplete?.kind === `emoji`)
    const emojiCandidates =
      autocomplete?.kind === `emoji` && emojiData
        ? searchEmoji(emojiData, autocomplete.query, 8)
        : []
    const candidateCount =
      autocomplete?.kind === `mention`
        ? mentionCandidates.length
        : autocomplete?.kind === `issueRef`
          ? issueCandidates.length
          : emojiCandidates.length

    // Replace the in-progress `@query`/`#query` token with the canonical
    // plain-text interchange form (`@<email>` / `#<IDENTIFIER>`). insertText
    // keeps it plain text — never a custom node — so the markdown round-trip
    // stays untouched.
    const insertToken = (token: string, trailingSpace = true) => {
      const range = autocomplete
      if (!range || !editor) return
      editor
        .chain()
        .focus()
        .command(({ tr }) => {
          tr.insertText(
            trailingSpace ? `${token} ` : token,
            range.from,
            range.to
          )
          return true
        })
        .run()
    }

    // An emoji pick inserts the unicode — never the shortcode.
    const insertEmoji = (
      emoji: (typeof emojiCandidates)[number],
      trailingSpace: boolean
    ) => {
      pushRecentEmoji(emoji.u)
      insertToken(emoji.u, trailingSpace)
    }

    const insertActive = (index: number) => {
      if (autocomplete?.kind === `mention` && mentionCandidates[index]) {
        insertToken(`@${mentionCandidates[index].email}`)
      } else if (autocomplete?.kind === `issueRef` && issueCandidates[index]) {
        insertToken(`#${issueCandidates[index].identifier}`)
      } else if (autocomplete?.kind === `emoji` && emojiCandidates[index]) {
        insertEmoji(emojiCandidates[index], true)
      }
    }

    // `:tada:` typed in full (closing colon) commits the exact shortcode
    // immediately, without the trailing space — the habitual GitHub/Slack
    // gesture must not leave literal shortcode text behind. Runs once the
    // dataset is loaded; the token stays reported until then.
    const autoCommitRef = useRef<string | null>(null)
    useEffect(() => {
      if (!autocomplete || autocomplete.kind !== `emoji`) {
        autoCommitRef.current = null
        return
      }
      if (!autocomplete.closed || !emojiData) return
      const key = `${autocomplete.from}:${autocomplete.query}`
      if (autoCommitRef.current === key) return
      const exact = findEmojiByShortcode(emojiData, autocomplete.query)
      if (!exact) return
      autoCommitRef.current = key
      insertEmoji(exact, false)
    }, [autocomplete, emojiData])

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
      if (
        (event.key === `Enter` || event.key === `Tab`) &&
        !event.metaKey &&
        !event.ctrlKey
      ) {
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
        // Fit within the VISUAL viewport — with the mobile keyboard open it
        // is shorter than window.innerHeight, and a menu sized against the
        // layout viewport would open underneath the keyboard (EXP-198).
        const vv = window.visualViewport
        const visibleTop = vv?.offsetTop ?? 0
        const visibleBottom = visibleTop + (vv?.height ?? window.innerHeight)
        const left = Math.max(
          viewportPad,
          Math.min(coords.left, window.innerWidth - menuWidth - viewportPad)
        )
        const spaceBelow = visibleBottom - coords.bottom - viewportPad
        const spaceAbove = coords.top - visibleTop - viewportPad
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
      <div
        ref={wrapperRef}
        className={cn(`tiptap-wrapper`, appearance === `chat` && `tiptap-chat`)}
      >
        {/* EXP-568: no always-on toolbar — the formatting rail floats over a
            selection on desktop and rides the keyboard on phones. */}
        {editable && editor ? (
          isMobile ? (
            <EditorMobileFormattingBar
              editor={editor}
              imageUpload={imageUpload}
            />
          ) : (
            <EditorSelectionRail editor={editor} imageUpload={imageUpload} />
          )
        ) : null}
        <EditorContent editor={editor} />
        {/* EXP-587: emoji / image / attach sit under the editor on desktop —
            the selection rail only shows over a selection, where inserting
            would replace it. */}
        {editable && editor && !isMobile ? (
          <>
            <EditorInsertBar editor={editor} imageUpload={imageUpload} />
            {/* EXP-726: table +/grip chrome is desktop-editing only — a
                read-only body gets cells and nothing else, a phone gets
                cells plus Delete table on its keyboard bar (EXP-727). */}
            <EditorTableControls editor={editor} wrapperRef={wrapperRef} />
          </>
        ) : null}
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
                className="pointer-events-auto fixed w-72 overflow-y-auto rounded-lg border border-glass-stroke-card bg-glass-card-opaque"
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
                {autocomplete.kind === `emoji` &&
                  emojiCandidates.map((emoji, i) => (
                    <EmojiCandidateRow
                      key={emoji.u}
                      emoji={emoji}
                      unicode={emoji.u}
                      query={autocomplete.query}
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
