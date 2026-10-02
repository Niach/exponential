// The PR STACK edge, read-only: a pull request whose base is another issue's
// branch instead of the repository's default one. The edge is one synced
// column, `issues.pr_base_branch` (written by `pr_open{base}`): an upper PR is
// stacked on the lower one exactly when `upper.prBaseBranch === lower.branch`
// and that branch is non-empty. The "Related work" badge (`lib/pr-graph.ts`)
// draws it; nothing here plans, starts or merges a stack.
//
// Test names: `numbers a member from the bottom of the chain`, `stops at a
// base nobody in the list owns`, `breaks a cycle where it first appears`.

/** What a stack member must carry: its own branch and the branch its pull
 *  request is BASED on. Both are nullable: an issue with no run has neither. */
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
