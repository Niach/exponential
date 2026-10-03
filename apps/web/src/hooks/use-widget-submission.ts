import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"

/** The `widget_submissions` row behind a widget-filed issue — server-only,
 *  members-only (`widgets.submissionForIssue`). */
export type WidgetSubmission = NonNullable<
  Awaited<ReturnType<typeof trpc.widgets.submissionForIssue.query>>
>

/**
 * SLOP-4: ONE fetch per issue detail, shared by the submission card (the
 * metadata rows), the timeline (a reporter comment's name) and the comment
 * composer (the "Reply to reporter" toggle shows only when the row carries a
 * reporter email). `undefined` while loading, `null` for a non-widget issue
 * or a non-member (the server refuses; the card simply doesn't render).
 */
export function useWidgetSubmission(
  issueId: string,
  enabled = true
): WidgetSubmission | null | undefined {
  const [submission, setSubmission] = useState<
    WidgetSubmission | null | undefined
  >(undefined)

  useEffect(() => {
    if (!enabled) {
      setSubmission(null)
      return
    }
    let cancelled = false
    setSubmission(undefined)
    trpc.widgets.submissionForIssue.query({ issueId }).then(
        (row) => {
          if (!cancelled) setSubmission(row ?? null)
        },
        () => {
          if (!cancelled) setSubmission(null)
        }
      )
    return () => {
      cancelled = true
    }
  }, [issueId, enabled])

  return submission
}
