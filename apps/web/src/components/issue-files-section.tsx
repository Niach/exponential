import { useMemo, useState } from "react"
import { deleteFilePrompt, promptActions } from "@/lib/prompts"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { Download, ExternalLink, Eye, Trash2 } from "lucide-react"
import type { Attachment } from "@/db/schema"
import { attachmentCollection, commentCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { uploadIssueAttachment } from "@/lib/storage/issue-image-upload"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import {
  buildAttachmentDownloadUrl,
  formatAttachmentSize,
  getAttachmentIcon,
  isMarkdownAttachment,
  issueFileRows,
} from "@/lib/attachment-files"
import {
  Button,
  GlassRow,
  GlassSectionHeader,
  IconTooltip,
  Prompt,
  Spinner,
} from "@exp/ui"
import { IssueEditorAttachmentButton } from "@/components/issue-editor/attachment-button"
import { AttachmentMarkdownPreviewDialog } from "@/components/attachment-markdown-preview"

interface IssueFilesSectionProps {
  issueId: string
  /** The saved description: an inline row it links is not a Files row. */
  description: string | null
  readOnly?: boolean
}

/**
 * EXP-297 Files rail: the issue's attachments that are neither inline images
 * nor inline media (EXP-824), plus every row uploaded through a FILE path
 * (EXP-1247 `asFile`, e.g. a png picked with the paperclip), plus inline
 * rows no description or comment links (NEW-83, fixtures/issue-files.json),
 * rendered straight from the synced `attachments` shape. Members can attach any file
 * type, open/download it, or delete the row.
 */
export function IssueFilesSection({
  issueId,
  description,
  readOnly = false,
}: IssueFilesSectionProps) {
  const { data } = useLiveQuery(
    (query) =>
      query
        .from({ attachments: attachmentCollection })
        .where(({ attachments }) => eq(attachments.issueId, issueId)),
    [issueId]
  )

  const { data: commentRows } = useLiveQuery(
    (query) =>
      query
        .from({ comments: commentCollection })
        .where(({ comments }) => eq(comments.issueId, issueId))
        .select(({ comments }) => ({ body: comments.body })),
    [issueId]
  )

  const files = useMemo(() => {
    const rows = [...((data ?? []) as Attachment[])].sort(
      (a, b) =>
        new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime()
    )
    // Comment attachments (EXP-554) render under their comment, not here.
    return issueFileRows(rows, [
      description,
      ...(commentRows ?? []).map((comment) => comment.body),
    ])
  }, [data, commentRows, description])

  const [uploading, setUploading] = useState(false)

  const handleFiles = async (selected: File[]) => {
    const failures: string[] = []
    const failed: File[] = []

    setUploading(true)
    // EXP-1247: the paperclip is a FILE path — every pick, images and clips
    // included, lands here as a Files row (`asFile`) and is never inlined.
    // Every pick is attempted; failures are collected per file so one oversize
    // upload never silently drops the rest (parity with the native clients).
    for (const file of selected) {
      try {
        await uploadIssueAttachment(issueId, file, { asFile: true })
      } catch (uploadError) {
        failed.push(file)
        failures.push(
          uploadError instanceof Error
            ? `${file.name}: ${uploadError.message}`
            : `${file.name}: upload failed`
        )
      }
    }
    setUploading(false)

    // FEED-79: a toast; Retry uploads only the files that failed.
    if (failures.length > 0) {
      failedAction(
        `upload`,
        failed.length === 1 ? `the file` : `${failed.length} files`,
        undefined,
        {
          description:
            failures.length === selected.length
              ? failures.join(` · `)
              : `${failures.length} of ${selected.length} files failed: ${failures.join(` · `)}`,
          retry: () => void handleFiles(failed),
        }
      )
    }
  }

  const handleDelete = async (file: FilesSectionFile) => {
    const { txId } = await trpc.attachments.delete.mutate(
      { id: file.id },
      SKIP_ERROR_TOAST
    )
    await attachmentCollection.utils.awaitTxId(txId)
  }

  return (
    <FilesSectionView
      files={files}
      readOnly={readOnly}
      onAttach={handleFiles}
      onDelete={handleDelete}
      uploading={uploading}
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
  /** EXP-1247: uploaded through a FILE path (lists here even as an image). */
  asFile?: boolean
}

/**
 * EXP-1170: the Files section's PRESENTATION — the issue detail
 * (`IssueFilesSection`, the synced shape) and the New issue page (a draft's
 * server-only attachments) render the same rows, the same attach button and
 * the same delete confirm. `onDelete` rejects with the message to toast
 * (its tRPC call passes `SKIP_ERROR_TOAST`: the confirm raises the toast).
 */
export function FilesSectionView({
  files,
  readOnly = false,
  onAttach,
  onDelete,
  uploading = false,
}: {
  files: readonly FilesSectionFile[]
  readOnly?: boolean
  onAttach: (files: File[]) => void | Promise<void>
  onDelete: (file: FilesSectionFile) => Promise<void>
  uploading?: boolean
}) {
  const [pendingDelete, setPendingDelete] = useState<FilesSectionFile | null>(
    null
  )
  const [deleting, setDeleting] = useState(false)
  // EXP-955: the `.md` row being previewed in the markdown dialog.
  const [previewFile, setPreviewFile] = useState<FilesSectionFile | null>(null)

  const handleConfirmDelete = async () => {
    if (!pendingDelete) return
    setDeleting(true)
    try {
      await onDelete(pendingDelete)
      setPendingDelete(null)
    } catch (deleteFailure) {
      // A delete: the toast offers no Retry; the confirm stays open.
      failedAction(`delete`, `the file`, deleteFailure)
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
          <Spinner label="Uploading" />
          Uploading
        </p>
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
