// EXP-897 Part 4: the ONE model behind the stack/batch BADGE and its overlay.
// A piece of work can be related to other work three ways, and every client
// draws the same pill and the same three sections off this one function
// (iOS `PrGraph.swift`, Android `PrGraph.kt`, desktop `pr_graph.rs`):
//
//   · a STACK   — pull requests based on each other (`issues.pr_base_branch`)
//   · a BATCH   — issues sharing ONE pull request (`issues.pr_url`)
//   · a TREE    — runs that started other runs (`coding_sessions.parent_session_id`)
//
// A stack MEMBER is a pull request, not an issue, so a batch PR can itself sit
// in a stack — hence the entry type below. Nothing new is stored: every edge
// is already synced.
//
// Test names, mirrored ×4: `reports a stack badge for a stacked pr`,
// `reports a batch badge for a batch pr`, `reports both for a batch inside a
// stack`, `reports nothing for a lone pr`.

import { nestSessions, type SessionTreeRow, type TreeSession } from "@/lib/session-tree"
import { stackChain, type PrStackNode } from "@/lib/pr-stack"
import { openBlockers, type StackStartRelation } from "@/lib/stack-start"
import { batchRunIssues, isBatchRun, type BatchRunIssue } from "@/lib/batch-run"

export interface PrGraphIssue extends PrStackNode, BatchRunIssue {
  identifier: string
  /** The dual-written ANCHOR enum — what `openBlockers` judges. */
  status: string
  prUrl: string | null
}

export interface PrGraphSession extends TreeSession {
  issueId: string | null
  prUrl?: string | null
  /** EXP-876: a BATCH run's own subject — the issues it covers. Absent on a
   *  caller that does not carry them; such a run resolves no batch. */
  actionName?: string | null
  batchIssueIds?: string[] | null
  branch?: string | null
}

/** ONE pull request: a single issue, or every issue sharing its `prUrl`. */
export interface PrGraphEntry<I> {
  key: string
  /** The representative row — it carries the branch, base and PR fields. */
  issue: I
  /** Every issue on this pull request, in caller order (length 1 = plain). */
  issues: I[]
}

export interface PrGraph<I, S> {
  /** The pull request the subject belongs to; null when it has none. */
  entry: PrGraphEntry<I> | null
  /** The chain BOTTOM first, depth = distance from the bottom. Empty when the
   *  subject is in no stack. */
  stack: { entry: PrGraphEntry<I>; depth: number }[]
  /** The issues sharing the subject's pull request; null when it is not one. */
  batch: { issues: I[] } | null
  /** The subject run's whole tree (its root and every descendant), nested. */
  tree: SessionTreeRow<S>[]
  /** The issue face's "Blocked by" section — open blockers of the subject. */
  blockedBy: I[]
  /** 1-based position in `stack` (0 when not stacked) and the stack's size. */
  position: number
  size: number
}

export type BadgeKind = `stack` | `batch` | `stack+batch` | null

function groupEntries<I extends PrGraphIssue>(
  issues: readonly I[]
): { entries: PrGraphEntry<I>[]; byIssueId: Map<string, PrGraphEntry<I>> } {
  const entries: PrGraphEntry<I>[] = []
  const byKey = new Map<string, PrGraphEntry<I>>()
  const byIssueId = new Map<string, PrGraphEntry<I>>()
  for (const issue of issues) {
    // An issue with no pull request cannot collide — keyed by its own id.
    const key = issue.prUrl && issue.prUrl.length > 0 ? issue.prUrl : issue.id
    let entry = byKey.get(key)
    if (!entry) {
      entry = { key, issue, issues: [] }
      byKey.set(key, entry)
      entries.push(entry)
    }
    entry.issues.push(issue)
    byIssueId.set(issue.id, entry)
  }
  return { entries, byIssueId }
}

/**
 * EXP-876: a BATCH run's own entry. A batch links no issue and stamps no
 * `pr_url` of its own, so before this it resolved nothing at all — the pill
 * and its sheet, the one surface built to name work that spans several
 * issues, never appeared on the very run that spans them. Its covered set
 * (`batch_issue_ids`) IS the entry.
 *
 * The PR-grouped entry wins whenever there is one: it carries the branch and
 * the base the stack chains on, so a batch PR stacked on another still reads
 * `stack+batch` and still offers Merge stack. The synthesized entry is what a
 * batch wears BEFORE its PR exists — keyed by the run, since it has no url.
 */
function batchSessionEntry<I extends PrGraphIssue, S extends PrGraphSession>(
  session: S | null,
  issues: readonly I[],
  byIssueId: Map<string, PrGraphEntry<I>>
): PrGraphEntry<I> | null {
  if (!session || !isBatchRun({ issueId: session.issueId, actionName: session.actionName ?? null })) {
    return null
  }
  const covered = batchRunIssues(
    {
      issueId: session.issueId,
      actionName: session.actionName ?? null,
      batchIssueIds: session.batchIssueIds ?? null,
    },
    issues
  )
  const first = covered[0]
  if (!first) return null
  const grouped = byIssueId.get(first.id)
  if (grouped && grouped.issues.length > 1) return grouped
  return { key: `run:${session.id}`, issue: first, issues: covered }
}

/** The subtree ids of `rootId` plus the root itself, over raw sessions. */
function subtreeIds<S extends PrGraphSession>(
  sessions: readonly S[],
  rootId: string
): Set<string> {
  const childrenOf = new Map<string, S[]>()
  for (const session of sessions) {
    const parent = session.parentSessionId
    if (!parent || parent === session.id) continue
    const list = childrenOf.get(parent) ?? []
    list.push(session)
    childrenOf.set(parent, list)
  }
  const ids = new Set<string>([rootId])
  const queue = [rootId]
  while (queue.length > 0) {
    const id = queue.shift()!
    for (const child of childrenOf.get(id) ?? []) {
      if (ids.has(child.id)) continue
      ids.add(child.id)
      queue.push(child.id)
    }
  }
  return ids
}

/** The topmost ancestor of `session` present in `sessions` (itself when it has
 *  no listed parent). Cycle-safe. */
function rootOf<S extends PrGraphSession>(
  sessions: readonly S[],
  session: S
): S {
  const byId = new Map(sessions.map((row) => [row.id, row]))
  const seen = new Set<string>([session.id])
  let current = session
  for (;;) {
    const parentId = current.parentSessionId
    if (!parentId || seen.has(parentId)) return current
    const parent = byId.get(parentId)
    if (!parent) return current
    seen.add(parent.id)
    current = parent
  }
}

/**
 * Everything the badge and its overlay need for ONE subject — an issue, a run,
 * or both (a run's Issue face). Pure: every input is already synced.
 */
export function prGraph<
  I extends PrGraphIssue,
  S extends PrGraphSession,
>(input: {
  issue?: I | null
  session?: S | null
  issues: readonly I[]
  sessions: readonly S[]
  relations?: readonly StackStartRelation[]
}): PrGraph<I, S> {
  const { issues, sessions, relations = [] } = input
  const { entries, byIssueId } = groupEntries(issues)

  const subjectIssue =
    input.issue ??
    (input.session?.issueId
      ? (issues.find((row) => row.id === input.session!.issueId) ?? null)
      : null)

  const subjectEntry =
    (subjectIssue ? (byIssueId.get(subjectIssue.id) ?? null) : null) ??
    (input.session?.prUrl
      ? (entries.find((entry) => entry.key === input.session!.prUrl) ?? null)
      : null) ??
    batchSessionEntry(input.session ?? null, issues, byIssueId)

  // A stack walks over ENTRIES: the representatives carry the branch and the
  // base, and every issue of a batch PR shares them.
  const stack: { entry: PrGraphEntry<I>; depth: number }[] = []
  let position = 0
  let size = 0
  if (subjectEntry) {
    const representatives = entries.map((entry) => entry.issue)
    const chain = stackChain(subjectEntry.issue, representatives)
    if (chain.length >= 2) {
      size = chain.length
      chain.forEach((member, depth) => {
        const entry = byIssueId.get(member.id)
        if (!entry) return
        stack.push({ entry, depth })
        if (entry.key === subjectEntry.key) position = depth + 1
      })
    }
  }

  const batch =
    subjectEntry && subjectEntry.issues.length > 1
      ? { issues: subjectEntry.issues }
      : null

  let tree: SessionTreeRow<S>[] = []
  if (input.session) {
    const root = rootOf(sessions, input.session)
    const ids = subtreeIds(sessions, root.id)
    tree = nestSessions(sessions.filter((row) => ids.has(row.id)))
  } else if (subjectIssue) {
    const ids = new Set<string>()
    for (const row of sessions) {
      if (row.issueId !== subjectIssue.id) continue
      for (const id of subtreeIds(sessions, row.id)) ids.add(id)
    }
    tree = nestSessions(sessions.filter((row) => ids.has(row.id)))
  }

  const blockedBy = subjectIssue
    ? openBlockers(subjectIssue.id, relations, issues)
    : []

  return { entry: subjectEntry, stack, batch, tree, blockedBy, position, size }
}

/** Which glyph(s) the pill wears — `null` = no pill at all. */
export function badgeKind<I, S>(graph: PrGraph<I, S>): BadgeKind {
  const stacked = graph.stack.length >= 2
  const batched = graph.batch !== null
  if (stacked && batched) return `stack+batch`
  if (stacked) return `stack`
  if (batched) return `batch`
  return null
}

/** Which face the badge is drawn on — it decides the overlay's sections and,
 *  on the Run face, whether the session tree alone earns a pill. */
export type PrGraphFace = `issue` | `run` | `changes`

/** EXP-1079: what the header pill DRAWS — a PR relation (`badgeKind`), or,
 *  on the Run face of a run that has a family, the session tree alone
 *  (`runs`: the desktop's `BadgeGlyph::Runs`, the `session-tree` concept).
 *  `null` = no pill. The desktop twin is `BadgeSpec::is_visible` +
 *  `badge_glyphs`: there a lone run has an EMPTY tree, here the tree carries
 *  the subject itself, so "a family" is more than one row. */
export type BadgeShape = Exclude<BadgeKind, null> | `runs` | null

export function badgeShape<I, S>(
  graph: PrGraph<I, S>,
  face: PrGraphFace
): BadgeShape {
  const kind = badgeKind(graph)
  if (kind) return kind
  return face === `run` && graph.tree.length > 1 ? `runs` : null
}

/** The pill's own label: the stack position when there is one, else the
 *  batch's size. Byte-identical ×4. */
export function badgeLabel<I, S>(graph: PrGraph<I, S>): string | null {
  if (graph.stack.length >= 2) return `${graph.position} of ${graph.size}`
  if (graph.batch) {
    const count = graph.batch.issues.length
    return count === 1 ? `1 issue` : `${count} issues`
  }
  return null
}
