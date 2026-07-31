import {
  createFileRoute,
  Link,
  redirect,
  useRouter,
  type ErrorComponentProps,
} from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { DayBars, formatDateTime, StatCard } from "./-shared"

type WindowDays = 7 | 30 | 90

const WINDOWS: WindowDays[] = [7, 30, 90]

export const Route = createFileRoute(`/_authenticated/admin/conversions`)({
  // `days` stays OPTIONAL in the search schema so plain links to
  // /admin/conversions (e.g. the admin nav) don't have to carry it.
  validateSearch: (
    search: Record<string, unknown>
  ): { days?: WindowDays } => ({
    days: WINDOWS.includes(search.days as WindowDays)
      ? (search.days as WindowDays)
      : undefined,
  }),
  loaderDeps: ({ search }) => ({ days: search.days ?? (30 as const) }),
  // Cloud-only (EXP-362): self-hosted instances record nothing and hide the
  // nav entry; a hand-typed URL bounces back to the admin overview.
  beforeLoad: async () => {
    const config = await getRuntimeConfig()
    if (!config.isCloud) throw redirect({ to: `/admin` })
  },
  loader: async ({ deps }) => {
    const overview = await trpc.adminConversions.overview.query({
      days: deps.days,
    })
    return { overview }
  },
  component: AdminConversions,
  // Without a route-level boundary a failing loader escapes to the router's
  // global fallback, which replaces the ENTIRE app — on the forced dark theme
  // that reads as a black screen with no way back (EXP-373). Keep the failure
  // inside the admin shell so the nav survives and the reason is readable.
  errorComponent: ConversionsError,
})

function ConversionsError({ error }: ErrorComponentProps) {
  const router = useRouter()
  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <h1 className="text-2xl font-bold">Conversions</h1>
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Couldn’t load conversions</CardTitle>
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

function pct(part: number, whole: number): string | undefined {
  if (whole <= 0) return undefined
  return `${((part / whole) * 100).toFixed(1)}%`
}

function dayRows(
  eventsByDay: { day: string; name: string; count: number }[],
  names: string[]
): { day: string; count: number }[] {
  const byDay = new Map<string, number>()
  for (const row of eventsByDay) {
    if (!names.includes(row.name)) continue
    byDay.set(row.day, (byDay.get(row.day) ?? 0) + row.count)
  }
  return [...byDay.entries()].map(([day, count]) => ({ day, count }))
}

function AdminConversions() {
  const { overview } = Route.useLoaderData()
  const days = Route.useSearch().days ?? 30
  const { funnel } = overview

  const signupRows = dayRows(overview.eventsByDay, [`signup`])
  const landingRows = dayRows(overview.eventsByDay, [`landing`])
  const activationRows = dayRows(overview.eventsByDay, [
    `first_issue_created`,
    `invite_sent`,
  ])
  const paidRows = dayRows(overview.eventsByDay, [`subscription_first_active`])

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <div className="flex flex-wrap items-end justify-between gap-2">
        <div>
          <h1 className="text-2xl font-bold">Conversions</h1>
          <p className="text-sm text-muted-foreground">
            First-party funnel: period counts over the selected window, not
            cohorts. Visitors are unique visitor-days (cookieless daily ids, bot
            filtering is best-effort).
          </p>
        </div>
        <div className="flex items-center gap-1">
          {WINDOWS.map((w) => (
            <Button
              key={w}
              asChild
              variant={w === days ? `secondary` : `ghost`}
              size="sm"
            >
              <Link to="/admin/conversions" search={{ days: w }}>
                {w}d
              </Link>
            </Button>
          ))}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3 md:grid-cols-5">
        <StatCard label="Visitors" value={String(funnel.visitors)} />
        <StatCard
          label="Signups"
          value={String(funnel.signups)}
          hint={pct(funnel.signups, funnel.visitors)}
        />
        <StatCard
          label="Activated"
          value={String(funnel.activated)}
          hint={pct(funnel.activated, funnel.signups)}
        />
        <StatCard
          label="Paid"
          value={String(funnel.paid)}
          hint={pct(funnel.paid, funnel.activated)}
        />
        <StatCard label="Canceled" value={String(funnel.canceled)} />
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <ChartCard
          title={`Visitors (last ${days} days)`}
          rows={landingRows}
          days={days}
          unit="visitor-day"
        />
        <ChartCard
          title={`Signups (last ${days} days)`}
          rows={signupRows}
          days={days}
          unit="signup"
        />
        <ChartCard
          title={`Activations (last ${days} days)`}
          rows={activationRows}
          days={days}
          unit="activation event"
        />
        <ChartCard
          title={`Paid conversions (last ${days} days)`}
          rows={paidRows}
          days={days}
          unit="subscription"
        />
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Signup sources</CardTitle>
          <CardDescription className="text-xs">
            Users signed up in the window, grouped by claimed ref/UTM
            attribution. “(direct)” = no params survived to signup.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {overview.sources.length === 0 ? (
            <p className="text-sm text-muted-foreground">No signups yet.</p>
          ) : (
            <div className="rounded-md border">
              <div className="hidden md:grid grid-cols-[1fr_1fr_1fr_90px_70px] items-center gap-3 border-b px-4 py-2 text-xs font-medium text-muted-foreground">
                <div>ref</div>
                <div>utm_source</div>
                <div>utm_medium</div>
                <div className="text-right">Signups</div>
                <div className="text-right">Paid</div>
              </div>
              {overview.sources.map((source, i) => (
                <div
                  key={i}
                  className="grid grid-cols-2 md:grid-cols-[1fr_1fr_1fr_90px_70px] items-center gap-3 border-b px-4 py-2 text-sm last:border-b-0"
                >
                  <div className="truncate">
                    {source.ref ??
                      (source.utmSource || source.utmMedium ? `—` : `(direct)`)}
                  </div>
                  <div className="truncate">{source.utmSource ?? `—`}</div>
                  <div className="truncate hidden md:block">
                    {source.utmMedium ?? `—`}
                  </div>
                  <div className="text-right tabular-nums">
                    {source.signups}
                  </div>
                  <div className="text-right tabular-nums">{source.paid}</div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Paid conversions</CardTitle>
          <CardDescription className="text-xs">
            First activation per subscription, newest first (max 50).
          </CardDescription>
        </CardHeader>
        <CardContent>
          {overview.paidConversions.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              No paid conversions in this window.
            </p>
          ) : (
            <div className="rounded-md border">
              <div className="hidden md:grid grid-cols-[150px_1fr_1fr_60px_1fr_90px] items-center gap-3 border-b px-4 py-2 text-xs font-medium text-muted-foreground">
                <div>When</div>
                <div>User</div>
                <div>Team</div>
                <div className="text-right">Seats</div>
                <div>Source</div>
                <div className="text-right">Days to pay</div>
              </div>
              {overview.paidConversions.map((row) => (
                <div
                  key={row.id}
                  className="grid grid-cols-2 md:grid-cols-[150px_1fr_1fr_60px_1fr_90px] items-center gap-3 border-b px-4 py-2 text-sm last:border-b-0"
                >
                  <div className="text-muted-foreground whitespace-nowrap">
                    {formatDateTime(row.createdAt)}
                  </div>
                  <div className="truncate">{row.userEmail ?? `—`}</div>
                  <div className="truncate">{row.teamName ?? `—`}</div>
                  <div className="text-right tabular-nums">
                    {row.seats ?? `—`}
                  </div>
                  <div className="truncate">
                    {row.signupRef ?? row.signupUtmSource ?? `(direct)`}
                  </div>
                  <div className="text-right tabular-nums">
                    {row.daysFromSignup ?? `—`}
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Recent events</CardTitle>
          <CardDescription className="text-xs">
            Latest 50 conversion events across all names.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {overview.recentEvents.length === 0 ? (
            <p className="text-sm text-muted-foreground">No events yet.</p>
          ) : (
            <div className="rounded-md border">
              {overview.recentEvents.map((event) => (
                <div
                  key={event.id}
                  className="flex flex-wrap items-center gap-2 border-b px-4 py-2 text-sm last:border-b-0"
                >
                  <span className="text-muted-foreground whitespace-nowrap text-xs">
                    {formatDateTime(event.createdAt)}
                  </span>
                  <Badge variant="outline" className="text-xs">
                    {event.name}
                  </Badge>
                  <span className="truncate">
                    {event.userEmail ??
                      (event.anonymousId
                        ? `anon ${event.anonymousId.slice(0, 8)}`
                        : `—`)}
                  </span>
                  {event.properties && (
                    <span className="truncate text-xs text-muted-foreground">
                      {JSON.stringify(event.properties)}
                    </span>
                  )}
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
}

function ChartCard({
  title,
  rows,
  days,
  unit,
}: {
  title: string
  rows: { day: string; count: number }[]
  days: number
  unit: string
}) {
  const total = rows.reduce((sum, row) => sum + row.count, 0)
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">{title}</CardTitle>
        <CardDescription className="text-xs">
          {total} {unit}
          {total === 1 ? `` : `s`}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <DayBars rows={rows} days={days} />
      </CardContent>
    </Card>
  )
}
