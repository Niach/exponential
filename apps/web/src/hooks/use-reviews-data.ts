import { useCallback, useEffect, useMemo, useState } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { issueCollection } from "@/lib/collections"
import {
  useTeamBoards,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"
import type { OpenPull } from "@/lib/integrations/github-pr"
import type { Issue, Board, Team } from "@/db/schema"

// One open PR. A batch coding run links several issues to the same prUrl —
// they all ride ONE entry (EXP-131, never one row per issue); merging/closing
// through the representative issue acts on the PR, and the webhook then
// completes every linked issue.
export interface ReviewEntry {
  key: string
  // Representative row (newest) — carries prUrl/prNumber/branch for actions.
  issue: Issue
  // Every linked issue, newest first (length 1 for a plain single-issue PR).
  issues: Issue[]
}

export interface ReviewGroup {
  board: Board
  entries: ReviewEntry[]
}

export interface ExternalPullGroup {
  repositoryId: string
  fullName: string
  pulls: OpenPull[]
}

// Cross-board review queue: every issue in the team with an open pull
// request, grouped by board (board sortOrder, issues newest-first). Pure
// client work over the already-synced issues shape — prState arrives on every
// issue row, and the collections' snakeCamelMapper makes the camelCase filter
// match the Postgres pr_state column.
export function useReviewsData(team: Team | null | undefined) {
  const boards = useTeamBoards(team?.id)
  const teamId = team?.id
  const boardIds = useMemo(
    () => boards.map((board) => board.id),
    [boards]
  )

  const { data: issues, isReady } = useLiveQuery(
    (query) =>
      boardIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                inArray(issues.boardId, boardIds),
                eq(issues.prState, `open`)
              )
            )
        : undefined,
    [boardIds.join(`,`)]
  )

  const { userMap } = useTeamUsers(team?.id)

  // Open PRs with no issue link, fetched live from GitHub through the server
  // (they have no synced row to live-query). Failures degrade to an empty
  // list — the issue-linked queue still renders.
  const [externalGroups, setExternalGroups] = useState<ExternalPullGroup[]>([])
  const [externalLoading, setExternalLoading] = useState(false)
  useEffect(() => {
    if (!teamId) return
    let cancelled = false
    setExternalLoading(true)
    trpc.repositories.openPulls
      .query({ teamId })
      .then((result) => {
        if (cancelled) return
        setExternalGroups(result.repos.filter((repo) => repo.pulls.length > 0))
      })
      .catch(() => {
        if (!cancelled) setExternalGroups([])
      })
      .finally(() => {
        if (!cancelled) setExternalLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [teamId])

  // External PRs have no Electric echo — a successful merge removes the row
  // locally.
  const removeExternalPull = useCallback(
    (repositoryId: string, prNumber: number) => {
      setExternalGroups((groups) =>
        groups
          .map((group) =>
            group.repositoryId === repositoryId
              ? {
                  ...group,
                  pulls: group.pulls.filter((pull) => pull.number !== prNumber),
                }
              : group
          )
          .filter((group) => group.pulls.length > 0)
      )
    },
    []
  )

  return useMemo(() => {
    // Archived issues are hidden on every other surface (and mobile Reviews
    // already excludes them) — drop them at the issue level, like Android's
    // DAO filter: a batch PR entry survives with its remaining issues and
    // disappears only when ALL of its issues are archived.
    const list = ((issues ?? []) as Issue[]).filter(
      (issue) => issue.archivedAt == null
    )

    // Collapse issues sharing a prUrl into ONE entry (EXP-131: a batch PR must
    // not render flattened). Issues without a prUrl can't collide — keyed by id.
    const entriesByKey = new Map<string, ReviewEntry>()
    for (const issue of list) {
      const key = issue.prUrl ?? issue.id
      const entry = entriesByKey.get(key)
      if (entry) {
        entry.issues.push(issue)
      } else {
        entriesByKey.set(key, { key, issue, issues: [issue] })
      }
    }

    const byBoard = new Map<string, ReviewEntry[]>()
    for (const entry of entriesByKey.values()) {
      entry.issues.sort(
        (a, b) =>
          new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
      )
      entry.issue = entry.issues[0]
      // A batch PR's issues may span boards sharing one repo — the entry
      // lives under the representative (newest) issue's board.
      const bucket = byBoard.get(entry.issue.boardId)
      if (bucket) {
        bucket.push(entry)
      } else {
        byBoard.set(entry.issue.boardId, [entry])
      }
    }

    const groups: ReviewGroup[] = []
    // `boards` is already ordered by sortOrder.
    for (const board of boards) {
      const bucket = byBoard.get(board.id)
      if (!bucket) continue
      bucket.sort(
        (a, b) =>
          new Date(b.issue.createdAt).getTime() -
          new Date(a.issue.createdAt).getTime()
      )
      groups.push({ board, entries: bucket })
    }

    const externalCount = externalGroups.reduce(
      (sum, group) => sum + group.pulls.length,
      0
    )

    return {
      groups,
      externalGroups,
      count: entriesByKey.size + externalCount,
      // A team with no boards skips the query and can never deliver a
      // snapshot — treat it as ready-empty instead of loading forever. The
      // external fetch has its own flag so the synced queue renders without
      // waiting on GitHub.
      isLoading: !isReady && boards.length > 0,
      externalLoading,
      userMap,
      removeExternalPull,
    }
  }, [
    issues,
    isReady,
    boards,
    userMap,
    externalGroups,
    externalLoading,
    removeExternalPull,
  ])
}
