import type { ComponentProps, ReactElement, ReactNode } from "react"

import { cn } from "./cn"
import { ComposerTool } from "./composer"
import { conceptIcon } from "./icons.generated"

// EXP-1334: ONE composer for a start, a thread and a session (owner
// decision, the "old session composer layout"): THE Composer card holding
// the field, the "+" options menu followed by the inline edit tools (the
// comment composer's row: Add image · Attach files · # · emoji) and Send ⇄
// Stop; UNDER the card the muted options line. Before a start the line is
// device · account · model · Plan · repository, in a session model · Plan …
// the context ring. These are the layout halves every host shares; each
// host keeps its own field, upload and send logic.

const ImageIcon = conceptIcon(`editor-image`)
const AttachIcon = conceptIcon(`ui-attach`)
const IssueRefIcon = conceptIcon(`editor-issue-ref`)
const EmojiIcon = conceptIcon(`editor-emoji`)

/** The card with its options line under it. */
export function ComposerFrame({
  options,
  className,
  children,
  ...props
}: ComponentProps<`div`> & {
  /** The options line (`ComposerOptionsRow`), under the card. */
  options?: ReactNode
}) {
  return (
    <div
      data-slot="composer-frame"
      className={cn(`flex flex-col gap-1.5`, className)}
      {...props}
    >
      {children}
      {options}
    </div>
  )
}

/** The muted options line under the card: inline pickers, the Plan switch,
 *  a trailing item pushed right with `ml-auto`. Phones keep it on ONE
 *  horizontally scrolling row; md+ wraps. */
export function ComposerOptionsRow({ className, ...props }: ComponentProps<`div`>) {
  return (
    <div
      data-slot="composer-options"
      className={cn(
        `flex flex-wrap items-center gap-x-3 gap-y-1 px-1 text-xs text-muted-foreground`,
        `max-md:flex-nowrap max-md:overflow-x-auto max-md:whitespace-nowrap max-md:[scrollbar-width:none] max-md:[&>*]:shrink-0`,
        className
      )}
      {...props}
    />
  )
}

/** The inline edit tools after the "+": the comment composer's row. A host
 *  wraps the emoji button in its picker popover (`wrapEmoji`). */
export function ComposerEditTools({
  onAddImage,
  onAttachFiles,
  onIssueRef,
  wrapEmoji,
  disabled,
}: {
  onAddImage?: () => void
  onAttachFiles?: () => void
  onIssueRef?: () => void
  wrapEmoji?: (trigger: ReactElement) => ReactNode
  disabled?: boolean
}) {
  const emoji = (
    <ComposerTool aria-label="Insert emoji" title="Insert emoji" disabled={disabled}>
      <EmojiIcon />
    </ComposerTool>
  )
  return (
    <>
      <ComposerTool
        aria-label="Add image"
        title="Add image"
        disabled={disabled}
        onClick={onAddImage}
      >
        <ImageIcon />
      </ComposerTool>
      <ComposerTool
        aria-label="Attach files"
        title="Attach files"
        disabled={disabled}
        onClick={onAttachFiles}
      >
        <AttachIcon />
      </ComposerTool>
      <ComposerTool
        aria-label="Insert issue reference"
        title="Insert issue reference"
        disabled={disabled}
        onClick={onIssueRef}
      >
        <IssueRefIcon />
      </ComposerTool>
      {wrapEmoji ? wrapEmoji(emoji) : emoji}
    </>
  )
}
