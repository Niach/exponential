import { useMemo, useState } from "react"
import { useNavigate } from "@tanstack/react-router"
import { CircleUser } from "lucide-react"
import { BulkActionBar } from "@/components/bulk-action-bar"
import { EmptyState } from "@/components/empty-state"
import { IssueFilterBar } from "@/components/issue-filter-bar"
import { IssueList } from "@/components/issue-list"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useMyIssuesData } from "@/hooks/use-my-issues-data"
import { useSession } from "@/hooks/use-session"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { hasActiveFilters as filtersActive } from "@/lib/filters"
import type { IssueFilters } from "@/lib/filters"

// Cross-board "My Issues": every issue assigned to the signed-in user across
// all boards in the team, grouped by status like the board
// (masterplan §5a — a fixed built-in view, no saved-filter machinery). Rows
// span boards, so the identifier column (always `{PREFIX}-{number}`) carries
// the board context; clicking a row opens the full-page detail route.
//
// Lives as the "My Issues" tab of the Inbox page (EXP-186); filters stay in
// the URL (?tab=my-issues&status=…&priority=…&labels=…) so a filtered view
// is shareable and survives refresh.
export function MyIssuesView({
  teamSlug,
  filters,
  onFiltersChange,
}: {
  teamSlug: string
  filters: IssueFilters
  onFiltersChange: (filters: IssueFilters) => void
}) {
  const navigate = useNavigate()
  const { data: session } = useSession()
  const userId = session?.user?.id

  const {
    issueLabelMap,
    issuesReady,
    labelList,
    boardMap,
    totalIssueCount,
    users,
    userMap,
    visibleGroups,
    team,
  } = useMyIssuesData({ filters, userId, teamSlug })

  const permissions = useTeamPermissions(team)

  // Bulk-selection state lives here so the action bar can render in the
  // header region above the scroll container (EXP-251); IssueList keeps all
  // the selection mechanics.
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const selectedIssues = useMemo(
    () =>
      visibleGroups
        .flatMap((group) => group.issues)
        .filter((issue) => selectedIds.has(issue.id)),
    [visibleGroups, selectedIds]
  )

  if (!team) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  return (
    <div className="flex flex-col h-full">
      <IssueFilterBar
        title=""
        filters={filters}
        onFiltersChange={onFiltersChange}
        labels={labelList}
        onNewIssue={() => {}}
        canCreate={false}
      />

      {selectedIssues.length > 0 && (
        <div className="flex px-4 md:px-6 pb-2">
          <BulkActionBar
            issues={selectedIssues}
            issueLabelMap={issueLabelMap}
            labels={labelList}
            users={users}
            onClear={() => setSelectedIds(new Set())}
          />
        </div>
      )}

      <div className={`flex-1 overflow-auto ${TAB_BAR_CLEARANCE}`}>
        {issuesReady && totalIssueCount === 0 ? (
          <EmptyState
            icon={CircleUser}
            title="No issues assigned to you"
            description="Issues assigned to you across all boards in this team will show up here."
          />
        ) : (
          <IssueList
            groups={visibleGroups}
            issueLabelMap={issueLabelMap}
            labels={labelList}
            users={users}
            userMap={userMap}
            onNewIssue={() => {}}
            onIssueClick={(issue) => {
              const board = boardMap.get(issue.boardId)
              if (!board) return
              void navigate({
                to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
                params: {
                  teamSlug,
                  boardSlug: board.slug,
                  issueIdentifier: issue.identifier,
                },
              })
            }}
            canCreate={false}
            canMutateIssue={permissions.canMutateIssue}
            canModerate={permissions.isModerator}
            bulkTeamId={team.id}
            selectedIds={selectedIds}
            onSelectedIdsChange={setSelectedIds}
            isLoading={!issuesReady}
            hasAnyIssues={totalIssueCount > 0}
            hasActiveFilters={filtersActive(filters)}
            onClearFilters={() =>
              onFiltersChange({ statuses: [], priorities: [], labelIds: [] })
            }
          />
        )}
      </div>
    </div>
  )
}
