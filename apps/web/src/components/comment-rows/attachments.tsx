import { useState } from "react"
import { Download, ExternalLink, Eye } from "lucide-react"
import type { Attachment } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { attachmentCollection } from "@/lib/collections"
import {
  buildAttachmentDownloadUrl,
  formatAttachmentSize,
  getAttachmentIcon,
  isFileAttachment,
  isInlineImageAttachment,
  isInlineMediaAttachment,
  isMarkdownAttachment,
} from "@/lib/attachment-files"
import {
  buildAttachmentPosterUrl,
  isVideoContentType,
} from "@/lib/storage/issue-attachments"
import {
  AttachmentRemoveButton,
  AttachmentThumb,
  Button,
  IconTooltip,
  ImagePreviewDialog,
  type PreviewMediaKind,
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@exp/ui"
import { AttachmentMediaPlayer } from "@/components/attachment-media-player"
import { AttachmentMarkdownPreviewDialog } from "@/components/attachment-markdown-preview"

interface CommentAttachmentsProps {
  attachments: Attachment[]
  // Comment author: may delete individual attachments (attachments.delete —
  // permanent, so it confirms first).
  canModify: boolean
}

/**
 * EXP-554: a comment's linked attachments (attachments.comment_id), rendered
 * below the body — images as LARGE inline tiles opening the shared lightbox,
 * other files as chips with open/download. Never inlined into the markdown.
 *
 * EXP-723: the images used to be 64px squares, which turned every screenshot
 * into an icon you had to open to read. They are now full-width tiles stacked
 * vertically (Linear's activity feed), capped at 480px tall and reserving
 * their aspect ratio from the probed `width`/`height` so the feed doesn't jump
 * as they decode. The 64px thumb survives only in the pending/edit strips,
 * where the point IS the list, not the picture.
 */
export function CommentAttachments({
  attachments,
  canModify,
}: CommentAttachmentsProps) {
  const [preview, setPreview] = useState<Attachment | null>(null)
  // EXP-955: a `.md` chip previews through the markdown renderer.
  const [markdownPreview, setMarkdownPreview] = useState<Attachment | null>(
    null
  )
  const [pendingDelete, setPendingDelete] = useState<Attachment | null>(null)
  const [deleting, setDeleting] = useState(false)

  if (attachments.length === 0) return null

  const images = attachments.filter((row) =>
    isInlineImageAttachment(row.contentType)
  )
  // EXP-824: video/audio rows are inline players, never file chips.
  const media = attachments.filter((row) =>
    isInlineMediaAttachment(row.contentType)
  )
  const files = attachments.filter((row) => isFileAttachment(row.contentType))
  const previewKind: PreviewMediaKind = preview
    ? isVideoContentType(preview.contentType)
      ? `video`
      : isInlineMediaAttachment(preview.contentType)
        ? `audio`
        : `image`
    : `image`

  const handleConfirmDelete = async () => {
    if (!pendingDelete) return
    setDeleting(true)
    try {
      const { txId } = await trpc.attachments.delete.mutate({
        id: pendingDelete.id,
      })
      await attachmentCollection.utils.awaitTxId(txId)
      setPendingDelete(null)
    } finally {
      setDeleting(false)
    }
  }

  // The same corner badge the thumbs hang, for the rows that are NOT an
  // `AttachmentThumb` (the media players and the file chips).
  const removeButton = (row: Attachment) => (
    <AttachmentRemoveButton
      label={`Delete ${row.filename}`}
      onClick={() => setPendingDelete(row)}
      className="hidden group-hover/attachment:block"
    />
  )

  return (
    <div className="mt-2 flex flex-col items-start gap-2">
      {images.map((row) => (
        <AttachmentThumb
          key={row.id}
          size="inline"
          src={row.url}
          alt={row.filename}
          width={row.width}
          height={row.height}
          openLabel={`View ${row.filename}`}
          onOpen={() => setPreview(row)}
          removeLabel={canModify ? `Delete ${row.filename}` : undefined}
          onRemove={canModify ? () => setPendingDelete(row) : undefined}
          removeClassName="hidden group-hover/attachment:block"
          className="group/attachment"
        />
      ))}
      {media.map((row) => (
        <div
          key={row.id}
          className="group/attachment relative w-full max-w-[640px]"
        >
          <AttachmentMediaPlayer
            attachment={row}
            onExpand={
              isVideoContentType(row.contentType)
                ? () => setPreview(row)
                : undefined
            }
          />
          {canModify && removeButton(row)}
        </div>
      ))}
      {/* Chips keep wrapping in a row of their own — only the images stack. */}
      {files.length > 0 && (
      <div className="flex flex-wrap items-center gap-2">
      {files.map((row) => {
        const Icon = getAttachmentIcon(row.contentType)
        return (
          <div
            key={row.id}
            className="group/attachment relative flex max-w-60 items-center gap-1.5 rounded-md border border-glass-stroke-card bg-glass-section px-2 py-1.5"
          >
            <Icon className="size-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0 truncate text-xs">{row.filename}</span>
            <span className="shrink-0 text-[10px] text-muted-foreground tabular-nums">
              {formatAttachmentSize(row.sizeBytes)}
            </span>
            {isMarkdownAttachment(row.contentType, row.filename) ? (
              <IconTooltip label="Preview">
                <Button
                  variant="ghost"
                  size="icon-xs"
                  className="text-muted-foreground"
                  aria-label={`Preview ${row.filename}`}
                  onClick={() => setMarkdownPreview(row)}
                >
                  <Eye />
                </Button>
              </IconTooltip>
            ) : (
              <IconTooltip label="Open">
                <Button
                  variant="ghost"
                  size="icon-xs"
                  className="text-muted-foreground"
                  asChild
                >
                  <a
                    href={row.url}
                    target="_blank"
                    rel="noreferrer"
                    aria-label={`Open ${row.filename}`}
                  >
                    <ExternalLink />
                  </a>
                </Button>
              </IconTooltip>
            )}
            <IconTooltip label="Download">
              <Button
                variant="ghost"
                size="icon-xs"
                className="text-muted-foreground"
                asChild
              >
                <a
                  href={buildAttachmentDownloadUrl(row.url)}
                  download={row.filename}
                  aria-label={`Download ${row.filename}`}
                >
                  <Download />
                </a>
              </Button>
            </IconTooltip>
            {canModify && removeButton(row)}
          </div>
        )
      })}
      </div>
      )}

      <AttachmentMarkdownPreviewDialog
        attachment={markdownPreview}
        open={markdownPreview !== null}
        onOpenChange={(open) => {
          if (!open) setMarkdownPreview(null)
        }}
      />

      <ImagePreviewDialog
        open={preview !== null}
        onOpenChange={(open) => {
          if (!open) setPreview(null)
        }}
        src={preview?.url ?? ``}
        alt={preview?.filename}
        label={preview?.filename ?? `Attachment`}
        kind={previewKind}
        poster={
          preview?.posterStorageKey
            ? buildAttachmentPosterUrl(preview.id)
            : undefined
        }
      />

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open && !deleting) setPendingDelete(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete this attachment?</AlertDialogTitle>
            <AlertDialogDescription>
              {pendingDelete?.filename} will be permanently removed for
              everyone. This can&apos;t be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleting}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-white hover:bg-destructive/90"
              disabled={deleting}
              onClick={(event) => {
                event.preventDefault()
                void handleConfirmDelete()
              }}
            >
              {deleting ? `Deleting...` : `Delete`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
