import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  EmptyState,
  Button,
  GlassSectionHeader,
  ListRow,
  conceptIcon,
  BoardGlyph,
  toast,
} from "@exp/ui"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { relativeTime } from "@/components/comment-rows/format"
import { useDraftEntries } from "@/hooks/use-issue-drafts"
import { issueDraftCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { openDraftNavigation } from "@/lib/issue-draft-page"

const NavDraftsIcon = conceptIcon(`nav-drafts`)
const DeleteIcon = conceptIcon(`ui-delete`)

// EXP-878: the Drafts list — an issue list in everything but what it points
// at. A band over flat rows (`GlassSectionHeader` + `ListRow`, EXP-818), the
// status glyph a draft would file with, its board, when it was last touched,
// and a hover-revealed delete at the trailing end. Clicking a row reopens the
// draft on the New issue page (EXP-1170), which is the only way back into one.
export function DraftsList({
  teamId,
  teamSlug,
  className,
  from = `drafts`,
}: {
  teamId: string | undefined
  teamSlug: string
  className?: string
  /** The origin the reopened draft returns to: the md+ Drafts page, or the
   *  phone inbox's Drafts tab. */
  from?: `drafts` | `inbox:drafts`
}) {
  const entries = useDraftEntries(teamId)
  const navigate = useNavigate()
  const [deletingId, setDeletingId] = useState<string | null>(null)

  const handleDelete = async (draftId: string) => {
    if (deletingId) return
    setDeletingId(draftId)
    try {
      const { txId } = await trpc.issueDrafts.delete.mutate({ id: draftId })
      // Settle on the synced row, not the tRPC answer: the list is rendered
      // straight off the collection, so clearing early would flash the row
      // back in until Electric caught up.
      await issueDraftCollection.utils.awaitTxId(txId)
    } catch {
      toast.error(`Could not discard the draft`)
    } finally {
      setDeletingId(null)
    }
  }

  if (entries.length === 0) {
    return (
      <EmptyState
        icon={NavDraftsIcon}
        title="No drafts"
        description="A new issue you leave with something in it is kept here until you file it."
      />
    )
  }

  return (
    <div className={className} data-testid="drafts-list">
      <GlassSectionHeader label="Drafts" count={entries.length} />
      {entries.map((entry) => (
        <ListRow
          key={entry.draft.id}
          interactive
          className="group"
          onClick={() =>
            void navigate(
              openDraftNavigation({
                teamSlug,
                draftId: entry.draft.id,
                boardId: entry.board.id,
                from,
              })
            )
          }
        >
          <IssueStatusIcon
            issue={{ status: `backlog`, statusId: entry.draft.statusId }}
            className="size-4 shrink-0"
          />
          <span
            className={`min-w-0 flex-1 truncate text-sm ${
              entry.untitled ? `text-muted-foreground italic` : ``
            }`}
          >
            {entry.title}
          </span>
          <span className="hidden shrink-0 items-center gap-1.5 text-xs text-muted-foreground sm:flex">
            <BoardGlyph board={entry.board} className="size-3" />
            {entry.board.name}
          </span>
          <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
            {relativeTime(entry.draft.updatedAt)}
          </span>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={`Discard draft`}
            disabled={deletingId === entry.draft.id}
            // Touch surfaces have no hover to reveal it (EXP-858's rule for
            // row-level controls), so the phone list shows it outright.
            className="shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 max-md:opacity-100"
            onClick={(event) => {
              event.stopPropagation()
              void handleDelete(entry.draft.id)
            }}
          >
            <DeleteIcon className="size-3.5" />
          </Button>
        </ListRow>
      ))}
    </div>
  )
}
