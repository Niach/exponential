// The PR STACK edge, read-only: a pull request whose base is another issue's
// branch instead of the repository's default one. The edge is one synced
// column, `issues.pr_base_branch` (written by `pr_open{base}`): an upper PR is
// stacked on the lower one exactly when `upper.prBaseBranch === lower.branch`
// and that branch is non-empty. The "Related work" badge (`lib/pr-graph.ts`)
// draws it, and `stackMergeConfirm` below asks before a stack merges.
//
// Test names: `numbers a member from the bottom of the chain`, `stops at a
// base nobody in the list owns`, `breaks a cycle where it first appears`.
//
// EXP-1248: `prGraphShape` tells a TREE (any fork) from a linear STACK;
// `stackView` = the stack rail (Guide card + Reviews), `stackMergeConfirm` =
// the ONE confirm of Merge stack / Merge through here, both ×4 off
// `fixtures/pr-stack-view.json` and `fixtures/stack-merge-choice.json`
// (`confirm`).
import { contract } from "@exp/domain-contract"

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

/** The ONE merge control's label on an open-stack member. ×4. */
export const MERGE_STACK_LABEL = contract.diffUi.mergeStack
/** The ghost action on a hovered stack-rail member (long-press on phones). */
export const MERGE_THROUGH_LABEL = contract.diffUi.mergeThrough

/** A stack member as the choice reads it: the stack edge plus its PR's
 *  state and url (a batch PR's issues share one url). */
export interface StackMergeNode extends PrStackNode {
  identifier: string
  prState: string | null
  prUrl?: string | null
}

// ── EXP-1248: tree vs stack, the stack rail, the one confirm ───────────────

/** A base-chained component's shape: `tree` = a fork anywhere (follow-up
 *  runs; nests with tree guides), `stack` = linear (GitHub stacks it; the
 *  stack rail), `single` = one pull request. */
export type PrGraphShape = `tree` | `stack` | `single`

function ownerMap<T extends PrStackNode>(nodes: readonly T[]): Map<string, T> {
  const byBranch = new Map<string, T>()
  for (const node of nodes) {
    const branch = owned(node.branch)
    if (branch && !byBranch.has(branch)) byBranch.set(branch, node)
  }
  return byBranch
}

/** Every node base-chained to `node` (either direction), in input order. */
export function prComponent<T extends PrStackNode>(
  node: T,
  nodes: readonly T[]
): T[] {
  const byBranch = ownerMap(nodes)
  const seen = new Set<string>([node.id])
  const queue: T[] = [node]
  while (queue.length > 0) {
    const current = queue.shift()!
    const base = owned(current.prBaseBranch)
    const lower = base ? byBranch.get(base) : undefined
    const linked = [
      ...(lower ? [lower] : []),
      ...nodes.filter(
        (candidate) =>
          owned(current.branch) !== null &&
          owned(candidate.prBaseBranch) === owned(current.branch)
      ),
    ]
    for (const next of linked) {
      if (seen.has(next.id)) continue
      seen.add(next.id)
      queue.push(next)
    }
  }
  return nodes.filter((candidate) => seen.has(candidate.id))
}

export function prGraphShape<T extends PrStackNode>(
  component: readonly T[]
): PrGraphShape {
  if (component.length < 2) return `single`
  const byBranch = ownerMap(component)
  const children = new Map<string, number>()
  for (const node of component) {
    const base = owned(node.prBaseBranch)
    const parent = base ? byBranch.get(base) : undefined
    if (!parent || parent.id === node.id) continue
    const count = (children.get(parent.id) ?? 0) + 1
    if (count > 1) return `tree`
    children.set(parent.id, count)
  }
  return `stack`
}

const byIdentifier = <T extends { identifier: string }>(a: T, b: T): number =>
  a.identifier < b.identifier ? -1 : a.identifier > b.identifier ? 1 : 0

/** The OPEN rows with ONE representative per pull request (identifier
 *  order, so a batch PR is its lowest identifier); null when `issue` has
 *  no open pull request. */
function openRepresentatives<T extends StackMergeNode>(
  issue: T,
  issues: readonly T[]
): { open: T[]; reps: T[]; subject: T } | null {
  if (issue.prState !== `open`) return null
  const open = issues.filter((row) => row.prState === `open`)
  if (!open.some((row) => row.id === issue.id)) open.push(issue)
  open.sort(byIdentifier)
  const repOf = new Map<string, T>()
  const reps: T[] = []
  for (const row of open) {
    const url = row.prUrl ?? null
    const rep = url ? reps.find((other) => (other.prUrl ?? null) === url) : undefined
    if (rep) {
      repOf.set(row.id, rep)
    } else {
      reps.push(row)
      repOf.set(row.id, row)
    }
  }
  return { open, reps, subject: repOf.get(issue.id)! }
}

/** The shape of the open component `issue`'s pull request sits in
 *  (`single` without an open pull request). */
export function openPrShape<T extends StackMergeNode>(
  issue: T,
  issues: readonly T[]
): PrGraphShape {
  const picked = openRepresentatives(issue, issues)
  if (!picked) return `single`
  return prGraphShape(prComponent(picked.subject, picked.reps))
}

/** The linear open stack `issue` sits in: the chain bottom → top, the
 *  subject's representative and a batch PR's partner count. */
function openStack<T extends StackMergeNode>(
  issue: T,
  issues: readonly T[]
): { chain: T[]; subject: T; siblings: (row: T) => number } | null {
  const picked = openRepresentatives(issue, issues)
  if (!picked) return null
  const { open, reps, subject } = picked
  if (prGraphShape(prComponent(subject, reps)) !== `stack`) return null
  const chain = stackChain(subject, reps)
  const siblings = (row: T): number =>
    row.prUrl
      ? open.filter((other) => other.id !== row.id && other.prUrl === row.prUrl).length
      : 0
  return { chain, subject, siblings }
}

/** What a stack-rail row needs beyond the edge. */
export interface StackViewNode extends StackMergeNode {
  title: string
  prNumber: number | null
}

export interface StackViewRow {
  issueId: string
  identifier: string
  title: string
  prNumber: number | null
  /** The subject's pull request: the row wears the active wash. */
  isCurrent: boolean
}

export interface StackView {
  /** TOP first, one row per open pull request. */
  rows: StackViewRow[]
  /** The trailing muted row: the bottom member's base; null = unknown. */
  baseBranch: string | null
}

/**
 * The stack rail of `issue`'s pull request (the Guide's Stack card, a Reviews
 * stack group): null unless it sits in a linear open stack of 2+ pull
 * requests. Only OPEN pull requests count; a fork anywhere = a tree = null.
 */
export function stackView<T extends StackViewNode>(
  issue: T,
  issues: readonly T[]
): StackView | null {
  const stack = openStack(issue, issues)
  if (!stack || stack.chain.length < 2) return null
  return {
    rows: [...stack.chain].reverse().map((row) => ({
      issueId: row.id,
      identifier: row.identifier,
      title: row.title,
      prNumber: row.prNumber,
      isCurrent: row.id === stack.subject.id,
    })),
    baseBranch: owned(stack.chain[0]!.prBaseBranch),
  }
}

export const STACK_CONFIRM_CANCEL_LABEL = `Cancel`

/** `stack` = the merge control (the whole open chain, through its top);
 *  `through` = Merge through here on this member. */
export type StackConfirmMode = `stack` | `through`

export interface StackMergeConfirm {
  /** The title AND the primary button. */
  title: string
  /** What lands, bottom first; a batch PR = `EXP-874 +2`. */
  landing: string[]
  /** What stays open above it (GitHub retargets it). */
  staysOpen: string[]
  body: string
  /** The `issues.mergePr` input the primary button sends. */
  input: { issueId: string; mergeStack: true }
}

/**
 * EXP-1248: the ONE confirm a stack merge asks (it replaces the 3-way
 * dialog). Null = not in a linear open stack: the plain merge confirm.
 */
export function stackMergeConfirm<T extends StackMergeNode>(
  issue: T,
  issues: readonly T[],
  mode: StackConfirmMode
): StackMergeConfirm | null {
  const stack = openStack(issue, issues)
  if (!stack || stack.chain.length < 2) return null
  const label = (row: T): string => {
    const extra = stack.siblings(row)
    return extra > 0 ? `${row.identifier} +${extra}` : row.identifier
  }
  const index = stack.chain.findIndex((row) => row.id === stack.subject.id)
  const through = mode === `stack` ? stack.chain.length - 1 : index
  const landing = stack.chain.slice(0, through + 1).map(label)
  const staysOpen = stack.chain.slice(through + 1).map(label)
  const lands =
    landing.length === 1
      ? `Lands 1 pull request: ${landing[0]}.`
      : `Lands ${landing.length} pull requests, bottom-up: ${landing.join(`, `)}.`
  const open =
    staysOpen.length === 0
      ? ``
      : ` ${staysOpen.join(`, `)} ${staysOpen.length === 1 ? `stays` : `stay`} open.`
  return {
    title: mode === `stack` ? MERGE_STACK_LABEL : MERGE_THROUGH_LABEL,
    landing,
    staysOpen,
    body: `${lands}${open}`,
    input: {
      issueId: mode === `stack` ? stack.chain[stack.chain.length - 1]!.id : issue.id,
      mergeStack: true,
    },
  }
}
