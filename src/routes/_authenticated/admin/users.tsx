import { useState } from "react"
import { createFileRoute, useRouter } from "@tanstack/react-router"
import { MoreHorizontal, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { authClient } from "@/lib/auth-client"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Switch } from "@/components/ui/switch"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { getInitials } from "@/lib/utils"

type AdminUser = Awaited<ReturnType<typeof trpc.admin.listUsers.query>>[number]

export const Route = createFileRoute(`/_authenticated/admin/users`)({
  loader: async () => {
    const users = await trpc.admin.listUsers.query()
    return { users }
  },
  component: AdminUsers,
})

function AdminUsers() {
  const router = useRouter()
  const { users } = Route.useLoaderData()
  const { data: session } = authClient.useSession()
  const currentUserId = session?.user?.id
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [confirmDelete, setConfirmDelete] = useState<AdminUser | null>(null)

  const handleToggleAdmin = async (user: AdminUser, next: boolean) => {
    setError(null)
    setBusy(user.id)
    try {
      await trpc.admin.setUserAdmin.mutate({
        userId: user.id,
        isAdmin: next,
      })
      await router.invalidate()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(null)
    }
  }

  const handleDelete = async () => {
    if (!confirmDelete) return
    setError(null)
    setBusy(confirmDelete.id)
    try {
      await trpc.admin.deleteUser.mutate({ userId: confirmDelete.id })
      setConfirmDelete(null)
      await router.invalidate()
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusy(null)
    }
  }

  return (
    <div className="mx-auto max-w-5xl space-y-4 p-6">
      <div>
        <h1 className="text-2xl font-bold">Users</h1>
        <p className="text-sm text-muted-foreground">
          {users.length} {users.length === 1 ? `user` : `users`} on this
          instance.
        </p>
      </div>

      {error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <div className="rounded-md border">
        <div className="grid grid-cols-[1fr_140px_120px_100px_40px] items-center gap-3 border-b px-4 py-2 text-xs font-medium text-muted-foreground">
          <div>User</div>
          <div>Providers</div>
          <div>Workspaces</div>
          <div>Admin</div>
          <div />
        </div>
        {users.map((user) => {
          const isSelf = user.id === currentUserId
          return (
            <div
              key={user.id}
              className="grid grid-cols-[1fr_140px_120px_100px_40px] items-center gap-3 border-b px-4 py-3 last:border-b-0"
            >
              <div className="flex items-center gap-3 min-w-0">
                <Avatar className="h-8 w-8">
                  {user.image && <AvatarImage src={user.image} />}
                  <AvatarFallback className="text-xs">
                    {getInitials(user.name || user.email)}
                  </AvatarFallback>
                </Avatar>
                <div className="min-w-0">
                  <div className="text-sm font-medium truncate">
                    {user.name}
                    {isSelf && (
                      <span className="text-muted-foreground font-normal">
                        {` (you)`}
                      </span>
                    )}
                  </div>
                  <div className="text-xs text-muted-foreground truncate">
                    {user.email}
                  </div>
                </div>
              </div>
              <div className="flex flex-wrap gap-1">
                {user.providers.length === 0 ? (
                  <span className="text-xs text-muted-foreground">
                    password
                  </span>
                ) : (
                  user.providers.map((p) => (
                    <Badge key={p} variant="secondary" className="text-xs">
                      {p}
                    </Badge>
                  ))
                )}
              </div>
              <div className="text-sm tabular-nums">
                {user.workspaceCount}
              </div>
              <div>
                <Switch
                  checked={user.isAdmin}
                  disabled={busy === user.id}
                  onCheckedChange={(next) => handleToggleAdmin(user, next)}
                />
              </div>
              <div>
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8"
                      disabled={isSelf}
                      aria-label="User actions"
                    >
                      <MoreHorizontal className="h-4 w-4" />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem
                      variant="destructive"
                      onClick={() => setConfirmDelete(user)}
                    >
                      <Trash2 className="mr-2 h-4 w-4" />
                      Delete user
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            </div>
          )
        })}
      </div>

      <Dialog
        open={Boolean(confirmDelete)}
        onOpenChange={(open) => !open && setConfirmDelete(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete user?</DialogTitle>
            <DialogDescription>
              This permanently removes <strong>{confirmDelete?.email}</strong>.
              Their sessions, accounts, workspace memberships, and any issues
              or comments they authored will be deleted. This cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setConfirmDelete(null)}
              disabled={busy !== null}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={busy !== null}
            >
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
