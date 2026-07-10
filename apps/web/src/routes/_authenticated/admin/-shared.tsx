import { Badge } from "@/components/ui/badge"

// Admin console formatting helpers. tRPC serializes with plain JSON, so Date
// fields arrive as ISO strings (despite the inferred `Date` types) and
// Infinity limits arrive as `null` — every helper accepts the wire forms.

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
