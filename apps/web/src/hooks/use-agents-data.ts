import { useMemo } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import {
  useTeamBoards,
  useTeamUsers,
} from "@/hooks/use-team-data"
import type { CodingSession, Issue, Board, User } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"

export interface AgentSessionRow {
  session: CodingSession
  /** May be undefined while the issue row is still syncing. */
  issue: Issue | undefined
  board: Board | undefined
  /** Undefined for unsynced users (public boards) — render via displayUserName. */
  user: User | undefined
}

// Team Agents page + dock data: the LIVE coding sessions in the
// team (synced coding_sessions shape, team-scoped by the denormalized
// team_id), joined client-side to their issue / board / driving user,
// newest-first. Live = `running` OR `in_review` (EXP-194: the agent's PR is
// open, terminal still alive awaiting review — consumers read
// `session.status` to render "Ready for review" vs "Coding now"). Ended
// sessions dropped out with the redesign — the live trail lives on each
// issue, and the dock/Agents page only surface live work.
export function useAgentsData(teamId?: string) {
  const { data: sessionRows, isReady } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) => eq(sessions.teamId, teamId))
        : undefined,
    [teamId]
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

  const boards = useTeamBoards(teamId)
  const { userMap } = useTeamUsers(teamId)
  const now = useNow()

  return useMemo(() => {
    const issueMap = new Map(
      ((issueRows ?? []) as Issue[]).map((issue) => [issue.id, issue])
    )
    const boardMap = new Map(boards.map((board) => [board.id, board]))

    const toRow = (session: CodingSession): AgentSessionRow => {
      // Batch-scoped sessions carry no issue — render issueless.
      const issue = session.issueId ? issueMap.get(session.issueId) : undefined
      return {
        session,
        issue,
        board: issue ? boardMap.get(issue.boardId) : undefined,
        user: userMap.get(session.userId),
      }
    }

    // Staleness guard (EXP-153): heartbeat-dead rows render as absent
    // (not "ended" — swept rows leave no recap entry either).
    const running = sessions
      .filter(
        (session) =>
          (session.status === `running` || session.status === `in_review`) &&
          !isCodingSessionStale(session.updatedAt, now)
      )
      .sort(
        (a, b) =>
          new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime()
      )
      .map(toRow)

    return {
      running,
      // Without a team id the query is skipped and can never deliver a
      // snapshot — treat that as ready-empty instead of loading forever.
      isLoading: !isReady && Boolean(teamId),
    }
  }, [sessions, issueRows, boards, userMap, isReady, teamId, now])
}
