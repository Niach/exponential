import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  boards,
  issues,
  pullRequestIssues,
  pullRequests,
  repositories,
  sessions,
  teamMembers,
  teams,
  type PullRequest,
} from "@/db/schema"
import type { GuideEntry } from "@exp/db-schema/domain"
import { resolveTeamAccess } from "@/lib/auth/access"
import { boardVisible } from "@/lib/board-visibility"
import { loadSessionForUser } from "@/lib/sessions/access"
import { appBaseUrl, buildSessionDeepLinkPath } from "@/lib/notification-email-policy"
import { NOT_RUN_TOPIC, PR_BODY_MAX, prBodyFromGuide } from "@/lib/pr-body"
import { writeGuide } from "@/lib/sessions/guide"
import { prDefaultIssueIds } from "@/lib/sessions/pr-issues"
import {
  compareChangedPaths,
  missingPicturesMessage,
  missingPlatformPictures,
  uiPlatformsOfPaths,
} from "@/lib/pr-ui-guard"
import {
  closePullRequest as closeOnGithub,
  createPullRequest,
  diagnoseUnmergeablePr,
  findOpenPullByHead,
  getPullRequest,
  GitHubAsyncMergePending,
  GitHubMergeError,
  mergedByPerson,
  mergedByPersonNote,
  mergePullRequestSmart,
  PullAlreadyExistsError,
  retargetPullRequest as retargetOnGithub,
  setPullRequestDraft,
} from "@/lib/integrations/github-pr"
import { resolveRepoDefaultBranchCached } from "@/lib/integrations/github-app"
import { loadPrFiles } from "@/lib/integrations/pr-files"
import {
  claimPrMerge,
  claimPrOpen,
  releasePrMergeClaim,
  releasePrOpenClaim,
} from "@/lib/integrations/pr-actor-claims"
import {
  applyPullRequestBase,
  applyPullRequestClosed,
  applyPullRequestDraft,
  applyPullRequestMerged,
  livePullRequestsOfRepo,
  livePullRequestsOfRoot,
  loadLivePullRequestByHead,
  loadPullRequest,
  loadPullRequestByNumber,
  loadPullRequestByUrl,
  livePullRequestsForIssue,
  newestLivePullRequestForSession,
  recordPullRequestOpened,
  repoFromPrUrl,
  retargetChildrenOfMergedPr,
} from "@/lib/integrations/pr-sync"
import {
  awaitRebaseOffMergedBranch,
  basedOnMergedPr,
  isLivePrState,
  liveChildren,
  loadGuardDefaultBranches,
  openStackMember,
  prIssueIdentifiers,
  squashCommitTitle,
  stackLanding,
  stackStopBranches,
} from "@/lib/pr-merge-guard"
import { ensureGithubStack, GitHubStackForkError, inferBaseBranch, mergeThrough } from "@/lib/pr-stacks"
import { isNotMergeable, prMergeFailureError } from "@/lib/trpc/pr-merge-error"
import { patchPullDescription } from "@/lib/trpc/pr-update"
import {
  effectiveDefaultBranch,
  loadRepository,
  resolveGatedRepoToken,
} from "@/lib/trpc/repositories"

export { prBodyFromGuide } from "@/lib/pr-body"
export type { PullRequest } from "@/db/schema"

// EXP-1291: THE pull-request API. A thread (root session) has n PRs (a tree,
// a stack or independent ones), each a `pull_requests` row closing the
// issues in `pull_request_issues`; every PR body is the thread's ONE Guide
// (`prBodyFromGuide`, re-rendered by `syncPullRequestBodies`). MCP's pr_*
// tools and the `pullRequests` tRPC router both call these; the state writes
// (automation, issue events, the thread's event cards, notifications) live in
// `lib/integrations/pr-sync.ts`, shared with the webhook and the poller.
// Merging never ends a session.

type Repo = Awaited<ReturnType<typeof loadRepository>>

const fail = (code: TRPCError[`code`], message: string): never => {
  throw new TRPCError({ code, message })
}

/** Membership on the PR's team; NOT_FOUND otherwise (no existence oracle). */
async function assertPrAccess(userId: string, teamId: string): Promise<void> {
  try {
    await resolveTeamAccess(userId, teamId)
  } catch {
    fail(`NOT_FOUND`, `Pull request not found`)
  }
}

async function repoToken(repo: Repo): Promise<string> {
  const token = await resolveGatedRepoToken(repo)
  if (!token) {
    return fail(
      `PRECONDITION_FAILED`,
      `The GitHub App has no access to ${repo.fullName}. Re-grant it on GitHub (team settings → Repositories), then retry.`
    )
  }
  return token
}

/** EXP-1315: a thread's PR that changes UI needs a labelled Guide picture
 *  per touched platform, or `notRun` naming it. A failed compare never
 *  blocks the PR: it is logged and the guard skipped. */
async function assertUiPictures(args: {
  repo: string
  base: string
  head: string
  token: string
  guide: GuideEntry[] | null
  notRun: string | null
}): Promise<void> {
  let paths: string[]
  try {
    paths = await compareChangedPaths(args)
  } catch (err) {
    console.warn(`[pr_open] UI picture guard skipped for ${args.repo} ${args.base}...${args.head}:`, err)
    return
  }
  const missing = missingPlatformPictures(uiPlatformsOfPaths(paths), args.guide, args.notRun)
  if (missing.length > 0) fail(`BAD_REQUEST`, missingPicturesMessage(missing))
}

async function loadRoot(rootId: string): Promise<{ id: string; teamId: string; guide: GuideEntry[] | null } | null> {
  const [row] = await db
    .select({ id: sessions.id, teamId: sessions.teamId, guide: sessions.guide })
    .from(sessions)
    .where(eq(sessions.id, rootId))
    .limit(1)
  return row ?? null
}

/** The footer link of every PR body: the thread (root session) Guide. */
async function guideUrlFor(teamId: string, rootId: string): Promise<string | null> {
  const [team] = await db.select({ slug: teams.slug }).from(teams).where(eq(teams.id, teamId)).limit(1)
  return team ? `${appBaseUrl()}${buildSessionDeepLinkPath(team.slug, rootId)}?view=guide` : null
}

interface IssueForPr {
  id: string
  identifier: string
  teamId: string
  boardRepositoryId: string | null
  boardDefaultBranch: string | null
}

async function loadIssuesForPr(ids: string[]): Promise<IssueForPr[]> {
  if (ids.length === 0) return []
  return db
    .select({
      id: issues.id,
      identifier: issues.identifier,
      teamId: issues.teamId,
      boardRepositoryId: boards.repositoryId,
      boardDefaultBranch: boards.defaultBranch,
    })
    .from(issues)
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .where(and(inArray(issues.id, ids), boardVisible()))
}

/** Create the PR, or adopt the one GitHub already holds open on `head`
 *  (FEED-59: GitHub's 422 only fires for the same head AND base, so the
 *  lookup runs first and ignores the base). */
async function createOrAdoptOnGithub(opts: {
  repo: string
  head: string
  base: string
  title: string
  body: string
  token: string
  draft?: boolean
}): Promise<{ url: string; number: number; base: string; adopted: boolean }> {
  const lookup = async (base?: string) => {
    try {
      return await findOpenPullByHead(opts.repo, opts.head, opts.token, undefined, base)
    } catch {
      return null
    }
  }
  const open = await lookup()
  if (open) return { url: open.url, number: open.number, base: open.baseRef || opts.base, adopted: true }
  try {
    const created = await createPullRequest(opts)
    return { ...created, base: opts.base, adopted: false }
  } catch (e) {
    if (!(e instanceof PullAlreadyExistsError)) {
      return fail(`BAD_GATEWAY`, e instanceof Error ? e.message : String(e))
    }
    const raced = await lookup(opts.base)
    if (!raced) return fail(`BAD_GATEWAY`, e.message)
    return { url: raced.url, number: raced.number, base: raced.baseRef || opts.base, adopted: true }
  }
}

/**
 * EXP-1248: a PR opened on another live PR's head joins that PR's GitHub
 * stack (creating the line bottom→top when there is none). A FORK (another
 * live PR already sits on the lower one: a tree, never a linear stack) is
 * skipped. The PR is open and recorded either way: a failed join is logged,
 * never the caller's error.
 */
async function joinGithubStack(opts: {
  pr: PullRequest
  repoFullName: string
  token: string
}): Promise<void> {
  const lowerId = opts.pr.basePullRequestId
  if (!lowerId) return
  try {
    const lower = await loadPullRequest(lowerId)
    if (!lower || !isLivePrState(lower.state)) return
    const siblings = await liveChildren(db, [lower.id])
    if (siblings.some((child) => child.id !== opts.pr.id)) return
    await ensureGithubStack({
      repo: opts.repoFullName,
      token: opts.token,
      lowerPrNumber: lower.number,
      newPrNumber: opts.pr.number,
      stopBranches: stackStopBranches(
        await loadGuardDefaultBranches(db, { teamId: opts.pr.teamId, repoFullName: opts.repoFullName })
      ),
    })
  } catch (e) {
    if (e instanceof GitHubStackForkError) return
    console.warn(`[pull-requests] PR #${opts.pr.number} is open; joining its GitHub stack failed`, e)
  }
}

/**
 * Open a PR (the MCP `pr_open`). The caller session (X-Exp-Session-Id)
 * gives the defaults: its repository and branch, and its share of its
 * thread's issues when `issueIds` is omitted (EXP-1334, `prDefaultIssueIds`).
 * An omitted `base` = the nearest live PR below `head` (native stacks), else
 * the default branch.
 * Idempotent: a live PR already on `head` is updated and gains the issues.
 */
export async function openPullRequest(args: {
  userId: string
  callerSessionId: string | null
  repositoryId?: string
  head?: string
  base?: string
  title: string
  body?: string
  draft?: boolean
  issueIds?: string[]
  /** EXP-1315: the UI screens the agent could not run (`<platform>: <screen>`). */
  notRun?: string
}): Promise<{ pullRequestId: string; url: string; number: number; base: string; head: string }> {
  const caller = args.callerSessionId
    ? await loadSessionForUser(args.userId, args.callerSessionId)
    : null
  // EXP-1330: a `/btw` aside is a read-only fork: it never opens a PR.
  if (caller?.aside) fail(`BAD_REQUEST`, `An aside never opens a pull request`)
  const root = caller ? await loadRoot(caller.rootId) : null

  const explicitIds = args.issueIds ? [...new Set(args.issueIds)] : null
  const candidates = await loadIssuesForPr(
    explicitIds ??
      (root ? await prDefaultIssueIds(db, { rootId: root.id, sessionId: caller?.id ?? null }) : [])
  )
  if (explicitIds && candidates.length !== explicitIds.length) fail(`NOT_FOUND`, `Issue not found`)

  const repositoryId =
    args.repositoryId ??
    caller?.repositoryId ??
    candidates.find((issue) => issue.boardRepositoryId)?.boardRepositoryId ??
    null
  if (!repositoryId) {
    fail(`BAD_REQUEST`, `Pass 'repositoryId': neither the session nor its issues name a repository`)
  }
  const repo = await loadRepository(repositoryId!)
  await assertPrAccess(args.userId, repo.teamId)
  if (repo.archivedAt) {
    fail(
      `PRECONDITION_FAILED`,
      `${repo.fullName} is archived for this team. Restore it in team settings → Repositories.`
    )
  }
  if (caller && caller.teamId !== repo.teamId) {
    fail(`BAD_REQUEST`, `${repo.fullName} belongs to another team than this session`)
  }
  const head = args.head ?? caller?.branch ?? null
  if (!head) fail(`BAD_REQUEST`, `Pass 'head': the branch you pushed`)

  // One repo, one team: an explicit issue elsewhere is refused, the
  // thread's defaults are narrowed to this PR's repository.
  const belongs = (issue: IssueForPr) =>
    issue.teamId === repo.teamId &&
    (issue.boardRepositoryId === null || issue.boardRepositoryId === repo.id)
  if (explicitIds) {
    const stray = candidates.find((issue) => !belongs(issue))
    if (stray) {
      fail(
        `BAD_REQUEST`,
        stray.teamId !== repo.teamId
          ? `${stray.identifier} is in another team than ${repo.fullName}`
          : `${stray.identifier}'s board uses another repository than ${repo.fullName}`
      )
    }
  }
  const linked = candidates.filter(belongs)
  const notRun = args.notRun?.replace(/\s+/g, ` `).trim() || null
  const token = await repoToken(repo)
  // EXP-1315: `notRun` lives in the Guide (topic `Not run`), so every later
  // body render and the next pr_open keep it.
  let guide = root?.guide ?? null
  if (notRun && caller && root) {
    await writeGuide({ userId: args.userId, sessionId: caller.id, topic: NOT_RUN_TOPIC, text: notRun })
    guide = (await loadRoot(root.id))?.guide ?? null
  }
  const guideUrl = root ? await guideUrlFor(repo.teamId, root.id) : null
  const actor = {
    actorUserId: args.userId,
    actorViaAgent: Boolean(caller),
    origin: { viaMcp: Boolean(caller), sessionId: caller?.id ?? null },
  }
  const record = (pr: { url: string; number: number; base: string; title: string | null }) =>
    recordPullRequestOpened({
      teamId: repo.teamId,
      rootSessionId: root?.id ?? null,
      sessionId: caller?.id ?? null,
      repositoryId: repo.id,
      repoFullName: repo.fullName,
      number: pr.number,
      url: pr.url,
      title: pr.title,
      head: head!,
      base: pr.base,
      ...(args.draft ? { draft: true } : {}),
      issueIds: linked.map((issue) => issue.id),
      ...actor,
    })

  const existing = await loadLivePullRequestByHead(repo.id, head!)
  if (existing) {
    const body =
      args.body ??
      prBodyFromGuide(guide, { prUrl: existing.url, guideUrl }) ??
      undefined
    await patchPullDescription({
      repoFullName: repo.fullName,
      prNumber: existing.number,
      token,
      fields: { title: args.title, ...(body !== undefined ? { body: body.slice(0, PR_BODY_MAX) } : {}) },
    })
    if (args.draft !== undefined && args.draft !== (existing.state === `draft`)) {
      await setPullRequestDraft({ repo: repo.fullName, prNumber: existing.number, draft: args.draft, token })
      await applyPullRequestDraft({ pullRequestId: existing.id, draft: args.draft })
    }
    await record({ url: existing.url, number: existing.number, base: existing.base, title: args.title })
    return { pullRequestId: existing.id, url: existing.url, number: existing.number, base: existing.base, head: head! }
  }

  const defaults = await loadGuardDefaultBranches(db, { teamId: repo.teamId, repoFullName: repo.fullName })
  const defaultBranch =
    linked.find((issue) => issue.boardDefaultBranch)?.boardDefaultBranch ?? effectiveDefaultBranch(repo)
  const base =
    args.base ??
    (
      await inferBaseBranch({
        repo: repo.fullName,
        token,
        head: head!,
        defaultBranch,
        candidates: (await livePullRequestsOfRepo(repo.id))
          .filter((pr) => pr.head !== head)
          .map((pr) => ({ number: pr.number, branch: pr.head })),
        stopBranches: stackStopBranches(defaults),
      })
    ).base
  const body = args.body ?? prBodyFromGuide(guide, { prUrl: null, guideUrl }) ?? ``
  if (root) await assertUiPictures({ repo: repo.fullName, base, head: head!, token, guide, notRun })

  // EXP-494: the initiator is claimed BEFORE the GitHub call, so the
  // `opened` webhook (which beats our own write) fires attributed.
  claimPrOpen(repo.fullName, head!, { userId: args.userId, viaAgent: Boolean(caller) })
  let created: Awaited<ReturnType<typeof createOrAdoptOnGithub>>
  try {
    created = await createOrAdoptOnGithub({
      repo: repo.fullName,
      head: head!,
      base,
      title: args.title,
      body: body.slice(0, PR_BODY_MAX),
      token,
      ...(args.draft ? { draft: true } : {}),
    })
  } catch (e) {
    releasePrOpenClaim(repo.fullName, head!)
    throw e
  }
  // No `opened` webhook follows an adopted PR to consume the claim.
  if (created.adopted) releasePrOpenClaim(repo.fullName, head!)

  const recorded = await record({ url: created.url, number: created.number, base: created.base, title: args.title })
  await joinGithubStack({ pr: recorded.pullRequest, repoFullName: repo.fullName, token })
  return {
    pullRequestId: recorded.pullRequest.id,
    url: created.url,
    number: created.number,
    base: created.base,
    head: head!,
  }
}

/**
 * FEED-111: the issue's PR among its live linked ones. A thread-level PR
 * links every root issue, so newest-first is wrong: the PR the issue's
 * assigned session opened wins, else the one linking the fewest issues; a
 * tie refuses rather than guess (a merge must never land the wrong PR).
 */
export function pickIssuePullRequest(
  candidates: { pr: PullRequest; linkedIssues: number; assigned: boolean }[]
): PullRequest | null {
  if (candidates.length <= 1) return candidates[0]?.pr ?? null
  const assigned = candidates.filter((c) => c.assigned)
  if (assigned.length === 1) return assigned[0]!.pr
  const pool = assigned.length > 1 ? assigned : candidates
  const fewest = Math.min(...pool.map((c) => c.linkedIssues))
  const focused = pool.filter((c) => c.linkedIssues === fewest)
  if (focused.length === 1) return focused[0]!.pr
  return fail(
    `PRECONDITION_FAILED`,
    `This issue has ${focused.length} open pull requests (${focused
      .map((c) => `#${c.pr.number} ${c.pr.url}`)
      .join(`, `)}). Name one with repositoryId + prNumber or pullRequestId.`
  )
}

/**
 * One PR by id, url, (repositoryId + number) or issue (`pickIssuePullRequest`);
 * nothing given = the caller session's newest live PR. A PR GitHub holds
 * that nobody recorded (named by url or number, e.g. Reviews' unlinked
 * list) is recorded on first use, linked to nothing. Membership-checked.
 */
export async function resolvePullRequest(args: {
  userId: string
  pullRequestId?: string
  issueId?: string
  repositoryId?: string
  number?: number
  url?: string
  callerSessionId?: string | null
}): Promise<PullRequest> {
  let pr: PullRequest | null = null
  if (args.pullRequestId) {
    pr = await loadPullRequest(args.pullRequestId)
  } else if (args.url) {
    pr = (await loadPullRequestByUrl(args.url)) ?? (await adoptByUrl(args.userId, args.url))
  } else if (args.repositoryId && args.number != null) {
    pr =
      (await loadPullRequestByNumber(args.repositoryId, args.number)) ??
      (await adoptByNumber(args.userId, args.repositoryId, args.number))
  } else if (args.issueId) {
    const [issue] = await db
      .select({ teamId: issues.teamId })
      .from(issues)
      .where(eq(issues.id, args.issueId))
      .limit(1)
    if (!issue) fail(`NOT_FOUND`, `Issue not found`)
    await assertPrAccess(args.userId, issue!.teamId)
    pr = pickIssuePullRequest(await livePullRequestsForIssue(args.issueId))
    if (!pr) fail(`PRECONDITION_FAILED`, `This issue has no open pull request`)
  } else if (args.callerSessionId) {
    const session = await loadSessionForUser(args.userId, args.callerSessionId)
    pr = await newestLivePullRequestForSession(session)
    if (!pr) {
      fail(
        `PRECONDITION_FAILED`,
        `This session has no open pull request yet. Open one with exponential_pr_open, or name the PR.`
      )
    }
  } else {
    fail(`BAD_REQUEST`, `Name the pull request (pullRequestId, url, repositoryId + number or issueId)`)
  }
  if (!pr) return fail(`NOT_FOUND`, `Pull request not found`)
  await assertPrAccess(args.userId, pr.teamId)
  return pr
}

async function adoptByNumber(userId: string, repositoryId: string, number: number): Promise<PullRequest | null> {
  const repo = await loadRepository(repositoryId)
  await assertPrAccess(userId, repo.teamId)
  const token = await repoToken(repo)
  let pull: Awaited<ReturnType<typeof getPullRequest>>
  try {
    pull = await getPullRequest(repo.fullName, number, token)
  } catch {
    return null
  }
  const recorded = await recordPullRequestOpened({
    teamId: repo.teamId,
    rootSessionId: null,
    sessionId: null,
    repositoryId: repo.id,
    repoFullName: repo.fullName,
    number,
    url: pull.url,
    title: pull.title || null,
    head: pull.headRef,
    base: pull.baseRef,
    ...(pull.draft ? { draft: true } : {}),
    issueIds: [],
    actorUserId: null,
  })
  // A PR already closed or merged on GitHub is recorded as such.
  if (pull.merged) await applyPullRequestMerged({ pullRequestId: recorded.pullRequest.id })
  else if (pull.state === `closed`) await applyPullRequestClosed({ pullRequestId: recorded.pullRequest.id })
  return loadPullRequest(recorded.pullRequest.id)
}

async function adoptByUrl(userId: string, url: string): Promise<PullRequest | null> {
  const fullName = repoFromPrUrl(url)
  const number = Number(url.match(/\/pull\/(\d+)/)?.[1])
  if (!fullName || !Number.isFinite(number)) return null
  // The caller's teams' rows for that repository; the first that grants
  // access records it.
  const rows = await db
    .select({ id: repositories.id })
    .from(repositories)
    .innerJoin(teamMembers, eq(teamMembers.teamId, repositories.teamId))
    .where(and(eq(repositories.fullName, fullName), eq(teamMembers.userId, userId)))
    .limit(1)
  return rows[0] ? adoptByNumber(userId, rows[0].id, number) : null
}

/** The internal merge, throwing TRPCErrors whose CODE is the signal
 *  (EXP-533: CONFLICT = a diagnosed content conflict). The tRPC router
 *  passes them through; `mergePullRequest` turns them into `error`. */
export async function mergePullRequestOrThrow(args: {
  userId: string
  pullRequestId: string
  stack?: boolean
  callerSessionId?: string | null
}): Promise<{ merged: boolean; queued?: boolean; note?: string; mergedIds?: string[]; txid?: number }> {
  const pr = await resolvePullRequest({ userId: args.userId, pullRequestId: args.pullRequestId })
  if (pr.state === `merged`) return { merged: true, mergedIds: [pr.id] }
  if (pr.state === `draft`) {
    fail(`PRECONDITION_FAILED`, `PR #${pr.number} is a draft. Mark it ready for review first.`)
  }
  if (!isLivePrState(pr.state)) {
    fail(`PRECONDITION_FAILED`, `The pull request is ${pr.state}. Only open pull requests can be merged.`)
  }
  const repo = await loadRepository(pr.repositoryId)
  // EXP-1248/1145: a stack member with live PRs beneath it lands only
  // through `stack`; a tree's child waits for its parent. Before any claim
  // or GitHub call.
  const landing = stackLanding(await openStackMember(db, pr.id), args.stack)
  const token = await repoToken(repo)
  const defaults = await loadGuardDefaultBranches(db, { teamId: pr.teamId, repoFullName: repo.fullName })

  // The PR below already MERGED: its EXP-324 heal is fire-and-forget and
  // this merge would beat it, squashing INTO the merged branch (EXP-320).
  // Await the heal, then refuse while GitHub still reports that base. A
  // merge through a stack heals its BOTTOM (every other member sits on a
  // live PR).
  const bottom = landing?.[0] ?? pr
  const mergedParent = await basedOnMergedPr(db, {
    pullRequestId: bottom.id,
    repositoryId: pr.repositoryId,
    basePullRequestId: bottom.basePullRequestId,
    base: bottom.base,
    defaults,
  })
  if (mergedParent) {
    try {
      await retargetChildrenOfMergedPr(mergedParent.id)
    } catch (err) {
      console.error(`retarget before merging ${repo.fullName}#${bottom.number}:`, err)
    }
    await awaitRebaseOffMergedBranch(db, {
      pullRequestId: bottom.id,
      repoFullName: repo.fullName,
      prNumber: bottom.number,
      base: bottom.base,
      mergedBranch: mergedParent.head,
      token,
    })
  }

  // A merge through a stack needs the line to BE a GitHub stack, or
  // merge-async would squash it INTO the branch below. No fallback.
  if (landing && landing.length > 1) {
    try {
      await ensureGithubStack({
        repo: repo.fullName,
        token,
        lowerPrNumber: landing[landing.length - 2]!.number,
        newPrNumber: pr.number,
        stopBranches: stackStopBranches(defaults),
      })
    } catch (err) {
      fail(`PRECONDITION_FAILED`, err instanceof Error ? err.message : String(err))
    }
  }

  const landed = landing ?? [{ id: pr.id, number: pr.number }]
  const viaAgent = Boolean(args.callerSessionId)
  for (const member of landed) claimPrMerge(repo.fullName, member.number, { userId: args.userId, viaAgent })
  const releaseClaims = () => {
    for (const member of landed) releasePrMergeClaim(repo.fullName, member.number)
  }
  const identifier = (await prIssueIdentifiers(db, [pr.id])).get(pr.id) ?? null
  const mergeOpts = {
    repo: repo.fullName,
    prNumber: pr.number,
    token,
    ...(pr.title ? { commitTitle: squashCommitTitle(identifier, pr.title, pr.number) } : {}),
  }
  let smart: Awaited<ReturnType<typeof mergePullRequestSmart>>
  try {
    smart = landing ? await mergeThrough(mergeOpts) : await mergePullRequestSmart(mergeOpts)
  } catch (err) {
    if (err instanceof GitHubAsyncMergePending) {
      // NOT a failure: GitHub's job is still running, the claims STAY.
      return fail(
        `PRECONDITION_FAILED`,
        `GitHub is still merging PR #${err.prNumber}. It did not finish in time — check the PR on GitHub; its issues complete when the merge lands.`
      )
    }
    releaseClaims()
    if (err instanceof GitHubMergeError) {
      const diagnosis = isNotMergeable(err)
        ? await diagnoseUnmergeablePr({
            repo: repo.fullName,
            prNumber: pr.number,
            token,
            defaultBranch:
              defaults?.defaultBranchOverride ??
              defaults?.defaultBranch ??
              effectiveDefaultBranch(repo),
          })
        : null
      throw prMergeFailureError(err, diagnosis)
    }
    throw err
  }

  // FEED-43 R1/EXP-897: the merge queue took it; nothing landed yet. The
  // webhook (or the poller) completes it; the claims stay for its echo.
  if (smart.queued) {
    return {
      merged: false,
      queued: true,
      note: `GitHub queued the merge of PR #${pr.number}. Nothing is merged yet; its issues complete when the merge lands.`,
    }
  }
  // FEED-64: a PERSON merged it on github.com; theirs to attribute, so the
  // claims go and the webhook/poller writes the state off `merged_by`.
  if (mergedByPerson(smart.mergedBy)) {
    releaseClaims()
    return { merged: true, note: mergedByPersonNote(pr.number, smart.mergedBy), mergedIds: landed.map((m) => m.id) }
  }

  // Every landed PR completes now (self-hosted instances behind NAT may
  // only have the slower poller); the webhook's echo degrades to a no-op.
  let txid: number | undefined
  for (const member of landed) {
    const applied = await applyPullRequestMerged({
      pullRequestId: member.id,
      mergedAt: new Date(),
      actorUserId: args.userId,
      actorViaAgent: viaAgent,
    })
    if (member.id === pr.id && applied.txid != null) txid = applied.txid
  }
  return { merged: true, mergedIds: landed.map((member) => member.id), ...(txid != null ? { txid } : {}) }
}

/** Squash-merge a PR; `stack` = merge THROUGH this member of a native stack
 *  (it lands with every live member beneath it, bottom-up). A refusal is
 *  `error`, never a throw (only an unknown/foreign PR throws). */
export async function mergePullRequest(args: {
  userId: string
  pullRequestId: string
  stack?: boolean
  callerSessionId?: string | null
}): Promise<{ merged: boolean; queued?: boolean; error?: string; mergedIds?: string[] }> {
  try {
    const { txid: _txid, note: _note, ...result } = await mergePullRequestOrThrow(args)
    void _txid
    void _note
    return result
  } catch (err) {
    if (err instanceof TRPCError && err.code !== `NOT_FOUND`) {
      return { merged: false, error: err.message }
    }
    throw err
  }
}

/** Change a live PR's base on GitHub (EXP-324's manual heal). Omitted =
 *  the branch the team develops on (the board pin of its first issue, the
 *  repo pin, GitHub's default). */
export async function retargetPullRequest(args: {
  userId: string
  pullRequestId: string
  base?: string
}): Promise<{ base: string }> {
  const { base } = await retargetPullRequestTx(args)
  return { base }
}

export async function retargetPullRequestTx(args: {
  userId: string
  pullRequestId: string
  base?: string
}): Promise<{ base: string; txid: number }> {
  const pr = await resolvePullRequest({ userId: args.userId, pullRequestId: args.pullRequestId })
  if (!isLivePrState(pr.state)) {
    fail(`PRECONDITION_FAILED`, `The pull request is ${pr.state}. Only open pull requests can be retargeted.`)
  }
  const repo = await loadRepository(pr.repositoryId)
  const token = await repoToken(repo)
  const [pin] = await db
    .select({ defaultBranch: boards.defaultBranch })
    .from(pullRequestIssues)
    .innerJoin(issues, eq(issues.id, pullRequestIssues.issueId))
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .where(eq(pullRequestIssues.pullRequestId, pr.id))
    .orderBy(asc(pullRequestIssues.createdAt))
    .limit(1)
  const base =
    args.base ??
    pin?.defaultBranch ??
    effectiveDefaultBranch(repo) ??
    (await resolveRepoDefaultBranchCached(repo.fullName))
  if (!base) return fail(`PRECONDITION_FAILED`, `Could not resolve the default branch of ${repo.fullName}`)
  try {
    await retargetOnGithub({ repo: repo.fullName, prNumber: pr.number, base, token })
  } catch (err) {
    if (err instanceof GitHubMergeError) {
      if (err.status === 422) {
        fail(`PRECONDITION_FAILED`, `'${base}' is not a valid base branch on ${repo.fullName}: ${err.message}`)
      }
      if (err.status === 404) fail(`NOT_FOUND`, `Pull request not found on GitHub`)
      fail(`INTERNAL_SERVER_ERROR`, `GitHub retarget failed (HTTP ${err.status}): ${err.message}`)
    }
    throw err
  }
  const { txid } = await applyPullRequestBase({ pullRequestId: pr.id, base })
  return { base, txid }
}

/** Rewrite a live PR's title/body on GitHub, or flip its draft state. A
 *  thread PR's body follows the Guide again on the next Guide write. */
export async function updatePullRequest(args: {
  userId: string
  pullRequestId: string
  title?: string
  body?: string
  draft?: boolean
}): Promise<void> {
  if (args.title === undefined && args.body === undefined && args.draft === undefined) {
    fail(`BAD_REQUEST`, `Pass a title, a body, draft, or any of them.`)
  }
  const pr = await resolvePullRequest({ userId: args.userId, pullRequestId: args.pullRequestId })
  if (!isLivePrState(pr.state)) {
    fail(`PRECONDITION_FAILED`, `The pull request is ${pr.state}. Only open pull requests can be edited.`)
  }
  const repo = await loadRepository(pr.repositoryId)
  const token = await repoToken(repo)
  if (args.title !== undefined || args.body !== undefined) {
    await patchPullDescription({
      repoFullName: repo.fullName,
      prNumber: pr.number,
      token,
      fields: { title: args.title, body: args.body },
    })
    if (args.title !== undefined) {
      await db
        .update(pullRequests)
        .set({ title: args.title, updatedAt: new Date() })
        .where(eq(pullRequests.id, pr.id))
    }
  }
  if (args.draft !== undefined && args.draft !== (pr.state === `draft`)) {
    try {
      await setPullRequestDraft({ repo: repo.fullName, prNumber: pr.number, draft: args.draft, token })
    } catch (err) {
      fail(`BAD_GATEWAY`, `GitHub refused the draft change: ${err instanceof Error ? err.message : String(err)}`)
    }
    await applyPullRequestDraft({ pullRequestId: pr.id, draft: args.draft })
  }
}

/** Detach one issue from a PR (a mislinked `pull_request_issues` row), in
 *  any PR state. The issue's status stays a human decision. */
export async function unlinkPullRequestIssue(args: {
  userId: string
  pullRequestId: string
  issueId: string
}): Promise<{ unlinked: boolean }> {
  const pr = await resolvePullRequest({ userId: args.userId, pullRequestId: args.pullRequestId })
  const deleted = await db
    .delete(pullRequestIssues)
    .where(and(eq(pullRequestIssues.pullRequestId, pr.id), eq(pullRequestIssues.issueId, args.issueId)))
    .returning({ issueId: pullRequestIssues.issueId })
  return { unlinked: deleted.length > 0 }
}

/** Close a live PR WITHOUT merging (the Reviews "reject" path). Issue
 *  statuses stay a human decision. */
export async function closePullRequest(args: { userId: string; pullRequestId: string }): Promise<void> {
  await closePullRequestTx(args)
}

export async function closePullRequestTx(args: {
  userId: string
  pullRequestId: string
}): Promise<{ txid?: number }> {
  const pr = await resolvePullRequest({ userId: args.userId, pullRequestId: args.pullRequestId })
  if (pr.state === `closed`) return {}
  if (pr.state === `merged`) fail(`PRECONDITION_FAILED`, `The pull request is merged. Only open pull requests can be closed.`)
  const repo = await loadRepository(pr.repositoryId)
  const token = await repoToken(repo)
  try {
    await closeOnGithub({ repo: repo.fullName, prNumber: pr.number, token })
  } catch (err) {
    if (err instanceof GitHubMergeError) {
      if (err.status === 404) fail(`NOT_FOUND`, `Pull request not found on GitHub`)
      fail(`INTERNAL_SERVER_ERROR`, `GitHub close failed (HTTP ${err.status}): ${err.message}`)
    }
    throw err
  }
  const { txid } = await applyPullRequestClosed({ pullRequestId: pr.id })
  return txid != null ? { txid } : {}
}

/** The PR's changed files (the Guide's Changes rows), from GitHub. */
export async function pullRequestFiles(args: {
  userId: string
  pullRequestId: string
}): Promise<{ files: { path: string; additions: number; deletions: number; patch: string | null }[] }> {
  const pr = await resolvePullRequest({ userId: args.userId, pullRequestId: args.pullRequestId })
  const answer = await loadPrFiles(pr.url, pr.number)
  return {
    files: answer.files.map((file) => ({
      path: file.filename,
      additions: file.additions,
      deletions: file.deletions,
      patch: file.patch ?? null,
    })),
  }
}

// One chain per thread: overlapping Guide writes PATCH in call order, and
// each link re-reads the committed Guide, so the last PATCH carries it.
const bodySyncChains = new Map<string, Promise<void>>()

/** Re-render every LIVE PR body of a thread from its Guide (each PR keeps
 *  its own topics plus the untagged ones). Best effort: never throws, a
 *  thread without Guide text leaves the bodies alone. */
export function syncPullRequestBodies(rootId: string): Promise<void> {
  const previous = bodySyncChains.get(rootId) ?? Promise.resolve()
  const run = previous.then(() => syncPullRequestBodiesNow(rootId))
  bodySyncChains.set(rootId, run)
  void run.finally(() => {
    if (bodySyncChains.get(rootId) === run) bodySyncChains.delete(rootId)
  })
  return run
}

async function syncPullRequestBodiesNow(rootId: string): Promise<void> {
  try {
    const root = await loadRoot(rootId)
    if (!root) return
    const prs = await livePullRequestsOfRoot(rootId)
    if (prs.length === 0) return
    const guideUrl = await guideUrlFor(root.teamId, root.id)
    const tokens = new Map<string, Promise<string | null>>()
    for (const pr of prs) {
      try {
        const body = prBodyFromGuide(root.guide, { prUrl: pr.url, guideUrl })
        if (!body) continue
        if (!tokens.has(pr.repositoryId)) {
          tokens.set(
            pr.repositoryId,
            loadRepository(pr.repositoryId).then((repo) => resolveGatedRepoToken(repo))
          )
        }
        const token = await tokens.get(pr.repositoryId)!
        const repoFullName = repoFromPrUrl(pr.url)
        if (!token || !repoFullName) continue
        await patchPullDescription({ repoFullName, prNumber: pr.number, token, fields: { body } })
      } catch (err) {
        console.warn(`[pull-requests] body sync of ${pr.url} failed`, err)
      }
    }
  } catch (err) {
    console.warn(`[pull-requests] body sync of thread ${rootId} failed`, err)
  }
}

/** Test seam: threads with a body sync in flight. */
export function pendingPullRequestBodySyncs(): number {
  return bodySyncChains.size
}
