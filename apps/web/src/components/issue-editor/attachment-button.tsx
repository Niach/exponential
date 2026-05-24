import { useRef } from "react"
import { LoaderCircle, Paperclip } from "lucide-react"
import { acceptedImageContentTypes } from "@/lib/storage/issue-attachments"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"

interface IssueEditorAttachmentButtonProps {
  disabled?: boolean
  disabledReason?: string
  onFiles?: (files: File[]) => void | Promise<void>
  uploading?: boolean
}

export function IssueEditorAttachmentButton({
  disabled,
  disabledReason,
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
        accept={acceptedImageContentTypes.join(`,`)}
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
        aria-label="Add image"
        disabled={isDisabled || uploading}
        onClick={() => {
          inputRef.current?.click()
        }}
      >
        {uploading ? (
          <LoaderCircle className="size-3 animate-spin" />
        ) : (
          <Paperclip className="size-3" />
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
