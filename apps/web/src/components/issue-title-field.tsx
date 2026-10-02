import { useEffect, useState } from "react"
import type { Issue } from "@/db/schema"
import { trpc } from "@/lib/trpc-client"
import { BARE_FIELD_CLASS, Textarea, cn } from "@exp/ui"

// EXP-877: the issue title, lifted out of the detail view so the unified work
// header can carry it on BOTH the issue route and the session route (an
// issue-bound run's header edits the same title). Local state + save on blur;
// Electric writes from other clients land unless the user is mid-edit.

/** The Textarea's own classes. EXP-1162: on md+ the title row shares its
 * first line with the floating work bar's cluster (`WorkHeader`, 48px), so
 * the 32px line sits 8px down there. */
export const ISSUE_TITLE_FIELD_CLASS = cn(
  BARE_FIELD_CLASS,
  `min-h-0 resize-none dark:bg-transparent !text-2xl font-semibold px-5 pt-4 md:pt-2 pb-1 placeholder:text-muted-foreground/50`
)

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

  return (
    <IssueTitleInput
      value={title}
      onChange={setTitle}
      onBlur={() => void handleTitleBlur()}
      readOnly={readOnly}
    />
  )
}

/**
 * EXP-1170: the bare title input both the issue detail and the New issue page
 * draw. A wrapping textarea (field-sizing-content), not an Input — long
 * titles must wrap on narrow viewports instead of clipping (EXP-189). Enter
 * never inserts a newline: by default it commits via blur, `onEnter` lets the
 * draft page move focus to the description instead. Titles stay
 * single-logical-line.
 */
export function IssueTitleInput({
  value,
  onChange,
  onBlur,
  onEnter,
  autoFocus,
  placeholder = `Issue title`,
  readOnly = false,
  className,
}: {
  value: string
  onChange: (value: string) => void
  onBlur?: () => void
  /** Replaces the default "Enter blurs". */
  onEnter?: () => void
  autoFocus?: boolean
  placeholder?: string
  readOnly?: boolean
  className?: string
}) {
  return (
    <Textarea
      value={value}
      rows={1}
      autoFocus={autoFocus}
      onBlur={onBlur}
      onChange={(e) => onChange(e.target.value.replace(/\n/g, ``))}
      onKeyDown={(e) => {
        if (e.key !== `Enter`) return
        e.preventDefault()
        // Cmd/Ctrl+Enter bubbles to a host with its own Enter handling (the
        // draft page's Create).
        if (onEnter) {
          if (!e.metaKey && !e.ctrlKey) onEnter()
          return
        }
        e.currentTarget.blur()
      }}
      placeholder={placeholder}
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
      className={cn(ISSUE_TITLE_FIELD_CLASS, className)}
    />
  )
}
