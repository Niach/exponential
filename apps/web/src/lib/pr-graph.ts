// EXP-897 Part 4: the ONE model behind the stack/batch BADGE and its overlay.
// A piece of work can be related to other work three ways, and every client
// draws the same badge and the same three bands off this one function
// (iOS `PrGraph.swift`, Android `PrGraph.kt`, desktop `pr_graph.rs`):
//
//   · BLOCKERS : the subject issue's direct open blockers (`blocks` rows)
//   · a BATCH  : issues sharing ONE pull request (`pull_request_issues`)
//   · a STACK  : pull requests based on each other (`lib/pr-stack.ts`)
//
// EXP-1293: PR state lives on `pull_requests` (+ `pull_request_issues`), so
// the rows below are `issuePrRows`: each issue joined to ITS pull request
// (the open one, else the newest), grouped by pull request. A stack MEMBER
// is a pull request, not an issue, so a batch PR can itself sit in a stack:
// hence the entry type below. Nothing new is stored: every edge is synced.
//
// Test names, mirrored ×4: `reports a stack badge for a stacked pr`,
// `reports a batch badge for a batch pr`, `reports both for a batch inside a
// stack`, `reports nothing for a lone pr`.

import { stackChain, type PrStackNode } from "@/lib/pr-stack"

// EXP-1248: a base-chained component's shape (tree | stack | single).
export { prComponent, prGraphShape, type PrGraphShape } from "@/lib/pr-stack"
import { RELATIONS_VIEW_COPY } from "@/lib/issue-relations-view"
import { openBlockers, type GraphRelation } from "@/lib/issue-graph"

/** An issue's view of its pull request (`issuePrRows`). */
export interface IssuePrFields {
  pullRequestId: string | null
  basePullRequestId: string | null
  prUrl: string | null
  prNumber: number | null
  prState: string | null
  /** The PR's head branch. */
  branch: string | null
  /** The branch the PR is based on. */
  prBaseBranch: string | null
}

export interface PrGraphIssue extends PrStackNode {
  id: string
  identifier: string
  /** The dual-written ANCHOR enum: what `openBlockers` judges. */
  status: string
  prUrl: string | null
}

/** The synced `pull_requests` columns the join reads. */
export interface PrLike {
  id: string
  number: number
  url: string
  head: string
  base: string
  basePullRequestId: string | null
  state: string
}

/** The synced `pull_request_issues` columns the join reads. */
export interface PrIssueLinkLike {
  pullRequestId: string
  issueId: string
}

/**
 * Each issue joined to ITS pull request: the open one, else the highest
 * number; an issue with none carries nulls. Every PR fact the client reads
 * about an issue comes from here, never from the legacy issue columns.
 */
export function issuePrRows<I extends { id: string }>(
  issues: readonly I[],
  pullRequests: readonly PrLike[],
  links: readonly PrIssueLinkLike[]
): (Omit<I, keyof IssuePrFields> & IssuePrFields)[] {
  const prById = new Map(pullRequests.map((pr) => [pr.id, pr]))
  const prOf = new Map<string, PrLike>()
  for (const link of links) {
    const pr = prById.get(link.pullRequestId)
    if (!pr) continue
    const held = prOf.get(link.issueId)
    const better =
      !held ||
      (pr.state === `open` && held.state !== `open`) ||
      ((pr.state === `open`) === (held.state === `open`) && pr.number > held.number)
    if (better) prOf.set(link.issueId, pr)
  }
  return issues.map((issue) => {
    const pr = prOf.get(issue.id) ?? null
    return {
      ...issue,
      pullRequestId: pr?.id ?? null,
      basePullRequestId: pr?.basePullRequestId ?? null,
      prUrl: pr?.url ?? null,
      prNumber: pr?.number ?? null,
      prState: pr?.state ?? null,
      branch: pr?.head ?? null,
      prBaseBranch: pr?.base ?? null,
    }
  })
}

export interface PrGraphEntry<I> {
  key: string
  /** The representative row: it carries the branch, base and PR fields. */
  issue: I
  /** Every issue on this pull request, in caller order (length 1 = plain). */
  issues: I[]
}

export interface PrGraph<I> {
  /** The subject issue. */
  subject: I | null
  /** The pull request the subject belongs to; null when it has none. */
  entry: PrGraphEntry<I> | null
  /** The chain BOTTOM first, depth = distance from the bottom. Empty when the
   *  subject is in no stack. */
  stack: { entry: PrGraphEntry<I>; depth: number }[]
  /** The issues sharing the subject's pull request; null when it is not one. */
  batch: { issues: I[] } | null
  /** The issue face's "Blocked by" section: open blockers of the subject. */
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
    // An issue with no pull request cannot collide: keyed by its own id.
    const key =
      issue.pullRequestId ??
      (issue.prUrl && issue.prUrl.length > 0 ? issue.prUrl : issue.id)
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
 * Everything the badge and its overlay need for ONE subject issue. Pure:
 * every input is already synced (`issues` = `issuePrRows`).
 */
export function prGraph<I extends PrGraphIssue>(input: {
  issue?: I | null
  issues: readonly I[]
  relations?: readonly GraphRelation[]
}): PrGraph<I> {
  const { issues, relations = [] } = input
  const { entries, byIssueId } = groupEntries(issues)

  const subjectIssue = input.issue
    ? (issues.find((row) => row.id === input.issue!.id) ?? input.issue)
    : null

  const subjectEntry = subjectIssue
    ? (byIssueId.get(subjectIssue.id) ?? null)
    : null

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

  const blockedBy = subjectIssue
    ? openBlockers(subjectIssue.id, relations, issues)
    : []

  return {
    subject: subjectIssue,
    entry: subjectEntry,
    stack,
    batch,
    blockedBy,
    position,
    size,
  }
}

/** Which relation(s) the subject has: `null` = no chip at all. */
export function badgeKind<I>(graph: PrGraph<I>): BadgeKind {
  const stacked = graph.stack.length >= 2
  const batched = graph.batch !== null
  if (stacked && batched) return `stack+batch`
  if (stacked) return `stack`
  if (batched) return `batch`
  return null
}

/** EXP-1079/EXP-1097: what the header badge DRAWS, the SAME on every face -
 *  Issue, Run and Changes alike. First match wins:
 *
 *    1. a PR relation (`badgeKind`: stack, batch, stack+batch);
 *    2. `blocked`: the subject issue has OPEN blockers (`graph.blockedBy`);
 *    3. `null` = no badge. A run family alone earns none (SLOP-16 r5).
 *
 *  The same rule, byte for byte, in desktop `pr_graph.rs` (`badge_shape`),
 *  iOS `PrGraph.swift` and Android `PrGraph.kt` (`badgeShape`). */
export type BadgeShape = Exclude<BadgeKind, null> | `blocked` | null

export function badgeShape<I>(graph: PrGraph<I>): BadgeShape {
  const kind = badgeKind(graph)
  if (kind) return kind
  if (graph.blockedBy.length > 0) return `blocked`
  return null
}

/** EXP-1058: what the header badge counts: the front issue and how many ride
 *  behind it (`+N`).
 *
 *  · stack / batch: `issue` = the subject's pull request's representative
 *    row, `count` = every OTHER issue on the stack (all its entries' issues)
 *    or batch;
 *  · `blocked` (EXP-1097): `issue` = the FIRST open blocker in `blockedBy`
 *    order, `count` = the other open blockers.
 *
 *  `null` = no badge, exactly when `badgeShape` is null. Face-independent and
 *  byte-identical ×4 (`pr_graph::badge_chip`, `PrGraph.badgeChip` ×2). */
export interface BadgeChip<I> {
  issue: I | null
  count: number
}

export function badgeChip<I>(graph: PrGraph<I>): BadgeChip<I> | null {
  const shape = badgeShape(graph)
  if (!shape) return null
  if (shape === `blocked`) {
    return { issue: graph.blockedBy[0] ?? null, count: graph.blockedBy.length - 1 }
  }
  const issue = graph.entry?.issue ?? null
  if (graph.stack.length >= 2) {
    const total = graph.stack.reduce((sum, row) => sum + row.entry.issues.length, 0)
    return { issue, count: total - 1 }
  }
  return { issue, count: (graph.batch?.issues.length ?? 1) - 1 }
}

/** One band of the "Related work" dialog. */
export type OverlaySection = `blocked` | `batch` | `stack`

const OVERLAY_SECTION_ORDER: OverlaySection[] = [`blocked`, `batch`, `stack`]

/** SLOP-16 r5: the batch PARTNERS: every issue sharing the subject's pull
 *  request but the subject itself. */
export function batchPartners<I extends { id: string }>(graph: PrGraph<I>): I[] {
  return (graph.batch?.issues ?? []).filter((row) => row.id !== graph.subject?.id)
}

/** SLOP-16 r5: the OTHER pull requests of the subject's stack, BOTTOM-UP,
 *  the subject's own pull request left out. */
export function stackOthers<I>(graph: PrGraph<I>): PrGraphEntry<I>[] {
  if (graph.stack.length < 2) return []
  return graph.stack
    .map((row) => row.entry)
    .filter((entry) => entry.key !== graph.entry?.key)
}

/** SLOP-16 r5: the dialog's bands, the SAME on every face and in this fixed
 *  order: Blocked by · Same pull request · Pull request stack: each only
 *  when it has rows. */
export function overlaySections<I extends { id: string }>(
  graph: PrGraph<I>
): OverlaySection[] {
  const present: Record<OverlaySection, boolean> = {
    blocked: graph.blockedBy.length > 0,
    batch: batchPartners(graph).length > 0,
    stack: stackOthers(graph).length > 0,
  }
  return OVERLAY_SECTION_ORDER.filter((section) => present[section])
}

/** SLOP-16 r3: THE "Related work" view's title: the platform's standard
 *  modal (web `Dialog`, which drops to its sheet arm on phones). */
export const RELATED_WORK_TITLE = `Related work`

/** SLOP-16 r5: the "Related work" view's copy, byte-identical ×4 (desktop
 *  `pr_graph.rs`, iOS `PrGraphBadge.swift`, Android `PrGraphBadge.kt`). Each
 *  band = the relations card's foldable band over its rows. */
export const PR_GRAPH_OVERLAY_COPY = {
  title: RELATED_WORK_TITLE,
  blocked: RELATIONS_VIEW_COPY.blockedBy,
  batch: `Same pull request`,
  stack: `Pull request stack`,
  empty: `Nothing else is linked to this issue.`,
} as const
