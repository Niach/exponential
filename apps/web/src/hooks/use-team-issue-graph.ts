import { useMemo } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { Board, Issue, IssueRelation } from "@/db/schema"
import {
  boardCollection,
  issueCollection,
  issueRelationCollection,
} from "@/lib/collections"
import { blockCounts, type BlockCounts } from "@/lib/issue-graph"

export interface TeamIssueGraph {
  /** The team's synced `parent` + `blocks` rows. */
  relations: IssueRelation[]
  /** Every synced issue of the team: a blocker outside the list still counts. */
  issues: Issue[]
  /** Per issue id; absent = no open blocker and blocking nothing open. */
  counts: Map<string, BlockCounts>
}

/**
 * The team's synced board ids, sorted. An issue row carries NO `team_id` on
 * the client (the issues shape drops its scoping columns), so "the team's
 * issues" is always "the issues of the team's boards".
 */
export function useTeamBoardIds(teamId: string | undefined): string[] {
  const { data: boardRows } = useLiveQuery(
    (query) =>
      teamId
        ? query.from({ b: boardCollection }).where(({ b }) => eq(b.teamId, teamId))
        : undefined,
    [teamId]
  )
  return useMemo(
    () => ((boardRows ?? []) as Board[]).map((board) => board.id).sort(),
    [boardRows]
  )
}

const EMPTY: TeamIssueGraph = { relations: [], issues: [], counts: new Map() }

/**
 * EXP-980: what an issue LIST needs of the relation graph, queried ONCE per
 * list (never per row): the rows that nest sub-issues and the per-row blocks
 * badge counts. `undefined` skips both queries.
 */
export function useTeamIssueGraph(teamId: string | undefined): TeamIssueGraph {
  const { data: relationRows } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ r: issueRelationCollection })
            .where(({ r }) => eq(r.teamId, teamId))
        : undefined,
    [teamId]
  )
  const boardIds = useTeamBoardIds(teamId)
  const { data: issueRows } = useLiveQuery(
    (query) =>
      boardIds.length > 0
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => inArray(i.boardId, boardIds))
        : undefined,
    [boardIds.join(`,`)]
  )

  return useMemo(() => {
    if (!teamId) return EMPTY
    const relations = ((relationRows ?? []) as IssueRelation[]).filter(
      (row) => row.type === `parent` || row.type === `blocks`
    )
    const issues = (issueRows ?? []) as Issue[]
    return { relations, issues, counts: blockCounts(relations, issues) }
  }, [teamId, relationRows, issueRows])
}
