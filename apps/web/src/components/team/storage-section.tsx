import { useCallback, useEffect, useMemo, useState } from "react"
import { Link } from "@tanstack/react-router"
import { useLiveQuery } from "@tanstack/react-db"
import { toast } from "sonner"
import { Eraser, LoaderCircle, Trash2 } from "lucide-react"
import type { Issue, Team } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { useBillingPlan, invalidateBillingCache } from "@/hooks/use-billing"
import { useTeamBoards, useTeamUsers } from "@/hooks/use-team-data"
import { formatAttachmentSize, getAttachmentIcon } from "@/lib/attachment-files"
import { formatStorage, UsageBar } from "@/components/team/billing-section"
import { Pill } from "@/components/ui/pill"
import { Button } from "@/components/ui/button"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import { ImagePreviewDialog } from "@/components/image-preview-dialog"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"

type StorageList = Awaited<
  ReturnType<typeof trpc.attachments.listForTeam.query>
>
type StorageRow = StorageList[`attachments`][number]

function formatDate(value: Date | string) {
  return new Date(value).toLocaleDateString(undefined, {
    year: `numeric`,
    month: `short`,
    day: `numeric`,
  })
}

/**
 * EXP-297 team file manager (web-only, like billing): what the team's
 * attachments cost, which images no markdown references any more, per-file
 * delete, and the bulk sweep of unreferenced images.
 */
export function TeamStorageSection({
  team,
  teamSlug,
}: {
  team: Team
  teamSlug: string
}) {
  const [list, setList] = useState<StorageList | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [pendingDelete, setPendingDelete] = useState<StorageRow | null>(null)
  const [previewRow, setPreviewRow] = useState<StorageRow | null>(null)
  const [deleting, setDeleting] = useState(false)
  const [sweepConfirmOpen, setSweepConfirmOpen] = useState(false)
  const [sweeping, setSweeping] = useState(false)

  const billingPlan = useBillingPlan(team.id)
  const boards = useTeamBoards(team.id)
  const { userMap } = useTeamUsers(team.id)

  const { data: issueRows } = useLiveQuery((query) =>
    query.from({ issues: issueCollection })
  )

  const issuesById = useMemo(
    () => new Map(((issueRows ?? []) as Issue[]).map((row) => [row.id, row])),
    [issueRows]
  )
  const boardSlugById = useMemo(
    () => new Map(boards.map((board) => [board.id, board.slug])),
    [boards]
  )

  const load = useCallback(async () => {
    try {
      const result = await trpc.attachments.listForTeam.query({
        teamId: team.id,
      })
      setList(result)
      setLoadError(null)
    } catch (error) {
      setLoadError(
        error instanceof Error ? error.message : `Couldn't load attachments`
      )
    }
  }, [team.id])

  useEffect(() => {
    void load()
  }, [load])

  const rows = list?.attachments ?? []
  const sweepCandidateCount = rows.filter(
    (row) => row.isImage && !row.referenced
  ).length

  const handleDelete = async () => {
    if (!pendingDelete) return
    setDeleting(true)
    try {
      await trpc.attachments.delete.mutate({ id: pendingDelete.id })
      setPendingDelete(null)
      invalidateBillingCache()
      await load()
      toast.success(`Attachment deleted`)
    } catch (error) {
      toast.error(
        error instanceof Error && error.message
          ? error.message
          : `Couldn't delete the attachment`
      )
    } finally {
      setDeleting(false)
    }
  }

  const handleSweep = async () => {
    setSweeping(true)
    try {
      const result = await trpc.attachments.sweepUnreferencedImages.mutate({
        teamId: team.id,
      })
      setSweepConfirmOpen(false)
      invalidateBillingCache()
      await load()

      if (result.deletedCount === 0) {
        toast.success(
          result.skippedRecentCount > 0
            ? `Nothing swept. ${result.skippedRecentCount} recent upload${
                result.skippedRecentCount === 1 ? `` : `s`
              } are still inside the 24h grace window.`
            : `Nothing to sweep. Every image is still referenced.`
        )
        return
      }

      toast.success(
        `Deleted ${result.deletedCount} image${
          result.deletedCount === 1 ? `` : `s`
        }, freeing ${formatAttachmentSize(result.freedBytes)}.`
      )
    } catch (error) {
      toast.error(
        error instanceof Error && error.message
          ? error.message
          : `Couldn't sweep unreferenced images`
      )
    } finally {
      setSweeping(false)
    }
  }

  return (
    <div className="space-y-4">
      <div>
        <GlassSectionHeader
          label="Storage"
          trailing={
            <Pill
              mode="action"
              disabled={sweepCandidateCount === 0 || sweeping}
              onClick={() => setSweepConfirmOpen(true)}
            >
              <Eraser />
              Sweep unreferenced images
              {sweepCandidateCount > 0 ? ` (${sweepCandidateCount})` : ``}
            </Pill>
          }
        />
        <p className="px-1 pb-2 text-xs text-foreground/50">
          Files and images attached to this team&apos;s issues. Deleting an
          attachment is permanent.
        </p>
        <div className="space-y-3">
          {billingPlan && billingPlan.plan !== `unlimited` && (
            <UsageBar
              label="Attachment storage"
              current={billingPlan.usage.storageMb}
              max={billingPlan.limits.storageMb}
              formatValue={formatStorage}
            />
          )}
          {list && (
            <p className="text-sm text-muted-foreground">
              {rows.length} attachment{rows.length === 1 ? `` : `s`} ·{` `}
              {formatAttachmentSize(list.totalBytes)}
            </p>
          )}
        </div>
      </div>

      <div>
        {loadError ? (
          <p className="text-sm text-destructive">{loadError}</p>
        ) : !list ? (
          <p className="flex items-center gap-2 text-sm text-muted-foreground">
            <LoaderCircle className="size-3.5 animate-spin" />
            Loading attachments...
          </p>
        ) : rows.length === 0 ? (
          <p className="text-sm text-muted-foreground">No attachments yet.</p>
        ) : (
          // IDE-parity rows (EXP-316) — the old fixed table overflowed the
          // settings column and clipped the status + delete controls.
          // EXP-718: in a wide enough list every cell but the filename is a
          // definite width (the desktop's STORAGE_COL_* ladder), so size/
          // issue/author/date/status stack in straight columns instead of
          // zig-zagging around each row's own content; author and date join
          // as the list widens. In a narrow one the same cells STACK under a
          // full-width filename, so a phone never truncates the name to
          // "imag…" while three trailing chips keep their full width. The
          // thresholds are CONTAINER queries on the list: the settings column
          // is ~450px at the `md` viewport (sidebar), so viewport breakpoints
          // overflowed the ladder exactly where the tab strip turns into it.
          <ul className="@container flex flex-col gap-2">
            {rows.map((row) => {
              const Icon = getAttachmentIcon(row.contentType)
              const issue = issuesById.get(row.issueId)
              const boardSlug = issue
                ? boardSlugById.get(issue.boardId)
                : undefined
              const uploader = row.uploaderId
                ? userMap.get(row.uploaderId)
                : undefined

              return (
                <GlassRow
                  key={row.id}
                  asChild
                  className="min-w-0 gap-2 px-2 py-1.5"
                >
                  <li>
                    <Icon className="size-4 shrink-0 text-muted-foreground" />
                    {/* Stacked when narrow; `@lg:contents` flattens the stack
                        so the cells become the row's own flex items. */}
                    <div className="flex min-w-0 flex-1 flex-col gap-1 @lg:contents">
                      {row.isImage ? (
                        <button
                          type="button"
                          className="min-w-0 cursor-pointer truncate text-left text-sm hover:underline @lg:flex-1"
                          title={`Preview ${row.filename}`}
                          onClick={() => setPreviewRow(row)}
                        >
                          {row.filename}
                        </button>
                      ) : (
                        <span
                          className="min-w-0 truncate text-sm @lg:flex-1"
                          title={row.filename}
                        >
                          {row.filename}
                        </span>
                      )}
                      <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground @lg:contents">
                        <span className="shrink-0 tabular-nums @lg:w-20">
                          {formatAttachmentSize(row.sizeBytes)}
                        </span>
                        <span className="flex shrink-0 @lg:w-24">
                          {issue && boardSlug ? (
                            <Pill mode="action" asChild className="font-mono">
                              <Link
                                to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
                                params={{
                                  teamSlug,
                                  boardSlug,
                                  issueIdentifier: issue.identifier,
                                }}
                                title={issue.title}
                              >
                                #{issue.identifier}
                              </Link>
                            </Pill>
                          ) : (
                            <span className="@max-lg:hidden">—</span>
                          )}
                        </span>
                        <span className="min-w-0 truncate @lg:hidden @lg:w-28 @2xl:inline">
                          {uploader?.name || uploader?.email || `—`}
                        </span>
                        <span className="shrink-0 whitespace-nowrap @lg:hidden @lg:w-24 @3xl:inline">
                          {formatDate(row.createdAt)}
                        </span>
                        <span className="flex shrink-0 @lg:w-26">
                          {!row.isImage ? (
                            <Pill>File</Pill>
                          ) : row.referenced ? (
                            <Pill>In use</Pill>
                          ) : (
                            <Pill>Unreferenced</Pill>
                          )}
                        </span>
                      </div>
                    </div>
                    <Button
                      variant="glass"
                      size="icon-sm"
                      className="shrink-0 hover:text-destructive"
                      aria-label={`Delete ${row.filename}`}
                      onClick={() => setPendingDelete(row)}
                    >
                      <Trash2 />
                    </Button>
                  </li>
                </GlassRow>
              )
            })}
          </ul>
        )}
      </div>

      {previewRow && (
        <ImagePreviewDialog
          open
          onOpenChange={(open) => {
            if (!open) setPreviewRow(null)
          }}
          src={`/api/attachments/${previewRow.id}`}
          alt={previewRow.filename}
          label={previewRow.filename}
        />
      )}

      <AlertDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => {
          if (!open && !deleting) setPendingDelete(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete this attachment?</AlertDialogTitle>
            <AlertDialogDescription>
              {pendingDelete?.filename} is deleted for everyone and cannot be
              restored. Every description or comment that embeds it is rewritten
              in the same step, replacing the image with a plain &ldquo;deleted
              image&rdquo; note.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleting}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-white hover:bg-destructive/90"
              disabled={deleting}
              onClick={(event) => {
                event.preventDefault()
                void handleDelete()
              }}
            >
              {deleting ? `Deleting...` : `Delete attachment`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog
        open={sweepConfirmOpen}
        onOpenChange={(open) => {
          if (!open && !sweeping) setSweepConfirmOpen(false)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Sweep {sweepCandidateCount} unreferenced image
              {sweepCandidateCount === 1 ? `` : `s`}?
            </AlertDialogTitle>
            <AlertDialogDescription>
              These images are no longer embedded in any description or comment
              in this team, so deleting them changes no text. Images uploaded in
              the last 24 hours are kept. They may still be sitting in an
              unsaved draft. Files are never swept.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={sweeping}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              className="bg-destructive text-white hover:bg-destructive/90"
              disabled={sweeping}
              onClick={(event) => {
                event.preventDefault()
                void handleSweep()
              }}
            >
              {sweeping ? `Sweeping...` : `Sweep images`}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}
