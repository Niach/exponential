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
