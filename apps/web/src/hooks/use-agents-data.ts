import { useMemo } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { useTeamBoards, useTeamUsers } from "@/hooks/use-team-data"
import type { CodingSession, Issue, Board, User } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"

export interface AgentSessionRow {
  session: CodingSession
  /** May be undefined while the issue row is still syncing. */
  issue: Issue | undefined
  board: Board | undefined
  /** May be undefined while the user row is still syncing — render via displayUserName. */
  user: User | undefined
  /** EXP-535: a batch session's resolved open PR, as a representative linked
   * issue (merging through it merges the ONE batch PR — Reviews pattern).
   * Set only on issueless batch rows in review whose OWN PR (matched by the
   * stamped session branch, EXP-545) is open and unambiguous. */
  batchPrIssue: Issue | undefined
}

// Team Agents page + dock data: the caller's OWN live coding sessions in the
// team (synced coding_sessions shape, team-scoped by the denormalized
// team_id), joined client-side to their issue / board / driving user,
// newest-first. Live = `running` OR `in_review` OR `merged` (EXP-194: the
// agent's PR is open, terminal still alive awaiting review; EXP-358: it
// survives the merge too — consumers read `session.status` to render
// "Ready for review" vs "Merged" vs "Coding now"). Ended
// sessions dropped out with the redesign — the live trail lives on each
// issue, and the dock/Agents page only surface live work.
// EXP-312 follow-up: a live session is viewable/steerable only by its owner,
// so a teammate's row in these lists could only ever read as "unavailable".
// Such rows are filtered out here entirely; they still sync, and still
// surface as status badges on issue detail and in the Reviews queue.
// Both params stay REQUIRED (optional-typed, not optional-arity) so no future
// caller can silently ask for the whole team's sessions again.
export function useAgentsData(
  teamId: string | undefined,
  currentUserId: string | undefined
) {
  const { data: sessionRows, isReady } = useLiveQuery(
    (query) =>
      teamId && currentUserId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                eq(sessions.teamId, teamId),
                eq(sessions.userId, currentUserId)
              )
            )
        : undefined,
    [teamId, currentUserId]
  )
  const sessions = useMemo(
    () => (sessionRows ?? []) as CodingSession[],
    [sessionRows]
  )

  // Sorted so the same id set always yields the same dep string (no query
  // churn from heap-order flips).
  const issueIds = useMemo(() => {
    const ids = [...new Set(sessions.map((session) => session.issueId))]
    ids.sort()
    return ids
  }, [sessions])

  const { data: issueRows } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.id, issueIds))
        : undefined,
    [issueIds.join(`,`)]
  )

  // EXP-535: batch sessions carry no issue linkage, so a batch row resolves
  // its open PR client-side: the team's open-PR issues on an `exp/batch-`
  // branch, collapsed by prUrl like Reviews, then matched to the branch the
  // pr_open flip stamped on the row (EXP-545). Team scoping rides the board
  // join below (the issues shape drops team_id). Queried only while an
  // issueless, actionless in-review batch row actually needs it — this hook
  // also backs the always-mounted dock.
  const needsBatchPr = useMemo(
    () =>
      sessions.some(
        (session) =>
          !session.issueId &&
          session.actionName == null &&
          session.status === `in_review`
      ),
    [sessions]
  )
  const { data: openPrIssueRows } = useLiveQuery(
    (query) =>
      teamId && needsBatchPr
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => eq(issues.prState, `open`))
        : undefined,
    [teamId, needsBatchPr]
  )

  const boards = useTeamBoards(teamId)
  const { userMap } = useTeamUsers(teamId)
  const now = useNow()

  return useMemo(() => {
    const issueMap = new Map(
      ((issueRows ?? []) as Issue[]).map((issue) => [issue.id, issue])
    )
    const boardMap = new Map(boards.map((board) => [board.id, board]))

    // The team's open batch PRs, one representative (newest) issue per
    // distinct prUrl. A session resolves ITS OWN PR by the branch the
    // pr_open batch flip stamped on the row (EXP-545) — matching by "the
    // team's sole open batch PR" alone could target a teammate's PR once
    // the session's own PR closed unmerged. Rows flipped before the stamp
    // existed (branch NULL) keep the legacy sole-open-PR fallback; anything
    // ambiguous resolves to nothing — Reviews still lists every PR.
    const batchPrByUrl = new Map<string, Issue>()
    for (const issue of (openPrIssueRows ?? []) as Issue[]) {
      if (!issue.prUrl || !issue.branch?.startsWith(`exp/batch-`)) continue
      if (!boardMap.has(issue.boardId)) continue
      const current = batchPrByUrl.get(issue.prUrl)
      if (
        !current ||
        new Date(issue.createdAt).getTime() >
          new Date(current.createdAt).getTime()
      ) {
        batchPrByUrl.set(issue.prUrl, issue)
      }
    }
    const batchPrReps = [...batchPrByUrl.values()]
    const resolveBatchPr = (sessionBranch: string | null): Issue | undefined => {
      if (sessionBranch) {
        const matches = batchPrReps.filter(
          (issue) => issue.branch === sessionBranch
        )
        return matches.length === 1 ? matches[0] : undefined
      }
      return batchPrReps.length === 1 ? batchPrReps[0] : undefined
    }

    const toRow = (session: CodingSession): AgentSessionRow => {
      // Batch-scoped sessions carry no issue — render issueless.
      const issue = session.issueId ? issueMap.get(session.issueId) : undefined
      // EXP-535: an issueless, actionless batch run whose PR is open
      // (status in_review — flipped in the pr_open transaction) gets the
      // resolved batch PR for its Merge button.
      const isBatch = !session.issueId && session.actionName == null
      return {
        session,
        issue,
        board: issue ? boardMap.get(issue.boardId) : undefined,
        user: userMap.get(session.userId),
        batchPrIssue:
          isBatch && session.status === `in_review`
            ? resolveBatchPr(session.branch)
            : undefined,
      }
    }

    // Staleness guard (EXP-153): heartbeat-dead rows render as absent
    // (not "ended" — swept rows leave no recap entry either).
    const running = sessions
      .filter(
        (session) =>
          (session.status === `running` ||
            session.status === `in_review` ||
            session.status === `merged`) &&
          !isCodingSessionStale(session.updatedAt, now)
      )
      .sort(
        (a, b) =>
          new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime()
      )
      .map(toRow)

    return {
      running,
      // Without a team id or a signed-in user the query is skipped and can
      // never deliver a snapshot — treat that as ready-empty instead of
      // loading forever.
      isLoading: !isReady && Boolean(teamId && currentUserId),
    }
  }, [
    sessions,
    issueRows,
    openPrIssueRows,
    boards,
    userMap,
    isReady,
    teamId,
    currentUserId,
    now,
  ])
}
