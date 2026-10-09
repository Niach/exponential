import { useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  EmptyState,
  GlassSectionHeader,
  ListRow,
  Menu,
  MenuGestureHost,
  conceptIcon,
  menuProps,
  BoardGlyph,
  toast,
} from "@exp/ui"
import type { MenuSessionProps } from "@exp/ui"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { compactRelativeTime } from "@/lib/relative-time"
import { useDraftEntries } from "@/hooks/use-issue-drafts"
import { issueDraftCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { openDraftNavigation } from "@/lib/issue-draft-page"

const NavDraftsIcon = conceptIcon(`nav-drafts`)
const DeleteIcon = conceptIcon(`ui-delete`)
const DRAFT_MENU_KIND = `draft`

// EXP-878: the Drafts list — an issue list in everything but what it points
// at. A band over flat rows (`GlassSectionHeader` + `ListRow`, EXP-818), the
// status glyph a draft would file with, its board and when it was last
// touched. Rows carry no buttons: Discard lives in the row's context Menu
// (right-click / long-press). Clicking a row reopens the draft on the New
// issue page (EXP-1170), which is the only way back into one.
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

  const draftMenu = ({ target, open, onOpenChange }: MenuSessionProps) => (
    <Menu
      mode="pointer"
      anchor={target.anchor}
      open={open}
      onOpenChange={onOpenChange}
      returnFocus={target.origin}
      aria-label="Draft actions"
      title="Draft"
      entries={[
        {
          kind: `item`,
          id: `discard`,
          label: `Discard draft`,
          icon: DeleteIcon,
          destructive: true,
          disabled: deletingId === target.id,
          onSelect: () => void handleDelete(target.id),
        },
      ]}
    />
  )

  return (
    <MenuGestureHost menus={{ [DRAFT_MENU_KIND]: draftMenu }}>
      <div className={className} data-testid="drafts-list">
        <GlassSectionHeader label="Drafts" />
        {entries.map((entry) => (
          <ListRow
            key={entry.draft.id}
            interactive
            {...menuProps(DRAFT_MENU_KIND, entry.draft.id)}
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
              {compactRelativeTime(entry.draft.updatedAt)}
            </span>
          </ListRow>
        ))}
      </div>
    </MenuGestureHost>
  )
}
