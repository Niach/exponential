import { useMemo } from "react"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { pullRequestCollection } from "@/lib/collections"
import type { PullRequestRow } from "@/lib/session-tree"

// The team's pull requests by chat (the rail and the Chats list read them;
// #EXP-1380 moved the Chats page itself onto `@exp/ui` `ChatList`).

export { chatWordOf } from "@exp/ui"

/** The team's PRs grouped by the chat (root) that opened them. */
export function useTeamPullRequestsByRoot(teamId: string | undefined): {
  byRoot: ReadonlyMap<string, PullRequestRow[]>
  all: PullRequestRow[]
} {
  const { data } = useLiveQuery(
    (query) =>
      teamId
        ? query.from({ prs: pullRequestCollection }).where(({ prs }) => eq(prs.teamId, teamId))
        : undefined,
    [teamId]
  )
  return useMemo(() => groupByRoot((data ?? []) as PullRequestRow[]), [data])
}

/** #EXP-2027: the PRs of several teams (the team-less Chats list) by chat. */
export function useTeamsPullRequestsByRoot(teamIds: readonly string[]): {
  byRoot: ReadonlyMap<string, PullRequestRow[]>
  all: PullRequestRow[]
} {
  const key = teamIds.join(`,`)
  const { data } = useLiveQuery(
    (query) =>
      teamIds.length > 0
        ? query.from({ prs: pullRequestCollection }).where(({ prs }) => inArray(prs.teamId, [...teamIds]))
        : undefined,
    [key]
  )
  return useMemo(() => groupByRoot((data ?? []) as PullRequestRow[]), [data])
}

function groupByRoot(all: PullRequestRow[]): { byRoot: ReadonlyMap<string, PullRequestRow[]>; all: PullRequestRow[] } {
  const byRoot = new Map<string, PullRequestRow[]>()
  for (const pr of all) {
    if (!pr.rootSessionId) continue
    const list = byRoot.get(pr.rootSessionId) ?? []
    list.push(pr)
    byRoot.set(pr.rootSessionId, list)
  }
  return { byRoot, all }
}
