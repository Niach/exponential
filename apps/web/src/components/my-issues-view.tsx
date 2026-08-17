import { useMemo, useState } from "react"
import { createPortal } from "react-dom"
import { useNavigate } from "@tanstack/react-router"
import { CircleUser } from "lucide-react"
import { ActiveFilterPills } from "@/components/active-filter-pills"
import { BulkActionBar } from "@/components/bulk-action-bar"
import { EmptyState } from "@/components/empty-state"
import { IssueFilterPopover } from "@/components/issue-filter-popover"
import { IssueList } from "@/components/issue-list"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useMyIssuesData } from "@/hooks/use-my-issues-data"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamLabels } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { emptyFilters, hasActiveFilters as filtersActive } from "@/lib/filters"
import type { IssueFilters } from "@/lib/filters"

// EXP-525: the filter trigger belongs in the Inbox page's TAB row, opposite
// "Mark all read" — the standalone control row below it was an always-present
// empty strip. The page owns the tabs, so the trigger renders there and the
// view keeps only the pills; the labels come from the same cheap client-side
// query the view's own data hook uses.
export function MyIssuesFilterAction({
  teamSlug,
  filters,
  onFiltersChange,
}: {
  teamSlug: string
  filters: IssueFilters
  onFiltersChange: (filters: IssueFilters) => void
}) {
  const team = useTeamBySlug(teamSlug)
  const labels = useTeamLabels(team?.id)
  return (
    <IssueFilterPopover
      filters={filters}
      onFiltersChange={onFiltersChange}
      labels={labels}
    />
  )
}

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
  bulkActionSlot,
}: {
  teamSlug: string
  filters: IssueFilters
  onFiltersChange: (filters: IssueFilters) => void
  /** The tab row's right-hand cell, where the bulk-action bar renders. */
  bulkActionSlot: HTMLElement | null
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
      {/* The bulk bar renders into the page's TAB row (EXP-525) — that row is
          always there, so starting a selection still never reflows the list
          (FEED-12) even though the old fixed-height control row is gone.
          Below md the bar floats itself above the tab bar regardless. */}
      {bulkActionSlot !== null &&
        selectedIssues.length > 0 &&
        createPortal(
          <BulkActionBar
            issues={selectedIssues}
            issueLabelMap={issueLabelMap}
            labels={labelList}
            users={users}
            onClear={() => setSelectedIds(new Set())}
          />,
          bulkActionSlot
        )}
      <div className="px-4 md:px-6">
        <ActiveFilterPills
          filters={filters}
          onFiltersChange={onFiltersChange}
          labels={labelList}
        />
      </div>

      <div
        className={`flex-1 overflow-auto ${
          selectedIssues.length > 0
            ? // Taller mobile clearance while the selection pill floats above
              // the tab bar: TAB_BAR_CLEARANCE + pill height + gap.
              `max-md:pb-[calc(9.25rem+env(safe-area-inset-bottom))]`
            : TAB_BAR_CLEARANCE
        }`}
      >
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
            onClearFilters={() => onFiltersChange(emptyFilters)}
          />
        )}
      </div>
    </div>
  )
}
