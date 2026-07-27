import { useMemo } from "react"
import { and, eq } from "@tanstack/react-db"
import { useLiveQuery } from "@tanstack/react-db"
import {
  issueCollection,
  issueLabelCollection,
  labelCollection,
  boardCollection,
} from "@/lib/collections"
import {
  useTeamBySlug,
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
import type { Issue, IssueLabel, Label, Board } from "@/db/schema"

export function useBoardViewData({
  filters,
  boardSlug,
  teamSlug,
}: {
  filters: IssueFilters
  boardSlug: string
  teamSlug: string
}) {
  const team = useTeamBySlug(teamSlug)

  const { data: boards, isReady: boardsQueryReady } = useLiveQuery(
    (query) =>
      team
        ? query
            .from({ boards: boardCollection })
            .where(({ boards }) =>
              and(
                eq(boards.teamId, team.id),
                eq(boards.slug, boardSlug)
              )
            )
        : undefined,
    [boardSlug, team?.id]
  )

  const board = (boards?.[0] ?? null) as Board | null
  // A DISABLED live query reports `isReady: true` (it never ran), so every
  // readiness signal has to carry its own enabling condition — otherwise
  // "ready and empty" fires while the gate is still resolving (REV2-59).
  const boardReady = Boolean(team) && boardsQueryReady

  const { data: issues, isReady: issuesQueryReady } = useLiveQuery(
    (query) =>
      board
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => eq(issues.boardId, board.id))
            .orderBy(({ issues }) => issues.createdAt)
        : undefined,
    [board?.id]
  )

  const issuesReady = Boolean(board) && issuesQueryReady

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
      board ? query.from({ issueLabels: issueLabelCollection }) : undefined,
    [board?.id]
  )

  const { userMap, users } = useTeamUsers(team?.id)
  // EXP-314: group by the team's own status rows (constructed defaults until
  // the issue_statuses shape lands, so the board renders from frame one).
  const { options: statusOptions, resolve: resolveStatus } = useTeamStatuses(
    team?.id
  )

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
      // True once the Electric issues collection delivered its first snapshot
      // for this board — lets the list render skeletons instead of a false
      // "no issues" empty state during initial sync.
      issuesReady,
      labelList,
      board,
      // True once the boards collection delivered its first snapshot for this
      // team — `boardReady && !board` means the slug matches no live board
      // (trashed, renamed, never existed), not "still syncing".
      boardReady,
      // Unfiltered count, so the list can tell "no issues at all" apart from
      // "filters hide everything".
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
    board,
    boardReady,
    statusOptions,
    resolveStatus,
    userMap,
    users,
    team,
  ])
}
