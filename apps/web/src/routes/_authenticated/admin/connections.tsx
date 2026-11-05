import { createFileRoute, Link } from "@tanstack/react-router"
import { Button, ListEmpty, Pill } from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { pageTitle } from "@/lib/page-title"
import {
  AdminCard,
  AdminLoadFailure,
  platformLabel,
  StatCard,
  type StatTone,
} from "./-shared"
import { formatCount, formatMs } from "./-perf-shared"

// EXP-1530: P2P connection analytics (fixtures/steer-enet.json `connStats`):
// how session feeds open per platform and version, over eNet or the sealed
// relay fallback. Admin-only, web-only, works self-hosted.

type ConnWindow = `24h` | `7d` | `30d`
const WINDOWS: ConnWindow[] = [`24h`, `7d`, `30d`]

// The contract's targets: success >= 99 %, p95 first frame < 2 s.
const SUCCESS_TARGET = 0.99
const P95_TARGET_MS = 2_000

export const Route = createFileRoute(`/_authenticated/admin/connections`)({
  head: () => ({ meta: [{ title: pageTitle(`Connections`, `Admin`) }] }),
  validateSearch: (search: Record<string, unknown>): { window?: ConnWindow } => ({
    window: WINDOWS.includes(search.window as ConnWindow)
      ? (search.window as ConnWindow)
      : undefined,
  }),
  loaderDeps: ({ search }) => ({ window: search.window ?? (`24h` as const) }),
  loader: async ({ deps }) => ({
    stats: await trpc.connStats.dashboard.query({ window: deps.window }),
  }),
  component: AdminConnections,
  errorComponent: () => <AdminLoadFailure heading="Connections" />,
})

type Stats = Awaited<ReturnType<typeof trpc.connStats.dashboard.query>>
type Bucket = Stats[`total`]

function pct(value: number | null): string {
  return value === null ? `—` : `${(value * 100).toFixed(1)}%`
}

function ms(value: number | null): string {
  return value === null ? `—` : formatMs(value)
}

function successTone(b: Bucket): StatTone {
  if (b.successRate === null) return `ok`
  return b.successRate >= SUCCESS_TARGET ? `ok` : b.successRate >= 0.95 ? `warn` : `bad`
}

function p95Tone(b: Bucket): StatTone {
  if (b.p95FirstFrameMs === null) return `ok`
  return b.p95FirstFrameMs < P95_TARGET_MS ? `ok` : `warn`
}

function AdminConnections() {
  const { stats } = Route.useLoaderData()
  const window = Route.useSearch().window ?? `24h`
  const { total } = stats

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <div className="flex flex-wrap items-end justify-between gap-2">
        <h1 className="text-2xl font-bold">Connections</h1>
        <div className="flex items-center gap-1">
          {WINDOWS.map((w) => (
            <Button key={w} asChild variant={w === window ? `secondary` : `ghost`} size="sm">
              <Link to="/admin/connections" search={{ window: w }}>
                {w}
              </Link>
            </Button>
          ))}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3 md:grid-cols-5">
        <StatCard label="Views" value={formatCount(total.views)} hint="session feeds opened" />
        <StatCard
          label="Success"
          value={pct(total.successRate)}
          hint="target ≥ 99%"
          tone={successTone(total)}
        />
        <StatCard label="First frame p50" value={ms(total.p50FirstFrameMs)} />
        <StatCard
          label="First frame p95"
          value={ms(total.p95FirstFrameMs)}
          hint="target < 2 s"
          tone={p95Tone(total)}
        />
        <StatCard label="Relay fallback" value={pct(total.fallbackRate)} hint="views that used the relay" />
      </div>

      <AdminCard title="By platform">
        <BucketTable rows={stats.platforms} showVersion={false} />
      </AdminCard>
      <AdminCard
        title="By version"
        description={stats.capped ? `Row cap reached: the window is truncated.` : undefined}
      >
        <BucketTable rows={stats.versions} showVersion />
      </AdminCard>
    </div>
  )
}

function BucketTable({ rows, showVersion }: { rows: Bucket[]; showVersion: boolean }) {
  if (rows.length === 0) return <ListEmpty>No connections yet</ListEmpty>
  const cols = showVersion
    ? `grid-cols-[90px_minmax(90px,1fr)_70px_80px_80px_80px_80px_minmax(180px,1.5fr)]`
    : `grid-cols-[minmax(90px,1fr)_70px_80px_80px_80px_80px_minmax(180px,1.5fr)]`
  return (
    <div className="rounded-md border overflow-x-auto">
      <div className={showVersion ? `min-w-[820px]` : `min-w-[720px]`}>
        <div
          className={`grid ${cols} items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground`}
        >
          <div>Platform</div>
          {showVersion && <div>Version</div>}
          <div className="text-right">Views</div>
          <div className="text-right">Success</div>
          <div className="text-right">p50</div>
          <div className="text-right">p95</div>
          <div className="text-right">Fallback</div>
          <div>Top errors</div>
        </div>
        {rows.map((b) => (
          <div
            key={`${b.platform}:${b.version}`}
            className={`grid ${cols} items-center gap-3 border-b px-3 py-2 last:border-b-0 text-xs tabular-nums`}
          >
            <div>{platformLabel(b.platform)}</div>
            {showVersion && (
              <div className="truncate text-muted-foreground" title={b.version}>
                {b.version}
              </div>
            )}
            <div className="text-right">{formatCount(b.views)}</div>
            <div
              className={`text-right ${successTone(b) === `ok` ? `` : successTone(b) === `warn` ? `text-yellow-400` : `text-destructive`}`}
            >
              {pct(b.successRate)}
            </div>
            <div className="text-right">{ms(b.p50FirstFrameMs)}</div>
            <div className={`text-right ${p95Tone(b) === `ok` ? `` : `text-yellow-400`}`}>
              {ms(b.p95FirstFrameMs)}
            </div>
            <div className="text-right">{pct(b.fallbackRate)}</div>
            <div className="flex flex-wrap gap-1">
              {b.topErrors.length === 0 ? (
                <span className="text-muted-foreground">—</span>
              ) : (
                b.topErrors.map((e) => (
                  <Pill key={e.errorClass} className="whitespace-nowrap">
                    {e.errorClass} · {formatCount(e.count)}
                  </Pill>
                ))
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
