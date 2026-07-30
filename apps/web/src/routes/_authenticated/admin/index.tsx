import { createFileRoute } from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { DayBars, formatStorageMb, StatCard } from "./-shared"

export const Route = createFileRoute(`/_authenticated/admin/`)({
  loader: async () => {
    const overview = await trpc.admin.overview.query()
    return { overview }
  },
  component: AdminOverview,
})

function AdminOverview() {
  const { overview } = Route.useLoaderData()
  const { totals } = overview
  const signupTotal = overview.signupsByDay.reduce((s, r) => s + r.count, 0)
  const wsTotal = overview.teamsByDay.reduce((s, r) => s + r.count, 0)

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
              Teams created — last 30 days
            </CardTitle>
            <CardDescription className="text-xs">
              {wsTotal} new {wsTotal === 1 ? `team` : `teams`}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <DayBars rows={overview.teamsByDay} />
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
