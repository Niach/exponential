import { useRef } from "react"
import { LoaderCircle, Paperclip, type LucideIcon } from "lucide-react"
import { acceptedImageContentTypes } from "@/lib/storage/issue-attachments"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"

interface IssueEditorAttachmentButtonProps {
  // Native file-picker filter. Defaults to the inline-image types; the Files
  // section (EXP-297) passes `*/*`.
  accept?: string
  disabled?: boolean
  disabledReason?: string
  // Glyph of the trigger; defaults to the paperclip.
  icon?: LucideIcon
  // Accessible name of the trigger; defaults to the image wording.
  label?: string
  onFiles?: (files: File[]) => void | Promise<void>
  uploading?: boolean
}

export function IssueEditorAttachmentButton({
  accept = acceptedImageContentTypes.join(`,`),
  disabled,
  disabledReason,
  icon: Icon = Paperclip,
  label = `Add image`,
  onFiles,
  uploading,
}: IssueEditorAttachmentButtonProps) {
  const inputRef = useRef<HTMLInputElement>(null)
  const isDisabled = disabled || !onFiles

  const button = (
    <>
      <Input
        ref={inputRef}
        type="file"
        accept={accept}
        className="hidden"
        disabled={isDisabled || uploading}
        onChange={(event) => {
          const files = Array.from(event.target.files ?? [])

          if (files.length > 0 && onFiles) {
            void onFiles(files)
          }

          event.target.value = ``
        }}
      />
      <Button
        variant="ghost"
        size="icon-xs"
        className="text-muted-foreground"
        type="button"
        aria-label={label}
        disabled={isDisabled || uploading}
        onClick={() => {
          inputRef.current?.click()
        }}
      >
        {uploading ? (
          <LoaderCircle className="size-3 animate-spin" />
        ) : (
          <Icon className="size-3" />
        )}
      </Button>
    </>
  )

  if (!disabledReason) {
    return button
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span>{button}</span>
      </TooltipTrigger>
      <TooltipContent>{disabledReason}</TooltipContent>
    </Tooltip>
  )
}
