import { useEffect, useRef, useState } from "react"
import type { Attachment, User } from "@/db/schema"
import type { CommentAudience } from "@exp/db-schema/domain"
import { MAX_COMMENT_ATTACHMENTS } from "@/lib/domain"
import { composerAfterSend } from "@exp/domain-contract/composer-send"
import {
  acceptedImageContentTypes,
  acceptedVideoUploadContentTypes,
  isAcceptedImageContentType,
  isVideoContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
} from "@/lib/storage/issue-attachments"
import {
  uploadIssueAttachment,
} from "@/lib/storage/issue-image-upload"
import {
  mediaPlayabilityHint,
  prepareMediaUpload,
  uploadIssueMediaFile,
} from "@/lib/storage/media-upload"
import {
  formatAttachmentSize,
  getAttachmentIcon,
  isInlineImageAttachment,
  isInlineMediaAttachment,
} from "@/lib/attachment-files"
import {
  AttachmentRemoveButton,
  AttachmentThumb,
  Pill,
  conceptIcon,
  Composer,
  ComposerSubmit,
  ComposerTool,
  toast,
} from "@exp/ui"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"
import { TRPCClientError } from "@trpc/client"
import { failedAction } from "@/lib/action-error"
import { EmojiPickerPopover } from "@/components/emoji-picker"
import { issueRefInsertionText } from "@/components/issue-editor/formatting-rail"
import { useComposerDraft } from "@/hooks/use-composer-draft"
import type { ComposerDraftTarget } from "@exp/domain-contract/composer-draft"
import {
  REPORTER_REPLY_COPY,
  reporterReplyPlaceholder,
} from "@/components/reporter-reply-copy"

// Multi-client surface (the natives mirror this row) — concept icons, never
// raw lucide imports (EXP-317).
const EmojiIcon = conceptIcon(`editor-emoji`)
const ImageIcon = conceptIcon(`editor-image`)
const AttachIcon = conceptIcon(`ui-attach`)
const IssueRefIcon = conceptIcon(`editor-issue-ref`)
const ReporterReplyIcon = conceptIcon(`notification-reporter-reply`)

/** A file picked into the composer, or (in edit mode) an already-linked row.
 *  `uploadedId` survives a failed send so a retry never re-uploads; `existing`
 *  rows are kept by id and never re-uploaded at all. */
type PendingCommentAttachment = {
  key: string
  file?: File
  // Object URL for image files — revoked on remove/unmount.
  previewUrl?: string
  uploadedId?: string
  existing?: Attachment
}

interface CommentComposerProps {
  issueId: string
  users: User[]
  /** `audience` is `reporter` while the "Reply to reporter" toggle is ON
   *  (SLOP-4), `team` otherwise; edit and reply composers always send
   *  `team`. */
  onSubmit: (
    body: string,
    attachmentIds: string[],
    audience: CommentAudience
  ) => Promise<void>
  initialText?: string
  // Edit mode: the comment's currently linked attachments. Removing one and
  // saving DELETES it server-side (comments.update reconciles to the sent
  // list), so removals are permanent once saved.
  initialAttachments?: Attachment[]
  onCancel?: () => void
  placeholder?: string
  autoFocus?: boolean
  /**
   * Focus left the composer card while it held nothing worth keeping — no
   * text, no pending attachments, no file chooser in flight. The phone bottom
   * bar (EXP-568) collapses back to its "+ Comment" pill on this.
   */
  onEmptyBlur?: () => void
  /**
   * #FEED-219 (composer-draft.json): the synced DRAFT the typed text lives in
   * (`comment:<issueId>`, `reply:<commentId>`), kept across a tab switch, a
   * reload and the person's devices; cleared by a send, deleted by Cancel.
   * Absent (edit mode) = nothing kept.
   */
  draft?: { target: ComposerDraftTarget; teamId: string }
  /**
   * SLOP-4 (fixture `reporter-reply.json`): the widget submission's reporter,
   * when the issue has one WITH an email — offers the "Reply to reporter"
   * pill in the leading row. OFF by default; ON swaps the placeholder and
   * sends `audience: 'reporter'`. Never passed to reply/edit composers.
   */
  reporter?: { name: string } | null
}

/**
 * EXP-554 comment composer, shared by create and edit: one rounded card with
 * the pending-attachment strip, the mention textarea, and an action row —
 * attach button left, primary send button inside the box (the style the
 * mobile composers share). Attachments upload on send and link via
 * `comments.create/update` `attachmentIds`; they are NEVER inlined into the
 * markdown body.
 */
export function CommentComposer({
  issueId,
  users,
  onSubmit,
  initialText = ``,
  initialAttachments,
  onCancel,
  placeholder = `Leave a reply…`,
  autoFocus = false,
  onEmptyBlur,
  draft,
  reporter = null,
}: CommentComposerProps) {
  const [toReporter, setToReporter] = useState(false)
  const audience: CommentAudience =
    reporter && toReporter ? `reporter` : `team`
  // EXP-1543: `textRef` = the live field, read when a send settles.
  const field = useComposerDraft(draft?.target ?? null, draft?.teamId)
  const [plainText, setPlainText] = useState(initialText)
  const plainRef = useRef(plainText)
  const text = draft ? field.text : plainText
  const textRef = draft ? field.textRef : plainRef
  const setText = (next: string, options?: { now?: boolean }) => {
    if (draft) {
      field.setText(next, options)
      return
    }
    plainRef.current = next
    setPlainText(next)
  }
  const [pending, setPending] = useState<PendingCommentAttachment[]>(() =>
    (initialAttachments ?? []).map((row) => ({ key: row.id, existing: row }))
  )
  const [submitting, setSubmitting] = useState(false)
  // EXP-824: narrated progress of a clip upload while sending.
  const [uploadStatus, setUploadStatus] = useState<string | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const imageInputRef = useRef<HTMLInputElement>(null)
  const textareaRef = useRef<MentionTextareaHandle>(null)
  // A file chooser steals focus without moving it anywhere in the document,
  // so the focusout below would read as "user left the composer". Latched on
  // the click that opens one, released when it resolves either way.
  const filePickerOpenRef = useRef(false)

  useEffect(() => {
    const release = () => {
      filePickerOpenRef.current = false
    }
    window.addEventListener(`focus`, release)
    return () => window.removeEventListener(`focus`, release)
  }, [])

  const pendingRef = useRef(pending)
  pendingRef.current = pending
  useEffect(
    () => () => {
      for (const item of pendingRef.current) {
        if (item.previewUrl) URL.revokeObjectURL(item.previewUrl)
      }
    },
    []
  )

  const addFiles = (files: File[]) => {
    const accepted: File[] = []
    for (const file of files) {
      const limit = isAcceptedImageContentType(file.type)
        ? maxImageUploadBytes
        : maxFileUploadBytes
      if (file.size > limit) {
        toast.error(
          `${file.name} is too large (max ${formatAttachmentSize(limit)})`
        )
        continue
      }
      accepted.push(file)
    }
    const room = Math.max(0, MAX_COMMENT_ATTACHMENTS - pending.length)
    const taking = accepted.slice(0, room)
    if (taking.length < accepted.length) {
      toast.error(`Up to ${MAX_COMMENT_ATTACHMENTS} attachments per comment`)
    }
    if (taking.length === 0) return
    setPending((prev) => [
      ...prev,
      ...taking.map((file) => ({
        key: `${file.name}-${crypto.randomUUID()}`,
        file,
        // Images and videos get an object URL for the strip thumb (a muted
        // `<video>` shows its first frame); audio keeps the file chip.
        previewUrl:
          isAcceptedImageContentType(file.type) || isVideoContentType(file.type)
            ? URL.createObjectURL(file)
            : undefined,
      })),
    ])
  }

  const removeItem = (key: string) => {
    setPending((prev) => {
      const item = prev.find((entry) => entry.key === key)
      if (item?.previewUrl) URL.revokeObjectURL(item.previewUrl)
      return prev.filter((entry) => entry.key !== key)
    })
  }

  const canSubmit = text.trim().length > 0 || pending.length > 0

  const submit = async () => {
    if (submitting || !canSubmit) return
    // EXP-1543 (composer-send.json): the field stays editable while the
    // comment is in flight; a success clears only what went out.
    const sentText = text
    setSubmitting(true)
    try {
      // Upload sequentially, persisting each id as it lands — a mid-batch
      // failure keeps the composer intact and a retry only uploads the rest.
      const items = [...pending]
      const ids: string[] = []
      for (let i = 0; i < items.length; i++) {
        const item = items[i]
        if (item.existing) {
          ids.push(item.existing.id)
          continue
        }
        if (!item.uploadedId && item.file) {
          let uploadedId: string
          if (isInlineMediaAttachment(item.file.type)) {
            // EXP-824: probe + poster + `.mov` remux, then a progress-
            // narrated upload; a non-H.264 clip gets the playability hint.
            const file = item.file
            setUploadStatus(`Preparing ${file.name}…`)
            const prepared = await prepareMediaUpload(file, (stage) =>
              setUploadStatus(
                stage === `remuxing`
                  ? `Converting ${file.name} to MP4…`
                  : `Preparing ${file.name}…`
              )
            )
            setUploadStatus(`Uploading ${prepared.file.name}…`)
            const uploaded = await uploadIssueMediaFile(issueId, prepared, {
              onProgress: (percent) =>
                setUploadStatus(`Uploading ${prepared.file.name}… ${percent}%`),
            })
            setUploadStatus(null)
            const hint = mediaPlayabilityHint(uploaded)
            if (hint) toast.message(hint)
            uploadedId = uploaded.id
          } else {
            const uploaded = isInlineImageAttachment(item.file.type)
              ? await uploadIssueAttachment(issueId, item.file)
              : await uploadIssueAttachment(issueId, item.file, { asFile: true })
            uploadedId = uploaded.id
          }
          items[i] = { ...item, uploadedId }
          const next = items[i]
          setPending((prev) =>
            prev.map((entry) => (entry.key === next.key ? next : entry))
          )
        }
        if (items[i].uploadedId) ids.push(items[i].uploadedId!)
      }
      await onSubmit(sentText.trim(), ids, audience)
      const after = composerAfterSend({
        outcome: `sent`,
        sent: { text: sentText, attachments: items.map((item) => item.key) },
        current: {
          text: textRef.current,
          attachments: pendingRef.current.map((item) => item.key),
        },
      })
      setText(after.text, { now: true })
      // SLOP-4: every send drops back to a team comment, so an internal
      // follow-up never mails the reporter (iOS and Android do the same).
      setToReporter(false)
      const keep = new Set(after.attachments)
      for (const item of items) {
        if (item.previewUrl && !keep.has(item.key)) URL.revokeObjectURL(item.previewUrl)
      }
      setPending((prev) => prev.filter((item) => keep.has(item.key)))
    } catch (error) {
      // FEED-79: the create mutation's refusal is already the tRPC link's
      // toast; an upload or media step that failed first gets this one. A
      // send could double-post, so no Retry.
      if (!(error instanceof TRPCClientError)) {
        failedAction(`post`, `the comment`, error)
      }
    } finally {
      setSubmitting(false)
      setUploadStatus(null)
    }
  }

  const strip = (pending.length > 0 || uploadStatus) && (
    <div className="flex flex-wrap items-center gap-2 px-2 pt-2">
      {uploadStatus ? (
        <span
          className="w-full truncate text-xs text-muted-foreground"
          role="status"
          aria-live="polite"
        >
          {uploadStatus}
        </span>
      ) : null}
      {pending.map((item) => {
        const contentType = item.existing?.contentType ?? item.file?.type ?? ``
        const filename = item.existing?.filename ?? item.file?.name ?? ``
        const isVideo = isVideoContentType(contentType)
        const imageSrc = item.existing
          ? isInlineImageAttachment(contentType)
            ? item.existing.url
            : undefined
          : isVideo
            ? undefined
            : item.previewUrl
        const videoSrc = isVideo
          ? (item.existing?.url ?? item.previewUrl)
          : undefined
        const remove = () => removeItem(item.key)
        const mediaSrc = imageSrc ?? videoSrc
        if (mediaSrc) {
          return (
            <AttachmentThumb
              key={item.key}
              src={mediaSrc}
              alt={filename}
              kind={imageSrc ? `image` : `video`}
              removeLabel={`Remove ${filename}`}
              onRemove={remove}
              disabled={submitting}
            />
          )
        }
        const Icon = getAttachmentIcon(contentType)
        return (
          <div
            key={item.key}
            className="relative flex max-w-48 items-center gap-1.5 rounded-md border border-glass-stroke-card bg-glass-row px-2 py-1.5"
          >
            <Icon className="size-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0 truncate text-xs">{filename}</span>
            <AttachmentRemoveButton
              label={`Remove ${filename}`}
              disabled={submitting}
              onClick={remove}
            />
          </div>
        )
      })}
    </div>
  )

  const tools = (
    <>
      <input
        ref={imageInputRef}
        type="file"
        // EXP-824: clips ride the image button — they render inline too.
        accept={[
          ...acceptedImageContentTypes,
          ...acceptedVideoUploadContentTypes,
          `audio/*`,
        ].join(`,`)}
        multiple
        className="hidden"
        onChange={(event) => {
          filePickerOpenRef.current = false
          if (event.target.files) addFiles(Array.from(event.target.files))
          event.target.value = ``
        }}
      />
      <input
        ref={fileInputRef}
        type="file"
        multiple
        className="hidden"
        onChange={(event) => {
          filePickerOpenRef.current = false
          if (event.target.files) addFiles(Array.from(event.target.files))
          event.target.value = ``
        }}
      />
      <ComposerTool
        aria-label="Add image"
        title="Add image"
        onClick={() => {
          filePickerOpenRef.current = true
          imageInputRef.current?.click()
        }}
      >
        <ImageIcon />
      </ComposerTool>
      <ComposerTool
        aria-label="Attach files"
        title="Attach files"
        onClick={() => {
          filePickerOpenRef.current = true
          fileInputRef.current?.click()
        }}
      >
        <AttachIcon />
      </ComposerTool>
      {/* The `#` picker only opens at a TOKEN start, so a `#` typed right
          after a word needs a leading space (shared helper, EXP-568). */}
      <ComposerTool
        aria-label="Insert issue reference"
        title="Insert issue reference"
        onClick={() =>
          textareaRef.current?.insertText(
            issueRefInsertionText(textareaRef.current.charBeforeCaret())
          )
        }
      >
        <IssueRefIcon />
      </ComposerTool>
      {/* EXP-551: the picker inserts at the textarea's caret; the popover
          keeps focus off its trigger on close and insertText re-focuses. */}
      <EmojiPickerPopover
        side="top"
        onPick={(unicode) => textareaRef.current?.insertText(unicode)}
      >
        <ComposerTool
          aria-label="Insert emoji"
          title="Insert emoji"
        >
          <EmojiIcon />
        </ComposerTool>
      </EmojiPickerPopover>
    </>
  )

  return (
    <Composer
      // The leading row is the slot the helpdesk's Reply/Note pills used —
      // one select pill now, in the same chrome.
      leading={
        reporter ? (
          <Pill
            size="sm"
            mode="select"
            selected={toReporter}
            leading={<ReporterReplyIcon className="size-3" />}
            disabled={submitting}
            onClick={() => setToReporter((value) => !value)}
          >
            {REPORTER_REPLY_COPY.toggleLabel}
          </Pill>
        ) : undefined
      }
      strip={strip}
      tools={tools}
      submit={
        <>
          {onCancel && (
            <Pill
              mode="action"
              size="sm"
              onClick={() => {
                setText(``, { now: true })
                onCancel()
              }}
              disabled={submitting}
            >
              Cancel
            </Pill>
          )}
          <ComposerSubmit
            aria-label="Send comment"
            disabled={submitting || !canSubmit}
            onClick={() => void submit()}
          />
        </>
      }
      onBlur={(event) => {
        if (!onEmptyBlur) return
        // focusout bubbles; ignore focus moves WITHIN the card.
        const next = event.relatedTarget
        if (next instanceof Node && event.currentTarget.contains(next)) return
        if (filePickerOpenRef.current) return
        if (text.trim().length > 0 || pending.length > 0) return
        onEmptyBlur()
      }}
      onDrop={(event) => {
        if (event.dataTransfer.files.length === 0) return
        event.preventDefault()
        addFiles(Array.from(event.dataTransfer.files))
      }}
      onDragOver={(event) => {
        if (event.dataTransfer.types.includes(`Files`)) event.preventDefault()
      }}
    >
      <MentionTextarea
        ref={textareaRef}
        autoFocus={autoFocus}
        placeholder={
          reporter && toReporter
            ? reporterReplyPlaceholder(reporter.name)
            : placeholder
        }
        value={text}
        onValueChange={(next) => setText(next)}
        users={users}
        // The card IS the field chrome — the textarea drops its own fill,
        // hairline and focus stroke (`resize-none` is stock since EXP-698).
        className="min-h-16 border-none bg-transparent text-sm shadow-none focus-visible:border-transparent dark:bg-transparent"
        onKeyDown={(event) => {
          if (
            event.key === `Enter` &&
            (event.metaKey || event.ctrlKey) &&
            canSubmit
          ) {
            event.preventDefault()
            void submit()
          }
        }}
        onPaste={(event) => {
          if (event.clipboardData.files.length === 0) return
          event.preventDefault()
          addFiles(Array.from(event.clipboardData.files))
        }}
      />
    </Composer>
  )
}
