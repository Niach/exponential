import { useMemo } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  issueCollection,
  issueLabelCollection,
} from "@/lib/collections"
import {
  useTeamBySlug,
  useTeamBoardsWithReady,
  useTeamLabels,
  useTeamUsers,
} from "@/hooks/use-team-data"
import type { IssueFilters } from "@/lib/filters"
import {
  buildFilteredIssues,
  buildIssueLabelIdsMap,
  buildIssueLabelMap,
  buildVisibleIssueGroups,
} from "@/lib/board-view"
import { useTeamStatuses } from "@/hooks/use-team-statuses"
import type { Issue, IssueLabel, Board } from "@/db/schema"

// Cross-board "My Issues" board data: every issue assigned to the current
// user across all boards in the team, reusing the board-view
// grouping/filter machinery (mirrors use-board-view-data, minus the single
// board scope). Pure client work over the already-synced issues shape.
export function useMyIssuesData({
  filters,
  userId,
  teamSlug,
}: {
  filters: IssueFilters
  userId: string | undefined
  teamSlug: string
}) {
  // Const binding so TS narrowing survives into the live-query closure.
  const assignee = userId
  const team = useTeamBySlug(teamSlug)
  const { boards, boardsReady } = useTeamBoardsWithReady(team?.id)
  const boardIds = useMemo(
    () => boards.map((board) => board.id),
    [boards]
  )
  const boardMap = useMemo(
    () => new Map<string, Board>(boards.map((p) => [p.id, p])),
    [boards]
  )

  const { data: issues, isReady: issuesReady } = useLiveQuery(
    (query) =>
      assignee && boardIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                inArray(issues.boardId, boardIds),
                eq(issues.assigneeId, assignee)
              )
            )
            .orderBy(({ issues }) => issues.createdAt)
            // Equal timestamps must not reorder between syncs (EXP-668).
            .orderBy(({ issues }) => issues.id)
        : undefined,
    [assignee, boardIds.join(`,`)]
  )

  const labelList = useTeamLabels(team?.id)

  const { data: issueLabels } = useLiveQuery(
    (query) =>
      team
        ? query
            .from({ issueLabels: issueLabelCollection })
            .where(({ issueLabels }) =>
              eq(issueLabels.teamId, team.id)
            )
        : undefined,
    [team?.id]
  )

  const { userMap, users } = useTeamUsers(team?.id)
  // EXP-314: My Issues is single-team, so it groups by the team's own status
  // rows exactly like the board (cross-TEAM surfaces keep anchor grouping).
  const { options: statusOptions, resolve: resolveStatus } = useTeamStatuses(
    team?.id
  )

  const issueList = (issues ?? []) as Issue[]
  const issueLabelList = (issueLabels ?? []) as IssueLabel[]

  return useMemo(() => {
    const issueLabelIdsMap = buildIssueLabelIdsMap(issueLabelList)
    const issueLabelMap = buildIssueLabelMap(issueLabelList, labelList)
    const filteredIssues = buildFilteredIssues(
      issueList,
      issueLabelIdsMap,
      filters,
      resolveStatus
    )

    return {
      issueLabelMap,
      // The issues query is skipped until the session user + boards are
      // known; a team with CONFIRMED zero boards can never deliver a
      // snapshot, so treat it as ready-empty instead of loading forever —
      // but only once the boards snapshot itself landed, or the empty
      // state flashes while boards are still syncing (REV2-59 class).
      issuesReady: issuesReady || (boardsReady && boardMap.size === 0),
      labelList,
      boardMap,
      totalIssueCount: issueList.length,
      users,
      userMap,
      visibleGroups: buildVisibleIssueGroups(
        filteredIssues,
        statusOptions,
        resolveStatus,
        filters.statusTokens
      ),
      statusOptions,
      team,
    }
  }, [
    filters,
    issueLabelList,
    issueList,
    issuesReady,
    labelList,
    boardMap,
    boardsReady,
    statusOptions,
    resolveStatus,
    userMap,
    users,
    team,
  ])
}
