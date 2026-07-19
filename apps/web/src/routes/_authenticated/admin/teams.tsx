import { useState } from "react"
import { createFileRoute, Link, useRouter } from "@tanstack/react-router"
import { MoreHorizontal, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { Badge } from "@/components/ui/badge"
import { PlanBadge, formatStorageMb } from "./-shared"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
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

type AdminTeam = Awaited<
  ReturnType<typeof trpc.admin.listTeams.query>
>[number]

export const Route = createFileRoute(`/_authenticated/admin/teams`)({
  loader: async () => {
    const teams = await trpc.admin.listTeams.query()
    return { teams }
  },
  component: AdminTeams,
})

function AdminTeams() {
  const router = useRouter()
  const { teams } = Route.useLoaderData()
  const [search, setSearch] = useState(``)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [confirmDelete, setConfirmDelete] = useState<AdminTeam | null>(
    null
  )

  const filteredTeams = teams.filter((ws: AdminTeam) => {
    if (!search) return true
    const q = search.toLowerCase()
    return ws.name.toLowerCase().includes(q) || ws.slug.toLowerCase().includes(q)
  })

  const handleDelete = async () => {
    if (!confirmDelete) return
    setError(null)
    setBusy(confirmDelete.id)
    try {
      await trpc.admin.deleteTeam.mutate({
        teamId: confirmDelete.id,
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
    <div className="mx-auto max-w-5xl space-y-4 p-4 md:p-6">
      <div>
        <h1 className="text-2xl font-bold">Teams</h1>
        <p className="text-sm text-muted-foreground">
          {filteredTeams.length}{` `}
          {filteredTeams.length === 1 ? `team` : `teams`} on this
          instance.
        </p>
      </div>

      <Input
        placeholder="Search by name or slug…"
        value={search}
        onChange={(e) => setSearch(e.target.value)}
        className="max-w-sm"
      />

      {error && (
        <div className="rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <div className="rounded-md border">
        <div className="hidden md:grid grid-cols-[1fr_110px_1fr_70px_70px_70px_80px_40px] items-center gap-3 border-b px-4 py-2 text-xs font-medium text-muted-foreground">
          <div>Name</div>
          <div>Plan</div>
          <div>Owners</div>
          <div>Members</div>
          <div>Boards</div>
          <div>Issues</div>
          <div>Storage</div>
          <div />
        </div>
        {filteredTeams.map((ws: AdminTeam) => (
          <div
            key={ws.id}
            className="flex flex-col md:grid md:grid-cols-[1fr_110px_1fr_70px_70px_70px_80px_40px] md:items-center gap-2 md:gap-3 border-b px-4 py-3 last:border-b-0"
          >
            <div className="flex items-start gap-2 min-w-0">
              <Link
                to="/admin/teams/$teamId"
                params={{ teamId: ws.id }}
                className="min-w-0 flex-1 group"
              >
                <div className="text-sm font-medium truncate group-hover:underline">
                  {ws.name}
                </div>
                <div className="text-xs text-muted-foreground truncate">
                  /{ws.slug}
                </div>
              </Link>
              <div className="md:hidden shrink-0">
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8"
                      aria-label="Team actions"
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
                      Delete team
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            </div>
            {/* Mobile meta row */}
            <div className="flex md:hidden items-center flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
              <PlanBadge plan={ws.plan} compApplied={ws.compApplied} />
              <span>
                {ws.memberCount}{` `}
                {ws.memberCount === 1 ? `member` : `members`}
              </span>
              <span>
                {ws.boardCount}{` `}
                {ws.boardCount === 1 ? `board` : `boards`}
              </span>
              <span>
                {ws.issueCount}{` `}
                {ws.issueCount === 1 ? `issue` : `issues`}
              </span>
              <span>{formatStorageMb(ws.storageMb)}</span>
              <div className="flex flex-wrap gap-1 w-full">
                {ws.owners.length === 0 ? (
                  <span>No owners</span>
                ) : (
                  ws.owners.map((o: { id: string; name: string | null; email: string }) => (
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
            </div>
            {/* Desktop columns */}
            <div className="hidden md:block">
              <PlanBadge plan={ws.plan} compApplied={ws.compApplied} />
            </div>
            <div className="hidden md:flex flex-wrap gap-1 min-w-0">
              {ws.owners.length === 0 ? (
                <span className="text-xs text-muted-foreground">
                  No owners
                </span>
              ) : (
                ws.owners.map((o: { id: string; name: string | null; email: string }) => (
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
            <div className="hidden md:block text-sm tabular-nums">
              {ws.memberCount}
            </div>
            <div className="hidden md:block text-sm tabular-nums">
              {ws.boardCount}
            </div>
            <div className="hidden md:block text-sm tabular-nums">
              {ws.issueCount}
            </div>
            <div className="hidden md:block text-xs text-muted-foreground tabular-nums">
              {formatStorageMb(ws.storageMb)}
            </div>
            <div className="hidden md:block">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-8 w-8"
                    aria-label="Team actions"
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
                    Delete team
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
            <DialogTitle>Delete team?</DialogTitle>
            <DialogDescription>
              This permanently removes <strong>{confirmDelete?.name}</strong>{` `}
              and cascades to all of its boards, issues, labels, comments,
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
