import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { getBoardIconName } from "@/lib/board-icons"
import { Badge } from "@/components/ui/badge"
import { Label } from "@/components/ui/label"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  BoardIconColorFields,
  BoardNameField,
} from "@/components/board-form-fields"
import { type PickerRepo } from "@/components/github-repo-picker"
import { ConnectedRepoPicker } from "@/components/connected-repo-picker"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import type { Board, Team } from "@/db/schema"

const PROTECTED_REPO_HINT = `This board is protected — its repository can't be changed.`

// Consolidated per-board settings (EXP-159): everything the create dialog
// offers, editable after creation — name, icon, color, repository. Receives
// the LIVE Electric row so every write reflects via sync; a concurrently-
// trashed board closes the dialog (board becomes null).
export function BoardSettingsDialog({
  board,
  team,
  onOpenChange,
  onRepoChanged,
}: {
  board: Board | null
  team: Team
  onOpenChange: (open: boolean) => void
  onRepoChanged: () => void
}) {
  const { canManageRepos } = useTeamPermissions(team)

  // Name is the one deferred write (save on blur / close) — swapping it live
  // under the user's caret would fight typing. Everything else mutates
  // immediately off the live row.
  const [name, setName] = useState(``)
  const [busyRepo, setBusyRepo] = useState(false)
  const [repoError, setRepoError] = useState<string | null>(null)

  useEffect(() => {
    setName(board?.name ?? ``)
    setBusyRepo(false)
    setRepoError(null)
    // Reset keyed on the target board only — remote edits while the dialog
    // is open deliberately don't stomp a local in-progress rename.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [board?.id])

  const saveName = (target: Board) => {
    const trimmed = name.trim()
    if (!trimmed || trimmed === target.name) return
    void trpc.boards.update.mutate({ id: target.id, name: trimmed })
  }

  const applyRepo = async (repositoryId: string) => {
    if (!board) return
    setBusyRepo(true)
    setRepoError(null)
    try {
      await trpc.boards.setRepository.mutate(
        { boardId: board.id, repositoryId },
        { context: { skipErrorToast: true } }
      )
      onRepoChanged()
    } catch (err) {
      setRepoError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusyRepo(false)
    }
  }

  // A brand-new repo: register it (idempotent upsert/un-archive) then point
  // the board at the returned repository id.
  const handleConnect = async (picked: PickerRepo) => {
    if (!board) return
    setBusyRepo(true)
    setRepoError(null)
    try {
      const { repository } = await trpc.repositories.add.mutate(
        {
          teamId: team.id,
          fullName: picked.fullName,
          defaultBranch: picked.defaultBranch,
          private: picked.private,
        },
        { context: { skipErrorToast: true } }
      )
      if (repository) {
        await applyRepo(repository.id)
        return
      }
      setRepoError(`Could not connect ${picked.fullName}.`)
    } catch (err) {
      setRepoError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusyRepo(false)
    }
  }

  return (
    <Dialog
      open={board !== null}
      onOpenChange={(open) => {
        // Blur doesn't reliably fire on unmount — flush a pending rename
        // before the dialog goes away.
        if (!open && board) saveName(board)
        onOpenChange(open)
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Board settings</DialogTitle>
          <DialogDescription>
            Same settings as board creation — changes apply to{` `}
            <span className="font-medium text-foreground">{board?.name}</span>
            {` `}immediately.
          </DialogDescription>
        </DialogHeader>

        {board && (
          <div className="space-y-4">
            <BoardNameField
              value={name}
              onChange={setName}
              onBlur={() => saveName(board)}
            />

            <div className="space-y-2">
              <Label>Prefix</Label>
              <div className="flex items-center gap-2">
                <Badge variant="outline" className="font-mono text-xs">
                  {board.prefix}
                </Badge>
                <p className="text-xs text-muted-foreground">
                  The prefix can&apos;t be changed after creation.
                </p>
              </div>
            </div>

            <BoardIconColorFields
              icon={getBoardIconName(board)}
              onIconChange={(icon) =>
                void trpc.boards.update.mutate({ id: board.id, icon })
              }
              color={board.color}
              onColorChange={(color) =>
                void trpc.boards.update.mutate({ id: board.id, color })
              }
            />

            {canManageRepos && (
              <div className="space-y-2">
                <Label>Repository</Label>
                {board.isProtected ? (
                  <p className="rounded-md border px-3 py-2 text-xs text-muted-foreground">
                    {PROTECTED_REPO_HINT}
                  </p>
                ) : (
                  <>
                    <ConnectedRepoPicker
                      teamId={team.id}
                      value={board.repositoryId}
                      disabled={busyRepo}
                      onSelectRegistry={(repo) => void applyRepo(repo.id)}
                      onConnectNew={(picked) => void handleConnect(picked)}
                    />
                    <p className="text-xs text-muted-foreground">
                      New &ldquo;Start coding&rdquo; launches use the selected
                      repo; existing worktrees keep working locally.
                    </p>
                  </>
                )}
                {repoError && (
                  <p className="text-xs text-destructive">{repoError}</p>
                )}
              </div>
            )}
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
