import { useEffect, useState } from "react"
import { Megaphone } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Badge } from "@/components/ui/badge"

type SubmissionRow = Awaited<
  ReturnType<typeof trpc.widgets.submissionForIssue.query>
>

// EXP-42b: compact members-only card surfacing the reporter/page/env metadata
// that no longer lives in widget-issue descriptions (it's PII on public
// boards). Backed by widgets.submissionForIssue (member-gated); renders
// nothing while loading, on error (non-member), or for non-widget issues.
export function WidgetSubmissionCard({ issueId }: { issueId: string }) {
  const [submission, setSubmission] = useState<SubmissionRow | null>(null)

  useEffect(() => {
    let cancelled = false
    setSubmission(null)
    trpc.widgets.submissionForIssue.query({ issueId }).then(
      (row) => {
        if (!cancelled) setSubmission(row)
      },
      () => {
        // Non-member / stale issue — the card simply doesn't render.
      }
    )
    return () => {
      cancelled = true
    }
  }, [issueId])

  if (!submission) return null

  const reporter =
    submission.reporterName && submission.reporterEmail
      ? `${submission.reporterName} <${submission.reporterEmail}>`
      : (submission.reporterName ?? submission.reporterEmail ?? `Anonymous`)

  const viewport =
    submission.viewportWidth && submission.viewportHeight
      ? `${submission.viewportWidth}×${submission.viewportHeight}` +
        (submission.devicePixelRatio ? ` @${submission.devicePixelRatio}x` : ``)
      : null
  const screen =
    submission.screenWidth && submission.screenHeight
      ? `${submission.screenWidth}×${submission.screenHeight}`
      : null
  const display = [viewport && `Viewport ${viewport}`, screen && `Screen ${screen}`]
    .filter(Boolean)
    .join(` · `)

  const customData =
    submission.customData && Object.keys(submission.customData).length > 0
      ? JSON.stringify(submission.customData, null, 2)
      : null

  const rows: { label: string; value: React.ReactNode }[] = [
    { label: `Reporter`, value: reporter },
    ...(submission.pageUrl
      ? [{ label: `Page`, value: submission.pageUrl }]
      : []),
    ...(display ? [{ label: `Display`, value: display }] : []),
    ...(submission.userAgent
      ? [{ label: `User agent`, value: submission.userAgent }]
      : []),
  ]

  return (
    <div className="mx-5 my-3 rounded-md border border-border bg-muted/30 px-3 py-2.5 text-xs">
      <div className="mb-2 flex items-center gap-1.5">
        <Megaphone className="size-3.5 text-muted-foreground" />
        <span className="font-medium">Reported via widget</span>
        <Badge variant="outline" className="text-[10px] font-normal text-muted-foreground">
          Members only
        </Badge>
      </div>
      <dl className="space-y-1">
        {rows.map((row) => (
          <div key={row.label} className="flex gap-2">
            <dt className="w-20 shrink-0 text-muted-foreground">{row.label}</dt>
            <dd className="min-w-0 break-all">{row.value}</dd>
          </div>
        ))}
        {customData && (
          <div className="flex gap-2">
            <dt className="w-20 shrink-0 text-muted-foreground">Custom data</dt>
            <dd className="min-w-0 flex-1">
              <pre className="overflow-x-auto rounded bg-muted/50 p-2 font-mono text-[11px] leading-relaxed">
                {customData}
              </pre>
            </dd>
          </div>
        )}
      </dl>
    </div>
  )
}
