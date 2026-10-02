// EXP-1145 (kept by SLOP-3): a PR whose recorded base (`issues.pr_base_branch`)
// is the head branch of ANOTHER open PR in the same repository is a follow-up
// sitting on its parent. A plain squash merge would land it INTO the parent's
// branch: the diff never reaches the default branch while the issue flips to
// Done. Every merge path (tRPC `issues.mergePr`, `codingSessions.mergePr`,
// `repositories.mergePull`, so MCP `pr_merge` and the yolo tree merge too)
// refuses it BEFORE any claim or GitHub call. Evaluated per PR at merge time.
// Once the parent MERGED the same paths await the EXP-324 heal and refuse
// while GitHub still reports the merged branch as the base (`basedOnMergedPr`
// + `awaitRebaseOffMergedBranch`): the heal alone is fire-and-forget, and a
// child merged right behind its root would land in the root's kept branch.
import { TRPCError } from "@trpc/server"
import { and, eq, isNotNull, like, ne } from "drizzle-orm"
import { boards, codingSessions, issues, repositories } from "@/db/schema"
import type { Context } from "@/lib/trpc"
import { escapeLikePattern } from "@/lib/like-pattern"
import { getPullRequest } from "@/lib/integrations/github-pr"

/** Byte-locked: the merge dialog toasts it and agents read it out of
 *  `exponential_pr_merge`. */
export function stackedOnMessage(parent: string): string {
  return `This pull request is stacked on ${parent}; merge ${parent} first`
}

/** The squash commit's title for an issue's PR, on every merge path. */
export function squashCommitTitle(
  identifier: string,
  title: string,
  prNumber: number
): string {
  return `${identifier}: ${title} (#${prNumber})`
}

function repoPrUrlPattern(repoFullName: string): string {
  return `https://github.com/${escapeLikePattern(repoFullName)}/pull/%`
}

/** The branches a team DEVELOPS a repository on: GitHub's default as stored
 *  on the team's repo row, the team's pin (EXP-462) and every board pin on
 *  the repo (EXP-712). */
export interface RepoDefaultBranches {
  defaultBranch: string
  defaultBranchOverride: string | null
  boardDefaultBranches: string[]
}

/**
 * The team's repo row by full name plus its boards' pins, null when the team
 * has no row for the repo. Archived rows count: a pin on one still beats
 * treating its branch as somebody's PR head (EXP-466). Shared by the guards
 * below and the EXP-324 heal (`retargetChildrenOfMergedPr`).
 */
export async function loadRepoDefaultBranches(
  db: Pick<Context[`db`], `select`>,
  opts: { teamId: string; repoFullName: string }
): Promise<RepoDefaultBranches | null> {
  const [repo] = await db
    .select({
      id: repositories.id,
      defaultBranch: repositories.defaultBranch,
      defaultBranchOverride: repositories.defaultBranchOverride,
    })
    .from(repositories)
    .where(
      and(
        eq(repositories.teamId, opts.teamId),
        eq(repositories.fullName, opts.repoFullName)
      )
    )
    .limit(1)
  if (!repo) return null
  const pins = await db
    .select({ defaultBranch: boards.defaultBranch })
    .from(boards)
    .where(
      and(eq(boards.repositoryId, repo.id), isNotNull(boards.defaultBranch))
    )
  return {
    defaultBranch: repo.defaultBranch,
    defaultBranchOverride: repo.defaultBranchOverride,
    boardDefaultBranches: pins.flatMap((pin) =>
      pin.defaultBranch ? [pin.defaultBranch] : []
    ),
  }
}

/** Is `branch` one the repo is developed on (never a follow-up's parent)? */
export function isRepoDefaultBranch(
  branches: RepoDefaultBranches | null,
  branch: string
): boolean {
  if (!branches) return false
  return (
    branch === branches.defaultBranch ||
    branch === branches.defaultBranchOverride ||
    branches.boardDefaultBranches.includes(branch)
  )
}

/**
 * What this PR is stacked on: the identifier of the team issue whose OPEN PR
 * (same repository) has `prBaseBranch` as its head, else `#<number>` of an
 * issue-less run PR on that branch, else null (the default branch, a
 * merged/closed parent, a branch nobody's PR uses). A base the repo is
 * developed on is never a parent, even while a PR FROM it is open (a
 * `develop → main` release PR): every plain PR records that base.
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
  if (parent) {
    return (await isDefaultBased(db, opts)) ? null : parent.identifier
  }
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
  if (run?.prNumber == null) return null
  return (await isDefaultBased(db, opts)) ? null : `#${run.prNumber}`
}

/** Asked only once a PR was found on the base: plain merges pay nothing. */
async function isDefaultBased(
  db: Pick<Context[`db`], `select`>,
  opts: { teamId: string; repoFullName: string; prBaseBranch: string | null }
): Promise<boolean> {
  if (!opts.prBaseBranch) return false
  return isRepoDefaultBranch(
    await loadRepoDefaultBranches(db, opts),
    opts.prBaseBranch
  )
}

/**
 * The mirror of `stackedOnOpenPr` for a parent that already MERGED: the url
 * of the merged PR (an issue's, else an issue-less run's) in the same
 * repository whose head is `prBaseBranch`, else null. A hit means this PR may
 * still sit on the merged PR's kept branch, so the caller awaits the EXP-324
 * heal and `awaitRebaseOffMergedBranch` before it merges. Same default-branch
 * exit as above.
 */
export async function basedOnMergedPr(
  db: Pick<Context[`db`], `select`>,
  opts: {
    teamId: string
    repoFullName: string
    prBaseBranch: string | null
  }
): Promise<{ prUrl: string } | null> {
  if (!opts.prBaseBranch) return null
  const pattern = repoPrUrlPattern(opts.repoFullName)
  const [parent] = await db
    .select({ prUrl: issues.prUrl })
    .from(issues)
    .where(
      and(
        eq(issues.teamId, opts.teamId),
        eq(issues.branch, opts.prBaseBranch),
        eq(issues.prState, `merged`),
        like(issues.prUrl, pattern)
      )
    )
    .limit(1)
  let prUrl = parent?.prUrl ?? null
  if (!prUrl) {
    const [run] = await db
      .select({ prUrl: codingSessions.prUrl })
      .from(codingSessions)
      .where(
        and(
          eq(codingSessions.teamId, opts.teamId),
          eq(codingSessions.branch, opts.prBaseBranch),
          eq(codingSessions.prState, `merged`),
          like(codingSessions.prUrl, pattern)
        )
      )
      .limit(1)
    prUrl = run?.prUrl ?? null
  }
  if (!prUrl) return null
  return (await isDefaultBased(db, opts)) ? null : { prUrl }
}

/** GitHub computes mergeability lazily after a base move: `mergeable` reads
 *  null for a moment (and the base ref may lag one read). ≈10 s in all. */
const REBASE_SETTLE_MS = [1_000, 2_000, 3_000, 4_000]
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/**
 * After the PR below merged and the EXP-324 heal ran: wait until GitHub
 * reports this PR off the merged branch with its mergeability computed, and
 * sync the recorded base when GitHub moved it (the merge guard reads it).
 * Never squash INTO the merged branch: a base that stayed put refuses, and so
 * does a PR GitHub would not show us.
 */
export async function awaitRebaseOffMergedBranch(
  db: Pick<Context[`db`], `update`>,
  opts: {
    repoFullName: string
    prNumber: number
    prUrl: string
    prBaseBranch: string | null
    mergedBranch: string
    token: string
  }
): Promise<void> {
  let pull: Awaited<ReturnType<typeof getPullRequest>> | null = null
  for (const delay of [0, ...REBASE_SETTLE_MS]) {
    if (delay) await sleep(delay)
    try {
      pull = await getPullRequest(opts.repoFullName, opts.prNumber, opts.token)
    } catch {
      throw new TRPCError({
        code: `PRECONDITION_FAILED`,
        message: `GitHub did not confirm it left ${opts.mergedBranch}, whose pull request already merged; merge again`,
      })
    }
    if (pull.baseRef !== opts.mergedBranch && pull.mergeable !== null) break
  }
  if (!pull) return
  if (pull.baseRef === opts.mergedBranch) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `It is still based on ${opts.mergedBranch}; retarget it onto the default branch (exponential_pr_retarget) and merge again`,
    })
  }
  if (pull.baseRef && pull.baseRef !== opts.prBaseBranch) {
    await db
      .update(issues)
      .set({ prBaseBranch: pull.baseRef })
      .where(eq(issues.prUrl, opts.prUrl))
  }
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
 * open issue PR or a branch the repo is developed on. Empty when the issue
 * itself has no open PR (the single merge then names why). Members above
 * `issueId` are never included.
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
  // Loaded once, and only when a PR was found on a base.
  let defaults: RepoDefaultBranches | null | undefined
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
    // A base the repo is developed on ends the stack, even while a PR FROM
    // it is open (a `develop → main` release PR must never ride along).
    if (defaults === undefined) {
      defaults = await loadRepoDefaultBranches(db, {
        teamId: opts.teamId,
        repoFullName,
      })
    }
    if (isRepoDefaultBranch(defaults, base)) break
    seenPrUrls.add(lower.prUrl)
    chain.unshift(toMember(lower))
    base = lower.prBaseBranch
  }
  return chain
}
