import { Megaphone } from "lucide-react"
import { conceptIcon } from "@exp/ui"
import type { WidgetSubmission } from "@/hooks/use-widget-submission"
import { relativeTime } from "@/components/comment-rows/format"
import { reporterDisplayName } from "@/components/reporter-reply-copy"

const AgentSourceIcon = conceptIcon(`ui-agent-source`)

// EXP-42b: compact members-only card surfacing the reporter/page/env metadata
// that no longer lives in widget-issue descriptions (it's PII on public
// boards). SLOP-4: fed by the detail's ONE `useWidgetSubmission` fetch (the
// timeline and the composer read the same row); the detail renders it only
// when the row exists, so a non-widget issue draws nothing.
// EXP-496: agent-filed bug reports carry the same submission row — the header
// keys off the issue's source.
export function WidgetSubmissionCard({
  submission,
  source,
}: {
  submission: WidgetSubmission
  source: string
}) {
  // The reporter line: the name they gave (else the anonymous label) with
  // their address beside it when they left one — the address is what the
  // composer's "Reply to reporter" toggle emails.
  const reporterName = reporterDisplayName(submission.reporterName)
  const reporter = submission.reporterEmail
    ? `${reporterName} <${submission.reporterEmail}>`
    : reporterName

  const viewport =
    submission.viewportWidth && submission.viewportHeight
      ? `${submission.viewportWidth}×${submission.viewportHeight}` +
        (submission.devicePixelRatio ? ` @${submission.devicePixelRatio}x` : ``)
      : null
  const screen =
    submission.screenWidth && submission.screenHeight
      ? `${submission.screenWidth}×${submission.screenHeight}`
      : null
  const display = [
    viewport && `Viewport ${viewport}`,
    screen && `Screen ${screen}`,
  ]
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
        {source === `agent` ? (
          <AgentSourceIcon className="size-3.5 text-muted-foreground" />
        ) : (
          <Megaphone className="size-3.5 text-muted-foreground" />
        )}
        <span className="font-medium">
          {source === `agent` ? `Reported by agent` : `Reported via widget`}
        </span>
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
      {/* SLOP-4: the reporter page stamps `last_reporter_seen_at` on every
          open — the one signal that the emailed link is being read. */}
      {submission.lastReporterSeenAt && (
        <p className="mt-2 text-muted-foreground">
          Last opened the conversation {relativeTime(submission.lastReporterSeenAt)}
        </p>
      )}
    </div>
  )
}
