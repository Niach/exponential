import { useState } from "react"
import {
  createFileRoute,
  Link,
  redirect,
  useRouter,
  type ErrorComponentProps,
} from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { Pill } from "@/components/ui/pill"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { cn } from "@/lib/utils"
import {
  DayBars,
  formatDate,
  formatDateTime,
  formatRelative,
  PlatformPills,
  StatCard,
} from "./-shared"

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
  // One return_visit per user per UTC day, so the count IS active users.
  const returnRows = dayRows(overview.eventsByDay, [`return_visit`])
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
            Two lenses. The cards and charts count EVENTS in the window (period
            counts, not cohorts; visitors are unique visitor-days from
            cookieless daily ids, bot filtering is best-effort). The signup
            cohort below follows the USERS who signed up in the window through
            onboarding by what they actually did.
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
        <StatCard
          label="Visitors"
          value={String(funnel.visitors)}
          hint="landing on / or /auth/*"
        />
        <StatCard
          label="Signups"
          value={String(funnel.signups)}
          hint={`accounts created${pct(funnel.signups, funnel.visitors) ? ` · ${pct(funnel.signups, funnel.visitors)} of visitors` : ``}`}
        />
        <StatCard
          label="Activated"
          value={String(funnel.activated)}
          hint={`created an issue or sent an invite${pct(funnel.activated, funnel.signups) ? ` · ${pct(funnel.activated, funnel.signups)} of signups` : ``}`}
        />
        <StatCard
          label="Paid"
          value={String(funnel.paid)}
          hint={`first subscription activations${pct(funnel.paid, funnel.activated) ? ` · ${pct(funnel.paid, funnel.activated)} of activated` : ``}`}
        />
        <StatCard
          label="Canceled"
          value={String(funnel.canceled)}
          hint="subscriptions ended"
        />
      </div>

      <SignupCohortCard cohort={overview.cohort} days={days} />
      <RecentSignupsCard rows={overview.recentSignups} days={days} />

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
          title={`Activation events (last ${days} days)`}
          rows={activationRows}
          days={days}
          unit="activation event"
          description="first_issue_created + invite_sent rows per day; the Activated card above counts distinct users, so the two need not agree."
        />
        <ChartCard
          title={`Paid conversions (last ${days} days)`}
          rows={paidRows}
          days={days}
          unit="subscription"
        />
        <ChartCard
          title={`Active users (last ${days} days)`}
          rows={returnRows}
          days={days}
          unit="active user-day"
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
            Latest 50 conversion events in the window, across all names.
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
                  <Pill>{event.name}</Pill>
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
  description,
}: {
  title: string
  rows: { day: string; count: number }[]
  days: number
  unit: string
  description?: string
}) {
  const total = rows.reduce((sum, row) => sum + row.count, 0)
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">{title}</CardTitle>
        <CardDescription className="text-xs">
          {total} {unit}
          {total === 1 ? `` : `s`}
          {description ? ` — ${description}` : ``}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <DayBars rows={rows} days={days} unit={unit} />
      </CardContent>
    </Card>
  )
}

// ── Signup cohort (EXP-759) ───────────────────────────────────────────────────

type Overview = Awaited<ReturnType<typeof trpc.adminConversions.overview.query>>
type Cohort = Overview[`cohort`]

interface Stage {
  key: keyof Cohort
  label: string
  detail: string
}

// The onboarding chain (EXP-725: team → board → invite → devices) plus the
// two outcomes that matter: coming back and paying. Each stage is computed
// from state tables, so it does not depend on which steps emit events.
const STAGES: Stage[] = [
  { key: `signups`, label: `Signed up`, detail: `accounts created in the window` },
  { key: `withTeam`, label: `Has a team`, detail: `created one or accepted an invite` },
  { key: `onboarded`, label: `Finished onboarding`, detail: `created a board in the wizard, or joined via invite` },
  { key: `boardAfterSignup`, label: `Created a board`, detail: `a board in their team newer than the signup` },
  { key: `withIssue`, label: `Created an issue`, detail: `at least one issue with them as creator` },
  { key: `withInvite`, label: `Invited someone`, detail: `minted at least one team invite` },
  { key: `withDevice`, label: `Linked a device`, detail: `registered a desktop or CLI daemon` },
  { key: `returnedAnyClient`, label: `Came back`, detail: `any client seen on a later day than the signup` },
  { key: `paid`, label: `Paid`, detail: `member of a team with a live subscription` },
]

function SignupCohortCard({ cohort, days }: { cohort: Cohort; days: number }) {
  const [hover, setHover] = useState<number | null>(null)
  const total = cohort.signups
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">
          Signup cohort (signed up in the last {days} days)
        </CardTitle>
        <CardDescription className="text-xs">
          {total} {total === 1 ? `user` : `users`} followed through onboarding by
          what they did, not by which events fired. Hover a stage for the drop
          from the previous one. “Came back” via web page loads only:{` `}
          {cohort.returned}.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {total === 0 ? (
          <p className="text-sm text-muted-foreground">No signups in this window.</p>
        ) : (
          <div className="space-y-1.5" onPointerLeave={() => setHover(null)}>
            {STAGES.map((stage, i) => {
              const value = cohort[stage.key]
              const prev = i === 0 ? total : cohort[STAGES[i - 1].key]
              const share = total > 0 ? value / total : 0
              const isHover = hover === i
              return (
                <div
                  key={stage.key}
                  className="grid grid-cols-[150px_1fr_110px] items-center gap-3 text-xs"
                  onPointerEnter={() => setHover(i)}
                  title={`${stage.label}: ${value} of ${total} (${pct(value, total) ?? `0%`})`}
                >
                  <div className={cn(`truncate`, isHover && `text-foreground`)}>
                    {stage.label}
                  </div>
                  <div className="relative h-4 overflow-hidden rounded-[3px] bg-muted">
                    <div
                      className={cn(
                        `h-full rounded-[3px] bg-primary transition-opacity`,
                        hover !== null && !isHover && `opacity-50`
                      )}
                      style={{ width: `${Math.max(share * 100, value > 0 ? 1 : 0)}%` }}
                    />
                  </div>
                  <div className="text-right tabular-nums">
                    <span className="font-semibold text-foreground">{value}</span>
                    <span className="text-muted-foreground">
                      {` `}
                      {pct(value, total) ?? `0%`}
                    </span>
                  </div>
                  {isHover && (
                    <div className="col-span-3 -mt-0.5 text-muted-foreground">
                      {stage.detail}
                      {i > 0 && prev > 0
                        ? ` · ${prev - value} of ${prev} dropped after “${STAGES[i - 1].label}” (${pct(value, prev)} kept)`
                        : ``}
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function Check({ on, label }: { on: boolean; label?: string }) {
  return (
    <span
      className={cn(`tabular-nums`, on ? `text-foreground` : `text-muted-foreground/60`)}
      aria-label={on ? `yes` : `no`}
    >
      {label ?? (on ? `✓` : `–`)}
    </span>
  )
}

const JOURNEY_GRID = `md:grid-cols-[minmax(160px,1.4fr)_90px_minmax(90px,1fr)_110px_48px_56px_56px_56px_56px_56px_90px]`

function RecentSignupsCard({
  rows,
  days,
}: {
  rows: Overview[`recentSignups`]
  days: number
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Recent signups</CardTitle>
        <CardDescription className="text-xs">
          Newest {rows.length} of the last {days} days, one row per account: how
          far each one got. Counts, not booleans, where it helps (issues,
          invites, devices).
        </CardDescription>
      </CardHeader>
      <CardContent>
        {rows.length === 0 ? (
          <p className="text-sm text-muted-foreground">No signups yet.</p>
        ) : (
          <div className="rounded-md border overflow-x-auto">
            <div className="min-w-[980px]">
              <div
                className={`grid ${JOURNEY_GRID} items-center gap-2 border-b px-3 py-2 text-xs font-medium text-muted-foreground`}
              >
                <div>User</div>
                <div>Signed up</div>
                <div>Source</div>
                <div>Platforms</div>
                <div className="text-center">Team</div>
                <div className="text-center">Board</div>
                <div className="text-right">Issues</div>
                <div className="text-right">Invites</div>
                <div className="text-right">Devices</div>
                <div className="text-center">Back</div>
                <div>Last active</div>
              </div>
              {rows.map((row) => (
                <div
                  key={row.id}
                  className={`grid ${JOURNEY_GRID} items-center gap-2 border-b px-3 py-2 text-xs last:border-b-0`}
                >
                  <div className="min-w-0">
                    <Link
                      to="/admin/users/$userId"
                      params={{ userId: row.id }}
                      className="block truncate hover:underline"
                      title={row.email}
                    >
                      {row.name || row.email}
                    </Link>
                    {row.name && (
                      <div className="truncate text-muted-foreground">{row.email}</div>
                    )}
                  </div>
                  <div
                    className="text-muted-foreground"
                    title={formatDateTime(row.createdAt)}
                  >
                    {formatDate(row.createdAt)}
                  </div>
                  <div
                    className="truncate text-muted-foreground"
                    title={row.signupReferrer ?? undefined}
                  >
                    {row.signupRef ?? row.signupUtmSource ?? `(direct)`}
                  </div>
                  <div>
                    <PlatformPills platforms={row.platforms} />
                  </div>
                  <div className="text-center">
                    <Check on={row.teams > 0} label={row.teams > 1 ? String(row.teams) : undefined} />
                  </div>
                  <div
                    className="text-center"
                    title={
                      row.boards > 0 && !row.boardAfterSignup
                        ? `joined a team that already had boards`
                        : undefined
                    }
                  >
                    <Check
                      on={row.boards > 0}
                      label={row.boards > 0 && !row.boardAfterSignup ? `joined` : undefined}
                    />
                  </div>
                  <div className="text-right">
                    <Check on={row.issues > 0} label={row.issues > 0 ? String(row.issues) : undefined} />
                  </div>
                  <div className="text-right">
                    <Check on={row.invites > 0} label={row.invites > 0 ? String(row.invites) : undefined} />
                  </div>
                  <div className="text-right">
                    <Check on={row.devices > 0} label={row.devices > 0 ? String(row.devices) : undefined} />
                  </div>
                  <div
                    className="text-center"
                    title={`${row.returnDays} web return-visit day${row.returnDays === 1 ? `` : `s`}`}
                  >
                    <Check
                      on={
                        row.returnDays > 0 ||
                        (row.lastActiveAt !== null &&
                          new Date(row.lastActiveAt).getTime() -
                            new Date(row.createdAt).getTime() >
                            86_400_000)
                      }
                    />
                  </div>
                  <div
                    className="text-muted-foreground"
                    title={row.lastActiveAt ? formatDateTime(row.lastActiveAt) : undefined}
                  >
                    {formatRelative(row.lastActiveAt)}
                    {row.paidTeams > 0 ? ` · paid` : ``}
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  )
}
