import { useEffect, useRef, useState } from "react"
import { toast } from "sonner"
import type { Attachment, User } from "@/db/schema"
import { MAX_COMMENT_ATTACHMENTS } from "@/lib/domain"
import {
  acceptedImageContentTypes,
  acceptedVideoUploadContentTypes,
  isAcceptedImageContentType,
  isVideoContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
} from "@/lib/storage/issue-attachments"
import {
  uploadIssueFile,
  uploadIssueImageFile,
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
import { Pill } from "@/components/ui/pill"
import {
  Composer,
  ComposerSubmit,
  ComposerTool,
} from "@/components/composer"
import {
  MentionTextarea,
  type MentionTextareaHandle,
} from "@/components/mention-textarea"
import { EmojiPickerPopover } from "@/components/emoji-picker"
import { conceptIcon } from "@/lib/icons.generated"
import { issueRefInsertionText } from "@/components/issue-editor/formatting-rail"

// Multi-client surface (the natives mirror this row) — concept icons, never
// raw lucide imports (EXP-317).
const EmojiIcon = conceptIcon(`editor-emoji`)
const ImageIcon = conceptIcon(`editor-image`)
const AttachIcon = conceptIcon(`ui-attach`)
const IssueRefIcon = conceptIcon(`editor-issue-ref`)
const CloseIcon = conceptIcon(`ui-close`)

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
  onSubmit: (body: string, attachmentIds: string[]) => Promise<void>
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
}: CommentComposerProps) {
  const [text, setText] = useState(initialText)
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
              ? await uploadIssueImageFile(issueId, item.file)
              : await uploadIssueFile(issueId, item.file)
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
      await onSubmit(text.trim(), ids)
      setText(``)
      for (const item of items) {
        if (item.previewUrl) URL.revokeObjectURL(item.previewUrl)
      }
      setPending([])
    } catch (error) {
      toast.error(`Couldn't post comment`, {
        description: error instanceof Error ? error.message : undefined,
      })
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
        const removeButton = (
          <button
            type="button"
            aria-label={`Remove ${filename}`}
            disabled={submitting}
            onClick={() => removeItem(item.key)}
            className="absolute -right-1.5 -top-1.5 rounded-full border border-glass-stroke-card bg-popover p-0.5 text-muted-foreground hover:text-foreground"
          >
            <CloseIcon className="size-3" />
          </button>
        )
        if (imageSrc) {
          return (
            <div key={item.key} className="relative">
              <img
                src={imageSrc}
                alt={filename}
                className="size-16 rounded-md border border-glass-stroke-card object-cover"
              />
              {removeButton}
            </div>
          )
        }
        if (videoSrc) {
          // EXP-824: a muted, metadata-only `<video>` is the cheapest
          // first-frame thumb — no decode until the row exists server-side.
          return (
            <div key={item.key} className="relative">
              <video
                src={videoSrc}
                muted
                playsInline
                preload="metadata"
                aria-label={filename}
                className="size-16 rounded-md border border-glass-stroke-card bg-black object-cover"
              />
              {removeButton}
            </div>
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
            {removeButton}
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
        disabled={submitting}
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
        disabled={submitting}
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
        disabled={submitting}
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
          disabled={submitting}
        >
          <EmojiIcon />
        </ComposerTool>
      </EmojiPickerPopover>
    </>
  )

  return (
    <Composer
      strip={strip}
      tools={tools}
      submit={
        <>
          {onCancel && (
            <Pill
              mode="action"
              size="sm"
              onClick={onCancel}
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
        placeholder={placeholder}
        value={text}
        onValueChange={setText}
        users={users}
        disabled={submitting}
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
