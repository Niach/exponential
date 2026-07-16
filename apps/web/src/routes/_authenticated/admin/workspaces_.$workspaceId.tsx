import { useState } from "react"
import { createFileRoute, Link, useNavigate, useRouter } from "@tanstack/react-router"
import { ArrowLeft, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { getInitials } from "@/lib/utils"
import {
  EmailStatusBadge,
  PlanBadge,
  formatDate,
  formatDateTime,
  formatLimit,
  formatRelative,
  formatStorageMb,
} from "./-shared"

export const Route = createFileRoute(
  `/_authenticated/admin/workspaces_/$workspaceId`
)({
  loader: async ({ params }) => {
    const detail = await trpc.admin.getWorkspaceDetail.query({
      workspaceId: params.workspaceId,
    })
    return { detail }
  },
  component: AdminWorkspaceDetail,
})

type CompChoice = `none` | `pro` | `business` | `unlimited`

function eventText(
  type: string,
  payload: Record<string, unknown> | null
): string {
  switch (type) {
    case `status_changed`:
      return `changed status to ${String(payload?.to ?? `?`).replace(/_/g, ` `)}`
    case `assignee_changed`:
      return payload?.to ? `changed the assignee` : `removed the assignee`
    case `label_added`:
      return `added a label`
    case `label_removed`:
      return `removed a label`
    case `pr_opened`:
      return `opened a pull request`
    case `pr_merged`:
      return `merged the pull request`
    default:
      return type.replace(/_/g, ` `)
  }
}

function AdminWorkspaceDetail() {
  const router = useRouter()
  const navigate = useNavigate()
  const { detail } = Route.useLoaderData()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [pendingComp, setPendingComp] = useState<CompChoice | null>(null)

  const ws = detail.workspace
  const owners = detail.members.filter((m) => m.role === `owner`)
  // parseCompTier never yields `free`, so the wire value is a CompChoice.
  const currentComp = (detail.compTier ?? `none`) as CompChoice

  const handleSetComp = async () => {
    if (pendingComp === null) return
    setError(null)
    setBusy(true)
    try {
      await trpc.admin.setWorkspaceCompTier.mutate({
        workspaceId: ws.id,
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
      await trpc.admin.deleteWorkspace.mutate({ workspaceId: ws.id })
      await navigate({ to: `/admin/workspaces` })
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setBusy(false)
    }
  }

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <Button asChild variant="ghost" size="sm" className="-ml-2">
        <Link to="/admin/workspaces">
          <ArrowLeft className="h-4 w-4" />
          All workspaces
        </Link>
      </Button>

      {error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      {/* Header */}
      <Card>
        <CardContent className="flex flex-col gap-2 md:flex-row md:items-center md:justify-between">
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
                <Badge
                  key={o.userId}
                  variant="secondary"
                  className="text-xs max-w-[180px] truncate"
                  title={o.email}
                >
                  {o.name || o.email}
                </Badge>
              ))
            )}
          </div>
        </CardContent>
      </Card>

      <div className="grid gap-4 md:grid-cols-2">
        {/* Billing */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Billing</CardTitle>
            <CardDescription className="text-xs">
              {detail.compApplied
                ? `Effective plan comes from the admin comp override.`
                : detail.subscription
                  ? `Effective plan comes from the Creem subscription.`
                  : `No subscription — free tier.`}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
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
                  <SelectItem value="pro">Pro</SelectItem>
                  <SelectItem value="business">Business</SelectItem>
                  <SelectItem value="unlimited">Unlimited</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <p className="text-xs text-muted-foreground">
              A comp tier is a floor over the paid plan — it lifts the
              workspace to at least that tier for free but never lowers a paid
              subscription.
            </p>
          </CardContent>
        </Card>

        {/* Usage */}
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Usage</CardTitle>
            <CardDescription className="text-xs">
              Against the effective plan's limits.
            </CardDescription>
          </CardHeader>
          <CardContent>
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
                <span className="text-muted-foreground">Projects</span>
                <span className="tabular-nums">{detail.projects.length}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-muted-foreground">Issues</span>
                <span className="tabular-nums">{detail.issueCount}</span>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Members */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Members</CardTitle>
          <CardDescription className="text-xs">
            {detail.members.length}{` `}
            {detail.members.length === 1 ? `member` : `members`} (including
            agent users)
          </CardDescription>
        </CardHeader>
        <CardContent>
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
                  <Avatar className="h-6 w-6 shrink-0">
                    {m.image && <AvatarImage src={m.image} />}
                    <AvatarFallback className="text-[10px]">
                      {getInitials(m.name || m.email)}
                    </AvatarFallback>
                  </Avatar>
                  <span className="text-sm truncate group-hover:underline">
                    {m.name || m.email}
                  </span>
                  {m.isAgent && (
                    <Badge variant="outline" className="text-xs shrink-0">
                      agent
                    </Badge>
                  )}
                </Link>
                <div>
                  <Badge variant="secondary" className="text-xs capitalize">
                    {m.role}
                  </Badge>
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
        </CardContent>
      </Card>

      {/* Projects */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Projects</CardTitle>
          <CardDescription className="text-xs">
            {detail.projects.length}{` `}
            {detail.projects.length === 1 ? `project` : `projects`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {detail.projects.length === 0 ? (
            <p className="text-sm text-muted-foreground">No projects.</p>
          ) : (
            <div className="rounded-md border">
              <div className="hidden md:grid grid-cols-[1fr_100px_80px_120px] items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground">
                <div>Project</div>
                <div>Type</div>
                <div>Issues</div>
                <div>Created</div>
              </div>
              {detail.projects.map((p) => (
                <div
                  key={p.id}
                  className="flex flex-col md:grid md:grid-cols-[1fr_100px_80px_120px] md:items-center gap-1 md:gap-3 border-b px-3 py-2 last:border-b-0"
                >
                  <div className="min-w-0 flex items-center gap-2">
                    <span className="text-sm font-medium truncate">
                      {p.name}
                    </span>
                    <span className="text-xs text-muted-foreground truncate">
                      /{p.slug}
                    </span>
                    {p.deletedAt && (
                      <Badge variant="destructive" className="text-xs shrink-0">
                        pending deletion
                      </Badge>
                    )}
                  </div>
                  <div>
                    <Badge variant="outline" className="text-xs">
                      {p.isPublic ? `public` : `private`}
                    </Badge>
                  </div>
                  <div className="text-sm tabular-nums">{p.issueCount}</div>
                  <div className="text-xs text-muted-foreground">
                    {formatDate(p.createdAt)}
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Activity */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Recent activity</CardTitle>
          <CardDescription className="text-xs">
            Latest {detail.events.length} issue events
          </CardDescription>
        </CardHeader>
        <CardContent>
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
                      {e.actorName || e.actorEmail || `Someone`}
                    </span>
                    {` `}
                    {eventText(
                      e.type,
                      (e.payload ?? null) as Record<string, unknown> | null
                    )}
                    {` on `}
                    <span className="font-medium text-foreground">
                      {e.issueIdentifier}
                    </span>
                    {` — `}
                    {e.issueTitle}
                  </span>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Email deliveries */}
      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Email deliveries</CardTitle>
          <CardDescription className="text-xs">
            Latest {detail.emailDeliveries.length} emails to members or about
            this workspace's issues
          </CardDescription>
        </CardHeader>
        <CardContent>
          {detail.emailDeliveries.length === 0 ? (
            <p className="text-sm text-muted-foreground">No emails sent.</p>
          ) : (
            <div className="rounded-md border overflow-x-auto">
              <div className="min-w-[560px]">
                <div className="grid grid-cols-[1fr_110px_80px_90px_140px] items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground">
                  <div>To</div>
                  <div>Kind</div>
                  <div>Status</div>
                  <div>Issue</div>
                  <div>Sent</div>
                </div>
                {detail.emailDeliveries.map((d) => (
                  <div
                    key={d.id}
                    className="grid grid-cols-[1fr_110px_80px_90px_140px] items-center gap-3 border-b px-3 py-2 last:border-b-0 text-xs"
                  >
                    <div className="truncate">{d.toEmail}</div>
                    <div className="text-muted-foreground">{d.kind}</div>
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
          )}
        </CardContent>
      </Card>

      {/* Danger zone */}
      <Card className="border-destructive/50">
        <CardHeader>
          <CardTitle className="text-sm text-destructive">
            Danger zone
          </CardTitle>
          <CardDescription className="text-xs">
            Deleting a workspace cascades to all projects, issues, comments,
            and attachments.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <Button
            variant="destructive"
            size="sm"
            disabled={busy}
            onClick={() => setConfirmDelete(true)}
          >
            <Trash2 className="h-4 w-4" />
            Delete workspace
          </Button>
        </CardContent>
      </Card>

      {/* Comp-tier confirm */}
      <Dialog
        open={pendingComp !== null}
        onOpenChange={(open) => !open && setPendingComp(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {pendingComp === `none`
                ? `Clear the comp tier?`
                : `Comp this workspace to ${pendingComp}?`}
            </DialogTitle>
            <DialogDescription>
              {pendingComp === `none`
                ? `${ws.name} falls back to its Creem-derived plan (or free).`
                : `${ws.name} gets at least the ${pendingComp} tier's limits for free. An active subscription of a higher tier still wins.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setPendingComp(null)}
              disabled={busy}
            >
              Cancel
            </Button>
            <Button onClick={handleSetComp} disabled={busy}>
              Confirm
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete confirm */}
      <Dialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete workspace?</DialogTitle>
            <DialogDescription>
              This permanently removes <strong>{ws.name}</strong> and cascades
              to all of its projects, issues, labels, comments, and
              attachments. This cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setConfirmDelete(false)}
              disabled={busy}
            >
              Cancel
            </Button>
            <Button variant="destructive" onClick={handleDelete} disabled={busy}>
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
