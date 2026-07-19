import { useCallback, useEffect, useMemo, useState } from "react"
import { Github, Pencil, Plus, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { getBoardIcon } from "@/lib/board-icons"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
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
import { CreateBoardDialog } from "@/components/create-board-dialog"
import { BoardSettingsDialog } from "@/components/team/board-settings-dialog"
import { useTeamBoards } from "@/hooks/use-team-data"
import type { Team } from "@/db/schema"

type RepoList = Awaited<ReturnType<typeof trpc.repositories.list.query>>

export function TeamBoardsSection({
  team,
}: {
  team: Team
}) {
  const teamId = team.id
  const boards = useTeamBoards(teamId)
  const visibleBoards = boards.filter((p) => !p.archivedAt)

  // The team's connected repos — used to render each board's repo chip
  // (uuid → owner/name) and to feed the settings dialog's repo picker.
  const [repos, setRepos] = useState<RepoList | null>(null)
  const refreshRepos = useCallback(async () => {
    try {
      setRepos(await trpc.repositories.list.query({ teamId }))
    } catch {
      // The chips degrade to "No repository" if the list can't load.
    }
  }, [teamId])
  useEffect(() => {
    void refreshRepos()
  }, [refreshRepos])

  const repoMap = useMemo(
    () => new Map((repos ?? []).map((r) => [r.id, r])),
    [repos]
  )

  const [deleteTarget, setDeleteTarget] = useState<{
    id: string
    name: string
  } | null>(null)
  const [deleting, setDeleting] = useState(false)
  // Bumped on delete so the trash card refetches (restored boards re-appear
  // in the synced list on their own via Electric).
  const [trashRefreshKey, setTrashRefreshKey] = useState(0)
  const [createOpen, setCreateOpen] = useState(false)
  // Live row so edit-dialog toggle writes reflect immediately via Electric
  // sync (and a concurrently-trashed target closes the dialog).
  const [editTargetId, setEditTargetId] = useState<string | null>(null)
  const editTarget =
    visibleBoards.find((p) => p.id === editTargetId) ?? null

  const handleDelete = async () => {
    if (!deleteTarget) return
    setDeleting(true)
    try {
      await trpc.boards.delete.mutate({ boardId: deleteTarget.id })
      setTrashRefreshKey((key) => key + 1)
    } finally {
      setDeleting(false)
      setDeleteTarget(null)
    }
  }

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            Boards
            <Badge variant="secondary" className="text-xs font-normal">
              {visibleBoards.length}
            </Badge>
          </CardTitle>
          <CardDescription>
            Manage boards in this team.
          </CardDescription>
          <CardAction>
            <Button size="sm" onClick={() => setCreateOpen(true)}>
              <Plus className="mr-1 h-3.5 w-3.5" />
              New board
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          {visibleBoards.length === 0 ? (
            <div className="rounded-md border px-3 py-2 text-sm text-muted-foreground">
              No boards in this team yet.
            </div>
          ) : (
            <div className="divide-y rounded-md border">
              {visibleBoards.map((board) => {
                const repo = board.repositoryId
                  ? repoMap.get(board.repositoryId)
                  : undefined
                const TypeIcon = getBoardIcon(board)
                return (
                  <div
                    key={board.id}
                    className="flex cursor-pointer items-center gap-3 px-3 py-2.5 transition-colors hover:bg-accent/40"
                    onClick={() => setEditTargetId(board.id)}
                  >
                    <TypeIcon
                      className="h-4 w-4 shrink-0"
                      style={{ color: board.color }}
                    />
                    <span className="min-w-0 flex-1 truncate text-sm font-medium">
                      {board.name}
                    </span>
                    {repo && (
                      <Badge
                        variant="outline"
                        className="hidden max-w-[12rem] shrink-0 gap-1 sm:inline-flex"
                        title={repo?.fullName ?? `No repository`}
                      >
                        <Github className="h-3 w-3 shrink-0 text-muted-foreground" />
                        <span className="truncate">
                          {repo?.fullName ?? `No repository`}
                        </span>
                      </Badge>
                    )}
                    <Badge
                      variant="outline"
                      className="hidden shrink-0 font-mono text-xs sm:inline-flex"
                    >
                      {board.prefix}
                    </Badge>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 shrink-0 text-muted-foreground"
                      title="Board settings"
                      onClick={(e) => {
                        e.stopPropagation()
                        setEditTargetId(board.id)
                      }}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </Button>
                    {!board.isProtected && (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 shrink-0 text-muted-foreground hover:text-destructive"
                        title="Move to trash"
                        onClick={(e) => {
                          e.stopPropagation()
                          setDeleteTarget({
                            id: board.id,
                            name: board.name,
                          })
                        }}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    )}
                  </div>
                )
              })}
            </div>
          )}
        </CardContent>
      </Card>

      <PendingDeletionCard
        teamId={teamId}
        refreshKey={trashRefreshKey}
      />

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Move board to trash</DialogTitle>
            <DialogDescription>
              Move{` `}
              <span className="font-semibold text-foreground">
                {deleteTarget?.name}
              </span>
              {` `}
              to the trash? It is kept for 48 hours — owners can restore it from
              this page — then permanently deleted with all its issues.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setDeleteTarget(null)}
              disabled={deleting}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={handleDelete}
              disabled={deleting}
            >
              {deleting ? `Moving…` : `Move to trash`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <CreateBoardDialog
        open={createOpen}
        onOpenChange={(open) => {
          setCreateOpen(open)
          // An inline-connected repo should show up as a chip right away.
          if (!open) void refreshRepos()
        }}
        team={team}
      />

      <BoardSettingsDialog
        board={editTarget}
        team={team}
        onOpenChange={(open) => {
          if (!open) setEditTargetId(null)
        }}
        onRepoChanged={() => void refreshRepos()}
      />
    </>
  )
}

type TrashedBoard = Awaited<
  ReturnType<typeof trpc.boards.listDeleted.query>
>[number]

// Dates cross the tRPC boundary as ISO strings (no transformer), so coerce.
function formatPurgeCountdown(purgeAt: Date | string | null): string {
  if (!purgeAt) return `Purges soon`
  const ms = new Date(purgeAt).getTime() - Date.now()
  if (ms <= 0) return `Purging soon`
  const hours = Math.ceil(ms / (60 * 60 * 1000))
  return `Purges in ~${hours}h`
}

// The team's trashed boards. Renders NOTHING when the trash is empty —
// the trash surface only exists while something is pending deletion.
function PendingDeletionCard({
  teamId,
  refreshKey,
}: {
  teamId: string
  refreshKey: number
}) {
  const [trashed, setTrashed] = useState<TrashedBoard[] | null>(null)
  const [restoringId, setRestoringId] = useState<string | null>(null)
  // Bumped every 60s so the purge countdown re-renders while the page stays open.
  const [, setTick] = useState(0)

  const refresh = useCallback(async () => {
    try {
      setTrashed(await trpc.boards.listDeleted.query({ teamId }))
    } catch {
      setTrashed([])
    }
  }, [teamId])

  useEffect(() => {
    void refresh()
  }, [refresh, refreshKey])

  useEffect(() => {
    const id = setInterval(() => setTick((tick) => tick + 1), 60_000)
    return () => clearInterval(id)
  }, [])

  const handleRestore = async (id: string) => {
    setRestoringId(id)
    try {
      await trpc.boards.restore.mutate({ boardId: id })
    } finally {
      setRestoringId(null)
      // Refresh on success AND failure — a restore can fail because the row was
      // purged out from under us, in which case it should drop off the card.
      await refresh()
    }
  }

  if (!trashed || trashed.length === 0) return null

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <Trash2 className="h-4 w-4" />
          Trash
        </CardTitle>
        <CardDescription>
          Deleted boards are kept for 48 hours, then permanently removed with
          all their issues.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="divide-y rounded-md border">
          {trashed.map((board) => {
            const TypeIcon = getBoardIcon(board)
            return (
              <div
                key={board.id}
                className="flex items-center gap-3 px-3 py-2.5"
              >
                <TypeIcon
                  className="h-4 w-4 shrink-0"
                  style={{ color: board.color }}
                />
                <span className="min-w-0 flex-1 truncate text-sm font-medium">
                  {board.name}
                </span>
                <Badge
                  variant="outline"
                  className="hidden shrink-0 font-mono text-xs sm:inline-flex"
                >
                  {board.prefix}
                </Badge>
                <span className="shrink-0 text-xs text-muted-foreground">
                  {formatPurgeCountdown(board.purgeAt)}
                </span>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 shrink-0"
                  disabled={restoringId === board.id}
                  onClick={() => void handleRestore(board.id)}
                >
                  {restoringId === board.id ? `Restoring…` : `Restore`}
                </Button>
              </div>
            )
          })}
        </div>
      </CardContent>
    </Card>
  )
}
