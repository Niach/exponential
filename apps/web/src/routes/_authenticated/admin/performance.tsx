import {
  createFileRoute,
  Link,
  useRouter,
  type ErrorComponentProps,
} from "@tanstack/react-router"
import { useCallback, useEffect, useState } from "react"
import type React from "react"
import { ChevronDown } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Pill } from "@/components/ui/pill"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { cn } from "@/lib/utils"
import {
  DayBars,
  EmailStatusBadge,
  formatRelative,
  pluralize,
  StatCard,
  type StatTone,
} from "./-shared"
import {
  formatBytes,
  formatCount,
  formatMs,
  formatUptime,
  Meter,
  MinuteBars,
} from "./-perf-shared"

// Admin console → Performance (EXP-553): live process/Electric/database/relay
// insight plus windowed notification+email aggregates. In-memory series reset
// on deploy; the header says so. EXP-759 layout: a health strip (six numbers
// with warn/bad tones) and the minute graphs come first; every table and
// counter block sits behind a per-section Details toggle.

type WindowDays = 7 | 30 | 90

const WINDOWS: WindowDays[] = [7, 30, 90]

const RUNTIME_POLL_MS = 5_000
const RELAYS_POLL_MS = 15_000
const DATABASE_POLL_MS = 30_000
const NOTIF_EMAIL_POLL_MS = 60_000

export const Route = createFileRoute(`/_authenticated/admin/performance`)({
  // `days` stays OPTIONAL so plain links (the admin nav) don't carry it. The
  // window only scopes the Notifications & email section.
  validateSearch: (
    search: Record<string, unknown>
  ): { days?: WindowDays } => ({
    days: WINDOWS.includes(search.days as WindowDays)
      ? (search.days as WindowDays)
      : undefined,
  }),
  loaderDeps: ({ search }) => ({ days: search.days ?? (30 as const) }),
  loader: async ({ deps }) => {
    const [runtime, relays, database, notificationsEmail] = await Promise.all([
      trpc.adminPerformance.runtime.query(),
      trpc.adminPerformance.relays.query(),
      trpc.adminPerformance.database.query(),
      trpc.adminPerformance.notificationsEmail.query({ days: deps.days }),
    ])
    return { runtime, relays, database, notificationsEmail }
  },
  component: AdminPerformance,
  // Without a route-level boundary a failing loader escapes to the router's
  // global fallback, which replaces the ENTIRE app — on the forced dark theme
  // that reads as a black screen with no way back (EXP-373). Keep the failure
  // inside the admin shell so the nav survives and the reason is readable.
  errorComponent: PerformanceError,
})

function PerformanceError({ error }: ErrorComponentProps) {
  const router = useRouter()
  return (
    <div className="mx-auto max-w-6xl space-y-4 p-4 md:p-6">
      <h1 className="text-2xl font-bold">Performance</h1>
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">
            Couldn’t load performance data
          </CardTitle>
          <CardDescription className="text-xs">
            {error instanceof Error ? error.message : String(error)}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button
            size="sm"
            variant="secondary"
            onClick={() => void router.invalidate()}
          >
            Retry
          </Button>
        </CardContent>
      </Card>
    </div>
  )
}

// ── Polling ───────────────────────────────────────────────────────────────────

/** Poll `fetcher` on an interval, seeded (and re-seeded on loader refresh)
 * from `initial`. On failure the last good data stays up and `staleSince`
 * marks when refreshing started failing; polling pauses while the tab is
 * hidden. */
function usePolled<T>(
  initial: T,
  intervalMs: number,
  fetcher: () => Promise<T>
): { data: T; staleSince: number | null } {
  const [data, setData] = useState(initial)
  const [staleSince, setStaleSince] = useState<number | null>(null)
  useEffect(() => {
    setData(initial)
    setStaleSince(null)
  }, [initial])
  useEffect(() => {
    let cancelled = false
    const id = setInterval(() => {
      if (document.hidden) return
      fetcher()
        .then((next) => {
          if (cancelled) return
          setData(next)
          setStaleSince(null)
        })
        .catch(() => {
          if (cancelled) return
          setStaleSince((prev) => prev ?? Date.now())
        })
    }, intervalMs)
    return () => {
      cancelled = true
      clearInterval(id)
    }
  }, [intervalMs, fetcher])
  return { data, staleSince }
}

function StaleNote({ staleSince }: { staleSince: number | null }) {
  if (!staleSince) return null
  return (
    <p className="text-xs text-destructive">
      Refresh failing — showing data from {formatRelative(new Date(staleSince))}
      .
    </p>
  )
}

function pct(part: number, whole: number): string | undefined {
  if (whole <= 0) return undefined
  return `${((part / whole) * 100).toFixed(1)}%`
}

// ── Page ──────────────────────────────────────────────────────────────────────

function AdminPerformance() {
  const loaded = Route.useLoaderData()
  const days = Route.useSearch().days ?? 30

  const runtime = usePolled(
    loaded.runtime,
    RUNTIME_POLL_MS,
    useCallback(() => trpc.adminPerformance.runtime.query(), [])
  )
  const relays = usePolled(
    loaded.relays,
    RELAYS_POLL_MS,
    useCallback(() => trpc.adminPerformance.relays.query(), [])
  )
  const database = usePolled(
    loaded.database,
    DATABASE_POLL_MS,
    useCallback(() => trpc.adminPerformance.database.query(), [])
  )
  const notif = usePolled(
    loaded.notificationsEmail,
    NOTIF_EMAIL_POLL_MS,
    useCallback(
      () => trpc.adminPerformance.notificationsEmail.query({ days }),
      [days]
    )
  )

  return (
    <div className="mx-auto max-w-6xl space-y-6 p-4 md:p-6">
      <div>
        <h1 className="text-2xl font-bold">Performance</h1>
        <p className="text-sm text-muted-foreground">
          Live process, Electric-sync and relay metrics are in-memory and reset
          on deploy; database and notification stats are persistent. Sections
          refresh on their own cadence (5–60s). Hover a bar for its value.
        </p>
      </div>

      <HealthStrip
        runtime={runtime.data}
        database={database.data}
        relays={relays.data}
        staleSince={runtime.staleSince ?? database.staleSince ?? relays.staleSince}
      />
      <GraphsRow metrics={runtime.data.metrics} />

      <ServerSection runtime={runtime.data} staleSince={runtime.staleSince} />
      <ElectricSection metrics={runtime.data.metrics} />
      <DatabaseSection
        database={database.data}
        staleSince={database.staleSince}
      />
      <RelaysSection relays={relays.data} staleSince={relays.staleSince} />
      <NotificationsEmailSection
        data={notif.data}
        days={days}
        staleSince={notif.staleSince}
      />
    </div>
  )
}

// ── Details toggle ────────────────────────────────────────────────────────────

const DETAILS_STORAGE_PREFIX = `admin.perf.details.`

function readDetailsOpen(id: string): boolean {
  try {
    return localStorage.getItem(`${DETAILS_STORAGE_PREFIX}${id}`) === `1`
  } catch {
    return false
  }
}

/** Collapsed-by-default section body (EXP-759). Open state is remembered per
 * section in localStorage so an admin who lives in one table keeps it. */
function Details({
  id,
  children,
}: {
  id: string
  children: React.ReactNode
}) {
  const [open, setOpen] = useState(() => readDetailsOpen(id))
  const toggle = (next: boolean) => {
    setOpen(next)
    try {
      localStorage.setItem(`${DETAILS_STORAGE_PREFIX}${id}`, next ? `1` : `0`)
    } catch {
      // private mode etc. — the toggle still works for this page load
    }
  }
  return (
    <Collapsible open={open} onOpenChange={toggle} className="space-y-3">
      <CollapsibleTrigger asChild>
        <Button variant="ghost" size="sm" className="-ml-2 text-muted-foreground">
          <ChevronDown
            className={cn(`h-4 w-4 transition-transform`, open && `rotate-180`)}
          />
          {open ? `Hide details` : `Details`}
        </Button>
      </CollapsibleTrigger>
      <CollapsibleContent className="space-y-3">{children}</CollapsibleContent>
    </Collapsible>
  )
}

// ── Health strip ──────────────────────────────────────────────────────────────

type RuntimeData = Awaited<
  ReturnType<typeof trpc.adminPerformance.runtime.query>
>
type DatabaseData = Awaited<
  ReturnType<typeof trpc.adminPerformance.database.query>
>
type RelaysData = Awaited<
  ReturnType<typeof trpc.adminPerformance.relays.query>
>

function worst(...tones: StatTone[]): StatTone {
  if (tones.includes(`bad`)) return `bad`
  if (tones.includes(`warn`)) return `warn`
  return `ok`
}

function HealthStrip({
  runtime,
  database,
  relays,
  staleSince,
}: {
  runtime: RuntimeData
  database: DatabaseData
  relays: RelaysData
  staleSince: number | null
}) {
  const { metrics, process: proc } = runtime
  const lag = metrics.eventLoop
  const lagTone: StatTone =
    lag.max60mMs > 1000 ? `bad` : lag.max60mMs > 250 ? `warn` : `ok`

  const last5m = metrics.requests.reduce((sum, r) => sum + r.last5m, 0)
  const nowMinute = new Date().toISOString().slice(0, 16)
  const thisMinute = metrics.requests.reduce(
    (sum, r) => sum + (r.series.find((m) => m.minute === nowMinute)?.count ?? 0),
    0
  )

  const electricErrors60m = metrics.electric.upstream.errorSeries.reduce(
    (sum, m) => sum + m.count,
    0
  )
  const failedSweeps = metrics.schedulers.filter((s) => !s.lastOk)
  const errorTone: StatTone =
    failedSweeps.length > 0 || electricErrors60m > 0 ? `bad` : `ok`

  const { proxy, upstream } = metrics.electric
  const queueTone: StatTone =
    proxy.queued > 0 ? `warn` : proxy.active >= proxy.capacity ? `warn` : `ok`

  const dbTone = worst(
    database.cacheHitPct < 95 ? `warn` : `ok`,
    database.pool.waiting > 0 ? `warn` : `ok`,
    database.longRunningQueries > 0 ? `bad` : `ok`
  )
  const busy = database.pool.total - database.pool.idle

  const relayRows = [
    { name: `steer`, probe: relays.steer },
    { name: `push`, probe: relays.push },
  ]
  const configured = relayRows.filter((r) => r.probe.configured)
  const online = configured.filter((r) => r.probe.ok)
  const relayTone: StatTone =
    configured.length > online.length ? `bad` : `ok`
  const relayValue =
    configured.length === 0
      ? `none`
      : `${online.length} / ${configured.length} online`
  const relayHint =
    configured.length === 0
      ? `no relay configured`
      : configured
          .map((r) =>
            r.probe.ok
              ? `${r.name} ${r.probe.latencyMs ?? `?`} ms`
              : `${r.name} unreachable`
          )
          .join(` · `)

  return (
    <section className="space-y-3">
      <StaleNote staleSince={staleSince} />
      <div className="grid grid-cols-2 gap-3 md:grid-cols-3 lg:grid-cols-6">
        <StatCard
          label="Event-loop lag"
          value={formatMs(lag.lastMs)}
          hint={`max 60m ${formatMs(lag.max60mMs)} · up ${formatUptime(proc.uptimeSeconds)}`}
          tone={lagTone}
        />
        <StatCard
          label="Requests (5m)"
          value={formatCount(last5m)}
          hint={`${formatCount(thisMinute)} this minute`}
        />
        <StatCard
          label="Errors (60m)"
          value={formatCount(electricErrors60m + failedSweeps.length)}
          hint={
            failedSweeps.length > 0
              ? `${failedSweeps.length} failed sweep${failedSweeps.length === 1 ? `` : `s`}: ${failedSweeps.map((s) => SCHEDULER_LABELS[s.name] ?? s.name).join(`, `)}`
              : `Electric 5xx ${formatCount(electricErrors60m)} · sweeps ok`
          }
          tone={errorTone}
        />
        <StatCard
          label="Electric snapshots"
          value={`${formatCount(proxy.active)} / ${formatCount(proxy.capacity)}`}
          hint={`${formatCount(proxy.queued)} queued · avg ${formatMs(upstream.snapshotAvgMs)}`}
          tone={queueTone}
        />
        <StatCard
          label="Database"
          value={`${database.cacheHitPct.toFixed(1)}% hit`}
          hint={`pool ${busy}/${database.pool.total}${database.pool.waiting > 0 ? ` (${database.pool.waiting} waiting)` : ``} · ${formatCount(database.longRunningQueries)} slow`}
          tone={dbTone}
        />
        <StatCard
          label="Relays"
          value={relayValue}
          hint={relayHint}
          tone={relayTone}
        />
      </div>
    </section>
  )
}

// ── Graphs ────────────────────────────────────────────────────────────────────

const REQUEST_CLASS_LABELS: Record<string, string> = {
  "shape-live": `Shape long-polls`,
  "shape-snapshot": `Shape snapshots`,
  trpc: `tRPC`,
  "api-other": `Other API`,
  asset: `Assets`,
  app: `App navigations`,
}

const SCHEDULER_LABELS: Record<string, string> = {
  "email-digest": `Email digest`,
  "board-trash": `Board trash purge`,
  "coding-session-sweep": `Coding session sweep`,
  "session-attachment-sweep": `Steer image reclaim`,
  "fcm-token-sweep": `FCM token sweep`,
  "device-code-sweep": `Device code sweep`,
}

function GraphCard({
  title,
  description,
  children,
}: {
  title: string
  description?: string
  children: React.ReactNode
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">{title}</CardTitle>
        {description && (
          <CardDescription className="text-xs">{description}</CardDescription>
        )}
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  )
}

function GraphsRow({ metrics }: { metrics: RuntimeData[`metrics`] }) {
  // One total strip; the readout carries the per-class split for that minute.
  const totalByMinute = new Map<string, number>()
  const byMinuteClass = new Map<string, string[]>()
  for (const r of metrics.requests) {
    for (const m of r.series) {
      totalByMinute.set(m.minute, (totalByMinute.get(m.minute) ?? 0) + m.count)
      if (m.count > 0) {
        const parts = byMinuteClass.get(m.minute) ?? []
        parts.push(`${REQUEST_CLASS_LABELS[r.cls] ?? r.cls} ${formatCount(m.count)}`)
        byMinuteClass.set(m.minute, parts)
      }
    }
  }
  return (
    <div className="grid gap-3 md:grid-cols-3">
      <GraphCard
        title="Requests per minute"
        description="Trailing 60 minutes, all classes. Empty under `vite dev`."
      >
        <MinuteBars
          rows={[...totalByMinute.entries()].map(([minute, value]) => ({ minute, value }))}
          format={(v) => pluralize(v, `request`)}
          detail={(minute) => byMinuteClass.get(minute)?.join(` · `)}
        />
      </GraphCard>
      <GraphCard
        title="Event-loop lag"
        description="Worst lag sampled in each minute. Sustained spikes mean the process is CPU-bound."
      >
        <MinuteBars
          rows={metrics.eventLoop.series.map((m) => ({ minute: m.minute, value: m.maxMs }))}
          format={formatMs}
        />
      </GraphCard>
      <GraphCard
        title="Electric errors per minute"
        description="Upstream 5xx responses from Electric (499 client hang-ups are not errors)."
      >
        <MinuteBars
          rows={metrics.electric.upstream.errorSeries.map((m) => ({ minute: m.minute, value: m.count }))}
          format={(v) => pluralize(v, `error`)}
        />
      </GraphCard>
    </div>
  )
}

// ── Server ────────────────────────────────────────────────────────────────────

function ServerSection({
  runtime,
  staleSince,
}: {
  runtime: RuntimeData
  staleSince: number | null
}) {
  const { process: proc, metrics } = runtime
  // Fixed roster joined against what actually reported, plus anything extra
  // the server knows that this build doesn't.
  const reported = new Map(metrics.schedulers.map((s) => [s.name, s]))
  const schedulerNames = [
    ...Object.keys(SCHEDULER_LABELS),
    ...metrics.schedulers
      .map((s) => s.name)
      .filter((name) => !(name in SCHEDULER_LABELS)),
  ]
  return (
    <section className="space-y-3">
      <div>
        <h2 className="text-lg font-semibold">Server</h2>
        <p className="text-xs text-muted-foreground">
          Up {formatUptime(proc.uptimeSeconds)}
          {proc.bunVersion ? ` on Bun ${proc.bunVersion}` : ``} · RSS{` `}
          {formatBytes(proc.rssBytes)} (heap {formatBytes(proc.heapUsedBytes)} /{` `}
          {formatBytes(proc.heapTotalBytes)}) · CPU {proc.cpuPercent.toFixed(1)}% of one
          core
        </p>
      </div>
      <StaleNote staleSince={staleSince} />
      <Details id="server">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Requests by class</CardTitle>
            <CardDescription className="text-xs">
              Trailing 60 minutes. Shape long-poll durations include the ~20s
              poll window by design.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="rounded-md border overflow-x-auto">
              <div className="min-w-[520px]">
                <div className="grid grid-cols-[1fr_80px_80px_90px_90px] items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground">
                  <div>Class</div>
                  <div className="text-right">5m</div>
                  <div className="text-right">60m</div>
                  <div className="text-right">avg</div>
                  <div className="text-right">max</div>
                </div>
                {metrics.requests.map((r) => (
                  <div
                    key={r.cls}
                    className="grid grid-cols-[1fr_80px_80px_90px_90px] items-center gap-3 border-b px-3 py-2 text-xs last:border-b-0"
                  >
                    <div>{REQUEST_CLASS_LABELS[r.cls] ?? r.cls}</div>
                    <div className="text-right tabular-nums">
                      {formatCount(r.last5m)}
                    </div>
                    <div className="text-right tabular-nums">
                      {formatCount(r.last60m)}
                    </div>
                    <div className="text-right tabular-nums">
                      {formatMs(r.avgMs)}
                    </div>
                    <div className="text-right tabular-nums">
                      {formatMs(r.maxMs)}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Schedulers</CardTitle>
            <CardDescription className="text-xs">
              In-process sweeps started by the production server entry. “Never
              ran” is normal in dev and right after a deploy.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="rounded-md border overflow-x-auto">
              <div className="min-w-[640px]">
                <div className="grid grid-cols-[1fr_110px_80px_70px_1fr_90px] items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground">
                  <div>Sweep</div>
                  <div>Last run</div>
                  <div className="text-right">Duration</div>
                  <div>Status</div>
                  <div>Detail</div>
                  <div className="text-right">Runs / fails</div>
                </div>
                {schedulerNames.map((name) => {
                  const run = reported.get(name)
                  return (
                    <div
                      key={name}
                      className="grid grid-cols-[1fr_110px_80px_70px_1fr_90px] items-center gap-3 border-b px-3 py-2 text-xs last:border-b-0"
                    >
                      <div>{SCHEDULER_LABELS[name] ?? name}</div>
                      <div className="text-muted-foreground">
                        {run ? formatRelative(run.lastRunAt) : `never ran`}
                      </div>
                      <div className="text-right tabular-nums">
                        {run ? formatMs(run.lastDurationMs) : `—`}
                      </div>
                      <div>
                        {run ? (
                          <Pill
                            className={
                              run.lastOk ? undefined : `text-destructive`
                            }
                          >
                            {run.lastOk ? `ok` : `failed`}
                          </Pill>
                        ) : (
                          `—`
                        )}
                      </div>
                      <div
                        className="truncate text-muted-foreground"
                        title={run?.lastError ?? run?.lastDetail ?? undefined}
                      >
                        {run?.lastError ?? run?.lastDetail ?? `—`}
                      </div>
                      <div className="text-right tabular-nums">
                        {run ? `${run.runs} / ${run.failures}` : `—`}
                      </div>
                    </div>
                  )
                })}
              </div>
            </div>
          </CardContent>
        </Card>
      </Details>
    </section>
  )
}

// ── Electric sync ─────────────────────────────────────────────────────────────

function ElectricSection({ metrics }: { metrics: RuntimeData[`metrics`] }) {
  const { proxy, upstream, perTable } = metrics.electric
  const gzipSavings =
    upstream.rawBytes > 0
      ? pct(upstream.rawBytes - upstream.sentBytes, upstream.rawBytes)
      : undefined
  return (
    <section className="space-y-3">
      <div>
        <h2 className="text-lg font-semibold">Electric sync</h2>
        <p className="text-xs text-muted-foreground">
          Since process start: {formatCount(upstream.status2xx)} 2xx ·{` `}
          {formatCount(upstream.status4xx)} 4xx ·{` `}
          <span className={upstream.status5xx > 0 ? `text-destructive` : ``}>
            {formatCount(upstream.status5xx)} 5xx
          </span>
          {` `}· {formatCount(upstream.aborted499)} client hang-ups ·{` `}
          {formatBytes(upstream.sentBytes)} sent
          {gzipSavings ? ` (${gzipSavings} saved by gzip)` : ``}
        </p>
      </div>
      <Details id="electric">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Snapshot semaphore</CardTitle>
            <CardDescription className="text-xs">
              Concurrently-proxied snapshot-class requests (live long-polls are
              never gated). Sustained queueing means clients are waiting on
              cold starts.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <Meter
              label={`Active slots${proxy.queued > 0 ? ` (${proxy.queued} queued)` : ``}`}
              value={proxy.active}
              max={proxy.capacity}
              warn={proxy.queued > 0}
            />
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
              <StatCard
                label="High-water active"
                value={formatCount(proxy.activeHighWater)}
                hint={`of ${proxy.capacity}`}
              />
              <StatCard
                label="High-water queued"
                value={formatCount(proxy.queuedHighWater)}
              />
              <StatCard
                label="Queue waits (60m)"
                value={formatCount(proxy.queueWaits.count)}
                hint={
                  proxy.queueWaits.count > 0
                    ? `avg ${formatMs(proxy.queueWaits.avgMs)}, max ${formatMs(proxy.queueWaits.maxMs)}`
                    : undefined
                }
              />
              <StatCard
                label="Snapshot latency (60m)"
                value={formatMs(upstream.snapshotAvgMs)}
                hint={`max ${formatMs(upstream.snapshotMaxMs)} · ${formatCount(upstream.snapshotCount60m)} req`}
              />
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Requests per shape</CardTitle>
            <CardDescription className="text-xs">
              Since process start, busiest first. Raw bytes{` `}
              {formatBytes(upstream.rawBytes)}, gzipped responses{` `}
              {formatCount(upstream.gzippedResponses)}.
            </CardDescription>
          </CardHeader>
          <CardContent>
            {perTable.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                No shape traffic yet.
              </p>
            ) : (
              <div className="rounded-md border overflow-x-auto">
                <div className="min-w-[400px]">
                  <div className="grid grid-cols-[1fr_110px_110px] items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground">
                    <div>Table</div>
                    <div className="text-right">Long-polls</div>
                    <div className="text-right">Snapshots</div>
                  </div>
                  {perTable.map((t) => (
                    <div
                      key={t.table}
                      className="grid grid-cols-[1fr_110px_110px] items-center gap-3 border-b px-3 py-2 text-xs last:border-b-0"
                    >
                      <div className="truncate">{t.table}</div>
                      <div className="text-right tabular-nums">
                        {formatCount(t.live)}
                      </div>
                      <div className="text-right tabular-nums">
                        {formatCount(t.snapshot)}
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      </Details>
    </section>
  )
}

// ── Database ──────────────────────────────────────────────────────────────────

function DatabaseSection({
  database,
  staleSince,
}: {
  database: DatabaseData
  staleSince: number | null
}) {
  return (
    <section className="space-y-3">
      <div>
        <h2 className="text-lg font-semibold">Database</h2>
        <p className="text-xs text-muted-foreground">
          {formatBytes(database.dbSizeBytes)} · {formatCount(database.backends)}{` `}
          backends ({database.connectionsByState
            .map((c) => `${c.count} ${c.state}`)
            .join(`, `)}) · {formatCount(database.xactCommit)} commits /{` `}
          {formatCount(database.xactRollback)} rollbacks ·{` `}
          {formatCount(database.deadlocks)} deadlocks · {formatCount(database.tempFiles)}{` `}
          temp files
        </p>
      </div>
      <StaleNote staleSince={staleSince} />
      <Details id="database">
        <Card>
          <CardContent className="space-y-3 pt-4">
            <Meter
              label="Cache hit ratio (since stats reset)"
              value={database.cacheHitPct}
              max={100}
              display={`${database.cacheHitPct.toFixed(2)}%`}
              warn={database.cacheHitPct < 95}
            />
            <Meter
              label="Pool (this web process)"
              value={database.pool.total - database.pool.idle}
              max={Math.max(database.pool.total, 1)}
              display={`${database.pool.total - database.pool.idle} busy / ${database.pool.total} open${database.pool.waiting > 0 ? ` · ${database.pool.waiting} waiting` : ``}`}
              warn={database.pool.waiting > 0}
            />
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Largest tables</CardTitle>
            <CardDescription className="text-xs">
              Total relation size (data + indexes + toast). Heavy seq scans on a
              big table usually mean a missing index.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="rounded-md border overflow-x-auto">
              <div className="min-w-[640px]">
                <div className="grid grid-cols-[1fr_90px_100px_100px_100px_100px] items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground">
                  <div>Table</div>
                  <div className="text-right">Size</div>
                  <div className="text-right">Rows</div>
                  <div className="text-right">Dead rows</div>
                  <div className="text-right">Seq scans</div>
                  <div className="text-right">Idx scans</div>
                </div>
                {database.topTables.map((t) => (
                  <div
                    key={t.table}
                    className="grid grid-cols-[1fr_90px_100px_100px_100px_100px] items-center gap-3 border-b px-3 py-2 text-xs last:border-b-0"
                  >
                    <div className="truncate">{t.table}</div>
                    <div className="text-right tabular-nums">
                      {formatBytes(t.totalBytes)}
                    </div>
                    <div className="text-right tabular-nums">
                      {formatCount(t.liveRows)}
                    </div>
                    <div className="text-right tabular-nums">
                      {formatCount(t.deadRows)}
                    </div>
                    <div className="text-right tabular-nums">
                      {formatCount(t.seqScans)}
                    </div>
                    <div className="text-right tabular-nums">
                      {formatCount(t.idxScans)}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </CardContent>
        </Card>
      </Details>
    </section>
  )
}

// ── Relays ────────────────────────────────────────────────────────────────────

function RelayStatusBadge({
  ok,
  latencyMs,
}: {
  ok?: boolean
  latencyMs?: number
}) {
  return (
    <Pill className={ok ? undefined : `text-destructive`}>
      {ok ? `online${latencyMs !== undefined ? ` · ${latencyMs} ms` : ``}` : `unreachable`}
    </Pill>
  )
}

function CounterRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between text-xs">
      <span className="text-muted-foreground">{label}</span>
      <span className="tabular-nums">{value}</span>
    </div>
  )
}

function RelaysSection({
  relays,
  staleSince,
}: {
  relays: RelaysData
  staleSince: number | null
}) {
  const { steer, push } = relays
  return (
    <section className="space-y-3">
      <div>
        <h2 className="text-lg font-semibold">Relays</h2>
        <p className="text-xs text-muted-foreground">
          Steer relay:{` `}
          {!steer.configured
            ? `not configured`
            : steer.ok
              ? `online · ${steer.latencyMs ?? `?`} ms · ${formatCount(steer.connections ?? 0)} connections, ${formatCount(steer.devices ?? 0)} devices, ${formatCount(steer.rooms ?? 0)} rooms`
              : `unreachable`}
          {` `}· Push relay:{` `}
          {!push.configured
            ? `not configured`
            : push.ok
              ? `online · ${push.latencyMs ?? `?`} ms${push.firebaseConfigured === false ? ` · FIREBASE NOT CONFIGURED` : ``}`
              : `unreachable`}
        </p>
      </div>
      <StaleNote staleSince={staleSince} />
      <Details id="relays">
        <div className="grid gap-3 md:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center justify-between text-sm">
                Steer relay
                {steer.configured && (
                  <RelayStatusBadge ok={steer.ok} latencyMs={steer.latencyMs} />
                )}
              </CardTitle>
              <CardDescription className="text-xs">
                Remote start + live steer hub. Gauges are current; counters are
                since relay start.
              </CardDescription>
            </CardHeader>
            <CardContent>
              {!steer.configured ? (
                <p className="text-sm text-muted-foreground">
                  Not configured (STEER_RELAY_URL / STEER_RELAY_SECRET unset).
                </p>
              ) : !steer.ok ? (
                <p className="text-sm text-muted-foreground">
                  Probe failed — relay down or unreachable from this server.
                </p>
              ) : (
                <div className="space-y-3">
                  <div className="grid grid-cols-3 gap-3">
                    <StatCard
                      label="Connections"
                      value={formatCount(steer.connections ?? 0)}
                    />
                    <StatCard
                      label="Devices"
                      value={formatCount(steer.devices ?? 0)}
                    />
                    <StatCard
                      label="Rooms"
                      value={formatCount(steer.rooms ?? 0)}
                    />
                  </div>
                  {steer.counters ? (
                    <div className="space-y-1">
                      <CounterRow
                        label="Connections accepted"
                        value={formatCount(steer.counters.connectionsAccepted)}
                      />
                      <CounterRow
                        label="Activity frames fanned"
                        value={formatCount(steer.counters.activityFramesFanned)}
                      />
                      <CounterRow
                        label="Remote starts routed"
                        value={formatCount(steer.counters.startsRouted)}
                      />
                      <CounterRow
                        label="Slow-consumer evictions"
                        value={formatCount(steer.counters.slowConsumerEvictions)}
                      />
                      <CounterRow
                        label="Rate-limited rejections"
                        value={formatCount(steer.counters.rateLimitedRejections)}
                      />
                      {steer.startedAt ? (
                        <CounterRow
                          label="Relay up since"
                          value={formatRelative(new Date(steer.startedAt))}
                        />
                      ) : null}
                    </div>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      Counters n/a — the deployed relay predates /stats.
                    </p>
                  )}
                </div>
              )}
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center justify-between text-sm">
                Push relay
                {push.configured && (
                  <RelayStatusBadge ok={push.ok} latencyMs={push.latencyMs} />
                )}
              </CardTitle>
              <CardDescription className="text-xs">
                FCM fan-out. Counters are since relay start.
              </CardDescription>
            </CardHeader>
            <CardContent>
              {!push.configured ? (
                <p className="text-sm text-muted-foreground">
                  Not configured (PUSH_RELAY_URL unset).
                </p>
              ) : !push.ok ? (
                <p className="text-sm text-muted-foreground">
                  Probe failed — relay down or unreachable from this server.
                </p>
              ) : (
                <div className="space-y-3">
                  {push.firebaseConfigured === false && (
                    <p className="text-xs text-destructive">
                      Firebase is not configured on the relay — pushes are being
                      dropped.
                    </p>
                  )}
                  {push.stats ? (
                    <div className="space-y-1">
                      <CounterRow
                        label="Send requests (ok / failed)"
                        value={`${formatCount(push.stats.sendOk)} / ${formatCount(push.stats.sendFailed)}`}
                      />
                      <CounterRow
                        label="FCM deadline timeouts"
                        value={formatCount(push.stats.deadlineTimeouts)}
                      />
                      <CounterRow
                        label="Tokens (ok / failed)"
                        value={`${formatCount(push.stats.tokensOk)} / ${formatCount(push.stats.tokensFailed)}`}
                      />
                      <CounterRow
                        label="Invalid tokens pruned"
                        value={formatCount(push.stats.invalidTokens)}
                      />
                      {push.stats.lastError && (
                        <CounterRow
                          label="Last error"
                          value={`${push.stats.lastError}${push.stats.lastErrorAt ? ` (${formatRelative(new Date(push.stats.lastErrorAt))})` : ``}`}
                        />
                      )}
                      {push.startedAt ? (
                        <CounterRow
                          label="Relay up since"
                          value={formatRelative(new Date(push.startedAt))}
                        />
                      ) : null}
                    </div>
                  ) : (
                    <p className="text-xs text-muted-foreground">
                      Counters n/a — the deployed relay predates /stats.
                    </p>
                  )}
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </Details>
    </section>
  )
}

// ── Notifications & email ─────────────────────────────────────────────────────

type NotifData = Awaited<
  ReturnType<typeof trpc.adminPerformance.notificationsEmail.query>
>

function sumByDay(
  rows: { day: string; count: number }[]
): { day: string; count: number }[] {
  const byDay = new Map<string, number>()
  for (const row of rows) {
    byDay.set(row.day, (byDay.get(row.day) ?? 0) + row.count)
  }
  return [...byDay.entries()].map(([day, count]) => ({ day, count }))
}

function NotificationsEmailSection({
  data,
  days,
  staleSince,
}: {
  data: NotifData
  days: WindowDays
  staleSince: number | null
}) {
  const { totals } = data
  const notificationsByDay = sumByDay(data.byDayType)
  const byType = new Map<string, number>()
  for (const row of data.byDayType) {
    byType.set(row.type, (byType.get(row.type) ?? 0) + row.count)
  }
  const typeRows = [...byType.entries()].sort((a, b) => b[1] - a[1])
  const backlogAgeMs = data.digestBacklog.oldestCreatedAt
    ? Date.now() - new Date(data.digestBacklog.oldestCreatedAt).getTime()
    : 0
  const backlogTone: StatTone =
    backlogAgeMs > 3 * 86_400_000 ? `bad` : backlogAgeMs > 86_400_000 ? `warn` : `ok`
  const failedEmails = data.emailByStatus
    .filter((row) => [`failed`, `bounced`, `complained`].includes(row.status))
    .reduce((sum, row) => sum + row.count, 0)
  return (
    <section className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-lg font-semibold">Notifications &amp; email</h2>
        <div className="flex items-center gap-1">
          {WINDOWS.map((w) => (
            <Button
              key={w}
              asChild
              variant={w === days ? `secondary` : `ghost`}
              size="sm"
            >
              <Link to="/admin/performance" search={{ days: w }}>
                {w}d
              </Link>
            </Button>
          ))}
        </div>
      </div>
      <StaleNote staleSince={staleSince} />
      <div className="grid grid-cols-2 gap-3 md:grid-cols-5">
        <StatCard
          label={`Notifications (${days}d)`}
          value={formatCount(totals.total)}
        />
        <StatCard
          label="Pushed"
          value={formatCount(totals.pushed)}
          hint={pct(totals.pushed, totals.total)}
        />
        <StatCard
          label="Emailed (digest)"
          value={formatCount(totals.emailed)}
          hint={pct(totals.emailed, totals.total)}
        />
        <StatCard
          label="Email failures"
          value={formatCount(failedEmails)}
          hint={`${formatCount(data.bounces.total)} bounced addresses (${formatCount(data.bounces.suppressed)} suppressed)`}
          tone={failedEmails > 0 ? `warn` : `ok`}
        />
        <StatCard
          label="Digest backlog"
          value={formatCount(data.digestBacklog.count)}
          hint={
            data.digestBacklog.oldestCreatedAt
              ? `oldest ${formatRelative(data.digestBacklog.oldestCreatedAt)}`
              : `unread, not yet emailed`
          }
          tone={backlogTone}
        />
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <GraphCard title={`Notifications per day (last ${days} days)`}>
          <DayBars rows={notificationsByDay} days={days} unit="notification" />
        </GraphCard>
        <GraphCard
          title={`Emails per day (last ${days} days)`}
          description="email_deliveries rows — digests, invites, support mail."
        >
          <DayBars rows={data.emailByDay} days={days} unit="email" />
        </GraphCard>
      </div>

      <Details id="notifications">
        <div className="grid gap-3 md:grid-cols-3">
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Notifications by type</CardTitle>
            </CardHeader>
            <CardContent>
              {typeRows.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  None in this window.
                </p>
              ) : (
                <div className="space-y-1">
                  {typeRows.map(([type, count]) => (
                    <CounterRow
                      key={type}
                      label={type}
                      value={formatCount(count)}
                    />
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Email by status</CardTitle>
            </CardHeader>
            <CardContent>
              {data.emailByStatus.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  None in this window.
                </p>
              ) : (
                <div className="space-y-1.5">
                  {data.emailByStatus.map((row) => (
                    <div
                      key={row.status}
                      className="flex items-center justify-between text-xs"
                    >
                      <EmailStatusBadge status={row.status} />
                      <span className="tabular-nums">
                        {formatCount(row.count)}
                      </span>
                    </div>
                  ))}
                  <CounterRow
                    label={`Bounced addresses (${days}d)`}
                    value={`${formatCount(data.bounces.total)} (${formatCount(data.bounces.suppressed)} suppressed, ${formatCount(data.bounces.complaints)} complaints)`}
                  />
                  <CounterRow
                    label="Read"
                    value={`${formatCount(totals.read)}${pct(totals.read, totals.total) ? ` (${pct(totals.read, totals.total)})` : ``}`}
                  />
                </div>
              )}
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Email by kind</CardTitle>
            </CardHeader>
            <CardContent>
              {data.emailByKind.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  None in this window.
                </p>
              ) : (
                <div className="space-y-1">
                  {data.emailByKind.map((row) => (
                    <CounterRow
                      key={row.kind}
                      label={row.kind}
                      value={formatCount(row.count)}
                    />
                  ))}
                </div>
              )}
            </CardContent>
          </Card>
        </div>
      </Details>
    </section>
  )
}
