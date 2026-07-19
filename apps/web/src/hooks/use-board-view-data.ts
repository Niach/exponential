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

  const { data: boards } = useLiveQuery(
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

  const { data: issues, isReady: issuesReady } = useLiveQuery(
    (query) =>
      board
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => eq(issues.boardId, board.id))
            .orderBy(({ issues }) => issues.createdAt)
        : undefined,
    [board?.id]
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
      board ? query.from({ issueLabels: issueLabelCollection }) : undefined,
    [board?.id]
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
      // True once the Electric issues collection delivered its first snapshot
      // for this board — lets the list render skeletons instead of a false
      // "no issues" empty state during initial sync.
      issuesReady,
      labelList,
      board,
      // Unfiltered count, so the list can tell "no issues at all" apart from
      // "filters hide everything".
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
    board,
    userMap,
    users,
    team,
  ])
}
