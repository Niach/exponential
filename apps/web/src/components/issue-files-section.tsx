import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Download, ExternalLink, LoaderCircle, Trash2 } from "lucide-react"
import type { Attachment } from "@/db/schema"
import { attachmentCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { uploadIssueFile } from "@/lib/storage/issue-image-upload"
import {
  buildAttachmentDownloadUrl,
  formatAttachmentSize,
  getAttachmentIcon,
  isInlineImageAttachment,
} from "@/lib/attachment-files"
import { Button } from "@/components/ui/button"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import { IconTooltip } from "@/components/icon-tooltip"
import { IssueEditorAttachmentButton } from "@/components/issue-editor/attachment-button"
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

interface IssueFilesSectionProps {
  issueId: string
  readOnly?: boolean
  /**
   * Receives inline-image files picked via the attach button (EXP-316: they
   * are inlined into the description instead of living here). When absent,
   * image picks are rejected with a pointer to the editor's image button.
   */
  onImageFiles?: (files: File[]) => void | Promise<void>
}

/**
 * EXP-297 Files rail: the issue's NON-inline-image attachments, rendered
 * straight from the synced `attachments` shape (they never appear in the
 * description markdown). Members can attach any file type, open/download it,
 * or delete the row.
 */
export function IssueFilesSection({
  issueId,
  readOnly = false,
  onImageFiles,
}: IssueFilesSectionProps) {
  const { data } = useLiveQuery(
    (query) =>
      query
        .from({ attachments: attachmentCollection })
        .where(({ attachments }) => eq(attachments.issueId, issueId)),
    [issueId]
  )

  const files = useMemo(() => {
    const rows = (data ?? []) as Attachment[]
    return rows
      .filter((row) => !isInlineImageAttachment(row.contentType))
      // Comment attachments (EXP-554) render under their comment, not here.
      .filter((row) => row.commentId === null)
      .sort(
        (a, b) =>
          new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime()
      )
  }, [data])

  const [uploading, setUploading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [pendingDelete, setPendingDelete] = useState<Attachment | null>(null)
  const [deleting, setDeleting] = useState(false)

  const handleFiles = async (selected: File[]) => {
    setError(null)

    // Inline-image types never live in the Files section — every client
    // filters them out (EXP-297). With an onImageFiles handler they are
    // inlined into the description instead (EXP-316); without one they are
    // deflected so the sweep can't silently delete an unreferenced upload.
    const images = selected.filter((file) =>
      isInlineImageAttachment(file.type)
    )
    const uploadable = selected.filter(
      (file) => !isInlineImageAttachment(file.type)
    )
    const failures: string[] = onImageFiles
      ? []
      : images.map(
          (file) =>
            `${file.name}: images go in the description. Add them with the editor's image button.`
        )

    setUploading(true)
    if (onImageFiles && images.length > 0) {
      // The handler owns its own error surface (the description editor's
      // upload status) — failures there don't join this section's list.
      await onImageFiles(images)
    }
    // Every pick is attempted; failures are collected per file so one oversize
    // upload never silently drops the rest (parity with the native clients).
    for (const file of uploadable) {
      try {
        await uploadIssueFile(issueId, file)
      } catch (uploadError) {
        failures.push(
          uploadError instanceof Error
            ? `${file.name}: ${uploadError.message}`
            : `${file.name}: upload failed`
        )
      }
    }
    setUploading(false)

    if (failures.length > 0) {
      setError(
        failures.length === selected.length
          ? failures.join(` · `)
          : `${failures.length} of ${selected.length} files failed: ${failures.join(` · `)}`
      )
    }
  }

  const handleConfirmDelete = async () => {
    if (!pendingDelete) return
    setDeleting(true)
    setError(null)
    try {
      const { txId } = await trpc.attachments.delete.mutate({
        id: pendingDelete.id,
      })
      await attachmentCollection.utils.awaitTxId(txId)
      setPendingDelete(null)
    } catch (deleteError) {
      setError(
        deleteError instanceof Error
          ? deleteError.message
          : `Failed to delete file`
      )
    } finally {
      setDeleting(false)
    }
  }

  // Nothing to show — stay out of the way entirely. Attaching the first file
  // happens through the description toolbar's attach button (EXP-335), so an
  // empty section has no affordance to render anymore.
  if (files.length === 0) {
    return null
  }

  return (
    // EXP-698 r4: the same gutter the coding / PR cards use, so every card
    // down the reading column shares an edge.
    <div
      className="mx-auto w-full max-w-3xl px-4 pt-3 pb-2"
      data-testid="issue-files-section"
    >
      <GlassSectionHeader
        label="Files"
        trailing={
          !readOnly && (
            <IssueEditorAttachmentButton
              accept="*/*"
              label="Attach file"
              onFiles={handleFiles}
              uploading={uploading}
            />
          )
        }
      />

      {files.length > 0 && (
        <ul className="flex flex-col gap-1">
          {files.map((file) => {
            const Icon = getAttachmentIcon(file.contentType)

            return (
              <GlassRow
                key={file.id}
                asChild
                className="min-w-0 gap-2 px-2 py-1.5"
              >
                <li data-testid={`issue-file-row-${file.id}`}>
                  <Icon className="size-4 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate text-sm">
                    {file.filename}
                  </span>
                  <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
                    {formatAttachmentSize(file.sizeBytes)}
                  </span>
                  <IconTooltip label="Open">
                    <Button variant="glass" size="icon-sm" asChild>
                      <a
                        href={file.url}
                        target="_blank"
                        rel="noreferrer"
                        aria-label={`Open ${file.filename}`}
                      >
                        <ExternalLink />
                      </a>
                    </Button>
                  </IconTooltip>
                  <IconTooltip label="Download">
                    <Button variant="glass" size="icon-sm" asChild>
                      <a
                        href={buildAttachmentDownloadUrl(file.url)}
                        download={file.filename}
                        aria-label={`Download ${file.filename}`}
                      >
                        <Download />
                      </a>
                    </Button>
                  </IconTooltip>
                  {!readOnly && (
                    <IconTooltip label="Delete">
                      <Button
                        variant="glass"
                        size="icon-sm"
                        className="hover:text-destructive"
                        aria-label={`Delete ${file.filename}`}
                        onClick={() => setPendingDelete(file)}
                      >
                        <Trash2 />
                      </Button>
                    </IconTooltip>
                  )}
                </li>
              </GlassRow>
            )
          })}
        </ul>
      )}

      {uploading && (
        <p className="mt-1.5 flex items-center gap-1.5 text-xs text-muted-foreground">
          <LoaderCircle className="size-3 animate-spin" />
          Uploading...
        </p>
      )}
      {error && <p className="mt-1.5 text-xs text-destructive">{error}</p>}

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open && !deleting) setPendingDelete(null)
        }}
      >
        <AlertDialogContent data-testid="issue-file-delete-confirm">
          <AlertDialogHeader>
            <AlertDialogTitle>Delete this file?</AlertDialogTitle>
            <AlertDialogDescription>
              {pendingDelete?.filename} will be permanently removed for
              everyone. This can&apos;t be undone. If it is referenced anywhere
              in a description or comment, that reference is replaced with a
              plain &ldquo;deleted image&rdquo; note.
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
              {deleting ? `Deleting...` : `Delete file`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
