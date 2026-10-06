import { useMemo, useState } from "react"
import { deleteFilePrompt, promptActions } from "@/lib/prompts"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Download, ExternalLink, Eye, LoaderCircle, Trash2 } from "lucide-react"
import type { Attachment } from "@/db/schema"
import { attachmentCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { uploadIssueFile } from "@/lib/storage/issue-image-upload"
import {
  buildAttachmentDownloadUrl,
  formatAttachmentSize,
  getAttachmentIcon,
  isFileAttachment,
  isMarkdownAttachment,
} from "@/lib/attachment-files"
import {
  Button,
  GlassRow,
  GlassSectionHeader,
  IconTooltip,
  Prompt,
} from "@exp/ui"
import { IssueEditorAttachmentButton } from "@/components/issue-editor/attachment-button"
import { AttachmentMarkdownPreviewDialog } from "@/components/attachment-markdown-preview"

interface IssueFilesSectionProps {
  issueId: string
  readOnly?: boolean
  /**
   * Receives INLINE files picked via the attach button — images (EXP-316)
   * and video/audio (EXP-824) are embedded in the description instead of
   * living here. When absent, such picks are rejected with a pointer to the
   * editor's image button.
   */
  onInlineFiles?: (files: File[]) => void | Promise<void>
}

/**
 * EXP-297 Files rail: the issue's attachments that are neither inline images
 * nor inline media (EXP-824), rendered straight from the synced `attachments`
 * shape (they never appear in the description markdown). Members can attach
 * any file type, open/download it, or delete the row.
 */
export function IssueFilesSection({
  issueId,
  readOnly = false,
  onInlineFiles,
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
      .filter((row) => isFileAttachment(row.contentType))
      // Comment attachments (EXP-554) render under their comment, not here.
      .filter((row) => row.commentId === null)
      .sort(
        (a, b) =>
          new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime()
      )
  }, [data])

  const [uploading, setUploading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleFiles = async (selected: File[]) => {
    setError(null)

    // Inline types (images, video, audio) never live in the Files section —
    // every client filters them out (EXP-297/EXP-824). With an onInlineFiles
    // handler they are embedded in the description instead (EXP-316); without
    // one they are deflected so the sweep can't silently delete an
    // unreferenced upload.
    const inline = selected.filter((file) => !isFileAttachment(file.type))
    const uploadable = selected.filter((file) => isFileAttachment(file.type))
    const failures: string[] = onInlineFiles
      ? []
      : inline.map(
          (file) =>
            `${file.name}: images and clips go in the description. Add them with the editor's image button.`
        )

    setUploading(true)
    if (onInlineFiles && inline.length > 0) {
      // The handler owns its own error surface (the description editor's
      // upload status) — failures there don't join this section's list.
      await onInlineFiles(inline)
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

  const handleDelete = async (file: FilesSectionFile) => {
    const { txId } = await trpc.attachments.delete.mutate({ id: file.id })
    await attachmentCollection.utils.awaitTxId(txId)
  }

  return (
    <FilesSectionView
      files={files}
      readOnly={readOnly}
      onAttach={handleFiles}
      onDelete={handleDelete}
      uploading={uploading}
      error={error}
    />
  )
}

/** One row of the Files section — every attachment list has these fields. */
export interface FilesSectionFile {
  id: string
  filename: string
  contentType: string
  sizeBytes: number
  url: string
}

/**
 * EXP-1170: the Files section's PRESENTATION — the issue detail
 * (`IssueFilesSection`, the synced shape) and the New issue page (a draft's
 * server-only attachments) render the same rows, the same attach button and
 * the same delete confirm. `onDelete` rejects with the message to show.
 */
export function FilesSectionView({
  files,
  readOnly = false,
  onAttach,
  onDelete,
  uploading = false,
  error = null,
}: {
  files: readonly FilesSectionFile[]
  readOnly?: boolean
  onAttach: (files: File[]) => void | Promise<void>
  onDelete: (file: FilesSectionFile) => Promise<void>
  uploading?: boolean
  error?: string | null
}) {
  const [pendingDelete, setPendingDelete] = useState<FilesSectionFile | null>(
    null
  )
  const [deleting, setDeleting] = useState(false)
  const [deleteError, setDeleteError] = useState<string | null>(null)
  // EXP-955: the `.md` row being previewed in the markdown dialog.
  const [previewFile, setPreviewFile] = useState<FilesSectionFile | null>(null)

  const handleConfirmDelete = async () => {
    if (!pendingDelete) return
    setDeleting(true)
    setDeleteError(null)
    try {
      await onDelete(pendingDelete)
      setPendingDelete(null)
    } catch (deleteFailure) {
      setDeleteError(
        deleteFailure instanceof Error
          ? deleteFailure.message
          : `Failed to delete file`
      )
    } finally {
      setDeleting(false)
    }
  }

  const shownError = deleteError ?? error

  // Nothing to show — stay out of the way entirely. Attaching the first file
  // happens through the description toolbar's attach button (EXP-335), so an
  // empty section has no affordance to render anymore.
  if (files.length === 0) {
    return null
  }

  const deleteCopy = deleteFilePrompt(pendingDelete?.filename ?? `this file`)

  return (
    // EXP-698 r4: the same gutter the coding / PR cards use, so every card
    // down the reading column shares an edge.
    <div
      className="mx-auto w-full max-w-4xl px-4 pt-3 pb-2 md:px-5"
      data-testid="issue-files-section"
    >
      <GlassSectionHeader
        label="Files"
        trailing={
          !readOnly && (
            <IssueEditorAttachmentButton
              accept="*/*"
              label="Attach file"
              onFiles={(picked) => void onAttach(picked)}
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
                  {/* EXP-955: a markdown file previews in the app — the
                      byte route would only download it. Everything else
                      opens in a tab (the route renders what a browser can). */}
                  {isMarkdownAttachment(file.contentType, file.filename) ? (
                    <IconTooltip label="Preview">
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={`Preview ${file.filename}`}
                        onClick={() => setPreviewFile(file)}
                      >
                        <Eye />
                      </Button>
                    </IconTooltip>
                  ) : (
                    <IconTooltip label="Open">
                      <Button variant="ghost" size="icon-sm" asChild>
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
                  )}
                  <IconTooltip label="Download">
                    <Button variant="ghost" size="icon-sm" asChild>
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
                        variant="ghost"
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
      {shownError && (
        <p className="mt-1.5 text-xs text-destructive">{shownError}</p>
      )}

      <AttachmentMarkdownPreviewDialog
        attachment={previewFile}
        open={previewFile !== null}
        onOpenChange={(open) => {
          if (!open) setPreviewFile(null)
        }}
      />

      <Prompt
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open) setPendingDelete(null)
        }}
        busy={deleting}
        data-testid="issue-file-delete-confirm"
        title={deleteCopy.title}
        body={deleteCopy.body}
        actions={promptActions(deleteCopy, {
          delete: { busy: deleting, onSelect: handleConfirmDelete },
        })}
      />
    </div>
  )
}
