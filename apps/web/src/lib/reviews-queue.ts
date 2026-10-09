import fixture from "@exp/domain-contract/fixtures/reviews-queue.json"
import { byCreatedAtDesc } from "@/lib/ordering"

/**
 * EXP-1244: the Reviews queue, ONE pure function ×4 (desktop
 * `domain::reviews_queue`, iOS `ReviewsQueue.build`, Android
 * `ReviewsQueue.build`), locked by
 * `packages/domain-contract/fixtures/reviews-queue.json` (its `_doc` = the
 * rules). Synced issues + runs + the `repositories.openPulls` results →
 * board bands, "Agent runs" bands and the unlinked-PR repo bands.
 *
 * EXP-1248 (rule 9, `_groupDoc`): inside a board band a PR TREE nests under
 * its root (`items` kind `pr` with a depth) and a linear STACK is ONE `stack`
 * item, top first, over its base branch. Bands render NO count (`count` is
 * the nav dot's input only).
 */

/** A repository band's trailing caption on a single-team list ×4. */
export const REPO_BAND_CAPTION = fixture.labels.repoBandCaption
/** The "Agent runs" band's trailing caption on a single-team list ×4. */
export const RUN_BAND_CAPTION = fixture.labels.runBandCaption

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
  /** EXP-1248: the stack edge (absent = no edge). */
  branch?: string | null
  prBaseBranch?: string | null
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

/** EXP-1248: one display item of a board band. */
export type QueueItem<I extends QueueIssue> =
  /** A lone PR (depth 0) or a member of a PR TREE, pre-order under its root. */
  | { kind: `pr`; entry: QueueEntry<I>; depth: number }
  /** A linear STACK: its entries TOP first, then the base-branch row. */
  | { kind: `stack`; entries: QueueEntry<I>[]; baseBranch: string | null }

export interface QueueBoardGroup<B extends QueueBoard, I extends QueueIssue> {
  board: B
  /** Every entry, flat, newest first (rule 3). */
  entries: QueueEntry<I>[]
  /** The same entries as the band draws them (rule 9). */
  items: QueueItem<I>[]
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

/**
 * The Reviews nav entry ×4 (`_navDoc`): the dot = anything in the queue; the
 * entry shows unless every team in scope runs yolo mode with nothing open.
 */
export function reviewsNav(input: {
  yolo: readonly boolean[]
  count: number
}): { dot: boolean; shows: boolean } {
  const dot = input.count > 0
  return {
    dot,
    shows: input.yolo.length === 0 || input.yolo.some((yolo) => !yolo) || dot,
  }
}

const edge = (branch: string | null | undefined): string | null =>
  branch != null && branch !== `` ? branch : null

/**
 * (9) A band's entries as items. Edge: an entry sits on the entry (same
 * band) whose representative's `branch` is its representative's
 * `prBaseBranch`. A component lists where its NEWEST entry would (its first
 * entry in band order), walked from its ROOT (the entry with no parent; a
 * cycle breaks where it first repeats). Any
 * entry with two children = a TREE: pre-order, children in band order,
 * depth = distance from the root. Otherwise 2+ entries = a STACK item, top
 * first, `baseBranch` = the root's `prBaseBranch`. One entry = depth 0.
 */
export function queueItems<I extends QueueIssue>(
  entries: readonly QueueEntry<I>[]
): QueueItem<I>[] {
  const rep = (entry: QueueEntry<I>) => entry.issues[0]!
  const owner = new Map<string, QueueEntry<I>>()
  for (const entry of entries) {
    const branch = edge(rep(entry).branch)
    if (branch && !owner.has(branch)) owner.set(branch, entry)
  }
  const parentOf = new Map<string, QueueEntry<I>>()
  const children = new Map<string, QueueEntry<I>[]>()
  for (const entry of entries) {
    const base = edge(rep(entry).prBaseBranch)
    const parent = base ? owner.get(base) : undefined
    if (!parent || parent.key === entry.key) continue
    parentOf.set(entry.key, parent)
    const bucket = children.get(parent.key)
    if (bucket) bucket.push(entry)
    else children.set(parent.key, [entry])
  }
  const placed = new Set<string>()
  const items: QueueItem<I>[] = []
  for (const start of entries) {
    if (placed.has(start.key)) continue
    // Climb to the root; a cycle stops where it first repeats.
    let root = start
    const climbed = new Set([root.key])
    for (;;) {
      const parent = parentOf.get(root.key)
      if (!parent || climbed.has(parent.key) || placed.has(parent.key)) break
      climbed.add(parent.key)
      root = parent
    }
    // The component under the root, pre-order, children in band order.
    const members: Array<{ entry: QueueEntry<I>; depth: number }> = []
    let fork = false
    const visit = (entry: QueueEntry<I>, depth: number) => {
      if (placed.has(entry.key)) return
      placed.add(entry.key)
      members.push({ entry, depth })
      const below = (children.get(entry.key) ?? []).filter(
        (child) => !placed.has(child.key)
      )
      if (below.length > 1) fork = true
      for (const child of below) visit(child, depth + 1)
    }
    visit(root, 0)
    if (members.length > 1 && !fork) {
      items.push({
        kind: `stack`,
        entries: members.map((member) => member.entry).reverse(),
        baseBranch: edge(rep(root).prBaseBranch),
      })
    } else {
      for (const member of members) items.push({ kind: `pr`, ...member })
    }
  }
  return items
}

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
      items: queueItems(boardEntries),
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
