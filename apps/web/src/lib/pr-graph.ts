// EXP-897 Part 4: the ONE model behind the stack/batch BADGE and its overlay.
// A piece of work can be related to other work three ways, and every client
// draws the same stacked chip and the same three sections off this one function
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
import {
  MERGE_STACK_LABEL,
  stackChain,
  type PrStackNode,
} from "@/lib/pr-stack"
import { RELATIONS_VIEW_COPY } from "@/lib/issue-relations-view"
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

/** Which relation(s) the subject has — `null` = no chip at all. */
export function badgeKind<I, S>(graph: PrGraph<I, S>): BadgeKind {
  const stacked = graph.stack.length >= 2
  const batched = graph.batch !== null
  if (stacked && batched) return `stack+batch`
  if (stacked) return `stack`
  if (batched) return `batch`
  return null
}

/** Which face the badge is drawn on. EXP-1097: it no longer decides WHETHER
 *  the chip draws (`badgeShape` is face-independent), only which overlay
 *  section leads when it opens (`overlaySections`). */
export type PrGraphFace = `issue` | `run` | `changes`

/** EXP-1079/EXP-1097: what the header chip DRAWS, the SAME on every face —
 *  Issue, Run and Changes alike. First match wins:
 *
 *    1. a PR relation (`badgeKind`: stack, batch, stack+batch);
 *    2. `runs` — the session tree has a FAMILY (the desktop's
 *       `BadgeGlyph::Runs`, the `session-tree` concept). Here the tree
 *       carries the subject itself, so "a family" is more than one row;
 *    3. `blocked` — the subject issue has OPEN blockers (`graph.blockedBy`);
 *    4. `null` = no chip.
 *
 *  The same rule, byte for byte, in desktop `pr_graph.rs` (`badge_shape`),
 *  iOS `PrGraph.swift` and Android `PrGraph.kt` (`badgeShape`). */
export type BadgeShape = Exclude<BadgeKind, null> | `runs` | `blocked` | null

export function badgeShape<I, S>(graph: PrGraph<I, S>): BadgeShape {
  const kind = badgeKind(graph)
  if (kind) return kind
  if (graph.tree.length > 1) return `runs`
  if (graph.blockedBy.length > 0) return `blocked`
  return null
}

/** EXP-1058: what the header's STACKED issue chip draws in place of the old
 *  pill — the front chip's issue and how many ride behind it (`+N`).
 *
 *  · stack / batch — `issue` = the subject's pull request's representative
 *    row, `count` = every OTHER issue on the stack (all its entries' issues)
 *    or batch;
 *  · `runs` — `issue` = that representative (null for a run with no issue,
 *    where the front chip names the run instead), `count` = every other run
 *    of the tree;
 *  · `blocked` (EXP-1097) — `issue` = the FIRST open blocker in `blockedBy`
 *    order, `count` = the other open blockers.
 *
 *  `null` = no chip, exactly when `badgeShape` is null. Face-independent and
 *  byte-identical ×4 (`pr_graph::badge_chip`, `PrGraph.badgeChip` ×2). */
export interface BadgeChip<I> {
  issue: I | null
  count: number
}

export function badgeChip<I, S>(graph: PrGraph<I, S>): BadgeChip<I> | null {
  const shape = badgeShape(graph)
  if (!shape) return null
  if (shape === `blocked`) {
    return { issue: graph.blockedBy[0] ?? null, count: graph.blockedBy.length - 1 }
  }
  const issue = graph.entry?.issue ?? null
  if (shape === `runs`) return { issue, count: graph.tree.length - 1 }
  if (graph.stack.length >= 2) {
    const total = graph.stack.reduce((sum, row) => sum + row.entry.issues.length, 0)
    return { issue, count: total - 1 }
  }
  return { issue, count: (graph.batch?.issues.length ?? 1) - 1 }
}

/** One section of the chip's overlay. */
export type OverlaySection = `blocked` | `batch` | `runs` | `stack`

const FACE_SECTION_ORDER: Record<PrGraphFace, OverlaySection[]> = {
  issue: [`blocked`, `batch`, `stack`, `runs`],
  run: [`batch`, `runs`, `stack`, `blocked`],
  changes: [`stack`, `batch`, `runs`, `blocked`],
}

/** EXP-1097: the overlay's sections — every relation the subject HAS, the
 *  face's own section first (Issue: Blocked by; Run: the run's issues and
 *  its tree; Changes: the pull requests). A section with nothing to list is
 *  left out, save the face's own lead on Run (its tree, even of one run) and
 *  on Changes (its pull request, even a lone one). */
export function overlaySections<I, S>(
  graph: PrGraph<I, S>,
  face: PrGraphFace
): OverlaySection[] {
  const present: Record<OverlaySection, boolean> = {
    blocked: graph.blockedBy.length > 0,
    batch: graph.batch !== null,
    runs: graph.tree.length > 1 || (face === `run` && graph.tree.length > 0),
    stack:
      graph.stack.length >= 2 || (face === `changes` && graph.entry !== null),
  }
  return FACE_SECTION_ORDER[face].filter((section) => present[section])
}

/** SLOP-16 r3: THE "Related work" view's title — the platform's standard
 *  modal (web `Dialog`, which drops to its sheet arm on phones). */
export const RELATED_WORK_TITLE = `Related work`

/** SLOP-16 r3: the "Related work" view's copy, byte-identical ×4 (desktop
 *  `pr_graph.rs`, iOS `PrGraphBadge.swift`, Android `PrGraphBadge.kt`). Each
 *  section = a group band over the product's existing rows. */
export const PR_GRAPH_OVERLAY_COPY = {
  title: RELATED_WORK_TITLE,
  blocked: RELATIONS_VIEW_COPY.blockedBy,
  /** The batch band on the Issue and Changes faces. */
  batch: `In batch with`,
  /** The batch band on the Run face — the run's own subject. */
  batchRun: `Issues`,
  runs: `Runs`,
  stack: `Pull requests`,
  mergeStack: MERGE_STACK_LABEL,
  empty: `Nothing else is linked to this issue.`,
} as const

/** The batch band's title on a face. */
export function batchBandTitle(face: PrGraphFace): string {
  return face === `run`
    ? PR_GRAPH_OVERLAY_COPY.batchRun
    : PR_GRAPH_OVERLAY_COPY.batch
}
