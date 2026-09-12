import { useMemo, useState } from "react"
import { createPortal } from "react-dom"
import { useNavigate } from "@tanstack/react-router"
import { CircleUser } from "lucide-react"
import { BulkActionBar } from "@/components/bulk-action-bar"
import { EmptyState } from "@/components/empty-state"
import { IssueList } from "@/components/issue-list"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useMyIssuesData } from "@/hooks/use-my-issues-data"
import { useSession } from "@/hooks/use-session"
import { useTeamPermissions } from "@/hooks/use-team-permissions"

// Cross-board "My Issues": every issue assigned to the signed-in user across
// all boards in the team, grouped by status like the board
// (masterplan §5a — a fixed built-in view, no saved-filter machinery). Rows
// span boards, so the identifier column (always `{PREFIX}-{number}`) carries
// the board context; clicking a row opens the full-page detail route.
//
// Lives as the "My Issues" tab of the Inbox page (EXP-186).
export function MyIssuesView({
  teamSlug,
  bulkActionSlot,
}: {
  teamSlug: string
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
  } = useMyIssuesData({ userId, teamSlug })

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
            teamId={team.id}
            onClear={() => setSelectedIds(new Set())}
          />,
          bulkActionSlot
        )}

      <div
        // EXP-698 r5: one clearance for both states — the bulk bar REPLACES
        // the tab bar on phones, so TAB_BAR_CLEARANCE's `max()` of the two
        // measured heights already covers a live selection.
        className={`flex-1 overflow-auto ${TAB_BAR_CLEARANCE}`}
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
                // EXP-851: the sidebar keeps this list beside the issue.
                search: { from: `inbox:my-issues` },
              })
            }}
            canCreate={false}
            canMutateIssue={permissions.canMutateIssue}
            canModerate={permissions.isModerator}
            bulkTeamId={team.id}
            selectedIds={selectedIds}
            onSelectedIdsChange={setSelectedIds}
            isLoading={!issuesReady}
          />
        )}
      </div>
    </div>
  )
}
