import { FilePlus2, X } from "lucide-react"
import { IssueEditorAttachmentButton } from "@/components/issue-editor/dialog-shell"
import { formatAttachmentSize, getAttachmentIcon } from "@/lib/attachment-files"
import type { DraftFile } from "@/lib/create-issue-helpers"
import type { MarkdownImageOccurrence } from "@/lib/storage/issue-attachments"
import { cn } from "@/lib/utils"

interface IssueEditorAttachmentRailProps {
  attachmentStatus?: string | null
  disabled?: boolean
  // EXP-297: non-image files queued for upload after the issue is created.
  // Omitted (with onAttachFiles) the rail behaves exactly as before.
  files?: DraftFile[]
  images: MarkdownImageOccurrence[]
  onAttachFiles?: (files: File[]) => void | Promise<void>
  onFiles?: (files: File[]) => void | Promise<void>
  onRemove?: (occurrenceIndex: number) => void
  onRemoveFile?: (draftFileId: string) => void
  uploading?: boolean
}

function getAttachmentLabel(image: MarkdownImageOccurrence) {
  if (image.alt.trim()) {
    return image.alt.trim()
  }

  const filename = image.url.split(`/`).pop()?.split(`?`)[0]
  return filename || `Image ${image.occurrenceIndex + 1}`
}

const chipClassName = `group flex shrink-0 items-center gap-1 rounded-md border border-glass-stroke-card bg-glass-section py-1 pr-1.5 pl-1 shadow-[inset_0_1px_0_rgba(255,255,255,0.03)] transition-colors hover:border-glass-stroke-active hover:bg-glass-card`

export function IssueEditorAttachmentRail({
  attachmentStatus,
  disabled,
  files,
  images,
  onAttachFiles,
  onFiles,
  onRemove,
  onRemoveFile,
  uploading,
}: IssueEditorAttachmentRailProps) {
  const draftFiles = files ?? []
  const countLabel = [
    `${images.length} image${images.length === 1 ? `` : `s`}`,
    draftFiles.length > 0
      ? `${draftFiles.length} file${draftFiles.length === 1 ? `` : `s`}`
      : null,
  ]
    .filter(Boolean)
    .join(`, `)
  const removable = !disabled && typeof onRemove === `function`
  const fileRemovable = !disabled && typeof onRemoveFile === `function`

  return (
    <div
      className="flex min-w-0 flex-1 items-center gap-2"
      data-testid="issue-attachment-rail"
    >
      <IssueEditorAttachmentButton
        onFiles={onFiles}
        uploading={uploading}
        disabled={disabled}
      />
      {onAttachFiles && (
        <IssueEditorAttachmentButton
          accept="*/*"
          icon={FilePlus2}
          label="Attach file"
          onFiles={onAttachFiles}
          uploading={uploading}
          disabled={disabled}
        />
      )}
      <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
        {attachmentStatus ? (
          <span className="min-w-0 truncate text-xs text-destructive">
            {attachmentStatus}
          </span>
        ) : (
          <>
            <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto py-0.5 pr-1">
              {images.map((image) => {
                const label = getAttachmentLabel(image)

                return (
                  <div
                    key={`${image.occurrenceIndex}-${image.url}-${image.start}`}
                    className={cn(
                      chipClassName,
                      removable ? `cursor-default` : `opacity-90`
                    )}
                    data-testid={`issue-attachment-chip-${image.occurrenceIndex}`}
                  >
                    <img
                      src={image.url}
                      alt=""
                      className="h-7 w-7 shrink-0 rounded-[6px] border border-glass-stroke-card object-cover"
                    />
                    <span className="max-w-24 truncate text-xs text-foreground/88">
                      {label}
                    </span>
                    {removable ? (
                      <button
                        type="button"
                        className="flex size-5 shrink-0 items-center justify-center rounded-full text-muted-foreground/70 transition-colors hover:bg-glass-active hover:text-foreground"
                        aria-label={`Remove attachment ${label}`}
                        onClick={() => onRemove?.(image.occurrenceIndex)}
                      >
                        <X className="size-3" />
                      </button>
                    ) : null}
                  </div>
                )
              })}
              {draftFiles.map((draftFile) => {
                const Icon = getAttachmentIcon(draftFile.file.type)

                return (
                  <div
                    key={draftFile.id}
                    className={cn(
                      chipClassName,
                      fileRemovable ? `cursor-default` : `opacity-90`
                    )}
                    data-testid={`issue-attachment-file-chip-${draftFile.id}`}
                  >
                    <span className="flex size-7 shrink-0 items-center justify-center rounded-[6px] border border-glass-stroke-card">
                      <Icon className="size-3.5 text-muted-foreground" />
                    </span>
                    <span className="max-w-24 truncate text-xs text-foreground/88">
                      {draftFile.file.name}
                    </span>
                    <span className="shrink-0 text-[10px] text-muted-foreground tabular-nums">
                      {formatAttachmentSize(draftFile.file.size)}
                    </span>
                    {fileRemovable ? (
                      <button
                        type="button"
                        className="flex size-5 shrink-0 items-center justify-center rounded-full text-muted-foreground/70 transition-colors hover:bg-glass-active hover:text-foreground"
                        aria-label={`Remove attachment ${draftFile.file.name}`}
                        onClick={() => onRemoveFile?.(draftFile.id)}
                      >
                        <X className="size-3" />
                      </button>
                    ) : null}
                  </div>
                )
              })}
            </div>
            <span className="shrink-0 text-xs text-muted-foreground">
              {uploading ? `Uploading...` : countLabel}
            </span>
          </>
        )}
      </div>
    </div>
  )
}
