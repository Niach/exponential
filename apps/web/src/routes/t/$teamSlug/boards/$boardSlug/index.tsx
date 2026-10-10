import { useMemo, useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { BoardNotFound } from "@/components/board-not-found"
import { BulkActionBar } from "@/components/bulk-action-bar"
import { GettingStartedSection } from "@/components/getting-started/getting-started-section"
import { IssueList } from "@/components/issue-list"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useBoardViewData } from "@/hooks/use-board-view-data"
import { useOpenNewDraft } from "@/hooks/use-open-new-draft"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import type { StatusRowOption } from "@/lib/team-statuses"
import { pageTitle, usePageTitle } from "@/lib/page-title"

// validateSearch drops anything unrecognised.
type BoardSearch = {
  /** EXP-856: the origin token a detail hands BACK when it returns here
   *  (`lib/detail-origin.ts`). A board is a list screen, so the sidebar keeps
   *  its main menu either way — the param only has to survive the round trip
   *  instead of being dropped on the way in. */
  from?: string
}

export const Route = createFileRoute(
  `/t/$teamSlug/boards/$boardSlug/`
)({
  // EXP-1170: no create keys any more — New issue is a page of its own.
  validateSearch: (search: Record<string, unknown>): BoardSearch => ({
    from:
      typeof search.from === `string` && search.from ? search.from : undefined,
  }),
  component: BoardPage,
})

function BoardPage() {
  const { boardSlug, teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const openNewDraft = useOpenNewDraft(teamSlug)

  const {
    issueLabelMap,
    issuesReady,
    labelList,
    board,
    boardReady,
    users,
    userMap,
    visibleGroups,
    issueGraph,
    team,
  } = useBoardViewData({
    boardSlug,
    teamSlug,
  })
  usePageTitle(board ? pageTitle(board.name) : undefined)

  const permissions = useTeamPermissions(team)

  // Bulk-selection state lives here so the floating action bar can render
  // over the list; IssueList keeps all the selection mechanics.
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const selectedIssues = useMemo(
    () =>
      visibleGroups
        .flatMap((group) => group.issues)
        .filter((issue) => selectedIds.has(issue.id)),
    [visibleGroups, selectedIds]
  )

  // EXP-1170: the empty state's pill and a group header's "+" open the New
  // issue page on THIS board, the "+" seeding its group's status.
  const handleNewIssue = (status?: StatusRowOption) => {
    if (!permissions.canCreate || !board) return
    openNewDraft({ boardId: board.id, status })
  }

  if (!board || !team) {
    // `boardReady` implies the team resolved and the boards snapshot landed,
    // so an absent board here is definitive, not a sync gap (REV2-59).
    if (boardReady) {
      return (
        <BoardNotFound
          boardSlug={boardSlug}
          teamSlug={teamSlug}
        />
      )
    }
    return (
      <div className="text-muted-foreground text-sm p-6">
        Loading board...
      </div>
    )
  }

  return (
    <div className="relative flex h-full flex-col">
      <div
        // EXP-698 r5: one clearance for both states — the bulk bar REPLACES
        // the tab bar on phones, so TAB_BAR_CLEARANCE's `max()` of the two
        // measured heights already covers a live selection; md+ reserves the
        // floating bar's height while a selection lives.
        className={`flex-1 overflow-auto ${TAB_BAR_CLEARANCE} ${
          selectedIssues.length > 0 ? `md:pb-20` : ``
        }`}
      >
        <IssueList
          groups={visibleGroups}
          issueGraph={issueGraph}
          graphTeamId={team?.id}
          issueLabelMap={issueLabelMap}
          users={users}
          userMap={userMap}
          menuFrom={`board:${boardSlug}`}
          onNewIssue={handleNewIssue}
          onIssueClick={(issue) =>
            void navigate({
              to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
              params: {
                teamSlug,
                boardSlug,
                issueIdentifier: issue.identifier,
              },
              // EXP-851: carry the origin, so the sidebar keeps THIS
              // board's list beside the issue.
              search: { from: `board:${boardSlug}` },
            })
          }
          canCreate={permissions.canCreate}
          canMutateIssue={permissions.canMutateIssue}
          canModerate={permissions.isModerator}
          bulkTeamId={team.id}
          selectedIds={selectedIds}
          onSelectedIdsChange={setSelectedIds}
          isLoading={!issuesReady}
          // Members only — the guidance block is meaningless before the
          // viewer's own member row has synced.
          emptyStateExtra={
            permissions.isMember ? (
              <GettingStartedSection
                team={team}
                teamSlug={teamSlug}
              />
            ) : undefined
          }
        />
      </div>
      {selectedIssues.length > 0 && (
        // The bulk bar FLOATS bottom-centre at every width ×4 (desktop, the
        // natives): md+ over the list's bottom edge (click-through strip),
        // phones in the tab bar's slot (the bar positions itself there).
        <div className="pointer-events-none absolute inset-x-0 bottom-4 z-10 flex justify-center px-4">
          <div className="pointer-events-auto">
            <BulkActionBar
              issues={selectedIssues}
              issueLabelMap={issueLabelMap}
              labels={labelList}
              users={users}
              teamId={team.id}
              onClear={() => setSelectedIds(new Set())}
            />
          </div>
        </div>
      )}
    </div>
  )
}
