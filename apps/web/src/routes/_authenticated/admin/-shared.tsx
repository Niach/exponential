import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
} from "@/components/ui/card"

// Admin console formatting helpers. tRPC serializes with plain JSON, so Date
// fields arrive as ISO strings (despite the inferred `Date` types) and
// Infinity limits arrive as `null` — every helper accepts the wire forms.

export function StatCard({
  label,
  value,
  hint,
}: {
  label: string
  value: string
  hint?: string
}) {
  return (
    <Card className="gap-1 py-4">
      <CardHeader className="px-4">
        <CardDescription className="text-xs">{label}</CardDescription>
      </CardHeader>
      <CardContent className="px-4">
        <div className="text-2xl font-bold tabular-nums">{value}</div>
        {hint && (
          <div className="text-xs text-muted-foreground mt-0.5">{hint}</div>
        )}
      </CardContent>
    </Card>
  )
}

// Simple CSS bar strip — one bar per day over the trailing `days` days, zero
// days rendered as a faint baseline. Deliberately no chart library.
export function DayBars({
  rows,
  days = 30,
}: {
  rows: { day: string; count: number }[]
  days?: number
}) {
  const byDay = new Map(rows.map((r) => [r.day, r.count]))
  const filled: { day: string; count: number }[] = []
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date()
    d.setDate(d.getDate() - i)
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, `0`)}-${String(d.getDate()).padStart(2, `0`)}`
    filled.push({ day: key, count: byDay.get(key) ?? 0 })
  }
  const max = Math.max(1, ...filled.map((d) => d.count))
  return (
    <div className="flex h-16 items-end gap-[3px]">
      {filled.map((d) => (
        <div
          key={d.day}
          className="flex h-full flex-1 flex-col justify-end"
          title={`${d.day}: ${d.count}`}
        >
          <div
            className={
              d.count > 0 ? `rounded-sm bg-primary` : `rounded-sm bg-muted`
            }
            style={{
              height:
                d.count > 0 ? `${Math.max(10, (d.count / max) * 100)}%` : `3px`,
            }}
          />
        </div>
      ))}
    </div>
  )
}

export function formatDate(value: Date | string | null | undefined): string {
  if (!value) return `—`
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return `—`
  return d.toLocaleDateString(undefined, {
    year: `numeric`,
    month: `short`,
    day: `numeric`,
  })
}

export function formatDateTime(
  value: Date | string | null | undefined
): string {
  if (!value) return `—`
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return `—`
  return d.toLocaleString(undefined, {
    year: `numeric`,
    month: `short`,
    day: `numeric`,
    hour: `2-digit`,
    minute: `2-digit`,
  })
}

export function formatRelative(
  value: Date | string | null | undefined
): string {
  if (!value) return `—`
  const d = new Date(value)
  if (Number.isNaN(d.getTime())) return `—`
  const mins = Math.floor((Date.now() - d.getTime()) / 60_000)
  if (mins < 1) return `just now`
  if (mins < 60) return `${mins}m ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days}d ago`
  return formatDate(d)
}

export function formatStorageMb(mb: number | null | undefined): string {
  if (mb === null || mb === undefined || !Number.isFinite(mb)) return `∞`
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`
  return `${Math.round(mb * 10) / 10} MB`
}

// Plan limits use Infinity server-side, which JSON serializes to null.
export function formatLimit(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) {
    return `∞`
  }
  return String(value)
}

export function PlanBadge({
  plan,
  compApplied,
}: {
  plan: string
  compApplied?: boolean
}) {
  return (
    <Badge variant="outline" className="text-xs capitalize whitespace-nowrap">
      {plan}
      {compApplied ? ` (comp)` : ``}
    </Badge>
  )
}

const EMAIL_STATUS_VARIANT: Record<
  string,
  `default` | `secondary` | `destructive` | `outline`
> = {
  sent: `secondary`,
  queued: `outline`,
  failed: `destructive`,
  bounced: `destructive`,
  complained: `destructive`,
}

export function EmailStatusBadge({ status }: { status: string }) {
  return (
    <Badge
      variant={EMAIL_STATUS_VARIANT[status] ?? `outline`}
      className="text-xs"
    >
      {status}
    </Badge>
  )
}

export interface EmailDeliveryRow {
  id: string
  toEmail: string
  // Absent on rows written before the column existed (and on queued-only
  // digest rows / invite-cap refusals, which never got a send result).
  subject?: string | null
  kind: string
  status: string
  error: string | null
  sentAt: Date | string | null
  createdAt: Date | string
  issueIdentifier: string | null
  userName?: string | null
}

// The one email-deliveries grid — shared by the global /admin/email list
// (showSubject) and the per-user/per-team detail cards.
export function EmailDeliveriesTable({
  rows,
  showSubject = false,
}: {
  rows: EmailDeliveryRow[]
  showSubject?: boolean
}) {
  const cols = showSubject
    ? `grid-cols-[minmax(160px,1fr)_minmax(160px,1.2fr)_130px_80px_90px_140px]`
    : `grid-cols-[1fr_130px_80px_90px_140px]`
  return (
    <div className="rounded-md border overflow-x-auto">
      <div className={showSubject ? `min-w-[780px]` : `min-w-[580px]`}>
        <div
          className={`grid ${cols} items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground`}
        >
          <div>To</div>
          {showSubject && <div>Subject</div>}
          <div>Kind</div>
          <div>Status</div>
          <div>Issue</div>
          <div>Sent</div>
        </div>
        {rows.map((d) => (
          <div
            key={d.id}
            className={`grid ${cols} items-center gap-3 border-b px-3 py-2 last:border-b-0 text-xs`}
          >
            <div className="min-w-0">
              <div className="truncate">{d.toEmail}</div>
              {d.userName && (
                <div className="truncate text-muted-foreground">
                  {d.userName}
                </div>
              )}
            </div>
            {showSubject && (
              <div
                className="truncate text-muted-foreground"
                title={d.subject ?? undefined}
              >
                {d.subject ?? `—`}
              </div>
            )}
            <div className="truncate text-muted-foreground" title={d.kind}>
              {d.kind}
            </div>
            <div title={d.error ?? undefined}>
              <EmailStatusBadge status={d.status} />
            </div>
            <div className="text-muted-foreground">
              {d.issueIdentifier ?? `—`}
            </div>
            <div className="text-muted-foreground">
              {formatDateTime(d.sentAt ?? d.createdAt)}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
