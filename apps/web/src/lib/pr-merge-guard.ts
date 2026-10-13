// EXP-1291: the merge rules over the PR model (`pull_requests`). A PR's
// `base_pull_request_id` = the open PR it is stacked on (native GitHub
// stacks), so the stack/tree shape is a walk over rows, never a branch-name
// join across issues.
//
// EXP-1248: a PR in an open STACK (a linear base chain, GitHub stacks it
// natively) with open PRs BENEATH it never merges plainly: `stackLanding`
// refuses it, naming what a merge through it would land; `stack: true`
// lands it through ONE merge-async (lib/pr-stacks.ts). The bottom lands
// alone, so it merges plainly. A FORK anywhere in the component is a tree:
// its children wait for the root (EXP-1145: a plain squash merge of a PR
// based on another open PR would land it INTO the parent's branch).
// Once the parent MERGED, the merge paths await the EXP-324 heal and refuse
// while GitHub still reports the merged branch as the base
// (`basedOnMergedPr` + `awaitRebaseOffMergedBranch`).
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNotNull, ne } from "drizzle-orm"
import {
  boards,
  issues,
  pullRequestIssues,
  pullRequests,
  repositories,
} from "@/db/schema"
import type { Context } from "@/lib/trpc"
import { getPullRequest } from "@/lib/integrations/github-pr"

type ReadDb = Pick<Context[`db`], `select`>

/** A PR still in review (GitHub's drafts are open PRs). */
export const LIVE_PR_STATES = [`open`, `draft`] as const
export function isLivePrState(state: string | null | undefined): boolean {
  return state === `open` || state === `draft`
}

/** Byte-locked: the merge dialog toasts it and agents read it out of
 *  `exponential_pr_merge`. */
export function stackedOnMessage(parent: string): string {
  return `This pull request is stacked on ${parent}; merge ${parent} first`
}

/** Byte-locked like `stackedOnMessage`: a plain merge on an open-stack
 *  member. Shown VERBATIM to people, so it names the product control, never
 *  the API flag; the members below it (bottom first) say what Merge stack
 *  lands with it. `landing` = the whole landing, this PR last. */
export function openStackMessage(landing: ReadonlyArray<{ name: string }>): string {
  const below = landing.length > 1 ? landing.slice(0, -1) : landing
  return `This pull request is part of an open stack. Use Merge stack to land it with the pull requests below it: ${below.map((member) => member.name).join(`, `)}.`
}

/** The squash commit's title: `IDENT: title (#N)` for an issue's PR. */
export function squashCommitTitle(
  identifier: string | null,
  title: string,
  prNumber: number
): string {
  return identifier ? `${identifier}: ${title} (#${prNumber})` : `${title} (#${prNumber})`
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
 * treating its branch as somebody's PR head (EXP-466).
 */
export async function loadRepoDefaultBranches(
  db: ReadDb,
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
    .where(and(eq(boards.repositoryId, repo.id), isNotNull(boards.defaultBranch)))
  return {
    defaultBranch: repo.defaultBranch,
    defaultBranchOverride: repo.defaultBranchOverride,
    boardDefaultBranches: pins.flatMap((pin) => (pin.defaultBranch ? [pin.defaultBranch] : [])),
  }
}

/**
 * The guards' view of the branches a repo is developed on: the team's repo
 * row when it has one, else GitHub's default branch. Null when neither
 * answers.
 */
export async function loadGuardDefaultBranches(
  db: ReadDb,
  opts: { teamId: string; repoFullName: string }
): Promise<RepoDefaultBranches | null> {
  const row = await loadRepoDefaultBranches(db, opts)
  if (row) return row
  try {
    // Lazy: keeps this module a leaf the routers load cheaply.
    const { resolveRepoDefaultBranchCached } = await import(`@/lib/integrations/github-app`)
    const defaultBranch = await resolveRepoDefaultBranchCached(opts.repoFullName)
    return defaultBranch
      ? { defaultBranch, defaultBranchOverride: null, boardDefaultBranches: [] }
      : null
  } catch {
    return null
  }
}

/** Is `branch` one the repo is developed on (never a stack parent)? */
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

/** The branches `ensureGithubStack` must never walk below. */
export function stackStopBranches(branches: RepoDefaultBranches | null): string[] {
  if (!branches) return []
  return [
    branches.defaultBranch,
    ...(branches.defaultBranchOverride ? [branches.defaultBranchOverride] : []),
    ...branches.boardDefaultBranches,
  ]
}

/** One PR of a stack, named the way the refusals print it. */
export interface StackMember {
  id: string
  number: number
  url: string
  head: string
  base: string
  basePullRequestId: string | null
  state: string
  /** `EXP-12 (#40)` for an issue's PR, `#40` without one. */
  name: string
}

/** Cycle-safe bound on the walk below a PR. */
export const MAX_STACK_DEPTH = 10
/** Cycle-safe bound on the open PRs walked above a PR. */
const MAX_COMPONENT_WALK = 30

const memberColumns = {
  id: pullRequests.id,
  number: pullRequests.number,
  url: pullRequests.url,
  head: pullRequests.head,
  base: pullRequests.base,
  basePullRequestId: pullRequests.basePullRequestId,
  state: pullRequests.state,
  repositoryId: pullRequests.repositoryId,
}

type MemberRow = { [K in keyof typeof memberColumns]: (typeof pullRequests.$inferSelect)[K] }

/** The first linked issue's identifier per PR (oldest link first). */
export async function prIssueIdentifiers(
  db: ReadDb,
  pullRequestIds: string[]
): Promise<Map<string, string>> {
  if (pullRequestIds.length === 0) return new Map()
  const rows = await db
    .select({ pullRequestId: pullRequestIssues.pullRequestId, identifier: issues.identifier })
    .from(pullRequestIssues)
    .innerJoin(issues, eq(issues.id, pullRequestIssues.issueId))
    .where(inArray(pullRequestIssues.pullRequestId, pullRequestIds))
    .orderBy(asc(pullRequestIssues.createdAt))
  const out = new Map<string, string>()
  for (const row of rows) if (!out.has(row.pullRequestId)) out.set(row.pullRequestId, row.identifier)
  return out
}

async function named(db: ReadDb, rows: MemberRow[]): Promise<StackMember[]> {
  const identifiers = await prIssueIdentifiers(db, rows.map((row) => row.id))
  return rows.map((row) => {
    const identifier = identifiers.get(row.id)
    return {
      id: row.id,
      number: row.number,
      url: row.url,
      head: row.head,
      base: row.base,
      basePullRequestId: row.basePullRequestId,
      state: row.state,
      name: identifier ? `${identifier} (#${row.number})` : `#${row.number}`,
    }
  })
}

async function loadMember(db: ReadDb, id: string): Promise<MemberRow | null> {
  const [row] = await db.select(memberColumns).from(pullRequests).where(eq(pullRequests.id, id)).limit(1)
  return row ?? null
}

/** The LIVE PRs stacked directly on any of `ids`. */
export async function liveChildren(db: ReadDb, ids: string[]): Promise<MemberRow[]> {
  if (ids.length === 0) return []
  return db
    .select(memberColumns)
    .from(pullRequests)
    .where(
      and(
        inArray(pullRequests.basePullRequestId, ids),
        inArray(pullRequests.state, [...LIVE_PR_STATES])
      )
    )
}

/**
 * The live chain BELOW AND INCLUDING `pullRequestId`, bottom first: each
 * step follows `base_pull_request_id` while the PR below is still live.
 * Empty when the PR itself is not live.
 */
export async function openLineThrough(db: ReadDb, pullRequestId: string): Promise<StackMember[]> {
  const start = await loadMember(db, pullRequestId)
  if (!start || !isLivePrState(start.state)) return []
  const chain: MemberRow[] = [start]
  const seen = new Set([start.id])
  let below = start.basePullRequestId
  while (below && chain.length < MAX_STACK_DEPTH) {
    if (seen.has(below)) break
    const lower = await loadMember(db, below)
    if (!lower || !isLivePrState(lower.state) || lower.repositoryId !== start.repositoryId) break
    seen.add(lower.id)
    chain.unshift(lower)
    below = lower.basePullRequestId
  }
  return named(db, chain)
}

/** What a merge of this PR is, stack-wise (EXP-1248). */
export type StackMembership =
  /** A linear stack member with live PRs beneath it: refused plainly,
   *  `stack: true` lands `landing` (bottom first, this PR last, 2+). */
  | { kind: `stack`; landing: StackMember[] }
  /** A fork somewhere in its component: a tree. `parent` (the PR below)
   *  merges first; null = the root, which merges plainly. */
  | { kind: `tree`; parent: string | null; landing: StackMember[] }

/**
 * Is this PR a member of an open stack? Classified over the WHOLE live
 * component (the line below plus every live PR above), like the clients'
 * `prGraphShape`: a fork anywhere = a `tree`. A linear line returns `stack`
 * only when PRs are live BENEATH this one; a bottom, a lone PR or a dead PR
 * = null (plain).
 */
export async function openStackMember(
  db: ReadDb,
  pullRequestId: string
): Promise<StackMembership | null> {
  const chain = await openLineThrough(db, pullRequestId)
  const self = chain[chain.length - 1]
  if (!self) return null
  let fork = false
  // Below: each member's ONLY live child is the next one up the chain.
  for (const [index, member] of chain.slice(0, -1).entries()) {
    const children = await liveChildren(db, [member.id])
    if (children.some((child) => child.id !== chain[index + 1]!.id)) {
      fork = true
      break
    }
  }
  // Above: walk every live descendant; any PR with two live children forks.
  const seen = new Set(chain.map((member) => member.id))
  const queue = [self.id]
  let walked = 0
  while (!fork && queue.length > 0 && walked < MAX_COMPONENT_WALK) {
    walked += 1
    const children = (await liveChildren(db, [queue.shift()!])).filter((child) => !seen.has(child.id))
    if (children.length > 1) fork = true
    for (const child of children) {
      seen.add(child.id)
      queue.push(child.id)
    }
  }
  if (fork) {
    return {
      kind: `tree`,
      parent: chain.length > 1 ? chain[chain.length - 2]!.name : null,
      landing: chain,
    }
  }
  if (chain.length > 1) return { kind: `stack`, landing: chain }
  return null
}

/**
 * The merge rule every path applies to `openStackMember`'s answer, before
 * any claim or GitHub call: a tree child waits for its parent; a member with
 * PRs live beneath it needs `stack` and lands `landing` (returned); a
 * bottom, a tree root or a lone PR merges plainly (null).
 */
export function stackLanding(
  membership: StackMembership | null,
  stack: boolean | undefined
): StackMember[] | null {
  if (membership?.kind === `tree` && membership.parent) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: stackedOnMessage(membership.parent) })
  }
  if (membership?.kind !== `stack` || membership.landing.length < 2) return null
  if (!stack) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: openStackMessage(membership.landing) })
  }
  return membership.landing
}

/**
 * The MERGED PR this one may still sit on: its `base_pull_request_id` row
 * once merged, else a merged PR of the same repository whose head is this
 * PR's base (a branch the repo is developed on never counts). A hit means
 * the caller awaits the EXP-324 heal and `awaitRebaseOffMergedBranch`.
 */
export async function basedOnMergedPr(
  db: ReadDb,
  opts: {
    pullRequestId: string
    repositoryId: string
    basePullRequestId: string | null
    base: string
    defaults: RepoDefaultBranches | null
  }
): Promise<{ id: string; url: string; head: string } | null> {
  if (isRepoDefaultBranch(opts.defaults, opts.base)) return null
  const columns = { id: pullRequests.id, url: pullRequests.url, head: pullRequests.head }
  if (opts.basePullRequestId) {
    const [parent] = await db
      .select(columns)
      .from(pullRequests)
      .where(and(eq(pullRequests.id, opts.basePullRequestId), eq(pullRequests.state, `merged`)))
      .limit(1)
    if (parent) return parent
  }
  const [parent] = await db
    .select(columns)
    .from(pullRequests)
    .where(
      and(
        eq(pullRequests.repositoryId, opts.repositoryId),
        eq(pullRequests.head, opts.base),
        eq(pullRequests.state, `merged`),
        ne(pullRequests.id, opts.pullRequestId)
      )
    )
    .limit(1)
  return parent ?? null
}

/** GitHub computes mergeability lazily after a base move: `mergeable` reads
 *  null for a moment (and the base ref may lag one read). ≈10 s in all. */
const REBASE_SETTLE_MS = [1_000, 2_000, 3_000, 4_000]
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/**
 * After the PR below merged and the EXP-324 heal ran: wait until GitHub
 * reports this PR off the merged branch with its mergeability computed, and
 * sync the stored base when GitHub moved it. Never squash INTO the merged
 * branch: a base that stayed put refuses, and so does a PR GitHub would not
 * show us.
 */
export async function awaitRebaseOffMergedBranch(
  db: Pick<Context[`db`], `update`>,
  opts: {
    pullRequestId: string
    repoFullName: string
    prNumber: number
    base: string
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
  if (pull.baseRef && pull.baseRef !== opts.base) {
    await db
      .update(pullRequests)
      .set({ base: pull.baseRef, basePullRequestId: null, updatedAt: new Date() })
      .where(eq(pullRequests.id, opts.pullRequestId))
  }
}
