import { useState } from "react"
import { createFileRoute, useRouter } from "@tanstack/react-router"
import { MoreHorizontal, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
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

type AdminWorkspace = Awaited<
  ReturnType<typeof trpc.admin.listWorkspaces.query>
>[number]

export const Route = createFileRoute(`/_authenticated/admin/workspaces`)({
  loader: async () => {
    const workspaces = await trpc.admin.listWorkspaces.query()
    return { workspaces }
  },
  component: AdminWorkspaces,
})

function AdminWorkspaces() {
  const router = useRouter()
  const { workspaces } = Route.useLoaderData()
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [confirmDelete, setConfirmDelete] = useState<AdminWorkspace | null>(
    null
  )

  const handleDelete = async () => {
    if (!confirmDelete) return
    setError(null)
    setBusy(confirmDelete.id)
    try {
      await trpc.admin.deleteWorkspace.mutate({
        workspaceId: confirmDelete.id,
      })
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
        <h1 className="text-2xl font-bold">Workspaces</h1>
        <p className="text-sm text-muted-foreground">
          {workspaces.length}{` `}
          {workspaces.length === 1 ? `workspace` : `workspaces`} on this
          instance.
        </p>
      </div>

      {error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <div className="rounded-md border">
        <div className="grid grid-cols-[1fr_1fr_120px_120px_40px] items-center gap-3 border-b px-4 py-2 text-xs font-medium text-muted-foreground">
          <div>Name</div>
          <div>Owners</div>
          <div>Members</div>
          <div>Projects</div>
          <div />
        </div>
        {workspaces.map((ws) => (
          <div
            key={ws.id}
            className="grid grid-cols-[1fr_1fr_120px_120px_40px] items-center gap-3 border-b px-4 py-3 last:border-b-0"
          >
            <div className="min-w-0">
              <div className="text-sm font-medium truncate">{ws.name}</div>
              <div className="text-xs text-muted-foreground truncate">
                /{ws.slug}
              </div>
            </div>
            <div className="flex flex-wrap gap-1 min-w-0">
              {ws.owners.length === 0 ? (
                <span className="text-xs text-muted-foreground">
                  No owners
                </span>
              ) : (
                ws.owners.map((o) => (
                  <Badge
                    key={o.id}
                    variant="secondary"
                    className="text-xs max-w-[180px] truncate"
                    title={o.email}
                  >
                    {o.name || o.email}
                  </Badge>
                ))
              )}
            </div>
            <div className="text-sm tabular-nums">{ws.memberCount}</div>
            <div className="text-sm tabular-nums">{ws.projectCount}</div>
            <div>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8"
                    aria-label="Workspace actions"
                  >
                    <MoreHorizontal className="h-4 w-4" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem
                    variant="destructive"
                    onClick={() => setConfirmDelete(ws)}
                  >
                    <Trash2 className="mr-2 h-4 w-4" />
                    Delete workspace
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          </div>
        ))}
      </div>

      <Dialog
        open={Boolean(confirmDelete)}
        onOpenChange={(open) => !open && setConfirmDelete(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete workspace?</DialogTitle>
            <DialogDescription>
              This permanently removes <strong>{confirmDelete?.name}</strong>{` `}
              and cascades to all of its projects, issues, labels, comments,
              and attachments. This cannot be undone.
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
