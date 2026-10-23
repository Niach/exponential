import { useImperativeHandle, useRef, type Ref, type RefObject } from "react"
import { ComposerEditTools } from "@exp/ui"
import type { MentionTextareaHandle } from "@/components/mention-textarea"
import { EmojiPickerPopover } from "@/components/emoji-picker"
import { issueRefInsertionText } from "@/components/issue-editor/formatting-rail"
import { acceptedImageContentTypes } from "@/lib/storage/issue-attachments"

// EXP-1334: THE composer's inline tools (the start, the thread and the
// session view alike), after its "+" options menu: Add image · Attach files
// · Insert issue reference (`#`, EXP-568's token-start rule) · Insert emoji
// (EXP-551, at the caret). The hidden file inputs live here; a host's "+"
// "Add file or image" row opens the same chooser through the handle.

export interface ComposerInlineToolsHandle {
  /** Opens the any-file chooser (the "+" menu's "Add file or image"). */
  attach: () => void
}

export function ComposerInlineTools({
  ref,
  fieldRef,
  onFiles,
  disabled,
  hidden = false,
}: {
  ref?: Ref<ComposerInlineToolsHandle>
  fieldRef: RefObject<MentionTextareaHandle | null>
  onFiles: (files: File[]) => void
  disabled?: boolean
  /** N-12: the file inputs alone (the session composer's "+" is its one
   *  tool ×4; the inputs still answer its Add file row). */
  hidden?: boolean
}) {
  const imageInputRef = useRef<HTMLInputElement>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const open = (input: HTMLInputElement | null) => input?.click()
  useImperativeHandle(ref, () => ({ attach: () => open(fileInputRef.current) }))

  const picked = (input: HTMLInputElement) => {
    if (input.files) onFiles(Array.from(input.files))
    input.value = ``
  }

  return (
    <>
      <input
        ref={imageInputRef}
        type="file"
        accept={acceptedImageContentTypes.join(`,`)}
        multiple
        hidden
        data-testid="composer-image-input"
        onChange={(event) => picked(event.target)}
      />
      <input
        ref={fileInputRef}
        type="file"
        multiple
        hidden
        data-testid="composer-file-input"
        onChange={(event) => picked(event.target)}
      />
      {!hidden && <ComposerEditTools
        disabled={disabled}
        onAddImage={() => open(imageInputRef.current)}
        onAttachFiles={() => open(fileInputRef.current)}
        onIssueRef={() =>
          fieldRef.current?.insertText(
            issueRefInsertionText(fieldRef.current.charBeforeCaret())
          )
        }
        wrapEmoji={(trigger) => (
          <EmojiPickerPopover
            side="top"
            onPick={(unicode) => fieldRef.current?.insertText(unicode)}
          >
            {trigger}
          </EmojiPickerPopover>
        )}
      />}
    </>
  )
}
