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
import {
  buildIssueLabelMap,
  buildVisibleIssueGroups,
  nestIssueGroups,
} from "@/lib/board-view"
import { boardMineIssues } from "@exp/domain-contract/chats"
import { useTeamIssueGraph } from "@/hooks/use-team-issue-graph"
import { useTeamStatuses } from "@/hooks/use-team-statuses"
import type { Issue, IssueLabel, Board } from "@/db/schema"

// "My Issues" board data: every issue assigned to the current user, reusing
// the board-view grouping machinery (mirrors use-board-view-data). #EXP-1836
// (chats.json `boardMineCases`): with `boardId` = mine on THAT board only
// (`boardMineIssues`, the board's My issues tab and its count); without it,
// across the team's boards. Pure client work over the synced issues shape.
export function useMyIssuesData({
  userId,
  teamSlug,
  boardId,
}: {
  userId: string | undefined
  teamSlug: string
  boardId?: string
}) {
  // Const binding so TS narrowing survives into the live-query closure.
  const assignee = userId
  const team = useTeamBySlug(teamSlug)
  const { boards, boardsReady } = useTeamBoardsWithReady(team?.id)
  const boardIds = useMemo(
    () => boards.map((board) => board.id).filter((id) => boardId === undefined || id === boardId),
    [boards, boardId]
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

  const issueList = useMemo(() => {
    const list = (issues ?? []) as Issue[]
    return boardId !== undefined ? boardMineIssues(list, boardId, assignee) : list
  }, [issues, boardId, assignee])
  const issueLabelList = (issueLabels ?? []) as IssueLabel[]

  const issueGraph = useTeamIssueGraph(team?.id)

  return useMemo(() => {
    const issueLabelMap = buildIssueLabelMap(issueLabelList, labelList)

    return {
      issueLabelMap,
      // The issues query is skipped until the session user + boards are
      // known; a team with CONFIRMED zero boards can never deliver a
      // snapshot, so treat it as ready-empty instead of loading forever —
      // but only once the boards snapshot itself landed, or the empty
      // state flashes while boards are still syncing (REV2-59 class).
      // EXP-1550: a SKIPPED query reports `isReady: true`, so it only counts
      // once the query actually runs.
      issuesReady:
        assignee && boardMap.size > 0
          ? issuesReady
          : boardsReady && boardMap.size === 0,
      labelList,
      boardMap,
      totalIssueCount: issueList.length,
      users,
      userMap,
      // EXP-980: sub-issues nest under their parent; the root decides the
      // group and the position.
      visibleGroups: nestIssueGroups(
        buildVisibleIssueGroups(issueList, statusOptions, resolveStatus),
        issueGraph.relations
      ),
      issueGraph,
      statusOptions,
      team,
    }
  }, [
    issueLabelList,
    issueList,
    issuesReady,
    labelList,
    assignee,
    boardMap,
    boardsReady,
    statusOptions,
    resolveStatus,
    issueGraph,
    userMap,
    users,
    team,
  ])
}
