import { useMemo, type ComponentProps } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { ThreadsList } from "@exp/ui"
import { useDeviceGlyphs } from "@/hooks/use-device-glyphs"
import { useEnteringRows } from "@/hooks/use-entering-rows"
import { pullRequestCollection } from "@/lib/collections"
import type { PullRequestRow } from "@/lib/session-tree"

// EXP-1293 Threads v2: the Threads page's list = `@exp/ui` `ThreadsList`
// (THE one the styleguide renders, EXP-1339) fed the synced devices and
// the band-change motion.

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

export function ThreadsListView(
  props: Omit<ComponentProps<typeof ThreadsList>, `deviceGlyph` | `entering`>
) {
  const deviceGlyph = useDeviceGlyphs()
  const entering = useEnteringRows(
    props.groups.flatMap((group) => group.rows.map((row) => `${group.key}:${row.sessionId}`))
  )
  return <ThreadsList {...props} deviceGlyph={deviceGlyph} entering={entering} />
}
