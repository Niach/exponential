// EXP-1248: a PR in an open STACK (a linear base chain, GitHub stacks it
// natively) with open PRs BENEATH it never merges plainly: `openStackMember`
// refuses it, naming what a merge through it would land; `mergeStack` lands
// it through ONE merge-async (lib/pr-stacks.ts). The bottom lands alone, so
// it merges plainly. A FORK anywhere in the component is a tree: its
// children wait for the root (the EXP-1145 rule below).
//
// EXP-1145 (kept by SLOP-3): a PR whose recorded base (`issues.pr_base_branch`,
// or for an issue-less run PR `coding_sessions.pr_base_branch`, EXP-1165)
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

/** A landing PR as the refusal names it; an issue-less run PR has no
 *  identifier. */
export interface LandingPr {
  identifier: string | null
  prNumber: number
}

function stackPrName(member: LandingPr): string {
  return member.identifier
    ? `${member.identifier} (#${member.prNumber})`
    : `#${member.prNumber}`
}

/** Byte-locked like `stackedOnMessage`: a plain merge on an open-stack
 *  member. Shown VERBATIM to people (the merge toast, old natives' Changes
 *  face and session rows), so it names the product control, never the API
 *  flag; the members below it (bottom first) say what Merge stack lands
 *  with it. `landing` = the whole landing, this PR last. */
export function openStackMessage(landing: ReadonlyArray<LandingPr>): string {
  const below = landing.length > 1 ? landing.slice(0, -1) : landing
  return `This pull request is part of an open stack. Use Merge stack to land it with the pull requests below it: ${below.map(stackPrName).join(`, `)}.`
}

/** The squash commit's title for an issue's PR, on every merge path. */
export function squashCommitTitle(
  identifier: string,
  title: string,
  prNumber: number
): string {
  return `${identifier}: ${title} (#${prNumber})`
}

/** The LIKE pattern matching every PR url of a repository (its canonical
 *  `https://github.com/<owner>/<repo>/pull/<n>` form). */
export function repoPrUrlPattern(repoFullName: string): string {
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

/**
 * The guards' view of the branches a repo is developed on: the team's repo
 * row when it has one, else (EXP-1165) GitHub's default branch, so a team
 * with no `repositories` row still gets the default-branch exit. Null when
 * neither answers.
 */
export async function loadGuardDefaultBranches(
  db: Pick<Context[`db`], `select`>,
  opts: { teamId: string; repoFullName: string }
): Promise<RepoDefaultBranches | null> {
  const row = await loadRepoDefaultBranches(db, opts)
  if (row) return row
  try {
    // Lazy: keeps this module a leaf the routers load cheaply.
    const { resolveRepoDefaultBranchCached } = await import(
      `@/lib/integrations/github-app`
    )
    const defaultBranch = await resolveRepoDefaultBranchCached(
      opts.repoFullName
    )
    return defaultBranch
      ? { defaultBranch, defaultBranchOverride: null, boardDefaultBranches: [] }
      : null
  } catch {
    return null
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
    await loadGuardDefaultBranches(db, opts),
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
    await db
      .update(codingSessions)
      .set({ prBaseBranch: pull.baseRef })
      .where(eq(codingSessions.prUrl, opts.prUrl))
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
      defaults = await loadGuardDefaultBranches(db, {
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

/** What a merge of this issue's PR is, stack-wise (EXP-1248). */
export type StackMembership =
  /** A linear stack member with open PRs beneath it: refused plainly,
   *  `mergeStack` lands `landing` (bottom first, this PR last, always 2+)
   *  in one merge-async. */
  | { kind: `stack`; landing: StackMember[] }
  /** A fork somewhere in its line: a tree. `parent` (the PR below) merges
   *  first; null = the root, which merges plainly. */
  | { kind: `tree`; parent: string | null; landing: StackMember[] }

/** The OPEN PRs (issue or run rows, same team + repo) based on `branch`,
 *  keyed by PR url, each with its head branch. */
export async function openChildPrs(
  db: Pick<Context[`db`], `select`>,
  opts: { teamId: string; pattern: string; branch: string }
): Promise<Map<string, string | null>> {
  const issueRows = await db
    .select({ prUrl: issues.prUrl, branch: issues.branch })
    .from(issues)
    .where(
      and(
        eq(issues.teamId, opts.teamId),
        eq(issues.prBaseBranch, opts.branch),
        eq(issues.prState, `open`),
        like(issues.prUrl, opts.pattern)
      )
    )
  const runRows = await db
    .select({ prUrl: codingSessions.prUrl, branch: codingSessions.branch })
    .from(codingSessions)
    .where(
      and(
        eq(codingSessions.teamId, opts.teamId),
        eq(codingSessions.prBaseBranch, opts.branch),
        eq(codingSessions.prState, `open`),
        like(codingSessions.prUrl, opts.pattern)
      )
    )
  const children = new Map<string, string | null>()
  for (const row of [...issueRows, ...runRows]) {
    if (row.prUrl && !children.get(row.prUrl)) {
      children.set(row.prUrl, row.branch ?? null)
    }
  }
  return children
}

/** Cycle-safe bound on the open PRs walked above a PR. */
const MAX_COMPONENT_WALK = 30

/**
 * EXP-1248: is this issue's open PR a member of an open stack? Classified
 * over the WHOLE open component (the line below from `openStackThrough` plus
 * every open PR above it), like the clients' `prGraphShape`: a fork anywhere
 * = a `tree` (the root merges plainly, every other member waits for its
 * parent). A linear line returns `stack` only when PRs are open BENEATH this
 * one: a bottom's landing is itself alone, so it merges plainly (`null`, as
 * for a lone PR or no open PR).
 */
export async function openStackMember(
  db: Pick<Context[`db`], `select`>,
  opts: { issueId: string; teamId: string }
): Promise<StackMembership | null> {
  const chain = await openStackThrough(db, opts)
  const self = chain[chain.length - 1]
  if (!self) return null
  const repoFullName = self.prUrl.match(
    /github\.com\/([^/]+\/[^/]+)\/pull\/\d+/
  )?.[1]
  if (!repoFullName) return null
  const pattern = repoPrUrlPattern(repoFullName)
  let defaults: RepoDefaultBranches | null | undefined
  const seen = new Set(chain.map((member) => member.prUrl))
  const childrenOf = async (
    branch: string | null
  ): Promise<Map<string, string | null>> => {
    if (!branch) return new Map()
    if (defaults === undefined) {
      defaults = await loadGuardDefaultBranches(db, {
        teamId: opts.teamId,
        repoFullName,
      })
    }
    if (isRepoDefaultBranch(defaults, branch)) return new Map()
    const children = await openChildPrs(db, {
      teamId: opts.teamId,
      pattern,
      branch,
    })
    return children
  }
  let fork = false
  // Below: each member's ONLY open child is the next one up the chain.
  for (const [index, member] of chain.slice(0, -1).entries()) {
    const children = await childrenOf(member.branch)
    children.delete(member.prUrl)
    children.delete(chain[index + 1]!.prUrl)
    if (children.size > 0) {
      fork = true
      break
    }
  }
  // Above: walk every open descendant; any PR with two open children forks.
  const queue: Array<string | null> = [self.branch]
  let walked = 0
  while (!fork && queue.length > 0 && walked < MAX_COMPONENT_WALK) {
    walked += 1
    const children = await childrenOf(queue.shift() ?? null)
    for (const url of [...children.keys()]) {
      if (seen.has(url)) children.delete(url)
    }
    if (children.size > 1) fork = true
    for (const [url, branch] of children) {
      seen.add(url)
      queue.push(branch)
    }
  }
  if (fork) {
    return {
      kind: `tree`,
      parent: chain.length > 1 ? chain[chain.length - 2]!.identifier : null,
      landing: chain,
    }
  }
  if (chain.length > 1) return { kind: `stack`, landing: chain }
  return null
}

/**
 * The merge rule every path applies to `openStackMember`'s answer, before
 * any claim or GitHub call: a tree child waits for its parent; a member with
 * PRs open beneath it needs `mergeStack` and lands `landing` (returned); a
 * bottom, a tree root or a lone PR merges plainly (null).
 */
export function stackLanding(
  membership: StackMembership | null,
  mergeStack: boolean | undefined
): StackMember[] | null {
  if (membership?.kind === `tree` && membership.parent) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: stackedOnMessage(membership.parent),
    })
  }
  if (membership?.kind !== `stack` || membership.landing.length < 2) return null
  if (!mergeStack) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: openStackMessage(membership.landing),
    })
  }
  return membership.landing
}

/** The branches `ensureGithubStack` must never walk below. */
export function stackStopBranches(branches: RepoDefaultBranches | null): string[] {
  if (!branches) return []
  return [
    branches.defaultBranch,
    ...(branches.defaultBranchOverride ? [branches.defaultBranchOverride] : []),
    ...branches.boardDefaultBranches,
  ]
}
