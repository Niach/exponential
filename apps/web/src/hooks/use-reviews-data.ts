import { useCallback, useEffect, useMemo, useState } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import {
  useBoardsForTeams,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"
import { byCreatedAtDesc } from "@/lib/ordering"
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

export interface ReviewGroup {
  board: Board
  /** EXP-1186: the board's team (a phone reads every member team). */
  team: Team | undefined
  /** One entry per open PR, newest first — a FLAT list. */
  entries: ReviewEntry[]
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
  teams?: readonly Team[]
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
      teamIds.length > 0
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                inArray(sessions.teamId, teamIds),
                eq(sessions.prState, `open`)
              )
            )
        : undefined,
    [teamKey]
  )

  // Open PRs with no issue link, fetched live from GitHub through the server
  // (they have no synced row to live-query). Failures degrade to an empty
  // list — the issue-linked queue still renders.
  const [externalGroups, setExternalGroups] = useState<ExternalPullGroup[]>([])
  const [externalLoading, setExternalLoading] = useState(false)
  useEffect(() => {
    if (teamIds.length === 0) return
    let cancelled = false
    setExternalLoading(true)
    // One request per team; a team that fails just lists nothing.
    Promise.all(
      teamIds.map((teamId) =>
        trpc.repositories.openPulls
          .query({ teamId })
          .then((result) =>
            result.repos
              .filter((repo) => repo.pulls.length > 0)
              .map((repo) => ({ ...repo, teamId }))
          )
          .catch(() => [] as ExternalPullGroup[])
      )
    )
      .then((perTeam) => {
        if (cancelled) return
        // Team order, like the board bands.
        const order = new Map(orderedTeamIds.map((id, index) => [id, index]))
        setExternalGroups(
          perTeam
            .flat()
            .sort(
              (left, right) =>
                (order.get(left.teamId) ?? 0) - (order.get(right.teamId) ?? 0)
            )
        )
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
    // `teamKey` names the id set; the order map only sorts.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [teamKey])

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

    // A batch PR's issues may span boards sharing one repo — the entry lives
    // under the representative (newest) issue's board.
    const byBoard = new Map<string, ReviewEntry[]>()
    for (const entry of allEntries) {
      const bucket = byBoard.get(entry.issue.boardId)
      if (bucket) bucket.push(entry)
      else byBoard.set(entry.issue.boardId, [entry])
    }

    const groups: ReviewGroup[] = []
    // `boards` is already ordered by sortOrder.
    for (const board of boards) {
      const bucket = byBoard.get(board.id)
      if (!bucket) continue
      groups.push({ board, team: teamById.get(board.teamId), entries: bucket })
    }

    // EXP-734: the run's OWN PR (no linked issue), newest per prUrl. A batch
    // run's combined PR links issues, so it already rides their entry.
    const issuePrUrls = new Set(
      list.map((issue) => issue.prUrl).filter((url): url is string => !!url)
    )
    const sessionByUrl = new Map<string, CodingSession>()
    for (const session of (sessionRows ?? []) as CodingSession[]) {
      if (session.issueId != null || !session.prUrl) continue
      if (issuePrUrls.has(session.prUrl)) continue
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

    // EXP-1186: one "Agent runs" band per team, in team order.
    const sessionGroups: SessionReviewGroup[] = orderedTeamIds
      .map((teamId) => ({
        team: teamById.get(teamId),
        entries: sessionEntries.filter(
          (entry) => entry.session.teamId === teamId
        ),
      }))
      .filter((group) => group.entries.length > 0)

    // The server already excludes linked PRs from `openPulls`, but its 60 s
    // cache (and a fetch taken before `pr_open` stamped the row, kept for as
    // long as the page stays open, EXP-1244) can still hand back one an issue
    // or a run just claimed — drop it here so the same PR never renders twice
    // nor as "not linked to an issue".
    const linkedUrls = new Set([...issuePrUrls, ...sessionByUrl.keys()])
    const externalPullGroups = externalGroups
      .map((group) => ({
        ...group,
        pulls: group.pulls.filter((pull) => !linkedUrls.has(pull.url)),
      }))
      .filter((group) => group.pulls.length > 0)

    const externalCount = externalPullGroups.reduce(
      (sum, group) => sum + group.pulls.length,
      0
    )

    return {
      groups,
      sessionEntries,
      sessionGroups,
      externalGroups: externalPullGroups,
      count:
        entriesByKey.size +
        sessionEntries.length +
        externalCount,
      // A team with no boards skips the query and can never deliver a
      // snapshot — treat it as ready-empty instead of loading forever. The
      // external fetch has its own flag so the synced queue renders without
      // waiting on GitHub.
      isLoading: !isReady && boards.length > 0,
      externalLoading,
      userMap,
      removeExternalPull,
      // EXP-1145: every open-PR issue of the team, the rows a Merge reads its
      // stack from (`stackMergeChoice`).
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
    removeExternalPull,
    orderedTeamIds,
    teamById,
  ])
}
