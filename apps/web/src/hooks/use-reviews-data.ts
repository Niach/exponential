import { useMemo } from "react"
import {
  and,
  eq,
  inArray,
  isNull,
  not,
  or,
  useLiveQuery,
} from "@tanstack/react-db"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import {
  useBoardsForTeams,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { removeOpenPull, useOpenPulls } from "@/lib/open-pulls-store"
import { reviewsQueue } from "@/lib/reviews-queue"
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

/** EXP-1248: one drawn item of a board band (`reviewsQueue` rule 9). */
export type ReviewItem =
  /** A lone PR (depth 0) or a PR-tree member, nested under its root. */
  | { kind: `pr`; entry: ReviewEntry; depth: number }
  /** A linear stack: entries TOP first, then the base-branch row. */
  | { kind: `stack`; entries: ReviewEntry[]; baseBranch: string | null }

export interface ReviewGroup {
  board: Board
  /** EXP-1186: the board's team (a phone reads every member team). */
  team: Team | undefined
  /** One entry per open PR, newest first — the flat list. */
  entries: ReviewEntry[]
  /** EXP-1248: the same entries as drawn: trees nest, stacks rail. */
  items: ReviewItem[]
}

// EXP-734: one open PR a coding run opened for ITSELF (an action or chat run
// with no linked issue, `exponential_pr_open({ repositoryId, head })`). The
// session row carries prUrl/prNumber/prState, so it needs no GitHub fetch.
// Every run stamps its own PR on its row — a batch run's combined PR too — so
// a run PR that also links issues stays with those issues' entry instead.
export interface SessionReviewEntry {
  key: string
  session: CodingSession
}

export interface ExternalPullGroup {
  /** EXP-1186: the team whose `openPulls` listed it. */
  teamId: string
  repositoryId: string
  fullName: string
  pulls: OpenPull[]
}

/** EXP-1186: the run PRs of one team, a band of their own per team. */
export interface SessionReviewGroup {
  team: Team | undefined
  entries: SessionReviewEntry[]
}

// Cross-board review queue: every issue in the team with an open pull
// request, grouped by board (board sortOrder, issues newest-first). Pure
// client work over the already-synced issues shape — prState arrives on every
// issue row, and the collections' snakeCamelMapper makes the camelCase filter
// match the Postgres pr_state column.
// EXP-1186: `teams` widens it to several teams (the phone reads every member
// team, `useCrossTeamScope`) — boards ordered team by team, in that order.
// Omitted = the active team alone.
export function useReviewsData(
  team: Team | null | undefined,
  teams?: readonly Team[],
  // EXP-1244: the page refetches the unlinked pulls on mount (`force`); the
  // nav reads the shared store and refetches only a stale team.
  options: { force?: boolean } = {}
) {
  const scopeTeams = useMemo<Team[]>(
    () =>
      teams && teams.length > 0 ? [...teams] : team ? [team] : [],
    [team, teams]
  )
  const orderedTeamIds = useMemo(
    () => scopeTeams.map((row) => row.id),
    [scopeTeams]
  )
  const teamIds = useMemo(() => [...orderedTeamIds].sort(), [orderedTeamIds])
  const teamKey = teamIds.join(`,`)
  const teamById = useMemo(
    () => new Map(scopeTeams.map((row) => [row.id, row])),
    [scopeTeams]
  )
  const { boards } = useBoardsForTeams(orderedTeamIds)
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
                // EXP-1244: every PR-carrying row, whatever its state — a
                // linked PR never lists as unlinked (`reviewsQueue` rule 5).
                or(eq(issues.prState, `open`), not(isNull(issues.prUrl)))
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
      teamIds.length > 0
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                inArray(sessions.teamId, teamIds),
                not(isNull(sessions.prUrl))
              )
            )
        : undefined,
    [teamKey]
  )

  // Open PRs with no issue link, fetched live from GitHub through the server
  // into the app-wide store the nav dot reads too (EXP-1244). Failures
  // degrade to an empty list — the issue-linked queue still renders.
  const { repos: externalGroups, loading: externalLoading } = useOpenPulls(
    orderedTeamIds,
    { force: options.force }
  )

  return useMemo(() => {
    const rows = (issues ?? []) as Issue[]
    // EXP-1244: the ONE queue ×4 (`lib/reviews-queue.ts`, fixture-locked).
    const queue = reviewsQueue({
      teams: scopeTeams,
      boards,
      issues: rows,
      sessions: (sessionRows ?? []) as CodingSession[],
      pulls: externalGroups,
    })
    const groups: ReviewGroup[] = queue.boardGroups.map((group) => {
      const entries: ReviewEntry[] = group.entries.map((entry) => ({
        key: entry.key,
        issue: entry.issues[0]!,
        issues: entry.issues,
      }))
      const byKey = new Map(entries.map((entry) => [entry.key, entry]))
      const items: ReviewItem[] = group.items.map((item) =>
        item.kind === `pr`
          ? { kind: `pr`, entry: byKey.get(item.entry.key)!, depth: item.depth }
          : {
              kind: `stack`,
              entries: item.entries.map((entry) => byKey.get(entry.key)!),
              baseBranch: item.baseBranch,
            }
      )
      return {
        board: group.board,
        team: teamById.get(group.board.teamId),
        entries,
        items,
      }
    })
    const sessionGroups: SessionReviewGroup[] = queue.runGroups.map(
      (group) => ({
        team: teamById.get(group.teamId),
        entries: group.sessions.map((session) => ({
          key: `session:${session.id}`,
          session,
        })),
      })
    )
    const sessionEntries = sessionGroups.flatMap((group) => group.entries)
    const list = rows.filter((issue) => issue.prState === `open`)

    return {
      groups,
      sessionEntries,
      sessionGroups,
      externalGroups: queue.repoGroups,
      // The nav dot's input only: no Reviews band renders a count (EXP-1248).
      count: queue.count,
      // A team with no boards skips the query and can never deliver a
      // snapshot — treat it as ready-empty instead of loading forever. The
      // external fetch has its own flag so the synced queue renders without
      // waiting on GitHub.
      isLoading: !isReady && boards.length > 0,
      externalLoading,
      userMap,
      removeExternalPull: removeOpenPull,
      // Every open-PR issue of the team, the rows a Merge reads its stack
      // from (`stackMergeConfirm`, `stackView`).
      openIssues: list,
    }
  }, [
    issues,
    sessionRows,
    isReady,
    boards,
    userMap,
    externalGroups,
    externalLoading,
    scopeTeams,
    teamById,
  ])
}
