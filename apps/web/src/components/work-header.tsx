import type { ReactNode, Ref } from "react"
import { DETAIL_STICKY_BAND_CLASS } from "@/components/team/app-shell"
import { cn } from "@/lib/utils"

// EXP-877: the ONE work header — the issue route and the session route both
// render it, so the title never moves between an issue and its run (the
// desktop IDE's `work_header.rs` twin). Fixed above the scrolling body, the
// same 896px column as the issue body, the transcript and the diff.
//
//   Row 1: the title (an editable field for issues, static text for runs)
//          | the right cluster on the SAME line, top-aligned
//   Row 2: the properties tray (issue-bound faces only)

/** The reading column every face shares: issue body, transcript, diff. */
export const WORK_COLUMN_CLASS = `mx-auto w-full max-w-4xl`

/** A static run title styled and padded exactly like the issue title's
 * Textarea (`issue-title-field.tsx`), so the baseline never moves when the
 * face flips. */
export const RUN_TITLE_CLASS = `block w-full min-w-0 break-words px-5 pt-4 pb-1 text-2xl font-semibold`

export function WorkHeader({
  ref,
  title,
  trailing,
  tray,
  className,
}: {
  /** The measured node — the issue editor's scroll inset reads its height. */
  ref?: Ref<HTMLDivElement>
  title: ReactNode
  /** The right cluster: face toggle, then the face's own controls. */
  trailing?: ReactNode
  /** Row 2 — the issue's properties tray. Absent on issue-less runs. */
  tray?: ReactNode
  className?: string
}) {
  return (
    <div
      ref={ref}
      className={cn(DETAIL_STICKY_BAND_CLASS, `shrink-0 pb-3`, className)}
      data-testid="work-header"
    >
      <div className={cn(WORK_COLUMN_CLASS, `flex items-start gap-2`)}>
        <div className="min-w-0 flex-1">{title}</div>
        {trailing && (
          <div className="flex shrink-0 items-center gap-1 pt-4 pr-4">
            {trailing}
          </div>
        )}
      </div>
      {tray}
    </div>
  )
}
