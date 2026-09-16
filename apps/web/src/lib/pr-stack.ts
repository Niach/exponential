// EXP-897: the PR STACK — a pull request whose base is another issue's branch
// instead of the repository's default one. The edge is one synced column,
// `issues.pr_base_branch`: an upper PR is stacked on the lower one exactly
// when `upper.prBaseBranch === lower.branch` and that branch is non-empty.
//
// The rules are pure and mirrored ×4 (iOS `ExpCore/Sources/Domain/PrStack.swift`,
// Android `domain/PrStack.kt`, desktop `queries::nest_review_entries`) with
// the same five test names:
//
// 1. `numbers a member from the bottom of the chain`
// 2. `stops at a base nobody in the list owns`
// 3. `breaks a cycle where it first appears`
// 4. `nests an upper entry under the one it is stacked on`
// 5. `keeps the caller's root order`

/** What a stack member must carry: its own branch and the branch its pull
 *  request is BASED on. Both are nullable — an issue with no run has neither. */
export interface PrStackNode {
  id: string
  /** Ordering key for a fork (two PRs on the same base); falls back to `id`. */
  identifier?: string
  branch: string | null
  prBaseBranch: string | null
}

function key(node: PrStackNode): string {
  return node.identifier ?? node.id
}

function owned(branch: string | null | undefined): string | null {
  return branch && branch.length > 0 ? branch : null
}

/**
 * The whole chain this issue belongs to, BOTTOM first and including itself.
 *
 * Down: follow `prBaseBranch` to the member that owns that branch, stopping at
 * a base nobody in the list owns (the repository's default branch, an
 * unsynced issue, another team). Up: follow the member whose `prBaseBranch` is
 * this one's branch; a FORK (two PRs on the same base) takes the first by
 * identifier, so the chain stays linear and the numbering stays stable. A
 * cycle breaks where it first repeats.
 *
 * A lone pull request returns a chain of one.
 */
export function stackChain<T extends PrStackNode>(
  issue: T,
  issues: readonly T[]
): T[] {
  const byBranch = new Map<string, T>()
  for (const candidate of issues) {
    const branch = owned(candidate.branch)
    if (branch && !byBranch.has(branch)) byBranch.set(branch, candidate)
  }
  const seen = new Set<string>([issue.id])

  const below: T[] = []
  let current: T = issue
  for (;;) {
    const base = owned(current.prBaseBranch)
    if (!base) break
    const lower = byBranch.get(base)
    if (!lower || lower.id === current.id || seen.has(lower.id)) break
    seen.add(lower.id)
    below.unshift(lower)
    current = lower
  }

  const above: T[] = []
  current = issue
  for (;;) {
    const branch = owned(current.branch)
    if (!branch) break
    const children = issues
      .filter(
        (candidate) =>
          candidate.id !== current.id &&
          owned(candidate.prBaseBranch) === branch &&
          !seen.has(candidate.id)
      )
      .sort((a, b) => (key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0))
    const upper = children[0]
    if (!upper) break
    seen.add(upper.id)
    above.push(upper)
    current = upper
  }

  return [...below, issue, ...above]
}

export interface StackPosition<T> {
  /** 1-based, counted from the BOTTOM of the chain. */
  position: number
  size: number
  /** The member directly below (the foundation), null at the bottom. */
  below: T | null
  /** The member directly above, null at the top. */
  above: T | null
}

/** Where this issue sits in its stack — `null` when it is in none (a lone
 *  pull request, or no pull request at all). */
export function stackPosition<T extends PrStackNode>(
  issue: T,
  issues: readonly T[]
): StackPosition<T> | null {
  const chain = stackChain(issue, issues)
  if (chain.length < 2) return null
  const index = chain.findIndex((member) => member.id === issue.id)
  if (index < 0) return null
  return {
    position: index + 1,
    size: chain.length,
    below: chain[index - 1] ?? null,
    above: chain[index + 1] ?? null,
  }
}

export interface PrStackRow<T> {
  entry: T
  /** 0 for a root, +1 per stacked level. */
  depth: number
  /** Whether an upper entry is nested right below this one. */
  hasChildren: boolean
}

/**
 * Flatten pull-request ENTRIES into nested list order — the Reviews queue's
 * shape. An entry is one pull request (a single issue, or a batch of issues
 * sharing one `prUrl`), represented by `entry.issue`.
 *
 * The caller's ROOT order is kept; an upper entry follows the entry it is
 * stacked on, children in caller order, recursively. A base nobody in the list
 * owns leaves the entry a root, and a cycle breaks where it first appears.
 */
export function nestPrStacks<T extends { issue: PrStackNode }>(
  entries: readonly T[]
): PrStackRow<T>[] {
  const byBranch = new Map<string, T>()
  for (const entry of entries) {
    const branch = owned(entry.issue.branch)
    if (branch && !byBranch.has(branch)) byBranch.set(branch, entry)
  }
  const parentOf = new Map<string, T>()
  const childrenOf = new Map<string, T[]>()
  for (const entry of entries) {
    const base = owned(entry.issue.prBaseBranch)
    if (!base) continue
    const parent = byBranch.get(base)
    if (!parent || parent.issue.id === entry.issue.id) continue
    parentOf.set(entry.issue.id, parent)
    const list = childrenOf.get(parent.issue.id) ?? []
    list.push(entry)
    childrenOf.set(parent.issue.id, list)
  }

  const out: PrStackRow<T>[] = []
  const placed = new Set<string>()
  const visit = (entry: T, depth: number) => {
    if (placed.has(entry.issue.id)) return
    placed.add(entry.issue.id)
    const children = (childrenOf.get(entry.issue.id) ?? []).filter(
      (child) => !placed.has(child.issue.id)
    )
    out.push({ entry, depth, hasChildren: children.length > 0 })
    for (const child of children) visit(child, depth + 1)
  }
  for (const entry of entries) {
    if (!parentOf.has(entry.issue.id)) visit(entry, 0)
  }
  // Anything left is an entry whose ancestry cycled without a root — keep it,
  // at depth 0, in caller order.
  for (const entry of entries) visit(entry, 0)
  return out
}

/** The caption an upper stack member wears: `on top of #ABC-12`. ×4. */
export function stackedOnCaption(identifier: string): string {
  return `on top of #${identifier}`
}

/** The run page's position line: `2 of 3 · on top of #ABC-12`. ×4. */
export function stackPositionLine(
  position: number,
  size: number,
  belowIdentifier: string | null
): string {
  const head = `${position} of ${size}`
  return belowIdentifier ? `${head} · ${stackedOnCaption(belowIdentifier)}` : head
}

/** The Reviews queue's stack merge control + its confirmation. ×4. */
export const MERGE_STACK_LABEL = `Merge stack`
export const MERGE_STACK_TITLE = `Merge the whole stack?`
export function mergeStackBody(count: number): string {
  return `${count} pull requests, bottom-up.`
}
