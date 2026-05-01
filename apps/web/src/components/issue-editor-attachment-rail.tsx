import { X } from "lucide-react"
import { IssueEditorAttachmentButton } from "@/components/issue-editor-dialog-shell"
import type { MarkdownImageOccurrence } from "@/lib/issue-attachments"
import { cn } from "@/lib/utils"

interface IssueEditorAttachmentRailProps {
  attachmentStatus?: string | null
  disabled?: boolean
  images: MarkdownImageOccurrence[]
  onFiles?: (files: File[]) => void | Promise<void>
  onRemove?: (occurrenceIndex: number) => void
  uploading?: boolean
}

function getAttachmentLabel(image: MarkdownImageOccurrence) {
  if (image.alt.trim()) {
    return image.alt.trim()
  }

  const filename = image.url.split(`/`).pop()?.split(`?`)[0]
  return filename || `Image ${image.occurrenceIndex + 1}`
}

export function IssueEditorAttachmentRail({
  attachmentStatus,
  disabled,
  images,
  onFiles,
  onRemove,
  uploading,
}: IssueEditorAttachmentRailProps) {
  const imageCountLabel = `${images.length} image${images.length === 1 ? `` : `s`}`
  const removable = !disabled && typeof onRemove === `function`

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
                      `group flex shrink-0 items-center gap-1 rounded-md border border-white/10 bg-white/[0.04] py-1 pr-1.5 pl-1 shadow-[inset_0_1px_0_rgba(255,255,255,0.03)] transition-colors hover:border-white/16 hover:bg-white/[0.06]`,
                      removable ? `cursor-default` : `opacity-90`
                    )}
                    data-testid={`issue-attachment-chip-${image.occurrenceIndex}`}
                  >
                    <img
                      src={image.url}
                      alt=""
                      className="h-7 w-7 shrink-0 rounded-[6px] border border-white/10 object-cover"
                    />
                    <span className="max-w-24 truncate text-xs text-foreground/88">
                      {label}
                    </span>
                    {removable ? (
                      <button
                        type="button"
                        className="flex size-5 shrink-0 items-center justify-center rounded-full text-muted-foreground/70 transition-colors hover:bg-white/10 hover:text-foreground"
                        aria-label={`Remove attachment ${label}`}
                        onClick={() => onRemove?.(image.occurrenceIndex)}
                      >
                        <X className="size-3" />
                      </button>
                    ) : null}
                  </div>
                )
              })}
            </div>
            <span className="shrink-0 text-xs text-muted-foreground">
              {uploading ? `Uploading...` : imageCountLabel}
            </span>
          </>
        )}
      </div>
    </div>
  )
}
