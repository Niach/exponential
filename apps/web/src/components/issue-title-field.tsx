import { useEffect, useState } from "react"
import type { Issue } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { Textarea } from "@exp/ui"

// EXP-877: the issue title, lifted out of the detail view so the unified work
// header can carry it on BOTH the issue route and the session route (an
// issue-bound run's header edits the same title). Local state + save on blur;
// Electric writes from other clients land unless the user is mid-edit.

/** The Textarea's own classes — `RUN_TITLE_CLASS` in `work-header.tsx`
 * mirrors the size and padding so a run's static title sits on the same
 * baseline. */
export const ISSUE_TITLE_FIELD_CLASS = `min-h-0 resize-none bg-transparent dark:bg-transparent border-none shadow-none !text-2xl font-semibold px-5 pt-4 pb-1 focus-visible:ring-0 placeholder:text-muted-foreground/50`

export function IssueTitleField({
  issue,
  readOnly = false,
}: {
  issue: Pick<Issue, `id` | `title`>
  readOnly?: boolean
}) {
  const [title, setTitle] = useState(issue.title)

  // Full reset when navigating to a different issue.
  useEffect(() => {
    setTitle(issue.title)
  }, [issue.id])

  // Sync title from Electric when another client changes it, but skip if the
  // local value matches what we'd save (user is editing).
  useEffect(() => {
    if (issue.title !== title && issue.title !== title.trim()) {
      setTitle(issue.title)
    }
  }, [issue.title])

  const handleTitleBlur = async () => {
    if (readOnly) return
    const trimmed = title.trim()
    if (trimmed && trimmed !== issue.title) {
      await trpc.issues.update.mutate({ id: issue.id, title: trimmed })
    }
  }

  // A wrapping textarea (field-sizing-content), not an Input — long titles
  // must wrap on narrow viewports instead of clipping (EXP-189). Enter
  // commits via blur; titles stay single-logical-line.
  return (
    <Textarea
      value={title}
      rows={1}
      onBlur={() => void handleTitleBlur()}
      onChange={(e) => setTitle(e.target.value.replace(/\n/g, ``))}
      onKeyDown={(e) => {
        if (e.key === `Enter`) {
          e.preventDefault()
          e.currentTarget.blur()
        }
      }}
      placeholder="Issue title"
      disabled={readOnly}
      // EXP-424: ProseMirror's image drag carries the image URL as
      // `text/plain`, which a textarea happily accepts — an image dragged
      // within the description would otherwise land as a URL in the title and
      // save on blur. Refuse every drop here; the editor keeps its own.
      onDragOver={(e) => {
        e.preventDefault()
        e.dataTransfer.dropEffect = `none`
      }}
      onDrop={(e) => e.preventDefault()}
      className={ISSUE_TITLE_FIELD_CLASS}
    />
  )
}
