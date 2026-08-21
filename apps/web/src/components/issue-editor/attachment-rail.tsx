import { X } from "lucide-react"
import { formatAttachmentSize, getAttachmentIcon } from "@/lib/attachment-files"
import type { DraftFile } from "@/lib/create-issue-helpers"
import { cn } from "@/lib/utils"

interface IssueEditorAttachmentRailProps {
  attachmentStatus?: string | null
  disabled?: boolean
  // EXP-297: non-image files queued for upload after the issue is created.
  files: DraftFile[]
  onRemoveFile?: (draftFileId: string) => void
  uploading?: boolean
}

const chipClassName = `group flex shrink-0 items-center gap-1 rounded-md border border-glass-stroke-card bg-glass-section py-1 pr-1.5 pl-1 shadow-[inset_0_1px_0_rgba(255,255,255,0.03)] transition-colors hover:border-glass-stroke-active hover:bg-glass-card`

// EXP-335: display-only — the pickers live in the editor's formatting toolbar
// (image + attach-file buttons). EXP-586: images render inline in the
// description only, so the rail carries just the queued non-image file chips
// (no image chips, no count).
export function IssueEditorAttachmentRail({
  attachmentStatus,
  disabled,
  files,
  onRemoveFile,
  uploading,
}: IssueEditorAttachmentRailProps) {
  const fileRemovable = !disabled && typeof onRemoveFile === `function`

  return (
    <div
      className="flex min-w-0 flex-1 items-center gap-2"
      data-testid="issue-attachment-rail"
    >
      <div className="flex min-w-0 flex-1 items-center gap-2 overflow-hidden">
        {attachmentStatus ? (
          <span className="min-w-0 truncate text-xs text-destructive">
            {attachmentStatus}
          </span>
        ) : (
          <>
            <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto py-0.5 pr-1">
              {files.map((draftFile) => {
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
            {uploading ? (
              <span className="shrink-0 text-xs text-muted-foreground">
                Uploading...
              </span>
            ) : null}
          </>
        )}
      </div>
    </div>
  )
}
