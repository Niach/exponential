import { useCallback, useEffect, useMemo, useState } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  codingSessionCollection,
  issueCollection,
} from "@/lib/collections"
import {
  useTeamBoards,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"
import { byCreatedAtDesc } from "@/lib/ordering"
import { nestPrStacks } from "@/lib/pr-stack"
import type { OpenPull } from "@/lib/integrations/github-pr"
import type { CodingSession, Issue, Board, Team } from "@/db/schema"

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

// EXP-897: one row of the queue — an entry plus where it sits in its PR
// STACK (`issues.pr_base_branch`, `lib/pr-stack.ts`). An upper entry follows
// the one it is based on, indented, and the BOTTOM row of a stack is the one
// that can merge the whole thing.
export interface ReviewRow {
  entry: ReviewEntry
  depth: number
  hasChildren: boolean
  /** The identifier this entry is stacked on — the `on top of #IDENT`
   *  caption. Null on a root. */
  stackedOn: string | null
  /** Bottom row of a real stack only: the TOP entry's representative issue id
   *  (what `issues.mergePr({ mergeStack: true })` takes) and the stack's size. */
  stackTopIssueId: string | null
  stackSize: number
}

export interface ReviewGroup {
  board: Board
  /** Nested rows, roots newest-first. */
  rows: ReviewRow[]
  /** The same entries, flat — the sidebar nav and the counts. */
  entries: ReviewEntry[]
}

// EXP-734: one open PR a coding run opened for ITSELF (an action or chat run
// with no linked issue, `exponential_pr_open({ repositoryId, head })`). The
// session row carries prUrl/prNumber/prState, so it needs no GitHub fetch.
export interface SessionReviewEntry {
  key: string
  session: CodingSession
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

  // EXP-734: run PRs that link no issue. Team-scoped over the synced
  // coding_sessions shape — the issue-less filter and the prUrl dedupe run in
  // JS below (a live query cannot express either).
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                eq(sessions.teamId, teamId),
                eq(sessions.prState, `open`)
              )
            )
        : undefined,
    [teamId]
  )

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
    const list = (issues ?? []) as Issue[]

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

    const allEntries: ReviewEntry[] = []
    for (const entry of entriesByKey.values()) {
      entry.issues.sort(byCreatedAtDesc)
      entry.issue = entry.issues[0]
      allEntries.push(entry)
    }
    allEntries.sort((a, b) => byCreatedAtDesc(a.issue, b.issue))

    // EXP-897: nest the WHOLE queue before bucketing — a stack can cross
    // boards (its members share a repository, not a board), and it must read
    // as one stack wherever its bottom lives.
    const nested = nestPrStacks(allEntries)
    const rows: ReviewRow[] = nested.map((row, index) => {
      // The parent is the nearest earlier row one level up.
      let stackedOn: string | null = null
      if (row.depth > 0) {
        for (let back = index - 1; back >= 0; back -= 1) {
          if (nested[back].depth === row.depth - 1) {
            stackedOn = nested[back].entry.issue.identifier
            break
          }
        }
      }
      // A root with children owns the stack: its size is its whole subtree,
      // and the TOP is the deepest row of it (the one GitHub merges).
      let stackTopIssueId: string | null = null
      let stackSize = 1
      if (row.depth === 0 && row.hasChildren) {
        let top = row.entry
        let deepest = 0
        for (let ahead = index + 1; ahead < nested.length; ahead += 1) {
          if (nested[ahead].depth === 0) break
          stackSize += 1
          if (nested[ahead].depth > deepest) {
            deepest = nested[ahead].depth
            top = nested[ahead].entry
          }
        }
        stackTopIssueId = top.issue.id
      }
      return {
        entry: row.entry,
        depth: row.depth,
        hasChildren: row.hasChildren,
        stackedOn,
        stackTopIssueId,
        stackSize,
      }
    })

    // A stack lives under its ROOT entry's board, so the nesting survives the
    // grouping. A batch PR's issues may span boards sharing one repo — the
    // entry lives under the representative (newest) issue's board.
    const byBoard = new Map<string, ReviewRow[]>()
    let rootBoardId = ``
    for (const row of rows) {
      if (row.depth === 0) rootBoardId = row.entry.issue.boardId
      const bucket = byBoard.get(rootBoardId)
      if (bucket) bucket.push(row)
      else byBoard.set(rootBoardId, [row])
    }

    const groups: ReviewGroup[] = []
    // `boards` is already ordered by sortOrder.
    for (const board of boards) {
      const bucket = byBoard.get(board.id)
      if (!bucket) continue
      groups.push({
        board,
        rows: bucket,
        entries: bucket.map((row) => row.entry),
      })
    }

    // EXP-734: the run's OWN PR (no linked issue), newest per prUrl.
    const sessionByUrl = new Map<string, CodingSession>()
    for (const session of (sessionRows ?? []) as CodingSession[]) {
      if (session.issueId != null || !session.prUrl) continue
      const current = sessionByUrl.get(session.prUrl)
      if (
        !current ||
        new Date(session.createdAt).getTime() >
          new Date(current.createdAt).getTime()
      ) {
        sessionByUrl.set(session.prUrl, session)
      }
    }
    const sessionEntries: SessionReviewEntry[] = [...sessionByUrl.values()]
      .sort(byCreatedAtDesc)
      .map((session) => ({ key: `session:${session.id}`, session }))

    // The server already excludes run PRs from `openPulls`, but its 60 s
    // cache can still hand back one that a run just claimed — drop it here so
    // the same PR never renders twice.
    const runUrls = new Set(sessionByUrl.keys())
    const externalPullGroups = externalGroups
      .map((group) => ({
        ...group,
        pulls: group.pulls.filter((pull) => !runUrls.has(pull.url)),
      }))
      .filter((group) => group.pulls.length > 0)

    const externalCount = externalPullGroups.reduce(
      (sum, group) => sum + group.pulls.length,
      0
    )

    return {
      groups,
      sessionEntries,
      externalGroups: externalPullGroups,
      count: entriesByKey.size + sessionEntries.length + externalCount,
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
    sessionRows,
    isReady,
    boards,
    userMap,
    externalGroups,
    externalLoading,
    removeExternalPull,
  ])
}
