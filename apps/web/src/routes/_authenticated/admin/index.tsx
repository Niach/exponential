import { createFileRoute } from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { formatStorageMb } from "./-shared"

export const Route = createFileRoute(`/_authenticated/admin/`)({
  loader: async () => {
    const overview = await trpc.admin.overview.query()
    return { overview }
  },
  component: AdminOverview,
})

function StatCard({
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

// Simple CSS bar strip — one bar per day over the trailing 30 days, zero days
// rendered as a faint baseline. Deliberately no chart library.
function DayBars({ rows }: { rows: { day: string; count: number }[] }) {
  const byDay = new Map(rows.map((r) => [r.day, r.count]))
  const days: { day: string; count: number }[] = []
  for (let i = 29; i >= 0; i--) {
    const d = new Date()
    d.setDate(d.getDate() - i)
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, `0`)}-${String(d.getDate()).padStart(2, `0`)}`
    days.push({ day: key, count: byDay.get(key) ?? 0 })
  }
  const max = Math.max(1, ...days.map((d) => d.count))
  return (
    <div className="flex h-16 items-end gap-[3px]">
      {days.map((d) => (
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
              height: d.count > 0 ? `${Math.max(10, (d.count / max) * 100)}%` : `3px`,
            }}
          />
        </div>
      ))}
    </div>
  )
}

function AdminOverview() {
  const { overview } = Route.useLoaderData()
  const { totals } = overview
  const signupTotal = overview.signupsByDay.reduce((s, r) => s + r.count, 0)
  const wsTotal = overview.workspacesByDay.reduce((s, r) => s + r.count, 0)

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <div>
        <h1 className="text-2xl font-bold">Overview</h1>
        <p className="text-sm text-muted-foreground">
          Instance totals and 30-day growth.
        </p>
      </div>

      <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <StatCard label="Users" value={String(totals.users)} />
        <StatCard label="Workspaces" value={String(totals.workspaces)} />
        <StatCard
          label="Issues"
          value={String(totals.issues)}
          hint={`${totals.projects} projects`}
        />
        <StatCard label="Storage" value={formatStorageMb(totals.storageMb)} />
        <StatCard
          label="Active subscriptions"
          value={String(totals.activeSubscriptions)}
          hint={`${totals.seats} paid seats`}
        />
        <StatCard
          label="Est. MRR"
          value={`$${totals.estimatedMrr}`}
          hint="yearly plans normalized to /mo"
        />
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Signups — last 30 days</CardTitle>
            <CardDescription className="text-xs">
              {signupTotal} new {signupTotal === 1 ? `user` : `users`}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <DayBars rows={overview.signupsByDay} />
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">
              Workspaces created — last 30 days
            </CardTitle>
            <CardDescription className="text-xs">
              {wsTotal} new {wsTotal === 1 ? `workspace` : `workspaces`}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <DayBars rows={overview.workspacesByDay} />
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
