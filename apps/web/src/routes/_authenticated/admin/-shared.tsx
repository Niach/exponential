import { useState } from "react"
import { Pill } from "@/components/ui/pill"
import { cn } from "@/lib/utils"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
} from "@/components/ui/card"

// Admin console formatting helpers. tRPC serializes with plain JSON, so Date
// fields arrive as ISO strings (despite the inferred `Date` types) and
// Infinity limits arrive as `null` — every helper accepts the wire forms.

export type StatTone = `ok` | `warn` | `bad`

const TONE_CLASS: Record<StatTone, string> = {
  ok: ``,
  warn: `text-amber-400`,
  bad: `text-destructive`,
}

export function StatCard({
  label,
  value,
  hint,
  tone = `ok`,
}: {
  label: string
  value: string
  hint?: string
  // EXP-759: the performance health strip colours the value — warn = worth a
  // look, bad = something is broken. Status colour never stands alone: the
  // label + hint always say what the number is.
  tone?: StatTone
}) {
  return (
    <Card className="gap-1 py-4">
      <CardHeader className="px-4">
        <CardDescription className="text-xs">{label}</CardDescription>
      </CardHeader>
      <CardContent className="px-4">
        <div className={cn(`text-2xl font-bold tabular-nums`, TONE_CLASS[tone])}>
          {value}
        </div>
        {hint && (
          <div className="text-xs text-muted-foreground mt-0.5">{hint}</div>
        )}
      </CardContent>
    </Card>
  )
}

export interface StripBar {
  key: string
  /** Where/when — the secondary line of the hover readout. */
  label: string
  value: number
  /** Optional breakdown under the value (e.g. per-class request counts). */
  detail?: string
}

export function pluralize(count: number, unit: string): string {
  return `${count.toLocaleString(`en-US`)} ${unit}${count === 1 ? `` : `s`}`
}

// EXP-759: the ONE bar strip behind DayBars and MinuteBars. Hand-rolled CSS
// (deliberately no chart library): thin bars from a single baseline, zero
// values as a faint stub, a hover/tap readout above the hovered bar with the
// value leading and the label second, the other bars dimmed while one is
// hovered, a `max` caption so the scale reads at a glance. The `title`
// attribute stays as the no-pointer fallback.
export function BarStrip({
  bars,
  heightClass = `h-16`,
  gapClass = `gap-[3px]`,
  format,
}: {
  bars: StripBar[]
  heightClass?: string
  gapClass?: string
  /** Formats the value for the readout and the max caption. */
  format: (value: number) => string
}) {
  const [hover, setHover] = useState<number | null>(null)
  const max = Math.max(1, ...bars.map((b) => b.value))
  const active = hover !== null ? bars[hover] : undefined
  const n = Math.max(1, bars.length)
  // Keep the readout inside the strip: anchor it to the left edge on the
  // first fifth, the right edge on the last fifth, centred elsewhere.
  const anchor =
    hover === null ? `center` : hover < n / 5 ? `left` : hover > (4 * n) / 5 ? `right` : `center`
  return (
    <div className="relative" onPointerLeave={() => setHover(null)}>
      {/* The readout sits in this row while hovering, so the caption yields. */}
      <div
        className={cn(
          `mb-1 flex h-3 items-center justify-end text-[10px] leading-none text-muted-foreground tabular-nums`,
          hover !== null && `invisible`
        )}
      >
        max {format(max)}
      </div>
      <div
        className={cn(`flex items-end`, heightClass, gapClass)}
        role="img"
        aria-label={bars.map((b) => `${b.label}: ${format(b.value)}`).join(`, `)}
      >
        {bars.map((b, i) => {
          const isActive = hover === i
          const dim = hover !== null && !isActive
          return (
            <div
              key={b.key}
              className="flex h-full flex-1 flex-col justify-end cursor-default"
              title={`${b.label}: ${format(b.value)}`}
              onPointerEnter={() => setHover(i)}
              onPointerDown={() => setHover(i)}
            >
              <div
                className={cn(
                  `rounded-t-[3px] transition-opacity`,
                  b.value > 0 ? `bg-primary` : `bg-muted`,
                  dim && `opacity-40`,
                  isActive && b.value > 0 && `ring-1 ring-foreground/40`
                )}
                style={{
                  height:
                    b.value > 0
                      ? `${Math.max(6, (b.value / max) * 100)}%`
                      : `3px`,
                }}
              />
            </div>
          )
        })}
      </div>
      {active && hover !== null && (
        <div
          className={cn(
            `glass-panel pointer-events-none absolute top-0 z-10 rounded-lg px-2.5 py-1.5 text-xs leading-tight whitespace-nowrap`,
            anchor === `center` && `-translate-x-1/2`,
            anchor === `right` && `-translate-x-full`
          )}
          style={{
            left:
              anchor === `left`
                ? `${(hover / n) * 100}%`
                : anchor === `right`
                  ? `${((hover + 1) / n) * 100}%`
                  : `${((hover + 0.5) / n) * 100}%`,
          }}
        >
          <div className="font-semibold tabular-nums text-foreground">
            {format(active.value)}
          </div>
          <div className="text-muted-foreground">{active.label}</div>
          {active.detail && (
            <div className="mt-0.5 text-muted-foreground">{active.detail}</div>
          )}
        </div>
      )}
    </div>
  )
}

export function localDayKey(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, `0`)}-${String(d.getDate()).padStart(2, `0`)}`
}

export function formatDayLabel(day: string): string {
  const [y, m, d] = day.split(`-`).map(Number)
  const date = new Date(y, (m ?? 1) - 1, d ?? 1)
  if (Number.isNaN(date.getTime())) return day
  return date.toLocaleDateString(undefined, {
    weekday: `short`,
    month: `short`,
    day: `numeric`,
  })
}

// One bar per day over the trailing `days` days (missing days are zero).
// `unit` is the singular noun the readout pluralises ("signup" → "3 signups").
export function DayBars({
  rows,
  days = 30,
  unit = `event`,
}: {
  rows: { day: string; count: number }[]
  days?: number
  unit?: string
}) {
  const byDay = new Map(rows.map((r) => [r.day, r.count]))
  const bars: StripBar[] = []
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date()
    d.setDate(d.getDate() - i)
    const key = localDayKey(d)
    bars.push({
      key,
      label: formatDayLabel(key),
      value: byDay.get(key) ?? 0,
    })
  }
  return (
    <BarStrip
      bars={bars}
      gapClass={days > 60 ? `gap-px` : `gap-[3px]`}
      format={(v) => pluralize(v, unit)}
    />
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

// EXP-759: client platforms from user_client_platforms (web|ios|android|
// desktop|cli), ordered by first use — the first pill is where the user
// started.
export const PLATFORM_LABELS: Record<string, string> = {
  web: `Web`,
  ios: `iOS`,
  android: `Android`,
  desktop: `Desktop`,
  cli: `CLI`,
}

export function platformLabel(platform: string): string {
  return PLATFORM_LABELS[platform] ?? platform
}

export function PlatformPills({
  platforms,
  emptyLabel = `—`,
}: {
  platforms: string[]
  emptyLabel?: string
}) {
  if (platforms.length === 0) {
    return <span className="text-xs text-muted-foreground">{emptyLabel}</span>
  }
  return (
    <div className="flex flex-wrap gap-1">
      {platforms.map((p) => (
        <Pill key={p} className="whitespace-nowrap">
          {platformLabel(p)}
        </Pill>
      ))}
    </div>
  )
}

export function PlanBadge({
  plan,
  compApplied,
}: {
  plan: string
  compApplied?: boolean
}) {
  return (
    <Pill className="capitalize whitespace-nowrap">
      {plan}
      {compApplied ? ` (comp)` : ``}
    </Pill>
  )
}

const EMAIL_STATUS_CLASS: Record<string, string | undefined> = {
  failed: `text-destructive`,
  bounced: `text-destructive`,
  complained: `text-destructive`,
}

export function EmailStatusBadge({ status }: { status: string }) {
  return <Pill className={EMAIL_STATUS_CLASS[status]}>{status}</Pill>
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
