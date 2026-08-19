import { useEffect, useRef, useState } from "react"
import { ArrowUp, Paperclip, X } from "lucide-react"
import { toast } from "sonner"
import type { Attachment, User } from "@/db/schema"
import { MAX_COMMENT_ATTACHMENTS } from "@/lib/domain"
import {
  isAcceptedImageContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
} from "@/lib/storage/issue-attachments"
import {
  uploadIssueFile,
  uploadIssueImageFile,
} from "@/lib/storage/issue-image-upload"
import {
  formatAttachmentSize,
  getAttachmentIcon,
  isInlineImageAttachment,
} from "@/lib/attachment-files"
import { Button } from "@/components/ui/button"
import { MentionTextarea } from "@/components/mention-textarea"

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
}: CommentComposerProps) {
  const [text, setText] = useState(initialText)
  const [pending, setPending] = useState<PendingCommentAttachment[]>(() =>
    (initialAttachments ?? []).map((row) => ({ key: row.id, existing: row }))
  )
  const [submitting, setSubmitting] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)

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
        previewUrl: isAcceptedImageContentType(file.type)
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
          const uploaded = isInlineImageAttachment(item.file.type)
            ? await uploadIssueImageFile(issueId, item.file)
            : await uploadIssueFile(issueId, item.file)
          items[i] = { ...item, uploadedId: uploaded.id }
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
    }
  }

  return (
    <div
      className="rounded-lg border border-border bg-muted/40"
      onDrop={(event) => {
        if (event.dataTransfer.files.length === 0) return
        event.preventDefault()
        addFiles(Array.from(event.dataTransfer.files))
      }}
      onDragOver={(event) => {
        if (event.dataTransfer.types.includes(`Files`)) event.preventDefault()
      }}
    >
      {pending.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 px-2 pt-2">
          {pending.map((item) => {
            const contentType = item.existing?.contentType ?? item.file?.type ?? ``
            const filename = item.existing?.filename ?? item.file?.name ?? ``
            const imageSrc = item.existing
              ? isInlineImageAttachment(contentType)
                ? item.existing.url
                : undefined
              : item.previewUrl
            const removeButton = (
              <button
                type="button"
                aria-label={`Remove ${filename}`}
                disabled={submitting}
                onClick={() => removeItem(item.key)}
                className="absolute -right-1.5 -top-1.5 rounded-full border border-border bg-background p-0.5 text-muted-foreground hover:text-foreground"
              >
                <X className="size-3" />
              </button>
            )
            if (imageSrc) {
              return (
                <div key={item.key} className="relative">
                  <img
                    src={imageSrc}
                    alt={filename}
                    className="size-16 rounded-md border border-border/60 object-cover"
                  />
                  {removeButton}
                </div>
              )
            }
            const Icon = getAttachmentIcon(contentType)
            return (
              <div
                key={item.key}
                className="relative flex max-w-48 items-center gap-1.5 rounded-md border border-border/60 bg-background px-2 py-1.5"
              >
                <Icon className="size-4 shrink-0 text-muted-foreground" />
                <span className="min-w-0 truncate text-xs">{filename}</span>
                {removeButton}
              </div>
            )
          })}
        </div>
      )}
      <MentionTextarea
        autoFocus={autoFocus}
        placeholder={placeholder}
        value={text}
        onValueChange={setText}
        users={users}
        disabled={submitting}
        className="min-h-16 border-none bg-transparent text-sm shadow-none focus-visible:ring-0 dark:bg-transparent"
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
      <div className="flex items-center gap-1 px-1.5 pb-1.5">
        <input
          ref={fileInputRef}
          type="file"
          multiple
          className="hidden"
          onChange={(event) => {
            if (event.target.files) addFiles(Array.from(event.target.files))
            event.target.value = ``
          }}
        />
        <Button
          type="button"
          variant="ghost"
          size="icon"
          className="text-muted-foreground"
          aria-label="Attach files"
          title="Attach files"
          disabled={submitting}
          onClick={() => fileInputRef.current?.click()}
        >
          <Paperclip />
        </Button>
        <div className="ml-auto flex items-center gap-2">
          {onCancel && (
            <Button
              type="button"
              size="xs"
              variant="ghost"
              onClick={onCancel}
              disabled={submitting}
            >
              Cancel
            </Button>
          )}
          <Button
            type="button"
            size="icon"
            className="shrink-0"
            aria-label="Send comment"
            disabled={submitting || !canSubmit}
            onClick={() => void submit()}
          >
            <ArrowUp />
          </Button>
        </div>
      </div>
    </div>
  )
}
