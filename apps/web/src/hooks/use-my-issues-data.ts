import { useMemo } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  issueCollection,
  issueLabelCollection,
  labelCollection,
} from "@/lib/collections"
import {
  useTeamBySlug,
  useTeamBoards,
  useTeamUsers,
} from "@/hooks/use-team-data"
import type { IssueFilters } from "@/lib/filters"
import {
  buildFilteredIssues,
  buildIssueLabelIdsMap,
  buildIssueLabelMap,
  buildVisibleIssueGroups,
} from "@/lib/board-view"
import type { Issue, IssueLabel, Label, Board } from "@/db/schema"

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
  const boards = useTeamBoards(team?.id)
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
        : undefined,
    [assignee, boardIds.join(`,`)]
  )

  const { data: labels } = useLiveQuery(
    (query) =>
      team
        ? query
            .from({ labels: labelCollection })
            .where(({ labels }) => eq(labels.teamId, team.id))
        : undefined,
    [team?.id]
  )

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

  const issueList = (issues ?? []) as Issue[]
  const labelList = (labels ?? []) as Label[]
  const issueLabelList = (issueLabels ?? []) as IssueLabel[]

  return useMemo(() => {
    const issueLabelIdsMap = buildIssueLabelIdsMap(issueLabelList)
    const issueLabelMap = buildIssueLabelMap(issueLabelList, labelList)
    const filteredIssues = buildFilteredIssues(
      issueList,
      issueLabelIdsMap,
      filters
    )

    return {
      issueLabelMap,
      // The issues query is skipped until the session user + boards are
      // known; a team with no boards can never deliver a snapshot, so
      // treat it as ready-empty instead of loading forever.
      issuesReady: issuesReady || boardMap.size === 0,
      labelList,
      boardMap,
      totalIssueCount: issueList.length,
      users,
      userMap,
      visibleGroups: buildVisibleIssueGroups(filteredIssues, filters.statuses),
      team,
    }
  }, [
    filters,
    issueLabelList,
    issueList,
    issuesReady,
    labelList,
    boardMap,
    userMap,
    users,
    team,
  ])
}
