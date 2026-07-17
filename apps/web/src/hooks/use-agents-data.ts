import { useMemo } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import {
  useWorkspaceProjects,
  useWorkspaceUsers,
} from "@/hooks/use-workspace-data"
import type { CodingSession, Issue, Project, User } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"

// The ended list is a recap, not an archive — cap it so a busy workspace's
// history doesn't grow the page unboundedly (the full trail lives on issues).
const MAX_ENDED_SESSIONS = 25

export interface AgentSessionRow {
  session: CodingSession
  /** May be undefined while the issue row is still syncing. */
  issue: Issue | undefined
  project: Project | undefined
  /** Undefined for unsynced users (public boards) — render via displayUserName. */
  user: User | undefined
}

// Workspace Agents page data: every coding session in the workspace (synced
// coding_sessions shape, workspace-scoped by the denormalized workspace_id),
// joined client-side to its issue / project / driving user. Running sessions
// newest-first, then the most recently ended ones.
export function useAgentsData(workspaceId?: string) {
  const { data: sessionRows, isReady } = useLiveQuery(
    (query) =>
      workspaceId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) => eq(sessions.workspaceId, workspaceId))
        : undefined,
    [workspaceId]
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

  const projects = useWorkspaceProjects(workspaceId)
  const { userMap } = useWorkspaceUsers(workspaceId)
  const now = useNow()

  return useMemo(() => {
    const issueMap = new Map(
      ((issueRows ?? []) as Issue[]).map((issue) => [issue.id, issue])
    )
    const projectMap = new Map(projects.map((project) => [project.id, project]))

    const toRow = (session: CodingSession): AgentSessionRow => {
      // Batch-scoped sessions carry no issue — render issueless.
      const issue = session.issueId ? issueMap.get(session.issueId) : undefined
      return {
        session,
        issue,
        project: issue ? projectMap.get(issue.projectId) : undefined,
        user: userMap.get(session.userId),
      }
    }

    // Staleness guard (EXP-153): heartbeat-dead `running` rows render as
    // absent (not "ended" — swept rows leave no recap entry either).
    const running = sessions
      .filter(
        (session) =>
          session.status === `running` &&
          !isCodingSessionStale(session.updatedAt, now)
      )
      .sort(
        (a, b) =>
          new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime()
      )
      .map(toRow)

    const ended = sessions
      .filter((session) => session.status === `ended`)
      .sort(
        (a, b) =>
          new Date(b.endedAt ?? b.startedAt).getTime() -
          new Date(a.endedAt ?? a.startedAt).getTime()
      )
      .slice(0, MAX_ENDED_SESSIONS)
      .map(toRow)

    return {
      running,
      ended,
      // Without a workspace id the query is skipped and can never deliver a
      // snapshot — treat that as ready-empty instead of loading forever.
      isLoading: !isReady && Boolean(workspaceId),
    }
  }, [sessions, issueRows, projects, userMap, isReady, workspaceId, now])
}
