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
import { resolveTeamAccess } from "@/lib/auth/access"
import { boardVisible, issueVisible } from "@/lib/board-visibility"
import { loadSessionForUser } from "@/lib/sessions/access"
import { PR_BODY_MAX, prBodyFromPictures } from "@/lib/pr-body"
import { expandTeamLinksInMarkdown } from "@/lib/app-link"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { compareCommitMessages, linkedIssuesOfPullRequest, loadPrPictures, chatUrlFor } from "@/lib/pr-pictures"
import { logicalSessionIds } from "@/lib/sessions/guide-entries"
import { prDefaultIssueIds } from "@/lib/sessions/pr-issues"
import {
  compareChangedPaths,
  missingPicturesMessage,
  missingPlatformPictures,
  uiPlatformsOfPaths,
} from "@/lib/pr-ui-guard"
import { ForgeError, forgeFor, type Forge, type ForgeRepo } from "@/lib/ecode/forge"
import {
  claimPrOpen,
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
} from "@/lib/integrations/pr-sync"
import {
  isLivePrState,
  loadGuardDefaultBranches,
  stackStopBranches,
} from "@/lib/pr-merge-guard"
import { prLinks } from "@exp/domain-contract/issue-lifecycle"
import { patchPullDescription } from "@/lib/trpc/pr-update"
import {
  effectiveDefaultBranch,
  loadRepository,
  resolveGatedRepoToken,
} from "@/lib/trpc/repositories"

export type { PullRequest } from "@/db/schema"

// EXP-1291: THE pull-request API. A chat (root session) has n PRs (a tree,
// a stack or independent ones), each a `pull_requests` row closing the
// issues in `pull_request_issues`; every PR body is the chat's ONE Guide
// (`prBodyFromGuide`, re-rendered by `syncPullRequestBodies`). MCP's pr_*
// tools and the `pullRequests` tRPC router both call these; the state writes
// (automation, issue events, the chat's event cards, notifications) live in
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

/** #EXP-1614: the repo's forge, acting with the gated `token`. */
async function forgeOf(repo: Repo, token: string): Promise<{ forge: Forge; at: ForgeRepo }> {
  const at: ForgeRepo = {
    id: repo.id,
    fullName: repo.fullName,
    installationId: repo.installationId,
    ...(repo.provider ? { provider: repo.provider } : {}),
    token,
  }
  return { forge: await forgeFor(at), at }
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

/** EXP-1315 → S-3: a chat's PR that changes UI needs a labelled
 *  picture per touched platform, or `notRun` naming it. A failed compare
 *  never blocks the PR: it is logged and the guard skipped. */
async function assertUiPictures(args: {
  repo: string
  base: string
  head: string
  token: string
  pictures: { label: string; attachmentId: string }[]
  notRun: string | null
}): Promise<void> {
  let paths: string[]
  try {
    paths = await compareChangedPaths(args)
  } catch (err) {
    console.warn(`[pr_open] UI picture guard skipped for ${args.repo} ${args.base}...${args.head}:`, err)
    return
  }
  const missing = missingPlatformPictures(uiPlatformsOfPaths(paths), args.pictures, args.notRun)
  if (missing.length > 0) fail(`BAD_REQUEST`, missingPicturesMessage(missing))
}

async function loadRoot(rootId: string): Promise<{ id: string; teamId: string } | null> {
  const [row] = await db
    .select({ id: sessions.id, teamId: sessions.teamId })
    .from(sessions)
    .where(eq(sessions.id, rootId))
    .limit(1)
  return row ?? null
}

/** #NEW-20: whether `openerId` is `callerId` or a session it continues. */
async function isSameLogicalSession(rootId: string, callerId: string, openerId: string): Promise<boolean> {
  if (callerId === openerId) return true
  const tree = await db
    .select({
      id: sessions.id,
      parentId: sessions.parentId,
      continuedFromId: sessions.continuedFromId,
      status: sessions.status,
      branch: sessions.branch,
    })
    .from(sessions)
    .where(eq(sessions.rootId, rootId))
  return logicalSessionIds(callerId, tree).includes(openerId)
}

/** The footer link of every PR body: the chat (root session). */
async function chatUrlForTeam(teamId: string, rootId: string): Promise<string | null> {
  const [team] = await db.select({ slug: teams.slug }).from(teams).where(eq(teams.id, teamId)).limit(1)
  return team ? chatUrlFor(team.slug, rootId) : null
}

/** #EXP-1674: a PR body leaves the app, so its `~/x` links go absolute. */
async function withAbsoluteLinks(teamId: string, body: string): Promise<string> {
  if (!body.includes(`](~/`)) return body
  const [team] = await db.select({ slug: teams.slug }).from(teams).where(eq(teams.id, teamId)).limit(1)
  return team ? expandTeamLinksInMarkdown(body, team.slug, appBaseUrl()) : body
}

interface IssueForPr {
  id: string
  identifier: string
  title: string
  teamId: string
  boardRepositoryId: string | null
  boardDefaultBranch: string | null
}

/** EXP-1706: the issues a PR names (branch, title prefix, closing body
 *  lines) by identifier, in `teamId`. */
async function loadNamedIssuesForPr(teamId: string, identifiers: string[]): Promise<IssueForPr[]> {
  if (identifiers.length === 0) return []
  const rows = await issueForPrSelect().where(
    and(eq(issues.teamId, teamId), inArray(issues.identifier, identifiers), boardVisible(), issueVisible())
  )
  return identifiers.flatMap((identifier) => rows.filter((row) => row.identifier === identifier))
}

async function loadIssuesForPr(ids: string[]): Promise<IssueForPr[]> {
  if (ids.length === 0) return []
  return issueForPrSelect().where(and(inArray(issues.id, ids), boardVisible(), issueVisible()))
}

function issueForPrSelect() {
  return db
    .select({
      id: issues.id,
      identifier: issues.identifier,
      title: issues.title,
      teamId: issues.teamId,
      boardRepositoryId: boards.repositoryId,
      boardDefaultBranch: boards.defaultBranch,
    })
    .from(issues)
    .innerJoin(boards, eq(boards.id, issues.boardId))
}

/** Create the PR, or adopt the one the forge already holds open on `head`
 *  (FEED-59: GitHub's 422 only fires for the same head AND base, so the
 *  lookup runs first and ignores the base). */
async function createOrAdoptOnForge(
  forge: Forge,
  at: ForgeRepo,
  opts: {
    head: string
    base: string
    title: string
    body: string
    draft?: boolean
  }
): Promise<{ url: string; number: number; base: string; adopted: boolean }> {
  const lookup = async (base?: string) => {
    try {
      return await forge.findOpenByHead(at, opts.head, base)
    } catch {
      return null
    }
  }
  const open = await lookup()
  if (open) return { url: open.url, number: open.number, base: open.baseRef || opts.base, adopted: true }
  try {
    const created = await forge.openPullRequest(at, opts)
    return { ...created, base: opts.base, adopted: false }
  } catch (e) {
    if (!(e instanceof ForgeError) || e.reason !== `exists`) {
      return fail(`BAD_GATEWAY`, e instanceof Error ? e.message : String(e))
    }
    const raced = await lookup(opts.base)
    if (!raced) return fail(`BAD_GATEWAY`, e.message)
    return { url: raced.url, number: raced.number, base: raced.baseRef || opts.base, adopted: true }
  }
}

/**
 * Open a PR (the MCP `pr_open`). The caller session (X-Exp-Session-Id)
 * gives the defaults: its repository and branch, and its share of its
 * chat's issues when `issueIds` is omitted (EXP-1334, `prDefaultIssueIds`).
 * EXP-1706 (issue-lifecycle.json `links`): the head branch's ident, the
 * title prefix's idents and the body's closing lines always link too.
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
}): Promise<{
  pullRequestId: string
  url: string
  number: number
  base: string
  head: string
  /** EXP-1377: the issues this PR links (they move on open and merge). */
  issues: string[]
}> {
  const caller = args.callerSessionId
    ? await loadSessionForUser(args.userId, args.callerSessionId)
    : null
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
  // chat's defaults are narrowed to this PR's repository.
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
  const named = await loadNamedIssuesForPr(
    repo.teamId,
    prLinks({ issueIds: [], branch: head!, title: args.title, body: args.body ?? ``, share: [], parents: [] }).link
  )
  const linked = [...candidates, ...named]
    .filter(belongs)
    .filter((issue, index, all) => all.findIndex((other) => other.id === issue.id) === index)
  const linkedIdentifiers = linked.map((issue) => issue.identifier)
  const token = await repoToken(repo)
  const { forge, at } = await forgeOf(repo, token)
  const existing = await loadLivePullRequestByHead(repo.id, head!)
  // #NEW-20: a head that only DEFAULTED to the session's recorded branch never
  // takes over another session's PR (its title, links and body); naming
  // `head` is how a session says it means that PR.
  if (
    existing?.sessionId &&
    caller &&
    !args.head &&
    !(await isSameLogicalSession(caller.rootId, caller.id, existing.sessionId))
  ) {
    fail(
      `PRECONDITION_FAILED`,
      `Your session's branch ${head} is the head of PR #${existing.number}, opened by another session. Pass 'head': the branch you pushed.`
    )
  }
  // S-3: `notRun` lives on the PR row (`pull_requests.not_run`), so every
  // later body render and the next pr_open keep it.
  const notRun = args.notRun?.replace(/\s+/g, ` `).trim() || existing?.notRun || null
  const chatUrl = root ? await chatUrlForTeam(repo.teamId, root.id) : null
  // S-3: the PR's pictures = the opener's (its logical session and
  // subtree); the body = a server summary + those pictures.
  const pictures = root
    ? await loadPrPictures(root.id, caller?.id ?? null, {
        pullRequestId: existing?.id ?? null,
        head,
        repositoryId: repo.id,
      })
    : []
  const pictureBody = async (base: string) =>
    prBodyFromPictures({
      title: args.title,
      issues: linked.map((issue) => ({ identifier: issue.identifier, title: issue.title })),
      commits: await compareCommitMessages({ repo: repo.fullName, base, head: head!, token }),
      pictures,
      notRun,
      resultsUrl: chatUrl,
    })
  const keepNotRun = async (pullRequestId: string) => {
    if (args.notRun === undefined) return
    await db.update(pullRequests).set({ notRun, updatedAt: new Date() }).where(eq(pullRequests.id, pullRequestId))
  }
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
      ...(explicitIds ? { reviveIssueIds: explicitIds } : {}),
      ...actor,
    })

  if (existing) {
    // #NEW-20: a PR's body comes from its own chat: another chat's
    // session may update it, but its pictures never replace that body.
    const ownChat = root && (!existing.rootSessionId || existing.rootSessionId === root.id)
    const body = args.body ?? (ownChat ? await pictureBody(existing.base) : undefined)
    await patchPullDescription({
      repoFullName: repo.fullName,
      prNumber: existing.number,
      token,
      fields: {
        title: args.title,
        ...(body !== undefined ? { body: (await withAbsoluteLinks(repo.teamId, body)).slice(0, PR_BODY_MAX) } : {}),
      },
    })
    if (args.draft !== undefined && args.draft !== (existing.state === `draft`)) {
      await forge.setDraft(at, { number: existing.number, draft: args.draft })
      await applyPullRequestDraft({ pullRequestId: existing.id, draft: args.draft })
    }
    await record({ url: existing.url, number: existing.number, base: existing.base, title: args.title })
    await keepNotRun(existing.id)
    return { pullRequestId: existing.id, url: existing.url, number: existing.number, base: existing.base, head: head!, issues: linkedIdentifiers }
  }

  const defaults = await loadGuardDefaultBranches(db, { teamId: repo.teamId, repoFullName: repo.fullName })
  const defaultBranch =
    linked.find((issue) => issue.boardDefaultBranch)?.boardDefaultBranch ?? effectiveDefaultBranch(repo)
  const base =
    args.base ??
    (
      await forge.inferBase(at, {
        head: head!,
        defaultBranch,
        candidates: (await livePullRequestsOfRepo(repo.id))
          .filter((pr) => pr.head !== head)
          .map((pr) => ({ number: pr.number, branch: pr.head })),
        stopBranches: stackStopBranches(defaults),
      })
    ).base
  if (root) await assertUiPictures({ repo: repo.fullName, base, head: head!, token, pictures, notRun })
  const body = await withAbsoluteLinks(repo.teamId, args.body ?? (root ? await pictureBody(base) : ``))

  // EXP-494: the initiator is claimed BEFORE the GitHub call, so the
  // `opened` webhook (which beats our own write) fires attributed.
  claimPrOpen(repo.fullName, head!, { userId: args.userId, viaAgent: Boolean(caller) })
  let created: Awaited<ReturnType<typeof createOrAdoptOnForge>>
  try {
    created = await createOrAdoptOnForge(forge, at, {
      head: head!,
      base,
      title: args.title,
      body: body.slice(0, PR_BODY_MAX),
      ...(args.draft ? { draft: true } : {}),
    })
  } catch (e) {
    releasePrOpenClaim(repo.fullName, head!)
    throw e
  }
  // No `opened` webhook follows an adopted PR to consume the claim.
  if (created.adopted) releasePrOpenClaim(repo.fullName, head!)

  const recorded = await record({ url: created.url, number: created.number, base: created.base, title: args.title })
  await keepNotRun(recorded.pullRequest.id)
  return {
    pullRequestId: recorded.pullRequest.id,
    url: created.url,
    number: created.number,
    base: created.base,
    head: head!,
    issues: linkedIdentifiers,
  }
}

/**
 * FEED-111: the issue's PR among its live linked ones. A chat-level PR
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
      .where(and(eq(issues.id, args.issueId), issueVisible()))
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
  const { forge, at } = await forgeOf(repo, await repoToken(repo))
  let pull: Awaited<ReturnType<Forge[`readPullRequest`]>>
  try {
    pull = await forge.readPullRequest(at, number)
  } catch {
    return null
  }
  if (!pull) return null
  const recorded = await recordPullRequestOpened({
    teamId: repo.teamId,
    rootSessionId: null,
    sessionId: null,
    repositoryId: repo.id,
    repoFullName: repo.fullName,
    number,
    url: pull.url ?? `https://github.com/${repo.fullName}/pull/${number}`,
    title: pull.title || null,
    head: pull.headRef ?? ``,
    base: pull.baseRef ?? ``,
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
  const { forge, at } = await forgeOf(repo, await repoToken(repo))
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
    (await forge.defaultBranch(at))
  if (!base) return fail(`PRECONDITION_FAILED`, `Could not resolve the default branch of ${repo.fullName}`)
  try {
    await forge.retarget(at, { number: pr.number, base }).catch(async (err: unknown) => {
      // #EXP-1711: GitHub refuses a stack member a new base: this retarget
      // is an explicit ask to leave the line, so leave the stack and retry.
      if (!(err instanceof ForgeError) || err.status !== 422 || !/stack/i.test(err.message)) throw err
      await forge.leaveNativeStack(at, pr.number)
      await forge.retarget(at, { number: pr.number, base })
    })
  } catch (err) {
    if (err instanceof ForgeError) {
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
 *  chat PR's body follows the Guide again on the next Guide write. */
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
      fields: {
        title: args.title,
        body: args.body === undefined ? undefined : await withAbsoluteLinks(repo.teamId, args.body),
      },
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
      const { forge, at } = await forgeOf(repo, token)
      await forge.setDraft(at, { number: pr.number, draft: args.draft })
    } catch (err) {
      fail(`BAD_GATEWAY`, `GitHub refused the draft change: ${err instanceof Error ? err.message : String(err)}`)
    }
    await applyPullRequestDraft({ pullRequestId: pr.id, draft: args.draft })
  }
}

/** Detach one issue from a PR (a mislinked `pull_request_issues` row), in
 *  any PR state. EXP-1706: the row stays as a REFERENCE (`closes = false`):
 *  the PR never moves the issue again and clients see it unlinked (the
 *  shape hides it), but the link's history survives for audits. The issue's
 *  status stays a human decision; only rows inside the PR's team. A later
 *  `pr open` naming it in `issueIds` makes it closing again. */
export async function unlinkPullRequestIssue(args: {
  userId: string
  pullRequestId: string
  issueId: string
}): Promise<{ unlinked: boolean }> {
  const pr = await resolvePullRequest({ userId: args.userId, pullRequestId: args.pullRequestId })
  const unlinked = await db
    .update(pullRequestIssues)
    .set({ closes: false, updatedAt: new Date() })
    .where(
      and(
        eq(pullRequestIssues.pullRequestId, pr.id),
        eq(pullRequestIssues.issueId, args.issueId),
        eq(pullRequestIssues.teamId, pr.teamId),
        eq(pullRequestIssues.closes, true)
      )
    )
    .returning({ issueId: pullRequestIssues.issueId })
  return { unlinked: unlinked.length > 0 }
}

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
  const { forge, at } = await forgeOf(repo, await repoToken(repo))
  try {
    await forge.close(at, { number: pr.number })
  } catch (err) {
    if (err instanceof ForgeError) {
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
  // The repo comes from the PR's URL (no PR row without one); the diff
  // view's read is cached per PR, so no token is resolved here.
  const fullName = repoFromPrUrl(pr.url)
  if (!fullName) return { files: [] }
  const at: ForgeRepo = { id: pr.repositoryId, fullName }
  let files: Awaited<ReturnType<Forge[`files`]>>
  try {
    files = await (await forgeFor(at)).files(at, pr.number)
  } catch (err) {
    if (err instanceof TRPCError) throw err
    return fail(`BAD_GATEWAY`, err instanceof Error ? err.message : `Failed to load the changes`)
  }
  return {
    files: files.map((file) => ({
      path: file.path,
      additions: file.additions,
      deletions: file.deletions,
      patch: file.patch ?? null,
    })),
  }
}

// One chain per chat: overlapping picture uploads PATCH in call order, and
// each link re-reads the committed cards, so the last PATCH carries it.
const bodySyncChains = new Map<string, Promise<void>>()

/** S-3: re-render every LIVE PR body of a chat from its cards (each
 *  PR: its title, linked issues and commits, its opener's pictures,
 *  its notRun). Best effort: never throws. */
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
    const chatUrl = await chatUrlForTeam(root.teamId, root.id)
    const tokens = new Map<string, Promise<string | null>>()
    for (const pr of prs) {
      try {
        if (!tokens.has(pr.repositoryId)) {
          tokens.set(
            pr.repositoryId,
            loadRepository(pr.repositoryId).then((repo) => resolveGatedRepoToken(repo))
          )
        }
        const token = await tokens.get(pr.repositoryId)!
        const repoFullName = repoFromPrUrl(pr.url)
        if (!token || !repoFullName) continue
        const pictureBody = prBodyFromPictures({
          title: pr.title ?? ``,
          issues: await linkedIssuesOfPullRequest(pr.id),
          commits: await compareCommitMessages({ repo: repoFullName, base: pr.base, head: pr.head, token }),
          pictures: await loadPrPictures(rootId, pr.sessionId, {
            pullRequestId: pr.id,
            head: pr.head,
            repositoryId: pr.repositoryId,
          }),
          notRun: pr.notRun,
          resultsUrl: chatUrl,
        })
        const body = await withAbsoluteLinks(root.teamId, pictureBody)
        await patchPullDescription({ repoFullName, prNumber: pr.number, token, fields: { body } })
      } catch (err) {
        console.warn(`[pull-requests] body sync of ${pr.url} failed`, err)
      }
    }
  } catch (err) {
    console.warn(`[pull-requests] body sync of chat ${rootId} failed`, err)
  }
}

/** Test seam: chats with a body sync in flight. */
export function pendingPullRequestBodySyncs(): number {
  return bodySyncChains.size
}
