import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { getBoardIconName } from "@/lib/board-icons"
import { Pill } from "@/components/ui/pill"
import { Label } from "@/components/ui/label"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  BoardColorField,
  BoardNameField,
} from "@/components/board-form-fields"
import { type PickerRepo } from "@/components/github-repo-picker"
import { BoardRepoField } from "@/components/board-repo-field"
import type { Board, Team } from "@/db/schema"

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
    void trpc.boards.update.mutate({ boardId: target.id, name: trimmed })
  }

  // Retargeting resets the board's branch pin server-side (EXP-712) — a
  // branch belongs to the repo it was picked in.
  const applyRepo = async (repositoryId: string | null) => {
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
        </DialogHeader>

        {board && (
          <DialogBody className="space-y-4">
            <BoardNameField
              value={name}
              onChange={setName}
              onBlur={() => saveName(board)}
              icon={getBoardIconName(board)}
              onIconChange={(icon) =>
                void trpc.boards.update.mutate({ boardId: board.id, icon })
              }
              color={board.color}
            />

            {/* Read-only: identifiers are minted from it. */}
            <div className="space-y-2">
              <Label>Prefix</Label>
              <div>
                <Pill className="font-mono">{board.prefix}</Pill>
              </div>
            </div>

            <BoardColorField
              color={board.color}
              onColorChange={(color) =>
                void trpc.boards.update.mutate({ boardId: board.id, color })
              }
            />

            {/* Member-level since EXP-557: retargeting uses the shared
                registry, and connect-new operates on YOUR OWN repos. */}
            <BoardRepoField
              teamId={team.id}
              repositoryId={board.repositoryId}
              disabled={busyRepo}
              onSelectRegistry={(repo) => void applyRepo(repo?.id ?? null)}
              onConnectNew={(picked) => void handleConnect(picked)}
              branch={board.defaultBranch}
              onBranchChange={(defaultBranch) => {
                setRepoError(null)
                trpc.boards.update
                  .mutate({ boardId: board.id, defaultBranch })
                  .catch((err: unknown) =>
                    setRepoError(err instanceof Error ? err.message : String(err))
                  )
              }}
              error={repoError}
            />
          </DialogBody>
        )}
      </DialogContent>
    </Dialog>
  )
}
