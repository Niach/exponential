import { useState } from "react"
import { createFileRoute, Link, useNavigate, useRouter } from "@tanstack/react-router"
import { ArrowLeft, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { useSession } from "@/hooks/use-session"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
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
import { getInitials } from "@/lib/utils"
import {
  EmailStatusBadge,
  PlanBadge,
  formatDate,
  formatDateTime,
  formatRelative,
} from "./-shared"

export const Route = createFileRoute(`/_authenticated/admin/users_/$userId`)({
  loader: async ({ params }) => {
    const detail = await trpc.admin.getUserDetail.query({
      userId: params.userId,
    })
    return { detail }
  },
  component: AdminUserDetail,
})

function AdminUserDetail() {
  const router = useRouter()
  const navigate = useNavigate()
  const { detail } = Route.useLoaderData()
  const { data: session } = useSession()
  const isSelf = session?.user?.id === detail.user.id
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)

  const { user } = detail

  const handleToggleAdmin = async (next: boolean) => {
    setError(null)
    setBusy(true)
    try {
      await trpc.admin.setUserAdmin.mutate({ userId: user.id, isAdmin: next })
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
      await trpc.admin.deleteUser.mutate({ userId: user.id })
      await navigate({ to: `/admin/users` })
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setBusy(false)
    }
  }

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <Button asChild variant="ghost" size="sm" className="-ml-2">
        <Link to="/admin/users">
          <ArrowLeft className="h-4 w-4" />
          All users
        </Link>
      </Button>

      {error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <Card>
        <CardContent className="flex flex-col gap-4 md:flex-row md:items-center">
          <div className="flex items-center gap-4 min-w-0 flex-1">
            <Avatar className="h-12 w-12 shrink-0">
              {user.image && <AvatarImage src={user.image} />}
              <AvatarFallback>
                {getInitials(user.name || user.email)}
              </AvatarFallback>
            </Avatar>
            <div className="min-w-0">
              <div className="text-lg font-semibold truncate">
                {user.name}
                {isSelf && (
                  <span className="text-muted-foreground font-normal text-sm">
                    {` (you)`}
                  </span>
                )}
              </div>
              <div className="text-sm text-muted-foreground truncate">
                {user.email}
              </div>
              <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                {user.providers.length === 0 ? (
                  <Badge variant="secondary" className="text-xs">
                    password
                  </Badge>
                ) : (
                  user.providers.map((p) => (
                    <Badge key={p} variant="secondary" className="text-xs">
                      {p}
                    </Badge>
                  ))
                )}
                {user.isAgent && (
                  <Badge variant="outline" className="text-xs">
                    agent
                  </Badge>
                )}
                <span>joined {formatDate(user.createdAt)}</span>
                <span aria-hidden>·</span>
                <span>
                  {detail.createdIssuesCount}{` `}
                  {detail.createdIssuesCount === 1 ? `issue` : `issues`} created
                </span>
              </div>
            </div>
          </div>
          <div className="flex items-center gap-4 shrink-0">
            <div className="flex items-center gap-2 text-sm">
              <span className="text-muted-foreground">Admin</span>
              <Switch
                checked={user.isAdmin}
                disabled={busy}
                onCheckedChange={handleToggleAdmin}
              />
            </div>
            <Button
              variant="destructive"
              size="sm"
              disabled={isSelf || busy}
              onClick={() => setConfirmDelete(true)}
            >
              <Trash2 className="h-4 w-4" />
              Delete
            </Button>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Teams</CardTitle>
          <CardDescription className="text-xs">
            {detail.workspaces.length}{` `}
            {detail.workspaces.length === 1 ? `membership` : `memberships`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {detail.workspaces.length === 0 ? (
            <p className="text-sm text-muted-foreground">No memberships.</p>
          ) : (
            <div className="rounded-md border">
              <div className="hidden md:grid grid-cols-[1fr_90px_110px_120px] items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground">
                <div>Team</div>
                <div>Role</div>
                <div>Plan</div>
                <div>Member since</div>
              </div>
              {detail.workspaces.map((ws) => (
                <div
                  key={ws.id}
                  className="flex flex-col md:grid md:grid-cols-[1fr_90px_110px_120px] md:items-center gap-1 md:gap-3 border-b px-3 py-2 last:border-b-0"
                >
                  <Link
                    to="/admin/workspaces/$workspaceId"
                    params={{ workspaceId: ws.id }}
                    className="min-w-0 hover:underline"
                  >
                    <span className="text-sm font-medium truncate block">
                      {ws.name}
                      <span className="text-xs text-muted-foreground font-normal">
                        {` `}/{ws.slug}
                      </span>
                    </span>
                  </Link>
                  <div>
                    <Badge variant="secondary" className="text-xs capitalize">
                      {ws.role}
                    </Badge>
                  </div>
                  <div>
                    <PlanBadge plan={ws.plan} compApplied={ws.compApplied} />
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {formatDate(ws.memberSince)}
                  </div>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Sessions</CardTitle>
          <CardDescription className="text-xs">
            Latest {detail.sessions.length}{` `}
            {detail.sessions.length === 1 ? `session` : `sessions`}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {detail.sessions.length === 0 ? (
            <p className="text-sm text-muted-foreground">No sessions.</p>
          ) : (
            <div className="rounded-md border overflow-x-auto">
              <div className="min-w-[560px]">
                <div className="grid grid-cols-[120px_140px_130px_1fr] items-center gap-3 border-b px-3 py-2 text-xs font-medium text-muted-foreground">
                  <div>Last active</div>
                  <div>Signed in</div>
                  <div>IP</div>
                  <div>User agent</div>
                </div>
                {detail.sessions.map((s) => (
                  <div
                    key={s.id}
                    className="grid grid-cols-[120px_140px_130px_1fr] items-center gap-3 border-b px-3 py-2 last:border-b-0 text-xs"
                  >
                    <div title={formatDateTime(s.updatedAt)}>
                      {formatRelative(s.updatedAt)}
                    </div>
                    <div className="text-muted-foreground">
                      {formatDateTime(s.createdAt)}
                    </div>
                    <div className="text-muted-foreground truncate">
                      {s.ipAddress || `—`}
                    </div>
                    <div
                      className="text-muted-foreground truncate"
                      title={s.userAgent ?? undefined}
                    >
                      {s.userAgent || `—`}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Email deliveries</CardTitle>
          <CardDescription className="text-xs">
            Latest {detail.emailDeliveries.length} outbound emails to this user
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

      <Dialog open={confirmDelete} onOpenChange={setConfirmDelete}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete user?</DialogTitle>
            <DialogDescription>
              This permanently removes <strong>{user.email}</strong>. Their
              sessions, accounts, workspace memberships, and any issues or
              comments they authored will be deleted. This cannot be undone.
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
