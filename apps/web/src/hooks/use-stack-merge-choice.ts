import { useMemo } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { Board, Issue } from "@/db/schema"
import { boardCollection, issueCollection } from "@/lib/collections"
import {
  stackMergeConfirm,
  type StackConfirmMode,
  type StackMergeConfirm,
} from "@/lib/pr-stack"
import { useTeamBoardIds } from "@/hooks/use-team-issue-graph"

export interface StackMergeConfirmState {
  /** False while the rows the decision needs have not arrived. */
  ready: boolean
  /** The stack confirm's content; null = a plain merge. */
  confirm: StackMergeConfirm | null
}

const IDLE: StackMergeConfirmState = { ready: true, confirm: null }

/**
 * EXP-1248: whether merging this issue's pull request is a STACK merge (the
 * one confirm, `stackMergeConfirm`), off the synced rows alone. `mode`
 * `stack` = the merge control (Merge stack), `through` = a stack-rail row's
 * Merge through here.
 *
 * The control knows only an `issueId` (an issue row carries no `team_id` on
 * the client, REV2-5), so the team is reached through the issue's board, and
 * "the team's open pull requests" = the open-PR issues of the team's boards.
 * `enabled` false skips every query: a list of Merge buttons must not fan out
 * four live queries per row: the control arms this on the click and reads
 * the answer before it opens anything.
 */
export function useStackMergeConfirm(
  issueId: string | undefined,
  enabled: boolean,
  mode: StackConfirmMode = `stack`
): StackMergeConfirmState {
  const active = enabled && issueId !== undefined
  const { data: issueRows } = useLiveQuery(
    (query) =>
      active
        ? query.from({ i: issueCollection }).where(({ i }) => eq(i.id, issueId))
        : undefined,
    [active, issueId]
  )
  const issue = ((issueRows ?? []) as Issue[])[0] ?? null
  const boardId = issue?.boardId
  const { data: boardRows } = useLiveQuery(
    (query) =>
      active && boardId
        ? query.from({ b: boardCollection }).where(({ b }) => eq(b.id, boardId))
        : undefined,
    [active, boardId]
  )
  const teamId = ((boardRows ?? []) as Board[])[0]?.teamId
  const boardIds = useTeamBoardIds(active ? teamId : undefined)
  const { data: openRows } = useLiveQuery(
    (query) =>
      active && boardIds.length > 0
        ? query
            .from({ i: issueCollection })
            .where(({ i }) =>
              and(inArray(i.boardId, boardIds), eq(i.prState, `open`))
            )
        : undefined,
    [active, boardIds.join(`,`)]
  )

  return useMemo(() => {
    if (!active) return IDLE
    // Nothing synced yet for the issue itself: wait, do not guess.
    if (!issue) return { ready: false, confirm: null }
    // An issue without an open PR is a plain merge, whatever the team looks
    // like; otherwise the team's open rows decide.
    if (issue.prState !== `open`) return IDLE
    if (!teamId || boardIds.length === 0 || openRows === undefined) {
      return { ready: false, confirm: null }
    }
    return {
      ready: true,
      confirm: stackMergeConfirm(issue, (openRows ?? []) as Issue[], mode),
    }
  }, [active, issue, teamId, boardIds, openRows, mode])
}
