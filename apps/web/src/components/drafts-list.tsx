import { useMemo, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  GlassSectionHeader,
  ListRow,
  Menu,
  MenuGestureHost,
  conceptIcon,
  menuProps,
  BoardGlyph,
  TeamAvatar,
} from "@exp/ui"
import type { MenuSessionProps } from "@exp/ui"
import { IssueStatusIcon } from "@/components/issue-properties/status-dropdown"
import { compactRelativeTime } from "@/lib/relative-time"
import { useCrossTeamDraftEntriesWithReady, useDraftEntries } from "@/hooks/use-issue-drafts"
import { issueDraftCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { Empty } from "@/components/empty"
import { openDraftNavigation } from "@/lib/issue-draft-page"

const DeleteIcon = conceptIcon(`ui-delete`)
const DRAFT_MENU_KIND = `draft`

// EXP-878: the Drafts list — an issue list in everything but what it points
// at. A band over flat rows (`GlassSectionHeader` + `ListRow`, EXP-818), the
// status glyph a draft would file with, its board and when it was last
// touched. #EXP-2027: the team-less Drafts page (web md+) lists every
// member team's (`teams`), each row with its team mark, opening in its own
// team. Rows carry no buttons: Discard lives in the row's context Menu
// (right-click / long-press). Clicking a row reopens the draft on the New
// issue page (EXP-1170), which is the only way back into one.
/** A team of the team-less list (its mark + where its rows open). */
export interface DraftsListTeam {
  id: string
  slug: string
  name: string
  iconUrl: string | null
}

export function DraftsList({
  teamId,
  teamSlug,
  teams,
  className,
  from = `drafts`,
}: {
  teamId: string | undefined
  teamSlug: string
  /** #EXP-2027: every member team, in team order (the team-less page);
   *  `teamId`/`teamSlug` then only name the fallback. */
  teams?: readonly DraftsListTeam[]
  className?: string
  /** The origin the reopened draft returns to: the md+ Drafts page, or the
   *  phone inbox's Drafts tab. */
  from?: `drafts` | `inbox:drafts`
}) {
  const teamEntries = useDraftEntries(teams ? undefined : teamId)
  const teamIds = useMemo(() => teams?.map((team) => team.id) ?? [], [teams])
  const cross = useCrossTeamDraftEntriesWithReady(teamIds)
  const entries = teams ? cross.entries : teamEntries
  const teamById = useMemo(() => new Map((teams ?? []).map((team) => [team.id, team])), [teams])
  const navigate = useNavigate()
  const [deletingId, setDeletingId] = useState<string | null>(null)

  const handleDelete = async (draftId: string) => {
    if (deletingId) return
    setDeletingId(draftId)
    try {
      const { txId } = await trpc.issueDrafts.delete.mutate(
        { id: draftId },
        SKIP_ERROR_TOAST
      )
      // Settle on the synced row, not the tRPC answer: the list is rendered
      // straight off the collection, so clearing early would flash the row
      // back in until Electric caught up.
      await issueDraftCollection.utils.awaitTxId(txId)
    } catch (error) {
      // A delete: no Retry.
      failedAction(`discard`, `the draft`, error)
    } finally {
      setDeletingId(null)
    }
  }

  if (entries.length === 0) {
    return (
      <Empty id="drafts" />
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
        {entries.map((entry) => {
          const team = teams ? teamById.get(entry.draft.teamId) : undefined
          return (
          <ListRow
            key={entry.draft.id}
            interactive
            {...menuProps(DRAFT_MENU_KIND, entry.draft.id)}
            onClick={() =>
              void navigate(
                openDraftNavigation({
                  teamSlug: team?.slug ?? teamSlug,
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
            <span className="flex max-w-[140px] shrink-0 items-center gap-1.5 text-xs text-muted-foreground sm:max-w-none">
              <BoardGlyph board={entry.board} className="size-3 shrink-0" />
              <span className="truncate">{entry.board.name}</span>
            </span>
            {team && (
              // #EXP-2027: the row's team mark.
              <span title={team.name} className="flex shrink-0 items-center">
                <TeamAvatar iconUrl={team.iconUrl} size={16} />
              </span>
            )}
            <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
              {compactRelativeTime(entry.draft.updatedAt)}
            </span>
          </ListRow>
          )
        })}
      </div>
    </MenuGestureHost>
  )
}
