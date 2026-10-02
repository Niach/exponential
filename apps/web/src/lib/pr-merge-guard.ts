// EXP-1145 (kept by SLOP-3): a PR whose recorded base (`issues.pr_base_branch`)
// is the head branch of ANOTHER open PR in the same repository is a follow-up
// sitting on its parent. A plain squash merge would land it INTO the parent's
// branch: the diff never reaches the default branch while the issue flips to
// Done. Every merge path (tRPC `issues.mergePr`, `codingSessions.mergePr`,
// `repositories.mergePull`, so MCP `pr_merge` and the yolo tree merge too)
// refuses it BEFORE any claim or GitHub call. Evaluated per PR at merge time:
// once the parent merged it is no longer open and the child passes (the
// EXP-324 heal retargets it onto the default branch).
import { and, eq, isNotNull, like, ne } from "drizzle-orm"
import { codingSessions, issues } from "@/db/schema"
import type { Context } from "@/lib/trpc"
import { escapeLikePattern } from "@/lib/like-pattern"

/** Byte-locked: the merge dialog toasts it and agents read it out of
 *  `exponential_pr_merge`. */
export function stackedOnMessage(parent: string): string {
  return `This pull request is stacked on ${parent}; merge ${parent} first`
}

function repoPrUrlPattern(repoFullName: string): string {
  return `https://github.com/${escapeLikePattern(repoFullName)}/pull/%`
}

/**
 * What this PR is stacked on: the identifier of the team issue whose OPEN PR
 * (same repository) has `prBaseBranch` as its head, else `#<number>` of an
 * issue-less run PR on that branch, else null (the default branch, a
 * merged/closed parent, a branch nobody's PR uses).
 */
export async function stackedOnOpenPr(
  db: Pick<Context[`db`], `select`>,
  opts: {
    issueId: string | null
    teamId: string
    repoFullName: string
    prBaseBranch: string | null
  }
): Promise<string | null> {
  if (!opts.prBaseBranch) return null
  const pattern = repoPrUrlPattern(opts.repoFullName)
  const [parent] = await db
    .select({ identifier: issues.identifier })
    .from(issues)
    .where(
      and(
        eq(issues.teamId, opts.teamId),
        ...(opts.issueId ? [ne(issues.id, opts.issueId)] : []),
        eq(issues.branch, opts.prBaseBranch),
        eq(issues.prState, `open`),
        like(issues.prUrl, pattern)
      )
    )
    .limit(1)
  if (parent) return parent.identifier
  // An issue-less parent (a chat/action run's PR) lives on its run row only.
  const [run] = await db
    .select({ prNumber: codingSessions.prNumber })
    .from(codingSessions)
    .where(
      and(
        eq(codingSessions.teamId, opts.teamId),
        eq(codingSessions.branch, opts.prBaseBranch),
        eq(codingSessions.prState, `open`),
        isNotNull(codingSessions.prNumber),
        like(codingSessions.prUrl, pattern)
      )
    )
    .limit(1)
  return run?.prNumber != null ? `#${run.prNumber}` : null
}

/** One open PR of a stack: the issue that represents it (a batch PR = ONE
 *  member, whichever of its issues the walk met). */
export interface StackMember {
  issueId: string
  identifier: string
  boardId: string | null
  prNumber: number
  prUrl: string
  branch: string | null
  prBaseBranch: string | null
}

/** Cycle-safe bound on the walk below a PR. */
export const MAX_STACK_DEPTH = 10

/**
 * SLOP-3 `mergePr({mergeStack})`: the open chain BELOW AND INCLUDING
 * `issueId`, bottom first. Each step follows `pr_base_branch` to the team
 * issue whose OPEN PR (same repo) has that head, until the base is nobody's
 * open issue PR. Empty when the issue itself has no open PR (the single
 * merge then names why). Members above `issueId` are never included.
 */
export async function openStackThrough(
  db: Pick<Context[`db`], `select`>,
  opts: { issueId: string; teamId: string }
): Promise<StackMember[]> {
  const columns = {
    issueId: issues.id,
    identifier: issues.identifier,
    boardId: issues.boardId,
    prNumber: issues.prNumber,
    prUrl: issues.prUrl,
    prState: issues.prState,
    branch: issues.branch,
    prBaseBranch: issues.prBaseBranch,
  }
  const [start] = await db
    .select(columns)
    .from(issues)
    .where(eq(issues.id, opts.issueId))
    .limit(1)
  if (!start || start.prState !== `open` || !start.prUrl || !start.prNumber) {
    return []
  }
  // Same parse as pr-sync's repoFromPrUrl (not imported: this module stays
  // a leaf the routers can load cheaply).
  const repoFullName = start.prUrl.match(
    /github\.com\/([^/]+\/[^/]+)\/pull\/\d+/
  )?.[1]
  if (!repoFullName) return []
  const pattern = repoPrUrlPattern(repoFullName)
  const toMember = (row: typeof start): StackMember => ({
    issueId: row.issueId,
    identifier: row.identifier,
    boardId: row.boardId,
    prNumber: row.prNumber!,
    prUrl: row.prUrl!,
    branch: row.branch,
    prBaseBranch: row.prBaseBranch,
  })
  const chain = [toMember(start)]
  const seenPrUrls = new Set([start.prUrl])
  let base = start.prBaseBranch
  while (base && chain.length < MAX_STACK_DEPTH) {
    const [lower] = await db
      .select(columns)
      .from(issues)
      .where(
        and(
          eq(issues.teamId, opts.teamId),
          eq(issues.branch, base),
          eq(issues.prState, `open`),
          like(issues.prUrl, pattern)
        )
      )
      .limit(1)
    if (!lower?.prUrl || !lower.prNumber || seenPrUrls.has(lower.prUrl)) break
    seenPrUrls.add(lower.prUrl)
    chain.unshift(toMember(lower))
    base = lower.prBaseBranch
  }
  return chain
}
