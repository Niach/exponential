// The PR STACK edge, read-only: a pull request whose base is another issue's
// branch instead of the repository's default one. The edge is one synced
// column, `issues.pr_base_branch` (written by `pr_open{base}`): an upper PR is
// stacked on the lower one exactly when `upper.prBaseBranch === lower.branch`
// and that branch is non-empty. The "Related work" badge (`lib/pr-graph.ts`)
// draws it, and `stackMergeChoice` below asks before a member merges.
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

/** The stack dialog's primary button. ×4. */
export const MERGE_STACK_LABEL = `Merge stack`

// EXP-1145: a Merge control on a member of an open PR stack asks first
// (every one a person can press: the issue header, the Changes faces, the
// review page, the Reviews rows, the run view). Merging a member lands every
// open member BELOW it. The decision and its copy are ONE pure function ×4 (desktop
// `pr_stack::stack_merge_choice`, iOS `PrStack.stackMergeChoice`, Android
// `PrStack.stackMergeChoice`), locked by
// `@exp/domain-contract/fixtures/stack-merge-choice.json`.

export const STACK_MERGE_CHOICE_TITLE = `This pull request is part of a stack`
export const MERGE_THIS_PR_LABEL = `Merge this pull request`
export const STACK_MERGE_CANCEL_LABEL = `Cancel`

/** A stack member as the choice reads it: the stack edge plus its PR's
 *  state and url (a batch PR's issues share one url). */
export interface StackMergeNode extends PrStackNode {
  identifier: string
  prState: string | null
  prUrl?: string | null
}

export interface StackMergeChoice {
  /** The chain's OPEN members, bottom → top, one label per pull request
   *  (`EXP-874 +2` for a batch PR). */
  members: string[]
  /** 1-based, from the bottom: where the pull request being merged sits. */
  position: number
  /** The bottom member's issue id. */
  bottomIssueId: string
  /** The top member's issue id: what `mergePr({ mergeStack: true })` takes. */
  topIssueId: string
  /** `EXP-1105 → EXP-1144 (this one) → EXP-1150`. */
  listing: string
  /** What Merge stack does. */
  stackSentence: string
  /** What Merge this pull request does: the truth about a mid-stack merge. */
  thisSentence: string
  /** The dialog's body: the listing, a blank line, the two sentences. */
  body: string
}

const THIS_ONE = `this one`

function joinIdentifiers(labels: readonly string[]): string {
  return labels.join(`, `)
}

/**
 * Whether merging `issue`'s pull request from a plain Merge control needs
 * the stack dialog, and everything that dialog says.
 *
 * `null` = a plain merge: the issue has no open pull request, or its stack
 * has no OTHER open member (everything below already merged, nothing open
 * above). Only OPEN pull requests form the chain: a merged foundation or a
 * closed member is no longer part of what a merge lands: and the candidates
 * are read in identifier order so every client picks the same
 * representative for a fork or a batch.
 */
export function stackMergeChoice<T extends StackMergeNode>(
  issue: T,
  issues: readonly T[]
): StackMergeChoice | null {
  if (issue.prState !== `open`) return null
  const open = issues
    .filter((row) => row.prState === `open`)
    .sort((a, b) =>
      a.identifier < b.identifier ? -1 : a.identifier > b.identifier ? 1 : 0
    )
  const self = open.find((row) => row.id === issue.id) ? open : [issue, ...open]
  const chain = stackChain(issue, self)
  if (chain.length < 2) return null
  const index = chain.findIndex((member) => member.id === issue.id)
  if (index < 0) return null

  // A batch PR's siblings share its url: one label per pull request.
  const label = (member: T): string => {
    const url = member.prUrl ?? null
    const siblings = url
      ? self.filter((row) => row.id !== member.id && (row.prUrl ?? null) === url)
          .length
      : 0
    return siblings > 0 ? `${member.identifier} +${siblings}` : member.identifier
  }
  const members = chain.map(label)
  const own = members[index]!
  const below = members.slice(0, index)
  const above = members.slice(index + 1)

  const listing = members
    .map((name, at) => (at === index ? `${name} (${THIS_ONE})` : name))
    .join(` → `)
  const stackSentence = `${MERGE_STACK_LABEL} lands all ${members.length} pull requests, bottom-up.`
  const landsBelow =
    below.length === 0
      ? `${own} alone`
      : below.length === 1
        ? `${own} and the one below it (${joinIdentifiers(below)})`
        : `${own} and the ${below.length} below it (${joinIdentifiers(below)})`
  const leftOpen =
    above.length === 0
      ? `, the whole stack.`
      : above.length === 1
        ? `; ${joinIdentifiers(above)} is retargeted onto the base branch and stays open.`
        : `; ${joinIdentifiers(above)} are retargeted onto the base branch and stay open.`
  const thisSentence = `${MERGE_THIS_PR_LABEL} lands ${landsBelow}${leftOpen}`

  return {
    members,
    position: index + 1,
    bottomIssueId: chain[0]!.id,
    topIssueId: chain[chain.length - 1]!.id,
    listing,
    stackSentence,
    thisSentence,
    body: `${listing}\n\n${stackSentence}\n${thisSentence}`,
  }
}
