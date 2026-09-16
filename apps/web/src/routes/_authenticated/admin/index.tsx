import { createFileRoute } from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@exp/ui"
import {
  DayBars,
  formatStorageMb,
  platformLabel,
  StatCard,
} from "./-shared"

export const Route = createFileRoute(`/_authenticated/admin/`)({
  loader: async () => {
    const [overview, platforms] = await Promise.all([
      trpc.admin.overview.query(),
      trpc.admin.platforms.query(),
    ])
    return { overview, platforms }
  },
  component: AdminOverview,
})

function pct(part: number, whole: number): string {
  if (whole <= 0) return `0%`
  return `${Math.round((part / whole) * 100)}%`
}

function AdminOverview() {
  const { overview, platforms } = Route.useLoaderData()
  const { totals } = overview
  const signupTotal = overview.signupsByDay.reduce((s, r) => s + r.count, 0)
  const wsTotal = overview.teamsByDay.reduce((s, r) => s + r.count, 0)
  const platformMax = Math.max(1, ...platforms.byPlatform.map((p) => p.users))
  const { activation } = overview
  const activationSteps = [
    { label: `Signed up`, users: activation.users },
    { label: `Joined a team`, users: activation.inTeam },
    { label: `Connected GitHub`, users: activation.githubConnected },
    { label: `Team has a repository`, users: activation.teamHasRepo },
    { label: `Registered a device`, users: activation.hasDevice },
    { label: `Started a coding session`, users: activation.startedSession },
  ]

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <div>
        <h1 className="text-2xl font-bold">Overview</h1>
        <p className="text-sm text-muted-foreground">
          Instance totals and 30-day growth. Hover a bar for the day’s count.
        </p>
      </div>

      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <StatCard label="Users" value={String(totals.users)} />
        <StatCard label="Teams" value={String(totals.teams)} />
        <StatCard
          label="Issues"
          value={String(totals.issues)}
          hint={`${totals.boards} boards`}
        />
        <StatCard label="Storage" value={formatStorageMb(totals.storageMb)} />
        <StatCard
          label="Active subscriptions"
          value={String(totals.activeSubscriptions)}
          hint={`${totals.seats} paid seats`}
        />
        <StatCard
          label="Est. MRR"
          value={`€${totals.estimatedMrr}`}
          hint="yearly plans normalized to /mo"
        />
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Signups (last 30 days)</CardTitle>
            <CardDescription className="text-xs">
              {signupTotal} new {signupTotal === 1 ? `user` : `users`}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <DayBars rows={overview.signupsByDay} unit="signup" />
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">
              Teams created (last 30 days)
            </CardTitle>
            <CardDescription className="text-xs">
              {wsTotal} new {wsTotal === 1 ? `team` : `teams`}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <DayBars rows={overview.teamsByDay} unit="team" />
          </CardContent>
        </Card>
      </div>

      {/* EXP-835: where users stop on the way to their first coding run. */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Activation</CardTitle>
          <CardDescription className="text-xs">
            Users who reached each step themselves, as a share of all users.
            Steps are not strictly nested: a teammate’s GitHub connection lets
            a member code without connecting.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="space-y-2">
            {activationSteps.map((step) => (
              <div key={step.label} className="space-y-1">
                <div className="flex items-center justify-between text-xs">
                  <span>{step.label}</span>
                  <span className="tabular-nums text-muted-foreground">
                    {step.users} ({pct(step.users, activation.users)})
                  </span>
                </div>
                <div className="h-2 overflow-hidden rounded-full bg-muted">
                  <div
                    className="h-full rounded-full bg-primary"
                    style={{
                      width: `${(step.users / Math.max(1, activation.users)) * 100}%`,
                    }}
                  />
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* EXP-759: who uses which client. */}
      <div className="grid gap-3 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Platforms</CardTitle>
            <CardDescription className="text-xs">
              Users seen per client (any time / last 30 days / last 7 days).{` `}
              {platforms.usersWithAny} of {platforms.usersTotal} users have a
              record; {platforms.multiPlatform} use two or more clients.
            </CardDescription>
          </CardHeader>
          <CardContent>
            {platforms.byPlatform.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                No client activity recorded yet.
              </p>
            ) : (
              <div className="space-y-2">
                {platforms.byPlatform.map((row) => (
                  <div key={row.platform} className="space-y-1">
                    <div className="flex items-center justify-between text-xs">
                      <span>{platformLabel(row.platform)}</span>
                      <span className="tabular-nums text-muted-foreground">
                        {row.users} · 30d {row.active30d} · 7d {row.active7d}
                      </span>
                    </div>
                    <div
                      className="h-2 overflow-hidden rounded-full bg-muted"
                      title={`${platformLabel(row.platform)}: ${row.users} users (${pct(row.users, platforms.usersTotal)} of all users)`}
                    >
                      <div
                        className="h-full rounded-full bg-primary"
                        style={{ width: `${(row.users / platformMax) * 100}%` }}
                      />
                    </div>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">First client used</CardTitle>
            <CardDescription className="text-xs">
              The client each user was first seen on. Accounts older than the
              ledger (Sep 2026) are the migration backfill’s best guess from
              sessions, devices and push tokens.
            </CardDescription>
          </CardHeader>
          <CardContent>
            {platforms.firstPlatform.length === 0 ? (
              <p className="text-sm text-muted-foreground">Nothing yet.</p>
            ) : (
              <div className="space-y-1">
                {platforms.firstPlatform.map((row) => (
                  <div
                    key={row.platform}
                    className="flex items-center justify-between text-xs"
                  >
                    <span>{platformLabel(row.platform)}</span>
                    <span className="tabular-nums text-muted-foreground">
                      {row.users} ({pct(row.users, platforms.usersWithAny)})
                    </span>
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
