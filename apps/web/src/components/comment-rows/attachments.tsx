import { useState } from "react"
import { Download, ExternalLink, X } from "lucide-react"
import type { Attachment } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { attachmentCollection } from "@/lib/collections"
import {
  buildAttachmentDownloadUrl,
  formatAttachmentSize,
  getAttachmentIcon,
  isInlineImageAttachment,
} from "@/lib/attachment-files"
import { Button } from "@/components/ui/button"
import { IconTooltip } from "@/components/icon-tooltip"
import { ImagePreviewDialog } from "@/components/image-preview-dialog"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"

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
  const [pendingDelete, setPendingDelete] = useState<Attachment | null>(null)
  const [deleting, setDeleting] = useState(false)

  if (attachments.length === 0) return null

  const images = attachments.filter((row) =>
    isInlineImageAttachment(row.contentType)
  )
  const files = attachments.filter(
    (row) => !isInlineImageAttachment(row.contentType)
  )

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

  const removeButton = (row: Attachment) => (
    <button
      type="button"
      aria-label={`Delete ${row.filename}`}
      onClick={() => setPendingDelete(row)}
      className="absolute -right-1.5 -top-1.5 hidden rounded-full border border-border bg-background p-0.5 text-muted-foreground hover:text-foreground group-hover/attachment:block"
    >
      <X className="size-3" />
    </button>
  )

  return (
    <div className="mt-2 flex flex-col items-start gap-2">
      {images.map((row) => (
        <div key={row.id} className="group/attachment relative max-w-full">
          <button
            type="button"
            aria-label={`View ${row.filename}`}
            onClick={() => setPreview(row)}
            className="block cursor-zoom-in"
          >
            <img
              src={row.url}
              alt={row.filename}
              loading="lazy"
              // The intrinsic size is what lets the browser reserve the box
              // before the bytes land; `aspect-ratio` keeps that reservation
              // correct once `max-h`/`max-w` clamp one side. Rows probed
              // before EXP-580 carry neither and simply reflow on decode.
              width={row.width ?? undefined}
              height={row.height ?? undefined}
              style={
                row.width && row.height
                  ? { aspectRatio: `${row.width} / ${row.height}` }
                  : undefined
              }
              className="block h-auto max-h-[480px] w-auto max-w-full rounded-lg border border-glass-stroke-card bg-glass-section object-contain"
            />
          </button>
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

      <ImagePreviewDialog
        open={preview !== null}
        onOpenChange={(open) => {
          if (!open) setPreview(null)
        }}
        src={preview?.url ?? ``}
        alt={preview?.filename}
        label={preview?.filename ?? `Attachment`}
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
