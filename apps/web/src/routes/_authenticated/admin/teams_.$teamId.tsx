import { useState } from "react"
import { deleteTeamPrompt, promptActions, WEB_PROMPTS } from "@/lib/prompts"
import { createFileRoute, Link, useNavigate, useRouter } from "@tanstack/react-router"
import { ArrowLeft, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import {
  Pill,
  Button,
  Prompt,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  UserAvatar,
} from "@exp/ui"
import {
  issueEventActorFallback,
  issueEventPhrase,
} from "@/lib/issue-event-labels"
import {
  AdminCard,
  EmailDeliveriesTable,
  PlanBadge,
  formatDate,
  formatDateTime,
  formatLimit,
  formatRelative,
  formatStorageMb,
} from "./-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(
  `/_authenticated/admin/teams_/$teamId`
)({
  loader: async ({ params }) => {
    const detail = await trpc.admin.getTeamDetail.query({
      teamId: params.teamId,
    })
    return { detail }
  },
  head: ({ loaderData }) => ({
    meta: [
      { title: pageTitle(loaderData?.detail.team.name, `Teams`, `Admin`) },
    ],
  }),
  component: AdminTeamDetail,
})

type CompChoice = `none` | `team` | `unlimited`

function AdminTeamDetail() {
  const router = useRouter()
  const navigate = useNavigate()
  const { detail } = Route.useLoaderData()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [pendingComp, setPendingComp] = useState<CompChoice | null>(null)

  const ws = detail.team
  const owners = detail.members.filter((m) => m.role === `owner`)
  // parseCompTier never yields `free`, so the wire value is a CompChoice.
  const currentComp = (detail.compTier ?? `none`) as CompChoice

  const handleSetComp = async () => {
    if (pendingComp === null) return
    setError(null)
    setBusy(true)
    try {
      await trpc.admin.setTeamCompTier.mutate({
        teamId: ws.id,
        compTier: pendingComp === `none` ? null : pendingComp,
      })
      setPendingComp(null)
      await router.invalidate()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(false)
    }
  }

  const handleDelete = async () => {
    setError(null)
    setBusy(true)
    try {
      await trpc.admin.deleteTeam.mutate({ teamId: ws.id })
      await navigate({ to: `/admin/teams` })
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setBusy(false)
    }
  }

  const compCopy =
    pendingComp === `none`
      ? WEB_PROMPTS.adminClearCompTier(ws.name)
      : WEB_PROMPTS.adminCompTier(ws.name, pendingComp ?? ``)
  const deleteCopy = deleteTeamPrompt(ws.name)

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <Button asChild variant="ghost" size="sm" className="-ml-2">
        <Link to="/admin/teams">
          <ArrowLeft className="h-4 w-4" />
          All teams
        </Link>
      </Button>

      {error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      {/* Header */}
      <AdminCard
        contentClassName="flex flex-col gap-2 md:flex-row md:items-center md:justify-between"
      >
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <h1 className="text-lg font-semibold truncate">{ws.name}</h1>
            <PlanBadge plan={detail.plan} compApplied={detail.compApplied} />
          </div>
          <div className="text-sm text-muted-foreground">
            /{ws.slug} · created {formatDate(ws.createdAt)}
          </div>
        </div>
        <div className="flex flex-wrap gap-1 shrink-0">
          {owners.length === 0 ? (
            <span className="text-xs text-muted-foreground">No owners</span>
          ) : (
            owners.map((o) => (
              <Pill
                key={o.userId}
                className="max-w-[180px] truncate"
                title={o.email}
              >
                {o.name || o.email}
              </Pill>
            ))
          )}
        </div>
      </AdminCard>

      <div className="grid gap-4 md:grid-cols-2">
        {/* Billing */}
        <AdminCard
          title="Billing"
          description={
            <>
              {detail.compApplied
                ? `Effective plan comes from the admin comp override.`
                : detail.subscription
                  ? `Effective plan comes from the Creem subscription.`
                  : `No subscription, so the team is on the free tier.`}
            </>
          }
          contentClassName="space-y-3"
        >
          <div className="flex items-center justify-between text-sm">
            <span className="text-muted-foreground">Effective plan</span>
            <PlanBadge plan={detail.plan} compApplied={detail.compApplied} />
          </div>
          {detail.subscription ? (
            <div className="space-y-1.5 rounded-md border p-3 text-xs">
              <div className="flex justify-between">
                <span className="text-muted-foreground">Subscription</span>
                <span className="capitalize">
                  {detail.subscription.tier} · {detail.subscription.seats}
                  {` `}
                  {detail.subscription.seats === 1 ? `seat` : `seats`}
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Status</span>
                <span>
                  {detail.subscription.status}
                  {detail.subscription.cancelAtPeriodEnd
                    ? ` (cancels at period end)`
                    : ``}
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Current period ends</span>
                <span>{formatDate(detail.subscription.periodEnd)}</span>
              </div>
            </div>
          ) : (
            <p className="text-xs text-muted-foreground">
              No active Creem subscription.
            </p>
          )}
          <div className="flex items-center justify-between gap-3">
            <div className="text-sm text-muted-foreground">Comp tier</div>
            <Select
              value={currentComp}
              onValueChange={(value) => {
                if (value !== currentComp) {
                  setPendingComp(value as CompChoice)
                }
              }}
            >
              <SelectTrigger className="w-[160px]" size="sm">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">None</SelectItem>
                <SelectItem value="team">Team</SelectItem>
                <SelectItem value="unlimited">Unlimited</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <p className="text-xs text-muted-foreground">
            A comp tier is a floor over the paid plan. It lifts the team
            to at least that tier for free but never lowers a paid
            subscription.
          </p>
        </AdminCard>

        {/* Usage */}
        <AdminCard title="Usage" description="Against the effective plan's limits.">
          <div className="space-y-2 text-sm">
            <div className="flex justify-between">
              <span className="text-muted-foreground">Seats</span>
              <span className="tabular-nums">
                {detail.usage.members} / {formatLimit(detail.limits.seats)}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Storage</span>
              <span className="tabular-nums">
                {formatStorageMb(detail.usage.storageMb)} /{` `}
                {formatStorageMb(detail.limits.storageMb)}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Widget configs</span>
              <span className="tabular-nums">
                {detail.usage.widgetConfigs} /{` `}
                {formatLimit(detail.limits.widgetConfigs)}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Boards</span>
              <span className="tabular-nums">{detail.boards.length}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-muted-foreground">Issues</span>
              <span className="tabular-nums">{detail.issueCount}</span>
            </div>
          </div>
        </AdminCard>
      </div>

      {/* Members */}
      <AdminCard
        title="Members"
        description={
          <>
            {detail.members.length}{` `}
            {detail.members.length === 1 ? `member` : `members`} (including
            agent users)
          </>
        }
      >
        <div className="rounded-md border">
          <div className="hidden md:grid grid-cols-[1fr_90px_120px_120px] items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground">
            <div>Member</div>
            <div>Role</div>
            <div>Last active</div>
            <div>Member since</div>
          </div>
          {detail.members.map((m) => (
            <div
              key={m.userId}
              className="flex flex-col md:grid md:grid-cols-[1fr_90px_120px_120px] md:items-center gap-1 md:gap-3 border-b px-3 py-2 last:border-b-0"
            >
              <Link
                to="/admin/users/$userId"
                params={{ userId: m.userId }}
                className="flex items-center gap-2 min-w-0 group"
              >
                <UserAvatar
                  size={24}
                  className="shrink-0"
                  user={{ id: m.userId, name: m.name, email: m.email, image: m.image }}
                />
                <span className="text-sm truncate group-hover:underline">
                  {m.name || m.email}
                </span>
              </Link>
              <div>
                <Pill className="capitalize">{m.role}</Pill>
              </div>
              <div
                className="text-xs text-muted-foreground"
                title={formatDateTime(m.lastActiveAt)}
              >
                {formatRelative(m.lastActiveAt)}
              </div>
              <div className="text-xs text-muted-foreground">
                {formatDate(m.memberSince)}
              </div>
            </div>
          ))}
        </div>
      </AdminCard>

      {/* Boards */}
      <AdminCard
        title="Boards"
        description={
          <>
            {detail.boards.length}{` `}
            {detail.boards.length === 1 ? `board` : `boards`}
          </>
        }
      >
        {detail.boards.length === 0 ? (
          <p className="text-sm text-muted-foreground">No boards.</p>
        ) : (
          <div className="rounded-md border">
            <div className="hidden md:grid grid-cols-[1fr_80px_120px] items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground">
              <div>Board</div>
              <div>Issues</div>
              <div>Created</div>
            </div>
            {detail.boards.map((p) => (
              <div
                key={p.id}
                className="flex flex-col md:grid md:grid-cols-[1fr_80px_120px] md:items-center gap-1 md:gap-3 border-b px-3 py-2 last:border-b-0"
              >
                <div className="min-w-0 flex items-center gap-2">
                  <span className="text-sm font-medium truncate">
                    {p.name}
                  </span>
                  <span className="text-xs text-muted-foreground truncate">
                    /{p.slug}
                  </span>
                  {p.deletedAt && (
                    <Pill className="shrink-0 text-destructive">
                      pending deletion
                    </Pill>
                  )}
                </div>
                <div className="text-sm tabular-nums">{p.issueCount}</div>
                <div className="text-xs text-muted-foreground">
                  {formatDate(p.createdAt)}
                </div>
              </div>
            ))}
          </div>
        )}
      </AdminCard>

      {/* Activity */}
      <AdminCard
        title="Recent activity"
        description={<>Latest {detail.events.length} issue events</>}
      >
        {detail.events.length === 0 ? (
          <p className="text-sm text-muted-foreground">No activity yet.</p>
        ) : (
          <div className="space-y-1">
            {detail.events.map((e) => (
              <div
                key={e.id}
                className="flex items-baseline gap-2 text-xs text-muted-foreground"
              >
                <span
                  className="shrink-0 tabular-nums"
                  title={formatDateTime(e.createdAt)}
                >
                  {formatRelative(e.createdAt)}
                </span>
                <span className="truncate">
                  <span className="font-medium text-foreground">
                    {e.actorName ||
                      e.actorEmail ||
                      issueEventActorFallback(
                        (e.payload ?? null) as Record<string, unknown> | null
                      )}
                  </span>
                  {` `}
                  {issueEventPhrase(
                    e.type,
                    (e.payload ?? null) as Record<string, unknown> | null
                  )}
                  {` on `}
                  <span className="font-medium text-foreground">
                    {e.issueIdentifier}
                  </span>
                  {` · `}
                  {e.issueTitle}
                </span>
              </div>
            ))}
          </div>
        )}
      </AdminCard>

      {/* Email deliveries */}
      <AdminCard
        title="Email deliveries"
        description={
          <>
            Latest {detail.emailDeliveries.length} emails to members or about
            this team's issues
          </>
        }
      >
        {detail.emailDeliveries.length === 0 ? (
          <p className="text-sm text-muted-foreground">No emails sent.</p>
        ) : (
          <EmailDeliveriesTable rows={detail.emailDeliveries} showSubject />
        )}
      </AdminCard>

      {/* Danger zone */}
      <AdminCard
        title="Danger zone"
        titleClassName="text-destructive"
        description={
          <>
            Deleting a team cascades to all boards, issues, comments,
            and attachments.
            {detail.subscription && !detail.subscription.cancelAtPeriodEnd
              ? ` This team has a live subscription. Cancel it first (team settings → Billing, or the Creem dashboard); the delete is refused until then.`
              : ``}
          </>
        }
        className="border-destructive/50"
      >
        <Button
          variant="destructive"
          size="sm"
          disabled={busy}
          onClick={() => setConfirmDelete(true)}
        >
          <Trash2 className="h-4 w-4" />
          Delete team
        </Button>
      </AdminCard>

      {/* Comp-tier confirm */}
      <Prompt
        open={pendingComp !== null}
        onOpenChange={(open) => !open && setPendingComp(null)}
        busy={busy}
        title={compCopy.title}
        body={compCopy.body}
        actions={promptActions(compCopy, {
          [pendingComp === `none` ? `clear` : `comp`]: {
            busy,
            onSelect: () => handleSetComp(),
          },
        })}
      />

      {/* Delete confirm */}
      <Prompt
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
        busy={busy}
        title={deleteCopy.title}
        body={deleteCopy.body}
        actions={promptActions(deleteCopy, {
          delete: { busy, onSelect: () => handleDelete() },
        })}
      />
    </div>
  )
}
