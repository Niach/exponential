import { useMemo } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { pullRequestCollection } from "@/lib/collections"
import type { PullRequestRow } from "@/lib/session-tree"

// The team's pull requests by thread (the rail and the Chats list read them;
// #EXP-1380 moved the Chats page itself onto `@exp/ui` `ChatList`).

export { threadWordOf } from "@exp/ui"

/** The team's PRs grouped by the thread (root) that opened them. */
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
  return useMemo(() => {
    const all = (data ?? []) as PullRequestRow[]
    const byRoot = new Map<string, PullRequestRow[]>()
    for (const pr of all) {
      if (!pr.rootSessionId) continue
      const list = byRoot.get(pr.rootSessionId) ?? []
      list.push(pr)
      byRoot.set(pr.rootSessionId, list)
    }
    return { byRoot, all }
  }, [data])
}
