import fixture from "@exp/domain-contract/fixtures/reviews-queue.json"
import { byCreatedAtDesc } from "@/lib/ordering"

/**
 * EXP-1244: the Reviews queue, ONE pure function ×4 (desktop
 * `domain::reviews_queue`, iOS `ReviewsQueue.build`, Android
 * `ReviewsQueue.build`), locked by
 * `packages/domain-contract/fixtures/reviews-queue.json` (its `_doc` = the
 * rules). Synced issues + runs + the `repositories.openPulls` results →
 * board bands, "Agent runs" bands and the unlinked-PR repo bands.
 */

/** A repository band's trailing caption on a single-team list ×4. */
export const REPO_BAND_CAPTION = fixture.labels.repoBandCaption

export interface QueueTeam {
  id: string
}

export interface QueueBoard {
  id: string
  teamId: string
  name: string
  sortOrder: number | null
}

export interface QueueIssue {
  id: string
  boardId: string
  createdAt: Date | string | number
  prUrl: string | null
  prState: string | null
}

export interface QueueSession {
  id: string
  teamId: string
  issueId: string | null
  prUrl: string | null
  prState: string | null
  createdAt: Date | string | number
}

export interface QueuePull {
  number: number
  url: string
}

export interface QueuePullRepo<P extends QueuePull = QueuePull> {
  teamId: string
  repositoryId: string
  fullName: string
  pulls: P[]
}

/** One open PR: every issue it links, `issues[0]` = the representative. */
export interface QueueEntry<I extends QueueIssue> {
  key: string
  issues: I[]
}

export interface QueueBoardGroup<B extends QueueBoard, I extends QueueIssue> {
  board: B
  entries: QueueEntry<I>[]
}

export interface QueueRunGroup<S extends QueueSession> {
  teamId: string
  sessions: S[]
}

export interface ReviewsQueue<
  B extends QueueBoard,
  I extends QueueIssue,
  S extends QueueSession,
  P extends QueuePull,
> {
  boardGroups: QueueBoardGroup<B, I>[]
  runGroups: QueueRunGroup<S>[]
  repoGroups: QueuePullRepo<P>[]
  count: number
}

const OPEN = `open`

const present = (url: string | null | undefined): url is string =>
  url != null && url !== ``

const byId = (a: { id: string }, b: { id: string }): number =>
  a.id < b.id ? -1 : a.id > b.id ? 1 : 0

export function reviewsQueue<
  B extends QueueBoard,
  I extends QueueIssue,
  S extends QueueSession,
  P extends QueuePull,
>(input: {
  teams: readonly QueueTeam[]
  boards: readonly B[]
  issues: readonly I[]
  sessions: readonly S[]
  pulls: readonly QueuePullRepo<P>[]
}): ReviewsQueue<B, I, S, P> {
  const teamOrder = new Map(input.teams.map((team, index) => [team.id, index]))
  const boardById = new Map(
    input.boards
      .filter((board) => teamOrder.has(board.teamId))
      .map((board) => [board.id, board])
  )
  const issues = input.issues.filter((issue) => boardById.has(issue.boardId))
  const sessions = input.sessions.filter((session) =>
    teamOrder.has(session.teamId)
  )

  // (2) One entry per open PR, issues newest first (id ascending on a tie).
  const byKey = new Map<string, I[]>()
  for (const issue of issues) {
    if (issue.prState !== OPEN) continue
    const key = present(issue.prUrl) ? issue.prUrl : `issue:${issue.id}`
    const bucket = byKey.get(key)
    if (bucket) bucket.push(issue)
    else byKey.set(key, [issue])
  }
  const entries: QueueEntry<I>[] = [...byKey].map(([key, rows]) => ({
    key,
    issues: [...rows].sort(byCreatedAtDesc),
  }))
  // (3) Newest representative first, under the representative's board.
  entries.sort((a, b) => byCreatedAtDesc(a.issues[0]!, b.issues[0]!))
  const entriesByBoard = new Map<string, QueueEntry<I>[]>()
  for (const entry of entries) {
    const boardId = entry.issues[0]!.boardId
    const bucket = entriesByBoard.get(boardId)
    if (bucket) bucket.push(entry)
    else entriesByBoard.set(boardId, [entry])
  }
  // (4) Team order, sort_order (null last), name, id.
  const boardGroups = [...entriesByBoard]
    .map(([boardId, boardEntries]) => ({
      board: boardById.get(boardId)!,
      entries: boardEntries,
    }))
    .sort((a, b) => {
      const team =
        teamOrder.get(a.board.teamId)! - teamOrder.get(b.board.teamId)!
      if (team !== 0) return team
      const left = a.board.sortOrder ?? Number.POSITIVE_INFINITY
      const right = b.board.sortOrder ?? Number.POSITIVE_INFINITY
      if (left !== right) return left < right ? -1 : 1
      const nameA = a.board.name.toLowerCase()
      const nameB = b.board.name.toLowerCase()
      if (nameA !== nameB) return nameA < nameB ? -1 : 1
      return byId(a.board, b.board)
    })

  // (5) Linked = any in-scope issue's or run's PR, whatever its state.
  const issueUrls = new Set(
    issues.map((issue) => issue.prUrl).filter(present)
  )
  const linked = new Set([
    ...issueUrls,
    ...sessions.map((session) => session.prUrl).filter(present),
  ])

  // (6) A run's OWN PR: newest row per pr_url, banded per team.
  const runByUrl = new Map<string, S>()
  for (const session of sessions) {
    if (session.issueId != null || session.prState !== OPEN) continue
    if (!present(session.prUrl) || issueUrls.has(session.prUrl)) continue
    const current = runByUrl.get(session.prUrl)
    if (!current || byCreatedAtDesc(session, current) < 0) {
      runByUrl.set(session.prUrl, session)
    }
  }
  const runs = [...runByUrl.values()].sort(byCreatedAtDesc)
  const runGroups = input.teams
    .map((team) => ({
      teamId: team.id,
      sessions: runs.filter((session) => session.teamId === team.id),
    }))
    .filter((group) => group.sessions.length > 0)

  // (7) The unlinked pulls, team order then fetch order.
  const repoGroups = input.pulls
    .filter((repo) => teamOrder.has(repo.teamId))
    .map((repo, index) => ({ repo, index }))
    .sort(
      (a, b) =>
        teamOrder.get(a.repo.teamId)! - teamOrder.get(b.repo.teamId)! ||
        a.index - b.index
    )
    .map(({ repo }) => ({
      ...repo,
      pulls: repo.pulls.filter((pull) => !linked.has(pull.url)),
    }))
    .filter((repo) => repo.pulls.length > 0)

  return {
    boardGroups,
    runGroups,
    repoGroups,
    count:
      entries.length +
      runs.length +
      repoGroups.reduce((sum, repo) => sum + repo.pulls.length, 0),
  }
}
