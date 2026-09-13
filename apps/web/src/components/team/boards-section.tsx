import { useCallback, useEffect, useRef, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { Archive, Plus, Trash2 } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { getBoardIconName } from "@/lib/board-icons"
import { Pill } from "@/components/ui/pill"
import { Button } from "@/components/ui/button"
import { BoardGlyph } from "@/components/board-glyph"
import {
  GlassGroup,
  GlassRow,
  GlassSectionHeader,
} from "@/components/ui/glass-rows"
import {
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { CreateBoardDialog } from "@/components/create-board-dialog"
import {
  BoardIdentityRow,
  BoardPrefixRow,
} from "@/components/board-form-fields"
import { type PickerRepo } from "@/components/github-repo-picker"
import { BoardRepoField } from "@/components/board-repo-field"
import type { Board, Team } from "@/db/schema"

// EXP-862: the Boards settings section is no longer one list page with an
// edit DIALOG per row — the settings nav lists every board (the desktop
// IDE's EXP-288 nav), and this is the selected board's PAGE: name, icon,
// colour, the read-only prefix, the repository, and the archive/trash
// actions the list used to carry. Archived + trashed boards live on their
// own page (`BoardsTrashPage`), reached from the same nav.
export function BoardSettingsPage({
  board,
  team,
}: {
  board: Board
  team: Team
}) {
  const navigate = useNavigate()
  // Name is the one deferred write (save on blur) — swapping it live under
  // the user's caret would fight typing. Everything else mutates immediately
  // off the live row.
  const [name, setName] = useState(``)
  // Blur is the page's only commit path, and pulling a focused input out of
  // the DOM dispatches no focusout — so an uncommitted draft is mirrored
  // here and flushed when the page unmounts or swaps board, the way the
  // settings DIALOG this page replaced flushed on close. Keyed by board id
  // so a nav between boards can never write one board's draft onto another.
  const draftRef = useRef<{ boardId: string; name: string } | null>(null)
  const [busyRepo, setBusyRepo] = useState(false)
  const [repoError, setRepoError] = useState<string | null>(null)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [archiveOpen, setArchiveOpen] = useState(false)
  const [archiving, setArchiving] = useState(false)

  useEffect(() => {
    const boardId = board.id
    const original = board.name
    setName(original)
    setBusyRepo(false)
    setRepoError(null)
    draftRef.current = null
    return () => {
      const draft = draftRef.current
      if (!draft || draft.boardId !== boardId) return
      draftRef.current = null
      const trimmed = draft.name.trim()
      if (!trimmed || trimmed === original) return
      void trpc.boards.update.mutate({ boardId, name: trimmed })
    }
    // Reset keyed on the target board only — remote edits while the page is
    // open deliberately don't stomp a local in-progress rename.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [board.id])

  const changeName = (next: string) => {
    setName(next)
    draftRef.current = { boardId: board.id, name: next }
  }

  const saveName = () => {
    // Committed one way or another: nothing is left for the unmount flush.
    draftRef.current = null
    const trimmed = name.trim()
    if (!trimmed || trimmed === board.name) return
    void trpc.boards.update.mutate({ boardId: board.id, name: trimmed })
  }

  // Retargeting resets the board's branch pin server-side (EXP-712) — a
  // branch belongs to the repo it was picked in.
  const applyRepo = async (repositoryId: string | null) => {
    setBusyRepo(true)
    setRepoError(null)
    try {
      await trpc.boards.setRepository.mutate(
        { boardId: board.id, repositoryId },
        { context: { skipErrorToast: true } }
      )
    } catch (err) {
      setRepoError(err instanceof Error ? err.message : String(err))
    } finally {
      setBusyRepo(false)
    }
  }

  // A brand-new repo: register it (idempotent upsert/un-archive) then point
  // the board at the returned repository id.
  const handleConnect = async (picked: PickerRepo) => {
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

  // The board is gone from the nav either way, so land on the section index —
  // it forwards to whatever board is first now (or the empty state). A draft
  // rename dies with it: the board this page is unmounting away from has just
  // been archived or trashed.
  const leaveSection = () => {
    draftRef.current = null
    void navigate({
      to: `/t/$teamSlug/settings/boards`,
      params: { teamSlug: team.slug },
      replace: true,
    })
  }

  const handleDelete = async () => {
    setDeleting(true)
    try {
      await trpc.boards.delete.mutate({ boardId: board.id })
      setDeleteOpen(false)
      leaveSection()
    } finally {
      setDeleting(false)
    }
  }

  const handleArchive = async () => {
    setArchiving(true)
    try {
      await trpc.boards.archive.mutate({ boardId: board.id })
      setArchiveOpen(false)
      leaveSection()
    } finally {
      setArchiving(false)
    }
  }

  return (
    <div className="space-y-4">
      <GlassGroup>
        <BoardIdentityRow
          name={name}
          onNameChange={changeName}
          onNameBlur={saveName}
          icon={getBoardIconName(board)}
          onIconChange={(icon) =>
            void trpc.boards.update.mutate({ boardId: board.id, icon })
          }
          color={board.color}
          onColorChange={(color) =>
            void trpc.boards.update.mutate({ boardId: board.id, color })
          }
        />
        <BoardPrefixRow prefix={board.prefix} />
      </GlassGroup>

      {/* Member-level since EXP-557: retargeting uses the shared registry,
          and connect-new operates on YOUR OWN repos. */}
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

      {/* Archive is deliberately NOT destructive styling — it hides, it does
          not destroy. Both confirm first (desktop `board_detail.rs` twin). */}
      <div className="flex flex-wrap gap-2 pt-2">
        <Button variant="outline" size="sm" onClick={() => setArchiveOpen(true)}>
          <Archive />
          Archive board
        </Button>
        <Button
          variant="destructive"
          size="sm"
          onClick={() => setDeleteOpen(true)}
        >
          <Trash2 />
          Move to trash
        </Button>
      </div>

      <Dialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <DialogContent mobile="alert">
          <DialogHeader>
            <DialogTitle>Move board to trash</DialogTitle>
            <DialogDescription>
              Move{` `}
              <span className="font-semibold text-foreground">
                {board.name}
              </span>
              {` `}
              to the trash? It is kept for 48 hours (owners can restore it from
              Archived boards), then permanently deleted with all its issues.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel
              variant="outline"
              onClick={() => setDeleteOpen(false)}
              disabled={deleting}
            />
            <Button
              variant="destructive"
              onClick={() => void handleDelete()}
              disabled={deleting}
            >
              {deleting ? `Moving…` : `Move to trash`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={archiveOpen} onOpenChange={setArchiveOpen}>
        <DialogContent mobile="alert">
          <DialogHeader>
            <DialogTitle>Archive board</DialogTitle>
            <DialogDescription>
              Archive{` `}
              <span className="font-semibold text-foreground">
                {board.name}
              </span>
              ? It disappears for the whole team — from the sidebar, search,
              pickers and every issue list — along with all of its issues.
              Nothing is deleted, and owners can bring it back from Archived
              boards at any time.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel
              variant="outline"
              onClick={() => setArchiveOpen(false)}
              disabled={archiving}
            />
            <Button onClick={() => void handleArchive()} disabled={archiving}>
              {archiving ? `Archiving…` : `Archive board`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

// What the section shows while the team has no boards at all: the nav's
// "New board" entry, spelled out on the page the index lands on.
export function BoardsEmptyState({ team }: { team: Team }) {
  const [createOpen, setCreateOpen] = useState(false)
  return (
    <div>
      <GlassSectionHeader label="Boards" />
      <GlassRow className="flex-col items-start gap-3 px-3 py-3">
        <span className="text-sm text-muted-foreground">
          No boards in this team yet.
        </span>
        <Pill mode="action" onClick={() => setCreateOpen(true)}>
          <Plus />
          New board
        </Pill>
      </GlassRow>
      <CreateBoardDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        team={team}
      />
    </div>
  )
}

type ArchivedBoard = Awaited<
  ReturnType<typeof trpc.boards.listArchived.query>
>[number]

// Dates cross the tRPC boundary as ISO strings (no transformer), so coerce.
function formatArchivedOn(archivedAt: Date | string | null): string {
  if (!archivedAt) return `Archived`
  return `Archived ${new Date(archivedAt).toLocaleDateString(undefined, {
    year: `numeric`,
    month: `short`,
    day: `numeric`,
  })}`
}

// The team's archived boards plus the 48h trash. Both tRPC reads are the only
// way to see either: archived and trashed boards are excluded from the
// Electric shape, which is what keeps them hidden everywhere else.
export function BoardsTrashPage({ teamId }: { teamId: string }) {
  return (
    <div className="space-y-6">
      <ArchivedBoardsCard teamId={teamId} />
      <PendingDeletionCard teamId={teamId} />
    </div>
  )
}

// Always renders (header, blurb, then the rows or a "No archived boards."
// row): unlike the trash card, this page exists to be found empty too.
function ArchivedBoardsCard({ teamId }: { teamId: string }) {
  const [archived, setArchived] = useState<ArchivedBoard[] | null>(null)
  const [restoringId, setRestoringId] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      setArchived(await trpc.boards.listArchived.query({ teamId }))
    } catch {
      setArchived([])
    }
  }, [teamId])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const handleUnarchive = async (id: string) => {
    setRestoringId(id)
    try {
      await trpc.boards.unarchive.mutate({ boardId: id })
    } finally {
      setRestoringId(null)
      // Refresh on success AND failure — an unarchive can fail because the
      // board was trashed out from under us, in which case it belongs to the
      // trash card now and should drop off this one.
      await refresh()
    }
  }

  return (
    <div>
      <GlassSectionHeader
        leading={<Archive className="size-3.5 text-foreground/50" />}
        label="Archived boards"
      />
      <p className="px-1 pb-2 text-xs text-foreground/50">
        Archived boards and their issues are hidden from everyone in the team.
        Nothing is deleted: unarchive to bring a board back exactly as it was.
      </p>
      {!archived || archived.length === 0 ? (
        <GlassRow className="px-3 py-2 text-sm text-muted-foreground">
          No archived boards.
        </GlassRow>
      ) : (
        <div className="space-y-2">
          {archived.map((board) => (
            <GlassRow key={board.id} className="px-3 py-2.5">
              <BoardGlyph board={board} />
              <span className="min-w-0 flex-1 truncate text-sm font-medium">
                {board.name}
              </span>
              <Pill className="hidden shrink-0 font-mono sm:inline-flex">
                {board.prefix}
              </Pill>
              <span className="shrink-0 text-xs text-muted-foreground">
                {formatArchivedOn(board.archivedAt)}
              </span>
              <Pill
                mode="action"
                disabled={restoringId === board.id}
                onClick={() => void handleUnarchive(board.id)}
              >
                {restoringId === board.id ? `Unarchiving…` : `Unarchive`}
              </Pill>
            </GlassRow>
          ))}
        </div>
      )}
    </div>
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
function PendingDeletionCard({ teamId }: { teamId: string }) {
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
  }, [refresh])

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
    <div>
      <GlassSectionHeader
        leading={<Trash2 className="size-3.5 text-foreground/50" />}
        label="Trash"
      />
      <p className="px-1 pb-2 text-xs text-foreground/50">
        Deleted boards are kept for 48 hours, then permanently removed with all
        their issues.
      </p>
      <div className="space-y-2">
        {trashed.map((board) => (
          <GlassRow key={board.id} className="px-3 py-2.5">
            <BoardGlyph board={board} />
            <span className="min-w-0 flex-1 truncate text-sm font-medium">
              {board.name}
            </span>
            <Pill className="hidden shrink-0 font-mono sm:inline-flex">
              {board.prefix}
            </Pill>
            <span className="shrink-0 text-xs text-muted-foreground">
              {formatPurgeCountdown(board.purgeAt)}
            </span>
            <Pill
              mode="action"
              disabled={restoringId === board.id}
              onClick={() => void handleRestore(board.id)}
            >
              {restoringId === board.id ? `Restoring…` : `Restore`}
            </Pill>
          </GlassRow>
        ))}
      </div>
    </div>
  )
}
