import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { contract } from "@exp/domain-contract"
import {
  actionInputsSchema,
  actionTriggersSchema,
  CATEGORY_ANCHOR,
  customizableStatusCategoryValues,
  dateOnlySchema,
  DEFAULT_ACCENT_COLOR,
  hexColorSchema,
  issueEstimateSchema,
  issueEstimationValues,
  MAX_ISSUE_DESCRIPTION,
  MAX_MCP_SERVER_NAME,
  MAX_START_PROMPT,
  SESSION_RESULT_TEXT_MAX,
  SESSION_RESULTS_MAX,
  SESSION_RESULT_REPORT_MAX,
  SESSION_RESULTS_REPORT_TOTAL_MAX,
  SESSION_RESULT_CAPTION_MAX,
  SESSION_RESULT_FILE_PATH_MAX,
  SESSION_RESULT_FILES_MAX,
  SESSION_RESULTS_FILES_TOTAL_MAX,
  SESSION_RESULT_PR_URL_MAX,
  SESSION_SHOW_DEFAULT_TOPIC,
  UUID_RE,
} from "@exp/db-schema/domain"
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import {
  and,
  asc,
  desc,
  eq,
  gte,
  inArray,
  isNotNull,
  isNull,
  like,
  lte,
  notInArray,
  or,
  sql,
  type SQL,
} from "drizzle-orm"
import { db } from "@/db/connection"
import { openStackThrough, type StackMember } from "@/lib/pr-merge-guard"
import {
  actions,
  attachments,
  codingSessions,
  comments,
  devices,
  deviceWorktrees,
  issueLabels,
  issueRelations,
  issues,
  issueStatuses,
  labels,
  mcpServers,
  notifications,
  boards,
  sessionAttachments,
  users,
  teamInvites,
  teamMembers,
  teams,
} from "@/db/schema"
import {
  commentAudienceValues,
  issuePriorityValues,
  issueRelationTypeValues,
  issueStatusValues,
  issueStatusCategoryDisplayOrder,
  issueStatusCategoryValues,
  boardIconValues,
} from "@/lib/domain"
import { teamColumns } from "@/lib/team-columns"
import {
  builtinCreateAction,
  builtinFixConflictsAction,
  builtinTidyUpAction,
  hasOwnTidyUpAction,
  isBuiltinActionId,
} from "@/lib/builtin-actions"
import {
  assertTeamMember,
  getAttachmentTeamContext,
  getSessionAttachmentTeamContext,
  getIssueTeamContext,
  getBoardTeamId,
  getUserTeamIds,
  resolveTeamAccess,
} from "@/lib/team-membership"
import { boardVisible } from "@/lib/board-visibility"
import {
  canonicalizeRelation,
  insertRelationInTx,
  loadIssueRelations,
} from "@/lib/issue-relations"
import { findRelationCycle } from "@/lib/relation-cycles"
import { escapeLikePattern } from "@/lib/like-pattern"
import { takeStartFailure } from "@/lib/start-failures"
import { resolveIssueReference, retiredIdentifiers } from "@/lib/issue-resolver"
import {
  issueWireColumns,
  withTruncatedDescriptions,
} from "@/lib/issue-columns"
import { deleteObject, getObject, uploadObject } from "@/lib/storage"
import {
  buildAttachmentStorageKey,
  buildAttachmentUrl,
  canonicalizeContentType,
  getMaxUploadBytesForContentType,
  isAcceptedImageContentType,
  isInlineMediaContentType,
  maxFileUploadBytes,
  maxImageUploadBytes,
  sanitizeUploadFilename,
} from "@/lib/storage/issue-attachments"
import { getImageDimensions } from "@/lib/storage/image-dimensions"
import {
  getVideoMetadata,
  isProbeableVideoContentType,
} from "@/lib/storage/video-metadata"
import { mintAttachmentToken } from "@/lib/storage/attachment-token"
import { mintSessionResultToken } from "@/lib/storage/session-result-token"
import {
  cleanSessionResultFiles,
  isTextEntry,
  missingGuideFiles,
  removeSessionResults,
  upsertSessionResultText,
  resultsSummary,
} from "@/lib/session-result-writes"
import { loadGuideDiff } from "@/lib/session-guide-diff"
import { publishSessionResultPicture } from "@/lib/session-result-publish"
import {
  runHasReportBody,
  runPrBody,
  stampRunResultsPrUrl,
  syncRunPrBody,
} from "@/lib/run-pr-body"
import {
  ensureGithubStack,
  GitHubStackForkError,
  inferBaseBranch,
  type InferredBase,
} from "@/lib/pr-stacks"
import {
  loadGuardDefaultBranches,
  openChildPrs,
  stackStopBranches,
} from "@/lib/pr-merge-guard"
import { prepareSessionImageBytes } from "@/lib/storage/session-attachment-upload"
import { appBaseUrl } from "@/lib/notification-email-policy"
import { assertWithinStorageLimit } from "@/lib/billing"
import { appRouter } from "@/routes/api/trpc/$"
import type { Context } from "@/lib/trpc"
import {
  branchExists,
  createPullRequest,
  findOpenPullByHead,
  type OpenPullByHead,
  PullAlreadyExistsError,
} from "@/lib/integrations/github-pr"
import { resolveRepoInstallationTokenInfo } from "@/lib/integrations/github-app"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { applyPrLifecycleStatusInTx } from "@/lib/integrations/pr-sync"
import {
  fireAndForgetPrNotify,
  sendAgentMessage,
} from "@/lib/integrations/notifications"
import {
  claimPrOpen,
  noteAgentIssueActivity,
  releasePrOpenClaim,
} from "@/lib/integrations/pr-actor-claims"
import { composeDeviceList } from "@/lib/steer-devices"
import {
  ISSUE_SEARCH_SCAN_CAP,
  issueSearchMatchIds,
} from "@/lib/issue-search-sql"
import { buildRuntimeConfig } from "@/lib/runtime-config"
import { createAgentBugReport } from "@/lib/widget/agent-report"
import { TokenBucketLimiter } from "@/lib/widget/rate-limit"
import {
  invalidateOpenPulls,
  loadRepositoryByFullName,
  loadRepositoryForTeam,
} from "@/lib/trpc/repositories"
import { visibleDeviceRows } from "@/lib/trpc/devices"
import { endSessionByAgent } from "@/lib/coding-session-end"
import {
  priorOf,
  revertMergedOwnPr as revertOwnPr,
  stampMergedOwnPr as stampOwnPr,
} from "@/lib/sessions/merged-own-pr"
import { maybeMergeYoloTree } from "@/lib/yolo-tree-merge"
import { getSteerRelayConfig, relayPostInput } from "@/lib/steer"
import { readRunTranscript, TranscriptReadError } from "@/lib/steer-transcript"
import { projectTranscript } from "@/lib/steer-transcript-project"
import {
  formatChildQuestion,
  formatParentAnswer,
  formatStarterMessage,
  loadChildParentContext,
  resolveLiveParentSessionId,
  notifyParentOfChildEnd,
  PARENT_LIVE_STATUSES,
} from "@/lib/steer-child-messages"
import { err, ok } from "./helpers"
import { inlineImageForContext } from "./inline-image"
// EXP-988: the per-tool handler files. The registry owns the
// registration, the zod schema and the access checks; each file owns ONE
// tool's behaviour and is filled by its leaf (EXP-979 / EXP-929 / EXP-936)
// without touching this module.
import {
  countIssueAttachments,
  listIssueAttachments,
} from "./handlers/attachments-list"
import {
  assertAttachmentUploadAccess,
  finalizeSignedAttachmentUpload,
  mintSignedAttachmentUpload,
} from "./handlers/attachments-upload"
import { requestSessionCompaction } from "./handlers/sessions-compact"
import { deviceAccountLogin } from "./handlers/device-account-login"
import { ALWAYS_LOAD_META } from "./always-load"
import { mcpAppToolMeta, type McpAppView } from "./apps"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "./gates"
import type { McpUser } from "./server"
import {
  assertFullAccess,
  assertBoardGranted,
  assertTeamFullyGranted,
  assertTeamVisible,
  filterVisibleTeamIds,
  grantScopeFilter,
  isBoardGranted,
  isRowGranted,
  isTeamVisible,
  GRANT_MATCHES_NOTHING,
  type McpAccess,
} from "./scope"

// EXP-496: per-user bound on agent bug reports. In-process like the widget
// buckets (rate-limit.ts documents why that is fine for this deploy).
const agentBugReportLimiter = new TokenBucketLimiter({
  capacity: 3,
  refillPerHour: 10,
})
// EXP-801: per-sender bound on agent messages to teammates.
const agentMessageLimiter = new TokenBucketLimiter({
  capacity: 10,
  refillPerHour: 30,
})

function buildCtx(user: McpUser, request: Request): Context {
  const now = new Date()
  return {
    db,
    request,
    viaMcp: true,
    session: {
      user: {
        id: user.id,
        email: user.email,
        name: user.name,
        image: user.image,
        emailVerified: user.emailVerified,
        createdAt: user.createdAt,
        updatedAt: user.updatedAt,
      },
      session: {
        id: `mcp`,
        userId: user.id,
        token: `mcp`,
        expiresAt: new Date(now.getTime() + 60 * 60 * 1000),
        createdAt: now,
        updatedAt: now,
        ipAddress: null,
        userAgent: `mcp`,
      },
    },
  } as unknown as Context
}

function caller(user: McpUser, request: Request) {
  return appRouter.createCaller(buildCtx(user, request))
}

// Resolve a UUID or human identifier ("MET-12") to an issue UUID via the
// shared resolver (lib/issue-resolver.ts — EXP-707: one resolver, both
// layers, deterministic newest-wins), intersected with the connection's
// OAuth grant. The team-level access check still runs in the caller — this
// only maps the friendly identifier the coding agent knows to the row id.
async function resolveIssueId(
  idOrIdentifier: string,
  userId: string,
  access: McpAccess
): Promise<string> {
  if (UUID_RE.test(idOrIdentifier)) return idOrIdentifier
  let grantedBoardIds: string[] | undefined
  if (!access.full) {
    const teamIds = await getUserTeamIds(userId)
    const boardRows =
      teamIds.length > 0
        ? await db
            .select({ id: boards.id, teamId: boards.teamId })
            .from(boards)
            .where(and(inArray(boards.teamId, teamIds), boardVisible()))
        : []
    grantedBoardIds = boardRows
      .filter((r) => isBoardGranted(access, r.id, r.teamId))
      .map((r) => r.id)
  }
  return resolveIssueReference(userId, idOrIdentifier, { grantedBoardIds })
}

// Comment id → its issue's team/board context, for grant checks on
// comment edit/delete (authorship itself is enforced in the comments router).
async function getCommentIssueContext(commentId: string) {
  const [row] = await db
    .select({ issueId: comments.issueId })
    .from(comments)
    .where(eq(comments.id, commentId))
    .limit(1)
  if (!row) throw new Error(`Comment not found`)
  return getIssueTeamContext(row.issueId)
}

// Label id → its team (EXP-707: row mutations never require a derivable
// teamId — the MCP layer derives it for the router).
async function getLabelContext(id: string) {
  const [row] = await db
    .select({ teamId: labels.teamId })
    .from(labels)
    .where(eq(labels.id, id))
    .limit(1)
  if (!row) throw new Error(`Label not found`)
  return row
}

// Status id → its team (same derivation rule as labels).
async function getStatusContext(id: string) {
  const [row] = await db
    .select({ teamId: issueStatuses.teamId })
    .from(issueStatuses)
    .where(eq(issueStatuses.id, id))
    .limit(1)
  if (!row) throw new Error(`Status not found`)
  return row
}

// Action id → its team, for grant checks on update/delete.
async function getActionContext(id: string) {
  const [row] = await db
    .select({ teamId: actions.teamId })
    .from(actions)
    .where(eq(actions.id, id))
    .limit(1)
  if (!row) throw new Error(`Action not found`)
  return row
}

const REUSED_PR_NOTE = (head: string) =>
  `${head} already had an open PR: linked to it (title and base unchanged; its body follows your report when you have one).`

// FEED-59: open the PR, or hand back the one already OPEN on `head` (a
// multi-issue run that implemented more issues on its branch) so the caller links the
// given issues to it. `reusedBase` = that PR's real base, null for a new PR.
// The lookup runs FIRST and ignores the base: GitHub's 422 only fires for the
// same head AND base, so a different base would silently open a second PR
// from the same head. A failed pre-lookup just falls through to the create;
// the 422 catch stays as the race fallback.
/**
 * EXP-1248: after `pr_open`, a PR whose base is another OPEN PR's head joins
 * that PR's GitHub stack (creating the line bottom→top when there is none).
 * The lower PR is the issue row's PR when a same-team issue owns the branch,
 * else the open PR GitHub lists on that head. A base that is a default
 * branch is no stack, and neither is a FORK (another open PR already sits on
 * the lower one: a follow-up tree, never a linear stack), skipped silently.
 * Returns the stack's `{number, position, size}` or null.
 */
async function joinGithubStack(opts: {
  repo: string
  token: string
  teamId: string
  defaultBranch: string
  base: string
  lowerPrNumber: number | null
  newPrNumber: number
}): Promise<{ number: number; position: number; size: number } | null> {
  const branches = await loadGuardDefaultBranches(db, {
    teamId: opts.teamId,
    repoFullName: opts.repo,
  })
  const stop = new Set([opts.defaultBranch, ...stackStopBranches(branches)])
  if (stop.has(opts.base)) return null
  let lowerPrNumber = opts.lowerPrNumber
  if (lowerPrNumber == null) {
    const lowerPull = await findOpenPullByHead(opts.repo, opts.base, opts.token)
    lowerPrNumber = lowerPull?.number ?? null
  }
  if (lowerPrNumber == null) return null
  // A sibling the team knows of already sits on the same base: a tree.
  const siblings = await openChildPrs(db, {
    teamId: opts.teamId,
    pattern: prUrlPattern(opts.repo),
    branch: opts.base,
  })
  const own = new Set([lowerPrNumber, opts.newPrNumber])
  for (const url of siblings.keys()) {
    const number = Number(url.match(/\/pull\/(\d+)/)?.[1])
    if (Number.isFinite(number) && !own.has(number)) return null
  }
  let stack: Awaited<ReturnType<typeof ensureGithubStack>>
  try {
    stack = await ensureGithubStack({
      repo: opts.repo,
      token: opts.token,
      lowerPrNumber,
      newPrNumber: opts.newPrNumber,
      stopBranches: [...stop],
    })
  } catch (e) {
    // GitHub's own view of a fork (a sibling opened outside Exponential).
    if (e instanceof GitHubStackForkError) return null
    throw new Error(
      `PR #${opts.newPrNumber} is open and linked, but joining the GitHub stack of #${lowerPrNumber} failed: ${e instanceof Error ? e.message : String(e)}`
    )
  }
  const position = stack.pulls.findIndex(
    (pr: { number: number }) => pr.number === opts.newPrNumber
  )
  return {
    number: stack.number,
    position: position >= 0 ? position + 1 : stack.pulls.length,
    size: stack.pulls.length,
  }
}

/**
 * EXP-1248 `pr_open` without a `base`: `inferBaseBranch` over the team's OWN
 * open PR branches (issue + run rows on this repo), skipping every branch
 * the repo is developed on.
 */
async function inferPrBase(opts: {
  repo: string
  token: string
  head: string
  defaultBranch: string
  teamIds: readonly string[]
}): Promise<InferredBase> {
  const pattern = prUrlPattern(opts.repo)
  const teamIds = [...new Set(opts.teamIds)]
  const issueRows = await db
    .select({ prNumber: issues.prNumber, branch: issues.branch })
    .from(issues)
    .where(
      and(
        inArray(issues.teamId, teamIds),
        eq(issues.prState, `open`),
        like(issues.prUrl, pattern)
      )
    )
  const runRows = await db
    .select({ prNumber: codingSessions.prNumber, branch: codingSessions.branch })
    .from(codingSessions)
    .where(
      and(
        inArray(codingSessions.teamId, teamIds),
        eq(codingSessions.prState, `open`),
        like(codingSessions.prUrl, pattern)
      )
    )
  const candidates = new Map<number, string>()
  for (const row of [...issueRows, ...runRows]) {
    if (row.prNumber != null && row.branch) candidates.set(row.prNumber, row.branch)
  }
  const stopBranches = (
    await Promise.all(
      teamIds.map((teamId) =>
        loadGuardDefaultBranches(db, { teamId, repoFullName: opts.repo })
      )
    )
  )
    .flatMap(stackStopBranches)
    .filter(Boolean)
  return inferBaseBranch({
    repo: opts.repo,
    token: opts.token,
    head: opts.head,
    defaultBranch: opts.defaultBranch,
    candidates: [...candidates].map(([number, branch]) => ({ number, branch })),
    stopBranches,
  })
}

async function openOrReusePull(
  opts: Parameters<typeof createPullRequest>[0]
): Promise<{ url: string; number: number; reusedBase: string | null }> {
  const lookup = async (base?: string) => {
    try {
      return await findOpenPullByHead(
        opts.repo,
        opts.head,
        opts.token,
        undefined,
        base
      )
    } catch {
      return null
    }
  }
  const reuse = (existing: OpenPullByHead) => ({
    url: existing.url,
    number: existing.number,
    reusedBase: existing.baseRef || opts.base,
  })
  const open = await lookup()
  if (open) return reuse(open)
  try {
    return { ...(await createPullRequest(opts)), reusedBase: null }
  } catch (e) {
    if (!(e instanceof PullAlreadyExistsError)) throw e
    // The PR that raced us in: same head AND base. GitHub's own 422 is the
    // error worth surfacing if this lookup fails or finds nothing.
    const existing = await lookup(opts.base)
    if (!existing) throw e
    return reuse(existing)
  }
}

// EXP-660: the coding_sessions projection the session tools return — the
// Electric shape allowlist (routes/api/shapes/coding-sessions.ts), camelCased,
// plus the linked issue's identifier/title. `host_user_id` and
// `merged_own_pr` are server-only and stay out; the board mirrors are
// WHERE-only. `acked_at` (EXP-701) is server-only too but IS returned here —
// orchestrating agents are exactly who needs the device's pickup ack.
// `owner/repo` out of a GitHub PR URL (EXP-734: the chore pr_merge own-PR
// test compares the run's stamped PR with the repo it is asked to merge in).
// A deliberate twin of the pr-sync/issues.ts helper: this module's tests
// mock pr-sync wholesale.
function repoFromPrUrl(prUrl: string): string | null {
  const match = prUrl.match(/github\.com\/([^/]+\/[^/]+)\/pull\/\d+/)
  return match ? match[1] : null
}

/** The LIKE pattern matching every PR url of one repository. */
function prUrlPattern(repoFullName: string): string {
  return `https://github.com/${escapeLikePattern(repoFullName)}/pull/%`
}

const sessionColumns = {
  id: codingSessions.id,
  issueId: codingSessions.issueId,
  issueIdentifier: issues.identifier,
  issueTitle: issues.title,
  teamId: codingSessions.teamId,
  boardId: codingSessions.boardId,
  actionId: codingSessions.actionId,
  actionName: codingSessions.actionName,
  startedReason: codingSessions.startedReason,
  automationId: codingSessions.automationId,
  userId: codingSessions.userId,
  deviceLabel: codingSessions.deviceLabel,
  deviceId: codingSessions.deviceId,
  agent: codingSessions.agent,
  status: codingSessions.status,
  branch: codingSessions.branch,
  prUrl: codingSessions.prUrl,
  prNumber: codingSessions.prNumber,
  prState: codingSessions.prState,
  endedBy: codingSessions.endedBy,
  resumedFromId: codingSessions.resumedFromId,
  parentSessionId: codingSessions.parentSessionId,
  needsInput: codingSessions.needsInput,
  // FEED-63: `status` is the PR/review state (in_review = PR open, the run
  // still live); `agentBusy` is the ONLY "working right now" signal (EXP-848,
  // device-written per turn edge), `agentCaption` what it is doing.
  agentBusy: codingSessions.agentBusy,
  agentCaption: codingSessions.agentCaption,
  // EXP-804: the agent's usage wall. Non-null on a row that still reads
  // `running` — the ONE state a polling orchestrator cannot infer.
  blocked: codingSessions.blocked,
  // EXP-1216: the question the run parked (ask_parent to a person or to an
  // MCP starter) and the run's own title — what a starter polling the row
  // needs to answer it without opening the app.
  pendingQuestion: codingSessions.pendingQuestion,
  agentTitle: codingSessions.agentTitle,
  ackedAt: codingSessions.ackedAt,
  startedAt: codingSessions.startedAt,
  endedAt: codingSessions.endedAt,
  createdAt: codingSessions.createdAt,
  updatedAt: codingSessions.updatedAt,
}

/**
 * EXP-679: the device creates a child's row, so its parent link is stamped
 * after `exponential_sessions_start`'s poll (when the row has none). History
 * only: the caller swallows a failure.
 */
async function stampChildOfRun(
  row: { id: unknown; parentSessionId?: unknown },
  sessionId: string
): Promise<void> {
  if (row.parentSessionId) return
  await db
    .update(codingSessions)
    .set({ parentSessionId: sessionId })
    .where(eq(codingSessions.id, row.id as string))
  row.parentSessionId = sessionId
}

// How long exponential_sessions_start waits for the device to report the
// row it created off the relay frame (heartbeat-class latency: the frame is
// pushed, the desktop registers the run via codingSessions.start).
const SESSION_START_POLL_MS = 10_000
const SESSION_START_POLL_STEP_MS = 500

// FEED-60/63: how long exponential_sessions_message waits for the target's
// agent to start a turn after the relay delivered the text (the device
// ticker writes `agent_busy` about 1s after a turn starts).
const MESSAGE_CONSUME_WAIT_MS = 6_000
const MESSAGE_CONSUME_STEP_MS = 500

/** FEED-57/46: what `exponential_sessions_start` answers when the device
 *  reported no run within the wait. */
export function noRunReportedMessage(deviceId: string): string {
  return `Device ${deviceId} took the start but reported no run within ${SESSION_START_POLL_MS / 1000}s. Check that it is online (exponential_devices_list), its app or daemon log, and whether a live run already holds the issue (exponential_sessions_list). Check exponential_sessions_list before starting again: the run may still appear.`
}

/** The row a BATCH start's device creates: issue-less and action-less in
 *  the batch's team, with no builtin name (FEED-57: a chat or action run is
 *  issue-less and action-less too) and a covered set naming the batch. Every
 *  device at or above the floor stamps `batch_issue_ids` on a batch start
 *  (EXP-876, desktop 0.14.43), so a NULL covered set never matches (compat
 *  cleanup round 25). */
export function batchStartRowMatch(
  teamIds: readonly string[],
  issueIds: readonly string[]
): SQL | undefined {
  return and(
    inArray(codingSessions.teamId, [...teamIds]),
    isNull(codingSessions.issueId),
    isNull(codingSessions.actionId),
    isNull(codingSessions.actionName),
    sql`${codingSessions.batchIssueIds} ?| ${sql.param([...issueIds])}::text[]`
  )
}

function sleep(ms: number) {
  return new Promise<void>((resolve) => setTimeout(resolve, ms))
}

// EXP-1216: sessions_get({waitForIdle}) re-reads the row this often, for at
// most SESSION_WAIT_MAX_S (well inside Bun.serve's 255s idleTimeout,
// server-bun.ts).
const SESSION_WAIT_POLL_MS = 2_000
const SESSION_WAIT_DEFAULT_S = 60
const SESSION_WAIT_MAX_S = 120
// A run that reads idle on the FIRST read may simply not have picked up the
// message the starter just sent (relay → device → turn start → setAgentBusy):
// wait this long for the turn to start before calling it settled.
const SESSION_WAIT_GRACE_MS = 10_000

/** EXP-1216: the run needs nobody's patience any more — its turn ended
 *  (`agentBusy` false, the ONE working signal), it asked, it hit a wall, or
 *  it ended. */
function sessionSettled(row: {
  status?: string | null
  agentBusy?: boolean | null
  needsInput?: boolean | null
  blocked?: unknown
}): boolean {
  return (
    row.status === `ended` ||
    !row.agentBusy ||
    row.needsInput === true ||
    (row.blocked !== null && row.blocked !== undefined)
  )
}

/** EXP-1216: idle but nothing else to report — the state a freshly messaged
 *  run sits in until its device starts the turn. */
function sessionIdleButLive(row: {
  status?: string | null
  agentBusy?: boolean | null
  needsInput?: boolean | null
  blocked?: unknown
}): boolean {
  return (
    row.status !== `ended` &&
    !row.agentBusy &&
    row.needsInput !== true &&
    (row.blocked === null || row.blocked === undefined)
  )
}

function rowTime(value: unknown): number | null {
  if (value === null || value === undefined) return null
  const time = new Date(value as string | number | Date).getTime()
  return Number.isNaN(time) ? null : time
}

/** FEED-63: the device's reported launch failure for this start, as the
 *  tool's error text; null while none was reported. */
function startFailureMessage(
  started: { startId?: string } | undefined,
  deviceId: string,
  userId: string
): string | null {
  if (!started?.startId) return null
  const failure = takeStartFailure(started.startId, userId)
  return failure
    ? `Device ${deviceId} could not start the run: ${failure.reason}`
    : null
}

/** FEED-60: after a delivered message, wait for the run's agent to start a
 *  turn (`agent_busy` flips true). False = the text reached the device but
 *  nothing picked it up within MESSAGE_CONSUME_WAIT_MS. */
async function waitForAgentTurn(sessionId: string): Promise<boolean> {
  const deadline = Date.now() + MESSAGE_CONSUME_WAIT_MS
  for (;;) {
    await sleep(MESSAGE_CONSUME_STEP_MS)
    const [row] = await db
      .select({ agentBusy: codingSessions.agentBusy })
      .from(codingSessions)
      .where(eq(codingSessions.id, sessionId))
      .limit(1)
    if (row?.agentBusy) return true
    if (Date.now() >= deadline) return false
  }
}

/** FEED-63: a killed run whose worktree held uncommitted work, as the
 *  device last reported it (`device_worktrees`). Best-effort: null on any
 *  miss or failure, never failing the kill. */
async function killedWorktreeNote(session: {
  deviceId?: string | null
  branch?: string | null
  userId?: string | null
  hostUserId?: string | null
}): Promise<string | null> {
  const { deviceId, branch } = session
  const ownerId = session.hostUserId ?? session.userId
  if (!deviceId || !branch || !ownerId) return null
  try {
    // coding_sessions.device_id is the device's own id string; the devices
    // row is keyed (owner, device_id) and device_worktrees hangs off its id.
    const [worktree] = await db
      .select({
        dirty: deviceWorktrees.dirty,
        reportedAt: deviceWorktrees.reportedAt,
      })
      .from(deviceWorktrees)
      .innerJoin(devices, eq(devices.id, deviceWorktrees.deviceRowId))
      .where(
        and(
          eq(devices.userId, ownerId),
          eq(devices.deviceId, deviceId),
          eq(deviceWorktrees.branch, branch)
        )
      )
      .orderBy(desc(deviceWorktrees.reportedAt))
      .limit(1)
    if (!worktree) return null
    if (worktree.dirty !== `tracked` && worktree.dirty !== `untracked`) {
      return null
    }
    return `The run's worktree on its device reported uncommitted changes (${worktree.dirty}, as of ${new Date(worktree.reportedAt).toISOString()}); the device saves them as a WIP commit on ${branch} when it tears the run down.`
  } catch {
    return null
  }
}

const codingAgentValues = contract.codingAgent.values as [string, ...string[]]

// EXP-704: a text-ish attachment at or under this size ALSO comes back inline
// in attachments_get's JSON payload (anything bigger — or any other type —
// rides only the signed downloadUrl, which has no size cap).
const MAX_INLINE_TEXT_BYTES = 32 * 1024

// EXP-705: every tool takes a STRICT object — an unknown key is an immediate
// "unrecognized key" error the agent can self-correct on, never a silent drop.
// Tool defs are deferred behind tool search, so agents guess param names from
// adjacent evidence; the server is the only party that always knows the shape.
// Serializes as additionalProperties:false (gated by api-conventions.test.ts).
const strictInput = <S extends z.ZodRawShape>(shape: S) => z.strictObject(shape)

// EXP-1251: the Guide tool's description (always-loaded bytes: the budget in
// context-budget.test.ts has ~10 bytes of headroom, keep it this short).
const GUIDE_TOOL_DESCRIPTION = `Your run's GUIDE = its PR body. Per topic: text (2-3 GFM sentences) + files (touched paths); 'Summary' first. Sections together cover every changed file; unassigned files land in Other changes; a listed file missing from the branch diff is reported back. prUrl = the topic's PR. label = picture: uploadUrl + curl (PNG/JPEG/WebP, 10 MB). remove drops a label, text (text: '') or topic.`
const MISSING_NOTE = `These listed files are not in the branch diff: fix the paths, push, or move them to the section they belong to.`

// FEED-25: every read declares MCP's `readOnlyHint`. Without it claude's
// plan mode (and any "ask before side effects" posture) raises a permission
// card for a plain `*_get`/`*_list` — the batch run behind FEED-25 sat on one
// such card for two hours. Gated by api-conventions.test.ts: reads carry it,
// nothing else does.
const READ_ONLY = { readOnlyHint: true } as const

// EXP-847: what "closed" means for `exponential_issues_list`'s default —
// the three terminal status CATEGORIES and the anchors they dual-write
// (CATEGORY_ANCHOR), so the filter works on custom statuses too.
const CLOSED_STATUS_CATEGORIES = [
  `completed`,
  `cancelled`,
  `duplicate`,
] as const satisfies ReadonlyArray<(typeof issueStatusCategoryValues)[number]>
const CLOSED_STATUS_ANCHORS = CLOSED_STATUS_CATEGORIES.map(
  (category) => CATEGORY_ANCHOR[category]
)

// EXP-707: the ONE pagination model — every *_list tool declares limit/offset
// (default 50, cap 200; gated by api-conventions.test.ts). Small-table tools
// slice after their existing filters rather than in SQL.
const pageInput = {
  limit: z.number().int().min(1).max(200).default(50),
  offset: z.number().int().min(0).default(0),
}
// Defaults repeated here because the zod defaults live in the schema layer —
// a caller invoking a handler directly (the test harness) bypasses them.
const page = <T>(rows: T[], limit?: number, offset?: number) =>
  rows.slice(offset ?? 0, (offset ?? 0) + (limit ?? 50))

// EXP-707: MCP reads ship the same pinned columns as the Electric shapes —
// never a bare select() that would leak the REV2-5/EXP-500 scoping mirrors
// (or any future server-only column) to agents unreviewed. Issues use the
// shared lib/issue-columns.ts mirror; these are the camelCase mirrors of the
// other shapes' allowlists (routes/api/shapes/*).
const boardWireColumns = {
  id: boards.id,
  teamId: boards.teamId,
  name: boards.name,
  slug: boards.slug,
  prefix: boards.prefix,
  color: boards.color,
  icon: boards.icon,
  repositoryId: boards.repositoryId,
  sortOrder: boards.sortOrder,
  createdAt: boards.createdAt,
  updatedAt: boards.updatedAt,
}
const commentWireColumns = {
  id: comments.id,
  issueId: comments.issueId,
  teamId: comments.teamId,
  boardId: comments.boardId,
  authorId: comments.authorId,
  parentId: comments.parentId,
  source: comments.source,
  body: comments.body,
  editedAt: comments.editedAt,
  createdAt: comments.createdAt,
  updatedAt: comments.updatedAt,
}
const notificationWireColumns = {
  id: notifications.id,
  userId: notifications.userId,
  issueId: notifications.issueId,
  teamId: notifications.teamId,
  type: notifications.type,
  title: notifications.title,
  body: notifications.body,
  readAt: notifications.readAt,
  pushedAt: notifications.pushedAt,
  createdAt: notifications.createdAt,
  updatedAt: notifications.updatedAt,
}

const issueStatusEnumSchema = z.enum(issueStatusValues)
const issuePriorityEnumSchema = z.enum(issuePriorityValues)
// EXP-353: keep the serialized tool context small — every schema below is part
// of the MCP client's system prompt. z.uuid()'s 155-char pattern and the
// 60-name icon enum each repeated across tools were ~10k chars of context, so
// both validate via refine (runtime-only, invisible to the JSON schema).
const uuidString = z.string().refine((v) => UUID_RE.test(v), `Expected a UUID`)
const boardIconEnumSchema = z
  .string()
  .refine(
    (v) => (boardIconValues as ReadonlyArray<string>).includes(v),
    `Unknown icon. Valid names: ${boardIconValues.join(`, `)}`
  )
  .transform((v) => v as (typeof boardIconValues)[number])
const DATE_ONLY_RE = /^\d{4}-\d{2}-\d{2}$/
const dateOnly = dateOnlySchema
const issueEstimate = issueEstimateSchema
// Same contract, no inline pattern (budget, see looseEnum below).
const dateOnlyLoose = z
  .string()
  .refine((v) => DATE_ONLY_RE.test(v), `Expected YYYY-MM-DD`)
// EXP-684: created/updated range bounds. Anything Date.parse accepts — a bare
// YYYY-MM-DD reads as midnight UTC, so "createdAfter: 2026-08-29" is the
// whole of that day onward.
const isoDateTime = z
  .string()
  .refine((v) => !Number.isNaN(Date.parse(v)), `Expected an ISO date or datetime`)
// Enum validated at runtime only (no inline JSON-schema enum) — the budget
// trick above, for a value list the same tool already spells out once.
const looseEnum = <T extends string>(values: ReadonlyArray<T>) =>
  z
    .string()
    .refine(
      (v) => (values as ReadonlyArray<string>).includes(v),
      `Expected one of: ${values.join(`, `)}`
    )
    .transform((v) => v as T)
const issueListSortFields = [`createdAt`, `updatedAt`, `priority`] as const
const issueListSort = z
  .string()
  .refine(
    (v) =>
      (issueListSortFields as ReadonlyArray<string>).includes(
        v.replace(/^-/, ``)
      ),
    `Expected createdAt, updatedAt or priority, optionally -prefixed`
  )
  .default(`-createdAt`)
// Sort rank for priority — the pg enum is declared none-first, which is not
// an order anyone wants to sort by.
const issuePriorityRank = sql<number>`case ${issues.priority} when 'urgent' then 4 when 'high' then 3 when 'medium' then 2 when 'low' then 1 else 0 end`

// EXP-1183: shared by exponential_issues_list and exponential_issues_show.
const issuesListInput = strictInput({
  boardId: uuidString.optional(),
  boardIds: z.array(uuidString).optional(),
  teamId: uuidString.optional(),
  status: z.array(looseEnum(issueStatusValues)).optional(),
  statusId: z.array(uuidString).optional(),
  statusCategory: z.array(looseEnum(issueStatusCategoryValues)).optional(),
  excludeStatus: z.array(looseEnum(issueStatusValues)).optional(),
  excludeStatusId: z.array(uuidString).optional(),
  excludeStatusCategory: z
    .array(looseEnum(issueStatusCategoryValues))
    .optional(),
  includeClosed: z.boolean().default(false),
  priority: z.array(looseEnum(issuePriorityValues)).optional(),
  assigneeId: z.string().nullable().optional(),
  source: looseEnum([`user`, `widget`] as const).optional(),
  labelIds: z.array(uuidString).optional(),
  labelMatch: z.enum([`any`, `all`]).optional(),
  unlabeled: z.boolean().optional(),
  hasComments: z.boolean().optional(),
  commentedBy: z.string().optional(),
  notCommentedBy: z.string().optional(),
  createdAfter: isoDateTime.optional(),
  createdBefore: isoDateTime.optional(),
  updatedAfter: isoDateTime.optional(),
  updatedBefore: isoDateTime.optional(),
  dueAfter: dateOnlyLoose.optional(),
  dueBefore: dateOnlyLoose.optional(),
  search: z
    .string()
    .refine((v) => v.length >= 1 && v.length <= 256, `1-256 chars`)
    .optional(),
  sort: issueListSort,
  limit: z.number().int().min(1).max(1000).default(50),
  offset: z.number().int().min(0).default(0),
})

export function registerExponentialTools(
  server: McpServer,
  user: McpUser,
  request: Request,
  access: McpAccess,
  // EXP-637: the coding_sessions row this MCP request runs inside, parsed
  // from the launcher-injected X-Exp-Session-Id header. Null for every caller
  // that is not a launched agent.
  sessionId: string | null = null,
  // EXP-660: which conditional tool families register for this caller
  // (resolved per request by the route). Defaults to everything so tests and
  // the context budget see the whole surface.
  gates: McpToolGates = ALL_MCP_TOOL_GATES
) {
  // EXP-1212: a coding run gets no MCP Apps binding (server.ts registers no
  // `ui://` resource for it either); the tools answer with their JSON.
  const appMeta = (view: McpAppView) =>
    sessionId ? undefined : mcpAppToolMeta(view)
  // The header session, but only when it is really THIS caller's run — owner
  // or host (EXP-432: a shared-device run is requester-owned while the
  // hosting daemon's key authenticates the agent). A foreign or vanished id
  // resolves to null and every caller degrades to its pre-EXP-637 behaviour;
  // the header is an identifier, never a credential.
  async function loadCallerSession(): Promise<{
    id: string
    teamId: string | null
    // EXP-639: what pr_merge needs to tell the run's OWN PR from any other —
    // its issue, the branch pr_open stamped on it, and the state to restore
    // when a merge it stamped for fails.
    issueId: string | null
    branch: string | null
    // EXP-734: the chore PR the run opened (null on issue/batch rows) — the
    // own-PR test.
    prUrl: string | null
    prNumber: number | null
    status: string
    needsInput: boolean
    mergedOwnPr: boolean
    // schedule | event | agent = an UNATTENDED run; null = person-started.
    startedReason: string | null
  } | null> {
    if (!sessionId) return null
    const [row] = await db
      .select({
        id: codingSessions.id,
        teamId: codingSessions.teamId,
        issueId: codingSessions.issueId,
        branch: codingSessions.branch,
        prUrl: codingSessions.prUrl,
        prNumber: codingSessions.prNumber,
        status: codingSessions.status,
        needsInput: codingSessions.needsInput,
        mergedOwnPr: codingSessions.mergedOwnPr,
        startedReason: codingSessions.startedReason,
        userId: codingSessions.userId,
        hostUserId: codingSessions.hostUserId,
      })
      .from(codingSessions)
      .where(eq(codingSessions.id, sessionId))
      .limit(1)
    if (!row) return null
    if (row.userId !== user.id && row.hostUserId !== user.id) return null
    return {
      id: row.id,
      teamId: row.teamId,
      issueId: row.issueId ?? null,
      branch: row.branch ?? null,
      prUrl: row.prUrl ?? null,
      prNumber: row.prNumber ?? null,
      status: row.status,
      needsInput: Boolean(row.needsInput),
      mergedOwnPr: Boolean(row.mergedOwnPr),
      startedReason: row.startedReason ?? null,
    }
  }

  // Park the run that just opened a PR in `in_review` and stamp the PR on it
  // (EXP-545/734: the row↔PR linkage clients tie their Merge shortcut to).
  // The EXP-637 session header names the EXACT row; a call without one parks
  // NOTHING (EXP-710). EVERY pr_open form stamps `branch/pr_url/pr_number/
  // pr_state` on the caller's live row, a reused open PR included, so the
  // run owns its PR: `applySessionPrState` (pr-sync.ts), the poller and
  // `codingSessions.mergePr` advance it off the exact url. Only a `running`
  // row flips to `in_review`; `needsInput` resets with the flip (EXP-531).
  // SLOP-3: only a run of the PR's own team — another team's PR on the row
  // would let its merge end this run; a mismatch skips, the PR stays open.
  async function parkSessionInReview(
    tx: Parameters<Parameters<typeof db.transaction>[0]>[0],
    opts: {
      callerSessionId: string | null
      prTeamId: string
      headBranch: string
      // EXP-1165: the base the PR was opened against (a reused PR's real
      // one), so the merge guards see an issue-less run PR's stack too.
      baseBranch: string
      pr: { url: string; number: number }
    }
  ): Promise<void> {
    if (!opts.callerSessionId) return
    const now = new Date()
    await tx
      .update(codingSessions)
      .set({ status: `in_review` as const, needsInput: false, updatedAt: now })
      .where(
        and(
          eq(codingSessions.id, opts.callerSessionId),
          eq(codingSessions.teamId, opts.prTeamId),
          eq(codingSessions.status, `running`)
        )
      )
    await tx
      .update(codingSessions)
      .set({
        branch: opts.headBranch,
        prUrl: opts.pr.url,
        prNumber: opts.pr.number,
        prState: `open` as const,
        prBaseBranch: opts.baseBranch,
        updatedAt: now,
      })
      .where(
        and(
          eq(codingSessions.id, opts.callerSessionId),
          eq(codingSessions.teamId, opts.prTeamId),
          inArray(codingSessions.status, [`running`, `in_review`])
        )
      )
  }

  // -----------------------------------------------------------------------
  // Teams
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_teams_list`,
    {
      annotations: READ_ONLY,
      description: `List teams the MCP user is a member of.`,
      inputSchema: strictInput({ ...pageInput }),
    },
    async ({ limit, offset }) => {
      try {
        const memberRows = await db
          .select({
            id: teams.id,
            name: teams.name,
            slug: teams.slug,
            iconUrl: teams.iconUrl,
            role: teamMembers.role,
            createdAt: teams.createdAt,
            updatedAt: teams.updatedAt,
          })
          .from(teams)
          .innerJoin(teamMembers, eq(teamMembers.teamId, teams.id))
          .where(eq(teamMembers.userId, user.id))
          .orderBy(asc(teams.name))

        // Membership-only, matching the sync semantics: a team appears
        // only once the user is a member.
        return ok(
          page(
            memberRows.filter((row) => isTeamVisible(access, row.id)),
            limit,
            offset
          )
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_teams_get`,
    {
      annotations: READ_ONLY,
      description: `Get a single team by id.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        assertTeamVisible(access, id)
        await resolveTeamAccess(user.id, id)
        // Projected, never `select()` — server-only columns (comp_tier) stay
        // behind the same allowlist the teams shape pins (REV2-67).
        const [row] = await db
          .select(teamColumns)
          .from(teams)
          .where(eq(teams.id, id))
          .limit(1)
        return ok(row)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Boards
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_boards_list`,
    {
      annotations: READ_ONLY,
      description: `List boards in a team, or across all teams the user belongs to.`,
      inputSchema: strictInput({
        teamId: uuidString.optional(),
        ...pageInput,
      }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        let allowedTeamIds: Array<string>
        if (teamId) {
          assertTeamVisible(access, teamId)
          await resolveTeamAccess(user.id, teamId)
          allowedTeamIds = [teamId]
        } else {
          allowedTeamIds = filterVisibleTeamIds(
            access,
            await getUserTeamIds(user.id)
          )
          if (allowedTeamIds.length === 0) return ok([])
        }

        const rows = await db
          .select(boardWireColumns)
          .from(boards)
          .where(and(inArray(boards.teamId, allowedTeamIds), boardVisible()))
          .orderBy(asc(boards.sortOrder), asc(boards.name))

        const filtered = rows.filter((row) =>
          isBoardGranted(access, row.id, row.teamId)
        )
        return ok(page(filtered, limit, offset))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_boards_get`,
    {
      annotations: READ_ONLY,
      description: `Get a single board by id.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        const board = await getBoardTeamId(id)
        assertBoardGranted(access, board.id, board.teamId)
        await resolveTeamAccess(user.id, board.teamId)
        const [row] = await db
          .select(boardWireColumns)
          .from(boards)
          .where(eq(boards.id, id))
          .limit(1)
        return ok(row)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_boards_create`,
    {
      description: `Create a board in a team (member; owner/admin to connect a new repo). The repository is optional. Coding features gate on repo presence. Pass repository.repositoryId (registry repo) or repository.fullName ("owner/name") to connect one inline; defaultBranch pins the branch this board's coding sessions branch from and its PRs target (omit = the repo's default). icon is a curated icon name.`,
      inputSchema: strictInput({
        teamId: uuidString,
        name: z.string().min(1).max(255),
        // Mirrors boards.create's floor (EXP-46): letter-led alphanumeric,
        // max 4, unique per team (REV-4) — identifiers stay `{PREFIX}-{number}`
        // referenceable and team-unique.
        prefix: z
          .string()
          .trim()
          .regex(
            /^[A-Za-z][A-Za-z0-9]{0,3}$/,
            `Prefix must be 1-4 letters or digits, starting with a letter`
          ),
        color: hexColorSchema.optional(),
        icon: boardIconEnumSchema.optional(),
        repository: z
          .union([
            z.object({ repositoryId: uuidString }),
            z.object({
              fullName: z
                .string()
                .min(1)
                .max(255)
                .regex(/^[^/\s]+\/[^/\s]+$/, `Expected "owner/name"`),
              defaultBranch: z.string().min(1).max(255).optional(),
              private: z.boolean().optional(),
              installationId: z.number().int().optional(),
            }),
          ])
          .optional(),
        defaultBranch: z.string().min(1).max(255).optional(),
      }),
    },
    async (input) => {
      try {
        // Creating a board needs the whole-team grant — a
        // single-board grant must not spawn siblings it can't see.
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).boards.create(input)
        return ok(result.board)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_boards_update`,
    {
      description: `Update a board's name, color, icon, or defaultBranch (the branch its coding sessions branch from and its PRs target; null = follow the repo's default).`,
      inputSchema: strictInput({
        id: uuidString,
        icon: boardIconEnumSchema.nullable().optional(),
        name: z.string().min(1).max(255).optional(),
        color: hexColorSchema.optional(),
        defaultBranch: z.string().min(1).max(255).nullable().optional(),
      }),
    },
    async ({ id, ...rest }) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(id)
          assertBoardGranted(access, board.id, board.teamId)
        }
        const result = await caller(user, request).boards.update({
          boardId: id,
          ...rest,
        })
        return ok(result.board)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Issues
  // -----------------------------------------------------------------------

  const listIssues = async ({
    boardId,
    boardIds,
    teamId,
    status,
    statusId,
    statusCategory,
    excludeStatus,
    excludeStatusId,
    excludeStatusCategory,
    includeClosed,
    priority,
    assigneeId,
    source,
    labelIds,
    labelMatch,
    unlabeled,
    hasComments,
    commentedBy,
    notCommentedBy,
    createdAfter,
    createdBefore,
    updatedAfter,
    updatedBefore,
    dueAfter,
    dueBefore,
    search,
    sort,
    limit,
    offset,
  }: z.output<typeof issuesListInput>) => {
    try {
      let allowedBoardIds: Array<string>

      // EXP-684: boardIds is the multi-board form of boardId — every named
      // board is access-checked exactly like the single one.
      const requestedBoardIds = boardId
        ? [boardId]
        : boardIds && boardIds.length > 0
          ? [...new Set(boardIds)]
          : null

      if (requestedBoardIds) {
        for (const id of requestedBoardIds) {
          const board = await getBoardTeamId(id)
          assertBoardGranted(access, board.id, board.teamId)
          await resolveTeamAccess(user.id, board.teamId)
        }
        allowedBoardIds = requestedBoardIds
      } else {
        let teamIds: Array<string>
        if (teamId) {
          assertTeamVisible(access, teamId)
          await resolveTeamAccess(user.id, teamId)
          teamIds = [teamId]
        } else {
          teamIds = filterVisibleTeamIds(
            access,
            await getUserTeamIds(user.id)
          )
        }
        if (teamIds.length === 0) return ok([])
        const boardRows = await db
          .select({ id: boards.id, teamId: boards.teamId })
          .from(boards)
          .where(
            and(inArray(boards.teamId, teamIds), boardVisible())
          )
        allowedBoardIds = boardRows
          .filter((r) => isBoardGranted(access, r.id, r.teamId))
          .map((r) => r.id)
      }

      if (allowedBoardIds.length === 0) return ok([])

      const conditions: Array<SQL> = [
        inArray(issues.boardId, allowedBoardIds),
      ]

      // Status: the builtin anchor enum, the precise per-team row, or the
      // row's category (a custom "Ideas" has no builtin key, so only the
      // latter two can name it). status_id is trigger-populated for every
      // writer (populate_issue_status_id), so the row filters key on it
      // alone; an exclude keeps the (theoretical) NULL row rather than
      // dropping it into nowhere.
      const statusIdsInCategories = (
        categories: Array<(typeof issueStatusCategoryValues)[number]>
      ) =>
        sql`(select ${issueStatuses.id} from ${issueStatuses} where ${inArray(issueStatuses.category, categories)})`
      if (status && status.length > 0) {
        conditions.push(inArray(issues.status, status))
      }
      if (statusId && statusId.length > 0) {
        conditions.push(inArray(issues.statusId, statusId))
      }
      if (statusCategory && statusCategory.length > 0) {
        conditions.push(
          sql`${issues.statusId} in ${statusIdsInCategories(statusCategory)}`
        )
      }
      if (excludeStatus && excludeStatus.length > 0) {
        conditions.push(notInArray(issues.status, excludeStatus))
      }
      if (excludeStatusId && excludeStatusId.length > 0) {
        conditions.push(
          or(
            isNull(issues.statusId),
            notInArray(issues.statusId, excludeStatusId)
          )!
        )
      }
      if (excludeStatusCategory && excludeStatusCategory.length > 0) {
        conditions.push(
          or(
            isNull(issues.statusId),
            sql`${issues.statusId} not in ${statusIdsInCategories(excludeStatusCategory)}`
          )!
        )
      }
      // EXP-847: a listing is about OPEN work. With no status filter of any
      // kind the closed categories drop out — `includeClosed: true` (or any
      // explicit status/statusId/statusCategory) asks for them back. The
      // predicate keys on the status ROW's category (customs included) and
      // falls back to the dual-written anchor for a (theoretical) NULL
      // status_id, so nothing is silently hidden or silently kept.
      const statusFiltered =
        (status && status.length > 0) ||
        (statusId && statusId.length > 0) ||
        (statusCategory && statusCategory.length > 0)
      if (!includeClosed && !statusFiltered) {
        conditions.push(
          or(
            sql`${issues.statusId} not in ${statusIdsInCategories([
              ...CLOSED_STATUS_CATEGORIES,
            ])}`,
            and(
              isNull(issues.statusId),
              notInArray(issues.status, [...CLOSED_STATUS_ANCHORS])
            )
          )!
        )
      }

      if (priority && priority.length > 0) {
        conditions.push(inArray(issues.priority, priority))
      }
      if (assigneeId === null) {
        conditions.push(isNull(issues.assigneeId))
      } else if (assigneeId !== undefined) {
        conditions.push(eq(issues.assigneeId, assigneeId))
      }
      // SLOP-4: widget-filed reports vs member-created issues.
      if (source) {
        conditions.push(eq(issues.source, source))
      }

      // Labels: any-of / all-of over issue_labels, plus the explicit
      // "no labels at all" the triage sweeps key on.
      const labelLink = sql`select 1 from ${issueLabels} where ${issueLabels.issueId} = ${issues.id}`
      if (unlabeled === true) {
        conditions.push(sql`not exists (${labelLink})`)
      } else if (unlabeled === false) {
        conditions.push(sql`exists (${labelLink})`)
      }
      if (labelIds && labelIds.length > 0) {
        const wanted = [...new Set(labelIds)]
        if (labelMatch === `all`) {
          conditions.push(
            sql`(select count(distinct ${issueLabels.labelId}) from ${issueLabels} where ${issueLabels.issueId} = ${issues.id} and ${inArray(issueLabels.labelId, wanted)}) = ${wanted.length}`
          )
        } else {
          conditions.push(
            sql`exists (${labelLink} and ${inArray(issueLabels.labelId, wanted)})`
          )
        }
      }

      // Comments: presence, and "has/hasn't this user already replied"
      // so a recurring run does not comment on the same issue twice.
      const commentLink = sql`select 1 from ${comments} where ${comments.issueId} = ${issues.id}`
      if (hasComments === true) {
        conditions.push(sql`exists (${commentLink})`)
      } else if (hasComments === false) {
        conditions.push(sql`not exists (${commentLink})`)
      }
      if (commentedBy) {
        conditions.push(
          sql`exists (${commentLink} and ${eq(comments.authorId, commentedBy)})`
        )
      }
      if (notCommentedBy) {
        conditions.push(
          sql`not exists (${commentLink} and ${eq(comments.authorId, notCommentedBy)})`
        )
      }

      if (createdAfter) {
        conditions.push(gte(issues.createdAt, new Date(createdAfter)))
      }
      if (createdBefore) {
        conditions.push(lte(issues.createdAt, new Date(createdBefore)))
      }
      if (updatedAfter) {
        conditions.push(gte(issues.updatedAt, new Date(updatedAfter)))
      }
      if (updatedBefore) {
        conditions.push(lte(issues.updatedAt, new Date(updatedBefore)))
      }
      if (dueAfter) conditions.push(gte(issues.dueDate, dueAfter))
      if (dueBefore) conditions.push(lte(issues.dueDate, dueBefore))
      // EXP-892: the same full-text + identifier predicate every client's
      // search box runs (lib/issue-search-sql.ts), over the boards this
      // call may see. Capped: unlike the team-scoped search box this spans
      // EVERY granted board, and a limit-1000 list must not scan them all.
      if (search) {
        conditions.push(
          sql`${issues.id} in (${issueSearchMatchIds(
            search,
            { boardIds: allowedBoardIds },
            { limit: ISSUE_SEARCH_SCAN_CAP }
          )})`
        )
      }

      const dir = sort.startsWith(`-`) ? desc : asc
      const sortField = sort.replace(/^-/, ``)
      const sortExpr =
        sortField === `updatedAt`
          ? issues.updatedAt
          : sortField === `priority`
            ? issuePriorityRank
            : issues.createdAt

      const rows = await db
        .select(issueWireColumns)
        .from(issues)
        .where(and(...conditions))
        // createdAt then id break ties so pages never overlap.
        .orderBy(dir(sortExpr), dir(issues.createdAt), dir(issues.id))
        .limit(limit)
        .offset(offset)

      // EXP-847: list rows carry a description HEAD, never whole bodies.
      return ok(withTruncatedDescriptions(rows))
    } catch (e) {
      return err(e)
    }
  }

  server.registerTool(
    `exponential_issues_list`,
    {
      annotations: READ_ONLY,
      // EXP-684: every filter a scheduled sweep needs server-side. The schema
      // is budget-trimmed (context-budget.test.ts): EXP-847 took the last two
      // inline value lists out too, so status/statusCategory, their exclude*
      // twins and priority all validate at runtime (the refusal names the
      // values; issues_create spells the status enum out).
      description: `List issues, OPEN only: completed/cancelled/duplicate need includeClosed or a status* filter. Descriptions cut at 200. statusId: exponential_statuses_list; exclude* invert. created*/updated*: ISO. sort: [-]createdAt|updatedAt|priority. search: text + identifier; assigneeId null = unassigned; source: user|widget.`,
      inputSchema: issuesListInput,
    },
    listIssues
  )

  // EXP-1183: the issue list as an MCP Apps view (lib/mcp/apps.ts) — its
  // own small tool because exponential_issues_list has no per-tool budget
  // left for the `_meta.ui` binding. Same query, same rows.
  server.registerTool(
    `exponential_issues_show`,
    {
      // The app's name in an MCP Apps host's catalog (OpenClaw's Apps page).
      title: `Exponential issues`,
      annotations: READ_ONLY,
      description: `Show issues as an interactive list where the client renders MCP Apps (OpenClaw, Claude, ChatGPT); elsewhere the same JSON as exponential_issues_list. OPEN only unless includeClosed.`,
      _meta: appMeta(`issues`),
      inputSchema: strictInput({
        boardId: uuidString.optional(),
        teamId: uuidString.optional(),
        assigneeId: z.string().nullable().optional(),
        search: z
          .string()
          .refine((v) => v.length >= 1 && v.length <= 256, `1-256 chars`)
          .optional(),
        includeClosed: z.boolean().default(false),
        limit: z.number().int().min(1).max(200).default(50),
      }),
    },
    (args) => listIssues(issuesListInput.parse({ ...args, sort: `-updatedAt` }))
  )

  server.registerTool(
    `exponential_issues_get`,
    {
      annotations: READ_ONLY,
      description: `Get a single issue by UUID or identifier (e.g. "MET-12"), including its label ids and latest comments (newest first, capped at 50; commentsLimit overrides).`,
      // EXP-1183: no MCP Apps view here — the always-loaded set has no room
      // for the `_meta.ui` binding (context-budget.test.ts); the issues view
      // opens an issue itself through the host's tools/call.
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        id: z.string().min(1),
        commentsLimit: z.number().int().min(0).max(200).optional(),
      }),
    },
    async ({ id: idInput, commentsLimit }) => {
      try {
        const id = await resolveIssueId(idInput, user.id, access)
        const ctxIssue = await getIssueTeamContext(id)
        assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        await resolveTeamAccess(user.id, ctxIssue.teamId)
        const [issue] = await db
          .select(issueWireColumns)
          .from(issues)
          .where(eq(issues.id, id))
          .limit(1)
        const labelRows = await db
          .select({ labelId: issueLabels.labelId })
          .from(issueLabels)
          .where(eq(issueLabels.issueId, id))
        const recentComments = await db
          .select({
            id: comments.id,
            authorId: comments.authorId,
            parentId: comments.parentId,
            source: comments.source,
            body: comments.body,
            createdAt: comments.createdAt,
            editedAt: comments.editedAt,
          })
          .from(comments)
          .where(eq(comments.issueId, id))
          .orderBy(desc(comments.createdAt))
          .limit(commentsLimit ?? 50)
        // EXP-736: both sides of the relation graph, folded to this issue's
        // point of view (direction + otherIdentifier). loadIssueRelations
        // already drops rows whose far board is trashed or archived; a
        // grant-confined token additionally sees only the boards its consent
        // named, so the far side is filtered here rather than leaking an
        // identifier from a board this connection was never granted.
        const relations = (await loadIssueRelations(db, id)).filter(
          (relation) =>
            isBoardGranted(access, relation.otherBoardId, relation.otherTeamId)
        )
        // EXP-1183: the issue's page in the app (the MCP Apps view's "Open").
        const [slugs] = await db
          .select({ teamSlug: teams.slug, boardSlug: boards.slug })
          .from(boards)
          .innerJoin(teams, eq(teams.id, boards.teamId))
          .where(eq(boards.id, ctxIssue.boardId))
          .limit(1)
        const origin = process.env.BETTER_AUTH_URL
          ? appBaseUrl()
          : new URL(request.url).origin
        return ok({
          ...issue,
          url: slugs
            ? `${origin}/t/${encodeURIComponent(slugs.teamSlug)}/boards/${encodeURIComponent(slugs.boardSlug)}/issues/${encodeURIComponent(issue.identifier)}`
            : null,
          labelIds: labelRows.map((r) => r.labelId),
          relations,
          recentComments,
          // EXP-988/EXP-979: how many files the issue carries, so an agent
          // knows whether exponential_attachments_list is worth a call.
          attachmentCount: await countIssueAttachments(id),
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_create`,
    {
      description: `Create a new issue in a board the MCP user has access to. Description must be plain text (no embedded images on creation). For a custom status pass statusId (not status); see exponential_statuses_list. parentId files it as a sub-issue of that issue.`,
      inputSchema: strictInput({
        boardId: uuidString,
        title: z.string().min(1).max(500),
        parentId: uuidString.optional(),
        status: issueStatusEnumSchema.optional(),
        statusId: uuidString.optional(),
        priority: issuePriorityEnumSchema.optional(),
        assigneeId: z.string().nullable().optional(),
        description: z
          .string()
          .max(MAX_ISSUE_DESCRIPTION)
          .nullable()
          .optional()
          .describe(`Plain GFM text; no embedded images on creation`),
        dueDate: dateOnly.nullable().optional(),
        estimate: issueEstimate.nullable().optional(),
        labelIds: z.array(uuidString).optional(),
      }),
    },
    async ({ description, ...rest }) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(rest.boardId)
          assertBoardGranted(access, board.id, board.teamId)
          if (rest.parentId) {
            const parent = await getIssueTeamContext(rest.parentId)
            assertBoardGranted(access, parent.boardId, parent.teamId)
          }
        }
        const result = await caller(user, request).issues.create({
          ...rest,
          description: description ? description : undefined,
        })
        // EXP-617: an issue filed mid-session has no coding_sessions row of
        // its own, so nothing else ties its later PR back to the human whose
        // agent wrote it. Exclusion-only (see noteAgentIssueActivity).
        noteAgentIssueActivity(result.issue.id, user.id)
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_update`,
    {
      description: `Update an issue's fields (by UUID or identifier, e.g. "MET-12"). Pass only the fields you want to change. For a custom status pass statusId (not status); see exponential_statuses_list.`,
      inputSchema: strictInput({
        id: z.string().min(1),
        title: z.string().min(1).max(500).optional(),
        status: issueStatusEnumSchema.optional(),
        statusId: uuidString.optional(),
        priority: issuePriorityEnumSchema.optional(),
        assigneeId: z.string().nullable().optional(),
        description: z
          .string()
          .max(MAX_ISSUE_DESCRIPTION)
          .nullable()
          .optional()
          .describe(`Plain GFM text; null clears`),
        dueDate: dateOnly.nullable().optional(),
        estimate: issueEstimate.nullable().optional(),
      }),
    },
    async ({ id: idOrIdentifier, ...rest }) => {
      try {
        const id = await resolveIssueId(idOrIdentifier, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).issues.update({
          id,
          ...rest,
        })
        noteAgentIssueActivity(id, user.id)
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_delete`,
    {
      description: `Permanently delete an issue (by UUID or identifier). Cascades to its labels, attachments, comments, and relations. Attachment storage objects are also removed.`,
      inputSchema: strictInput({ id: z.string().min(1) }),
    },
    async (rawInput) => {
      try {
        const input = {
          id: await resolveIssueId(rawInput.id, user.id, access),
        }
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(input.id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).issues.delete(input)
        return ok({ ok: true, id: input.id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Attachments
  // -----------------------------------------------------------------------

  // EXP-988/EXP-979: the issue's Files list. Access = attachments_get's rule
  // (grant on the issue's board + team membership); the query lives in the
  // handler file. Metadata only — bytes ride exponential_attachments_get.
  server.registerTool(
    `exponential_attachments_list`,
    {
      annotations: READ_ONLY,
      description: `List an issue's attachments (UUID or identifier), newest first: id, filename, contentType, sizeBytes, createdAt and the commentId when a file hangs on a comment. Metadata only; exponential_attachments_get fetches one file's bytes. Paged with limit/offset; total counts every row.`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      }),
    },
    async ({ issueId: issueIdInput, limit, offset }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const ctxIssue = await getIssueTeamContext(issueId)
        assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        await resolveTeamAccess(user.id, ctxIssue.teamId)
        return ok(await listIssueAttachments({ issueId, limit, offset }))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_attachments_get`,
    {
      annotations: READ_ONLY,
      description: `Fetch an attachment by id — every content type. Markdown embeds look like ![alt](/api/attachments/{id}); pass that {id}. Always returns metadata plus a short-lived signed downloadUrl: fetch it (curl/wget) into your working directory to read non-image files (xlsx, PDF, CSV, ...) with real tooling. Images additionally come back as inline image content, downscaled when large (the JSON's inline field reports what was sent; downloadUrl always has the original); small text files include their text inline.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        // Issue attachments first; steer images (EXP-702) live in the
        // server-only session_attachments table but
        // share the /api/attachments/{id} url shape, so an agent's fallback
        // fetch of a steered embed must resolve here too.
        const attachment = await getAttachmentTeamContext(id).then(
          (issueAttachment) => {
            assertBoardGranted(
              access,
              issueAttachment.boardId,
              issueAttachment.teamId
            )
            return issueAttachment
          },
          async (error: unknown) => {
            if (error instanceof TRPCError && error.code === `NOT_FOUND`) {
              const sessionAttachment =
                await getSessionAttachmentTeamContext(id)
              assertTeamFullyGranted(access, sessionAttachment.teamId)
              return sessionAttachment
            }
            throw error
          }
        )
        await resolveTeamAccess(user.id, attachment.teamId)

        // EXP-704: the token is minted only after the grant + membership
        // checks above, bound to this one attachment — the URL is a complete
        // credential the route accepts without a session, so any agent
        // (launcher or external MCP client) can download any content type
        // with no size cap and no base64 in context. The origin is
        // `appBaseUrl()` like every other outbound link: behind a TLS-
        // terminating proxy Bun sees plain HTTP, so the request origin mints
        // an `http://` URL that a plain `curl -o` (no `-L`) saves the
        // redirect body of. Falls back to the request origin only when
        // BETTER_AUTH_URL is unset (the caller reached us on this host, so
        // that URL is reachable too).
        const { token, expiresAt } = mintAttachmentToken(id, user.id)
        const origin = process.env.BETTER_AUTH_URL
          ? appBaseUrl()
          : new URL(request.url).origin
        const payload: Record<string, unknown> = {
          id,
          filename: attachment.filename,
          contentType: attachment.contentType,
          sizeBytes: attachment.sizeBytes,
          downloadUrl: `${origin}/api/attachments/${id}?token=${token}`,
          expiresAt: expiresAt.toISOString(),
        }
        if (`durationMs` in attachment && attachment.durationMs != null) {
          payload.durationMs = attachment.durationMs
        }

        const contentType = attachment.contentType
        // EXP-853 review: the inline copy goes through sharp, so classify
        // with the SAME accepted-image set the uploads use — a stored
        // `image/svg+xml` is an `image/` type sharp would hand to librsvg.
        const isImage = isAcceptedImageContentType(contentType)
        const isTextLike =
          contentType.startsWith(`text/`) ||
          contentType === `application/json` ||
          contentType.endsWith(`+json`) ||
          contentType === `application/csv`

        if (isTextLike && attachment.sizeBytes <= MAX_INLINE_TEXT_BYTES) {
          const object = await getObject(attachment.storageKey)
          if (!object?.Body) throw new Error(`Attachment object not found`)
          const bytes = await object.Body.transformToByteArray()
          payload.text = Buffer.from(bytes).toString(`utf8`)
        }

        if (isImage) {
          const object = await getObject(attachment.storageKey)
          if (!object?.Body) throw new Error(`Attachment object not found`)
          const bytes = await object.Body.transformToByteArray()
          // EXP-854: the inline copy is bounded (1280 px / 300 KB WebP) —
          // base64 of a raw 12 MP screenshot is tens of thousands of tokens
          // out of the window the run still needs for its work. The original
          // stays one `downloadUrl` fetch away. A null here means the bytes
          // could not be decoded (or sharp is unavailable): inline the
          // original rather than send no picture at all (EXP-511).
          const inlined = await inlineImageForContext(bytes, contentType)
          const inlineBytes = inlined?.data ?? bytes
          const inlineType = inlined?.mimeType ?? contentType
          payload.inline = {
            mimeType: inlineType,
            width: inlined?.width,
            height: inlined?.height,
            bytes: inlineBytes.byteLength,
            downscaled: inlined?.downscaled ?? false,
          }
          return {
            content: [
              {
                type: `image` as const,
                data: Buffer.from(inlineBytes).toString(`base64`),
                mimeType: inlineType,
              },
              {
                type: `text` as const,
                text: JSON.stringify(payload, null, 2),
              },
            ],
          }
        }

        return ok(payload)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Labels
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_labels_list`,
    {
      annotations: READ_ONLY,
      description: `List labels for a team.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        // Labels are team-level but issue workflows in a granted board
        // need them, so a visible (board-granted) team suffices to read.
        assertTeamVisible(access, teamId)
        await resolveTeamAccess(user.id, teamId)
        const rows = await db
          .select()
          .from(labels)
          .where(eq(labels.teamId, teamId))
          .orderBy(asc(labels.sortOrder), asc(labels.name))
          .limit(limit)
          .offset(offset)
        return ok(rows)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_labels_get`,
    {
      annotations: READ_ONLY,
      description: `Get a label by id (must be in a team the user belongs to).`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        const [label] = await db
          .select()
          .from(labels)
          .where(eq(labels.id, id))
          .limit(1)
        if (!label) return err(new Error(`Label not found`))
        assertTeamVisible(access, label.teamId)
        await resolveTeamAccess(user.id, label.teamId)
        return ok(label)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_labels_create`,
    {
      description: `Create a label in a team.`,
      inputSchema: strictInput({
        teamId: uuidString,
        name: z.string().min(1).max(255),
        color: hexColorSchema.default(DEFAULT_ACCENT_COLOR),
      }),
    },
    async (input) => {
      try {
        // Label mutations touch every board in the team — whole-
        // team grant required.
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).labels.create(input)
        return ok(result.label)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_labels_update`,
    {
      description: `Update a label's name or color (by its UUID). Returns the updated label.`,
      inputSchema: strictInput({
        id: uuidString,
        name: z.string().min(1).max(255).optional(),
        color: hexColorSchema.optional(),
      }),
    },
    async ({ id, ...rest }) => {
      try {
        const { teamId } = await getLabelContext(id)
        assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).labels.update({
          teamId,
          labelId: id,
          ...rest,
        })
        return ok(result.label)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_labels_delete`,
    {
      description: `Delete a label (by its UUID).`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        const { teamId } = await getLabelContext(id)
        assertTeamFullyGranted(access, teamId)
        await caller(user, request).labels.delete({ teamId, labelId: id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Issue ↔ Label
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_issue_labels_add`,
    {
      description: `Attach a label to an issue (UUID or identifier; teams must match).`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        labelId: uuidString,
      }),
    },
    async ({ issueId: issueIdInput, labelId }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).issueLabels.add({ issueId, labelId })
        return ok({ ok: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issue_labels_remove`,
    {
      description: `Detach a label from an issue (UUID or identifier).`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        labelId: uuidString,
      }),
    },
    async ({ issueId: issueIdInput, labelId }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).issueLabels.remove({ issueId, labelId })
        return ok({ ok: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Issue ↔ Issue (EXP-736)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_issue_relations_add`,
    {
      description: `Link two issues (UUIDs or identifiers, same team). Stored one way: blocks = issueId blocks relatedIssueId, parent = issueId is the parent, duplicate = issueId duplicates relatedIssueId, related is symmetric. Pass inverse:true to state it the other way round (blocked by / sub-issue of / duplicated by).`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        relatedIssueId: z.string().min(1),
        type: z.enum(issueRelationTypeValues),
        inverse: z.boolean().optional(),
      }),
    },
    async ({ issueId: issueIdInput, relatedIssueId: relatedInput, type, inverse }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const relatedIssueId = await resolveIssueId(
          relatedInput,
          user.id,
          access
        )
        if (!access.full) {
          for (const id of [issueId, relatedIssueId]) {
            const ctxIssue = await getIssueTeamContext(id)
            assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
          }
        }
        const result = await caller(user, request).relations.create({
          issueId,
          relatedIssueId,
          type,
          inverse,
        })
        noteAgentIssueActivity(issueId, user.id)
        noteAgentIssueActivity(relatedIssueId, user.id)
        return ok({ ok: true, txId: result.txId })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issue_relations_remove`,
    {
      description: `Unlink two issues. Name the pair in the direction the link is stored (see exponential_issue_relations_add); exponential_issues_get lists them.`,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        relatedIssueId: z.string().min(1),
        type: z.enum(issueRelationTypeValues),
      }),
    },
    async ({ issueId: issueIdInput, relatedIssueId: relatedInput, type }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const relatedIssueId = await resolveIssueId(
          relatedInput,
          user.id,
          access
        )
        if (!access.full) {
          // BOTH sides, exactly like _add: unlinking mutates the far issue's
          // graph too, so a token granted only one of the two boards must not
          // reach through the row to the other.
          for (const id of [issueId, relatedIssueId]) {
            const ctxIssue = await getIssueTeamContext(id)
            assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
          }
        }
        const canonical = canonicalizeRelation(issueId, relatedIssueId, type)
        const [row] = await db
          .select({ id: issueRelations.id })
          .from(issueRelations)
          .where(
            and(
              eq(issueRelations.issueId, canonical.issueId),
              eq(issueRelations.relatedIssueId, canonical.relatedIssueId),
              eq(issueRelations.type, canonical.type)
            )
          )
          .limit(1)
        if (!row) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Relation not found`,
          })
        }
        const result = await caller(user, request).relations.delete({
          id: row.id,
        })
        noteAgentIssueActivity(issueId, user.id)
        noteAgentIssueActivity(relatedIssueId, user.id)
        return ok({ ok: true, id: row.id, txId: result.txId })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Comments
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_comments_list`,
    {
      annotations: READ_ONLY,
      description: `List comments on an issue (oldest first) by UUID or human identifier (e.g. "MET-12"). Rows include their linked attachments. The MCP user must have access to the issue's team.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        limit: z.number().int().min(1).max(200).default(100),
        offset: z.number().int().min(0).default(0),
      }),
    },
    async ({ issueId: issueIdInput, limit, offset }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const ctxIssue = await getIssueTeamContext(issueId)
        assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        await resolveTeamAccess(user.id, ctxIssue.teamId)
        const rows = await db
          .select(commentWireColumns)
          .from(comments)
          .where(eq(comments.issueId, issueId))
          .orderBy(asc(comments.createdAt))
          .limit(limit)
          .offset(offset)
        const linked =
          rows.length > 0
            ? await db
                .select({
                  id: attachments.id,
                  commentId: attachments.commentId,
                  filename: attachments.filename,
                  contentType: attachments.contentType,
                  sizeBytes: attachments.sizeBytes,
                  url: attachments.url,
                })
                .from(attachments)
                .where(
                  inArray(
                    attachments.commentId,
                    rows.map((row) => row.id)
                  )
                )
            : []
        return ok(
          rows.map((row) => ({
            ...row,
            attachments: linked
              .filter((a) => a.commentId === row.id)
              .map(({ commentId: _commentId, ...rest }) => rest),
          }))
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_comments_create`,
    {
      description: `Comment on an issue (UUID or identifier, e.g. "MET-12") as the MCP user ("via MCP"); body = plain GFM. parentId (from exponential_comments_list) replies under that comment; threads are one level deep. audience "reporter" (top-level, a widget issue with a reporter email) also emails it; reporterEmailed says whether it went out.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        body: z.string().trim().min(1).max(10_000).describe(`Plain GFM text`),
        attachmentIds: z.array(uuidString).max(10).optional(),
        parentId: uuidString.optional(),
        audience: z.enum(commentAudienceValues).optional(),
      }),
    },
    async ({ issueId: issueIdInput, body, attachmentIds, parentId, audience }) => {
      try {
        // SLOP-4: a reporter reply EMAILS an outside address with text the
        // reporter chose, so it needs a person behind it: never a
        // board-confined OAuth grant or scoped key, never an unattended run.
        if (audience === `reporter`) {
          if (!access.full) {
            throw new Error(
              `audience "reporter" needs a full-access credential: this connection is confined to chosen teams/boards. Post a team comment instead.`
            )
          }
          const run = await loadCallerSession()
          if (run?.startedReason) {
            throw new Error(
              `audience "reporter" is refused in an unattended run: a person must send replies to a reporter. Post a team comment instead.`
            )
          }
        }
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).comments.create({
          issueId,
          body,
          ...(attachmentIds ? { attachmentIds } : {}),
          ...(parentId ? { parentId } : {}),
          ...(audience ? { audience } : {}),
        })
        noteAgentIssueActivity(issueId, user.id)
        return ok({ ...result.comment, reporterEmailed: result.reporterEmailed })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Coding flow (status + pull requests)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_issues_update_status`,
    {
      description: `Set an issue's status (UUID or identifier). Pass status (builtin enum) or statusId (a team status row from exponential_statuses_list). Status changes are normally AUTOMATIC — PR open/merge apply the team's configured status automation — so set one directly only when the user explicitly asks.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        id: z.string().min(1),
        status: issueStatusEnumSchema.optional(),
        statusId: uuidString.optional(),
      }),
    },
    async ({ id: idOrIdentifier, status, statusId }) => {
      try {
        if ((status === undefined) === (statusId === undefined)) {
          throw new Error(`Pass exactly one of status or statusId`)
        }
        const id = await resolveIssueId(idOrIdentifier, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).issues.update({
          id,
          status,
          statusId,
        })
        noteAgentIssueActivity(id, user.id)
        return ok(result.issue)
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-660: custom status CRUD (FEED-17). Membership and every invariant
  // (started ≤ 4, locked builtins, unique names, reassign-before-delete) live
  // in the statuses router — these only add the grant check and project.
  server.registerTool(
    `exponential_statuses_create`,
    {
      description: `Create a custom issue status in a category (never duplicate; started allows at most 4 rows per team). Names are unique per team, color is #rrggbb. Team members only; ids via exponential_statuses_list.`,
      inputSchema: strictInput({
        teamId: uuidString,
        category: z.enum(customizableStatusCategoryValues),
        name: z.string().min(1).max(255),
        color: hexColorSchema,
      }),
    },
    async (input) => {
      try {
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).statuses.create(input)
        return ok(result.status)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_statuses_update`,
    {
      description: `Rename or recolor a custom issue status (by its UUID). Builtin statuses (builtinKey set) are locked and the category is immutable. Team members only. Returns the updated status.`,
      inputSchema: strictInput({
        id: uuidString,
        name: z.string().min(1).max(255).optional(),
        color: hexColorSchema.optional(),
      }),
    },
    async ({ id, ...rest }) => {
      try {
        const { teamId } = await getStatusContext(id)
        assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).statuses.update({
          teamId,
          statusId: id,
          ...rest,
        })
        return ok(result.status)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_statuses_delete`,
    {
      description: `Delete a custom issue status (by its UUID). Builtins refuse. When issues still use it the call fails with their count until reassignToId names a same-team replacement (not duplicate). Team members only.`,
      inputSchema: strictInput({
        id: uuidString,
        reassignToId: uuidString.optional(),
      }),
    },
    async ({ id, reassignToId }) => {
      try {
        const { teamId } = await getStatusContext(id)
        assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).statuses.delete({
          teamId,
          statusId: id,
          reassignToId,
        })
        return ok({
          ok: true,
          id,
          reassigned: result.reassigned,
          reassignedToId: result.reassignedToId,
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_statuses_list`,
    {
      annotations: READ_ONLY,
      description: `List a team's issue statuses (id, name, category, color, position, builtinKey). Use id as statusId in exponential_issues_update.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        // Team-level read like labels: a visible (board-granted) team
        // suffices, membership is still checked.
        assertTeamVisible(access, teamId)
        await resolveTeamAccess(user.id, teamId)
        const rows = await db
          .select({
            id: issueStatuses.id,
            name: issueStatuses.name,
            category: issueStatuses.category,
            color: issueStatuses.color,
            builtinKey: issueStatuses.builtinKey,
            sortOrder: issueStatuses.sortOrder,
            createdAt: issueStatuses.createdAt,
          })
          .from(issueStatuses)
          .where(eq(issueStatuses.teamId, teamId))
        // Rule 1 of the cross-platform resolution contract
        // (lib/team-statuses.ts): category display order, then sortOrder,
        // createdAt, id.
        const rank = new Map<string, number>(
          issueStatusCategoryDisplayOrder.map((category, index) => [
            category,
            index,
          ])
        )
        rows.sort(
          (a, b) =>
            (rank.get(a.category) ?? 99) - (rank.get(b.category) ?? 99) ||
            a.sortOrder - b.sortOrder ||
            a.createdAt.getTime() - b.createdAt.getTime() ||
            (a.id < b.id ? -1 : a.id > b.id ? 1 : 0)
        )
        const positions = new Map<string, number>()
        return ok(
          page(
            rows.map((row) => {
              const position = (positions.get(row.category) ?? 0) + 1
              positions.set(row.category, position)
              return {
                id: row.id,
                name: row.name,
                category: row.category,
                color: row.color,
                position,
                builtinKey: row.builtinKey,
              }
            }),
            limit,
            offset
          )
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-1105: yolo mode. A team that switched it on merges every PR
  // exponential_pr_open opens, through the SAME path as exponential_pr_merge
  // (the caller's own-PR spare, status automation and the session sweep all
  // apply). A refused merge (conflict, branch protection, required checks)
  // never fails the open: the PR stays open — so Reviews reappears on every
  // client — and the result tells the agent to fix it.
  // EXP-1146: with a session header the PR belongs to a run that may still
  // spawn follow-up runs based on this very branch, so nothing merges while
  // that run is busy: `maybeMergeYoloTree` lands the run's whole PR tree once
  // every run in it is idle with its PR open (root first, EXP-324 retarget
  // between), and it is the busy→idle edge, `sessions_end`, the client end,
  // the kill and the sweep that fire it later. A tree of one that is already
  // idle (a device that never writes `agent_busy`) still merges right here.
  // Without a session header (an `expu_` key, no launched run) the PR cannot
  // grow a tree: it merges at once as before.
  const autoMergeIfYolo = async (
    args: z.infer<typeof prOpenInput>,
    opened: Awaited<ReturnType<typeof prOpen>>
  ) => {
    if (`isError` in opened) return opened
    try {
      const ids: string[] = []
      let teamId: string
      if (args.repositoryId) {
        teamId = (await loadRepositoryForTeam(args.repositoryId)).teamId
      } else {
        for (const raw of args.issueIds ?? [args.issueId!]) {
          const id = await resolveIssueId(raw, user.id, access)
          if (!ids.includes(id)) ids.push(id)
        }
        teamId = (await getIssueTeamContext(ids[0]!)).teamId
      }
      const [team] = await db
        .select({ yoloMode: teams.yoloMode })
        .from(teams)
        .where(eq(teams.id, teamId))
        .limit(1)
      if (!team?.yoloMode) return opened
      const pr = JSON.parse(opened.content[0]!.text) as { number: number }
      const callerSession = await loadCallerSession()
      if (callerSession) {
        const outcome = await maybeMergeYoloTree(callerSession.id)
        // EXP-1146: no tree to merge (a run outside yolo mode): nothing will ever auto-merge this PR, so no `deferred`
        // promise that "the tree merges when the last run ends".
        if (outcome.status === `not_yolo`) return opened
        const own =
          outcome.status === `merged`
            ? outcome.outcomes[callerSession.id]
            : undefined
        const deferred = {
          merged: false,
          deferred: true,
          note: `Yolo mode merges this run's PR tree once every run in it is idle with its PR open — root first, children retargeted onto the default branch between merges. Start any follow-up runs now; the tree merges when the last one ends. Do not merge it yourself.`,
        }
        if (!own) {
          return ok({
            ...pr,
            autoMerge:
              outcome.status === `error`
                ? {
                    merged: false,
                    error: outcome.error,
                    note: `Yolo mode could not merge this PR. Fix the cause (rebase, resolve conflicts, wait for checks), push, then call exponential_pr_merge.`,
                  }
                : deferred,
          })
        }
        return ok({
          ...pr,
          autoMerge:
            own.kind === `merged` || own.kind === `merged_before`
              ? { merged: true }
              : own.kind === `queued`
                ? { merged: false, queued: true }
                : own.kind === `waiting`
                  ? deferred
                  : {
                      merged: false,
                      error:
                        own.kind === `dirty`
                          ? `The PR has merge conflicts with its base.`
                          : (own.detail ?? own.kind),
                      note: `Yolo mode could not merge this PR. Fix the cause (rebase, resolve conflicts, wait for checks), push, then call exponential_pr_merge.`,
                    },
        })
      }
      const merged = await prMerge(
        args.repositoryId
          ? { repositoryId: args.repositoryId, prNumber: pr.number }
          : { issueIds: ids }
      )
      let error: string | undefined
      let queued = false
      if (`isError` in merged) {
        error = merged.content[0]!.text
      } else {
        const { results } = JSON.parse(merged.content[0]!.text) as {
          results: { merged: boolean; queued?: boolean; error?: string }[]
        }
        error = results.find((r) => r.error)?.error
        queued = results.some((r) => r.queued)
      }
      return ok({
        ...pr,
        autoMerge: error
          ? {
              merged: false,
              error,
              note: `Yolo mode could not merge this PR. Fix the cause (rebase, resolve conflicts, wait for checks), push, then call exponential_pr_merge.`,
            }
          : queued
            ? { merged: false, queued: true }
            : { merged: true },
      })
    } catch (e) {
      console.error(`[mcp] yolo auto-merge failed:`, e)
      return opened
    }
  }

  const prOpenInput = strictInput({
    issueId: z.string().min(1).optional(),
    issueIds: z.array(z.string().min(1)).min(1).max(30).optional(),
    repositoryId: uuidString.optional(),
    title: z.string().min(1).max(255),
    body: z.string().max(60_000).optional(),
    head: z.string().max(255).optional(),
    base: z.string().max(255).optional(),
  })
  const prOpen = (
    async ({
      issueId,
      issueIds,
      repositoryId,
      title,
      body,
      head,
      base,
    }: z.infer<typeof prOpenInput>) => {
      try {
        const subjects = [
          Boolean(issueId),
          Boolean(issueIds?.length),
          Boolean(repositoryId),
        ].filter(Boolean).length
        if (subjects !== 1) {
          throw new Error(
            `Provide exactly one of issueId, issueIds or repositoryId`
          )
        }
        if ((issueIds?.length || repositoryId) && !head) {
          throw new Error(
            `'head' is required with issueIds or repositoryId. Pass the pushed branch.`
          )
        }

        // EXP-626: the issue-LESS chore PR. Nothing is linked to an issue and
        // nothing moves — no issue events, no PR-open status flip, no
        // notifications, no agent-activity note (there is no issue to note
        // against). The only side effect beyond the PR itself is parking the
        // CALLING session in `in_review` with the PR stamped on it (EXP-734:
        // the run IS the link — clients merge and review it off the row),
        // and that needs the session header.
        if (repositoryId) {
          const repo = await loadRepositoryForTeam(repositoryId)
          assertTeamFullyGranted(access, repo.teamId)
          await resolveTeamAccess(user.id, repo.teamId)

          const resolvedRepo = await resolveRepoInstallationTokenInfo(
            repo.fullName
          )
          if (!resolvedRepo) {
            throw new Error(
              `The Exponential GitHub App is not installed on ${repo.fullName}.`
            )
          }

          // EXP-1154: the run's report IS the PR body ('body' only without one).
          const callerSession = await loadCallerSession()
          const prBody = await runPrBody(callerSession?.id ?? null, body)

          // EXP-1248: no 'base' = the nearest open PR below the head, else the
          // default branch; a PR opened on another open PR's branch joins its
          // GitHub stack (no fallback: a failed stack call is the tool's error).
          // Before the claim: a failed inference must not leak it.
          const inferredChore: InferredBase = base
            ? { base, prNumber: null }
            : await inferPrBase({
                repo: repo.fullName,
                token: resolvedRepo.token,
                head: head!,
                defaultBranch: repo.defaultBranch,
                teamIds: [repo.teamId],
              })
          const choreBase = inferredChore.base
          claimPrOpen(repo.fullName, head!, {
            userId: user.id,
            viaAgent: true,
          })
          let createdPr: Awaited<ReturnType<typeof openOrReusePull>>
          try {
            createdPr = await openOrReusePull({
              repo: repo.fullName,
              head: head!,
              base: choreBase,
              title,
              body: prBody.body ?? ``,
              token: resolvedRepo.token,
            })
          } catch (e) {
            releasePrOpenClaim(repo.fullName, head!)
            throw e
          }
          // No `opened` webhook follows a reused PR to consume the claim.
          if (createdPr.reusedBase != null) {
            releasePrOpenClaim(repo.fullName, head!)
          }

          if (callerSession) {
            await db.transaction(async (tx) => {
              await parkSessionInReview(tx, {
                callerSessionId: callerSession.id,
                prTeamId: repo.teamId,
                headBranch: head!,
                baseBranch: createdPr.reusedBase ?? choreBase,
                pr: { url: createdPr.url, number: createdPr.number },
              })
            })
            if (createdPr.reusedBase == null) {
              await stampRunResultsPrUrl(callerSession.id, createdPr.url)
            }
            // A reused PR kept its old body: bring it to the report.
            // Only the PR the row actually got (a team mismatch skips the stamp).
            if (createdPr.reusedBase != null && prBody.fromResults) {
              await syncRunPrBody(callerSession.id, {
                expectPrUrl: createdPr.url,
              })
            }
          }

          const choreFinalBase = createdPr.reusedBase ?? choreBase
          const choreStack = await joinGithubStack({
            repo: repo.fullName,
            token: resolvedRepo.token,
            teamId: repo.teamId,
            defaultBranch: repo.defaultBranch,
            base: choreFinalBase,
            lowerPrNumber:
              createdPr.reusedBase == null ? inferredChore.prNumber : null,
            newPrNumber: createdPr.number,
          })

          return ok({
            url: createdPr.url,
            number: createdPr.number,
            ...(choreStack ? { stack: choreStack } : {}),
            ...(prBody.fromResults ? { body: `report` } : {}),
            ...(createdPr.reusedBase != null
              ? { reused: true, note: REUSED_PR_NOTE(head!) }
              : {}),
          })
        }

        // Resolve + authorize every issue; a combined PR lands in ONE repo.
        const rawIds = issueIds ?? [issueId!]
        const ids: string[] = []
        for (const raw of rawIds) {
          const id = await resolveIssueId(raw, user.id, access)
          if (!ids.includes(id)) ids.push(id)
        }

        const teamIdByIssue = new Map<string, string>()
        let repo: {
          repositoryId: string
          fullName: string
          defaultBranch: string
        } | null = null
        for (const id of ids) {
          const issueCtx = await getIssueTeamContext(id)
          assertBoardGranted(access, issueCtx.boardId, issueCtx.teamId)
          await resolveTeamAccess(user.id, issueCtx.teamId)
          teamIdByIssue.set(id, issueCtx.teamId)

          const issueRepo = await caller(user, request).repositories.forIssue({
            issueId: id,
          })
          if (!issueRepo) {
            throw new Error(
              `No repository linked to this board. Link one in team settings.`
            )
          }
          if (repo && repo.repositoryId !== issueRepo.repositoryId) {
            throw new Error(
              `All issues in a combined PR must share one repository (${repo.fullName} vs ${issueRepo.fullName}).`
            )
          }
          // EXP-712: boards on one repo may develop on different branches —
          // a combined PR has exactly one base.
          if (!base && repo && repo.defaultBranch !== issueRepo.defaultBranch) {
            throw new Error(
              `All issues in a combined PR must share one base branch (${repo.defaultBranch} vs ${issueRepo.defaultBranch}). Pass 'base' to pick one.`
            )
          }
          repo = issueRepo
        }
        if (!repo) throw new Error(`Issue not found`)

        let headBranch = head
        // FEED-66: the identifier the head was GUESSED from, when nothing
        // recorded the pushed branch — refined against GitHub below.
        let guessedFromIdentifier: string | null = null
        if (!headBranch) {
          const [issue] = await db
            .select({ identifier: issues.identifier, branch: issues.branch })
            .from(issues)
            .where(eq(issues.id, ids[0]))
            .limit(1)
          if (!issue) throw new Error(`Issue not found`)
          headBranch = issue.branch ?? `exp/${issue.identifier}`
          if (!issue.branch) guessedFromIdentifier = issue.identifier
        }
        // A follow-up run bases on its parent's branch: a `base` that is a
        // same-team issue's OPEN PR branch makes that issue block this one.
        let lower: {
          issueId: string
          teamId: string
          prNumber: number | null
        } | null = null
        // `inferred`: the base came from an open PR, never the agent; a
        // merged row on that branch is history, not a mistake to refuse.
        const lowerFor = async (baseName: string, inferred = false) => {
          const [candidate] = await db
            .select({
              id: issues.id,
              identifier: issues.identifier,
              teamId: issues.teamId,
              prNumber: issues.prNumber,
              prState: issues.prState,
            })
            .from(issues)
            .where(
              and(
                inArray(issues.teamId, [...new Set(teamIdByIssue.values())]),
                eq(issues.branch, baseName),
                like(issues.prUrl, prUrlPattern(repo.fullName))
              )
            )
            .limit(1)
          if (candidate?.prState === `merged` && !inferred) {
            throw new Error(
              `'${baseName}' is the branch of merged PR #${candidate.prNumber} (${candidate.identifier}). Rebase onto ${repo.defaultBranch} and pass no base.`
            )
          }
          if (candidate?.prState === `open` && !ids.includes(candidate.id)) {
            return {
              issueId: candidate.id,
              teamId: candidate.teamId,
              prNumber: candidate.prNumber ?? null,
            }
          }
          return null
        }
        if (base && base !== repo.defaultBranch) lower = await lowerFor(base)
        let baseBranch = base ?? repo.defaultBranch
        // EXP-1248: the lower PR's number when the base is another open PR's
        // head but no same-team issue owns it (a chat/action run's PR).
        let inferredLowerPr: number | null = null

        const resolved = await resolveRepoInstallationTokenInfo(repo.fullName)
        if (!resolved) {
          throw new Error(
            `The Exponential GitHub App is not installed on ${repo.fullName}.`
          )
        }
        const token = resolved.token

        // FEED-66: a cross-board move renumbered the issue mid-run, so the
        // branch the run pushed carries the RETIRED identifier (`exp/FEED-50`
        // for what is now EXP-1147) and the guess above names a branch that
        // does not exist (GitHub: 422 "head invalid"). A moved issue's
        // candidates are tried on GitHub, current name first; a never-moved
        // issue costs nothing extra.
        if (guessedFromIdentifier) {
          const retired = await retiredIdentifiers(ids[0]!)
          if (retired.length > 0) {
            for (const identifier of [guessedFromIdentifier, ...retired]) {
              const candidate = `exp/${identifier}`
              if (await branchExists(repo.fullName, candidate, token)) {
                headBranch = candidate
                break
              }
            }
          }
        }

        // EXP-1248: no 'base' = the nearest open PR whose head is an ancestor
        // of ours, else the default branch (`inferPrBase`). Before the claim:
        // a failed inference must not leak it.
        if (!base) {
          const inferred = await inferPrBase({
            repo: repo.fullName,
            token,
            head: headBranch,
            defaultBranch: repo.defaultBranch,
            teamIds: [...teamIdByIssue.values()],
          })
          baseBranch = inferred.base
          if (inferred.prNumber != null) {
            lower = await lowerFor(inferred.base, true)
            inferredLowerPr = inferred.prNumber
          }
        }

        // EXP-1154: the run's report IS the PR body ('body' only without one).
        const callerSession = await loadCallerSession()
        const prBody = await runPrBody(callerSession?.id ?? null, body)

        // EXP-494: record the initiator BEFORE creating the PR — GitHub's
        // `opened` webhook reliably beats this handler's own DB write, and
        // without the claim it fans out anonymously (self-notifying the very
        // user whose agent opened the PR) whenever no coding_sessions row
        // survived to attribute to.
        claimPrOpen(repo.fullName, headBranch, {
          userId: user.id,
          viaAgent: true,
        })
        // EXP-617 backstop: the claim is keyed on the head branch, so it is
        // lost whenever the branch we compute here is not byte-identical to
        // the `head` GitHub reports back. The issue-keyed record has no such
        // dependency, and it only ever suppresses — never names.
        for (const id of ids) noteAgentIssueActivity(id, user.id)
        let created: Awaited<ReturnType<typeof openOrReusePull>>
        try {
          created = await openOrReusePull({
            repo: repo.fullName,
            head: headBranch,
            base: baseBranch,
            title,
            body: prBody.body ?? ``,
            token,
          })
        } catch (e) {
          // A failed create must not leave a claim that could misattribute a
          // later out-of-band PR on the same branch.
          releasePrOpenClaim(repo.fullName, headBranch)
          // FEED-66: GitHub's 422 on a head nobody pushed is only readable
          // when the caller learns the head was OUR guess.
          if (
            guessedFromIdentifier &&
            e instanceof Error &&
            /\(422\)/.test(e.message)
          ) {
            throw new Error(
              `${e.message} The head branch '${headBranch}' was inferred from the issue; pass 'head' with the branch you pushed.`
            )
          }
          throw e
        }
        // FEED-59: linking to the PR already open on `head`. Nothing new
        // exists on GitHub (no `opened` webhook to consume the claim) and the
        // edge is that PR's REAL base.
        const reused = created.reusedBase != null
        if (reused) {
          releasePrOpenClaim(repo.fullName, headBranch)
          if (created.reusedBase !== baseBranch) {
            lower = null
            inferredLowerPr = null
          }
          baseBranch = created.reusedBase!
        }

        // FEED-59: an issue already linked to the reused PR stays as it is —
        // no second `pr_opened` event, status move or notification.
        const alreadyLinked = new Set<string>()
        await db.transaction(async (tx) => {
          for (const id of ids) {
            const [current] = await tx
              .select({
                status: issues.status,
                prUrl: issues.prUrl,
                prState: issues.prState,
              })
              .from(issues)
              .where(eq(issues.id, id))
              .limit(1)
            // A stale closed/merged row on the same URL gets re-linked.
            if (
              reused &&
              current?.prUrl === created.url &&
              current.prState === `open`
            ) {
              alreadyLinked.add(id)
              continue
            }
            await tx
              .update(issues)
              .set({
                prUrl: created.url,
                prNumber: created.number,
                prState: `open`,
                branch: headBranch,
                // The synced base the PR was opened against (or, reused, its
                // real one).
                prBaseBranch: baseBranch,
              })
              .where(eq(issues.id, id))
            await recordIssueEvent(tx, {
              issueId: id,
              teamId: teamIdByIssue.get(id)!,
              actorUserId: user.id,
              type: `pr_opened`,
              payload: {
                prUrl: created.url,
                prNumber: created.number,
                branch: headBranch,
              },
            })
            // The open PR moves the issue to the team's PR-open target
            // (EXP-120; default In Review, per-team configurable — EXP-319).
            if (current) {
              await applyPrLifecycleStatusInTx(tx, {
                issueId: id,
                teamId: teamIdByIssue.get(id)!,
                actorUserId: user.id,
                currentStatus: current.status,
                event: `opened`,
              })
            }
          }

          // The run owns its PR (EXP-734): the CALLER's row is parked and
          // stamped on every form, a multi-issue run's combined PR included.
          // One repo, one team: every linked issue's team is the PR's.
          await parkSessionInReview(tx, {
            callerSessionId: callerSession?.id ?? null,
            prTeamId: teamIdByIssue.get(ids[0]!)!,
            headBranch,
            baseBranch,
            pr: { url: created.url, number: created.number },
          })

          if (lower) {
            // The lower PR must land first (idempotent — a re-open writes
            // nothing new).
            for (const id of ids) {
              if (teamIdByIssue.get(id) !== lower.teamId) continue
              const edge = lower
              // SLOP-3: in a SAVEPOINT — a SQL error aborts the transaction
              // it runs in, and the outer one carries the issue link and the
              // run's stamp for a PR already open on GitHub. A failure skips
              // the edge (the PR is open either way), but says so: a silent
              // skip hides a DB fault behind "there was a cycle".
              await tx
                .transaction(async (sp) => {
                  // EXP-980: the PR is already open on GitHub, so a cycle is
                  // skipped rather than refused — never written.
                  const cycle = await findRelationCycle(sp, {
                    issueId: edge.issueId,
                    relatedIssueId: id,
                    type: `blocks`,
                  })
                  if (cycle) return
                  await insertRelationInTx(sp, {
                    ...canonicalizeRelation(edge.issueId, id, `blocks`),
                    source: `reference`,
                    teamId: edge.teamId,
                    actorUserId: user.id,
                  })
                })
                .catch((err: unknown) => {
                  console.warn(
                    `[mcp] pr_open: blocks relation failed for ${edge.issueId} -> ${id}; skipping it`,
                    err
                  )
                })
            }
          }
        })

        // EXP-1244: the PR is linked now; a Reviews fetch cached in between
        // would list it as "not linked to an issue".
        invalidateOpenPulls(teamIdByIssue.get(ids[0]!)!)

        // EXP-1251: the Guide topics written before this PR opened belong to
        // it (a later stacked PR's body leaves them out).
        if (!reused && callerSession) {
          await stampRunResultsPrUrl(callerSession.id, created.url)
        }

        // EXP-1248: a PR opened on another open PR's branch joins (or starts)
        // its GitHub stack. The PR is open and linked either way; a failed
        // stack call is the tool's error, never a silent plain chain.
        const stack = await joinGithubStack({
          repo: repo.fullName,
          token,
          teamId: teamIdByIssue.get(ids[0]!)!,
          defaultBranch: repo.defaultBranch,
          base: baseBranch,
          lowerPrNumber: lower?.prNumber ?? inferredLowerPr,
          newPrNumber: created.number,
        })

        // EXP-1154: a reused PR kept its old body: bring it to the report.
        // Only the PR the row actually got (a team mismatch skips the stamp).
        if (reused && prBody.fromResults && callerSession) {
          await syncRunPrBody(callerSession.id, { expectPrUrl: created.url })
        }

        // Away/phone flow: "PR opened" reaches assignee + subscribers on
        // in-app + push + email (deliver()'s dedupe window absorbs the
        // near-simultaneous GitHub webhook `opened` fan-out).
        for (const id of ids) {
          if (alreadyLinked.has(id)) continue
          fireAndForgetPrNotify({
            issueId: id,
            type: `pr_opened`,
            actorUserId: user.id,
            actorViaAgent: true,
          })
        }

        return ok({
          url: created.url,
          number: created.number,
          base: baseBranch,
          ...(stack ? { stack } : {}),
          ...(prBody.fromResults ? { body: `report` } : {}),
          ...(reused ? { reused: true, note: REUSED_PR_NOTE(headBranch) } : {}),
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_pr_open`,
    {
      description: `Open a GitHub PR via the GitHub App (never 'gh') and link it to the issue(s). Body = your run's report ('body' only without one). Pass EXACTLY ONE of 'issueId', 'issueIds' (ONE combined PR, same repo; 'head' REQUIRED) or 'repositoryId' + 'head' (issue-less). 'head' defaults to the issue's branch or 'exp/<IDENTIFIER>', 'base' to the nearest open PR below 'head' or the default branch; basing on an open PR joins its GitHub stack. Linked issues move to the team's PR-open status (default 'in_review'). UUIDs or identifiers ("MET-12").`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: prOpenInput,
    },
    async (args) => autoMergeIfYolo(args, await prOpen(args))
  )

  const prMergeInput = strictInput({
    issueId: z.string().min(1).optional(),
    issueIds: z.array(z.string().min(1)).min(1).max(30).optional(),
    repositoryId: uuidString.optional(),
    prNumber: z.number().int().positive().optional(),
    endSessions: z.boolean().optional(),
    mergeStack: z.boolean().optional(),
  })
  const prMerge = (
    async ({
      issueId,
      issueIds,
      repositoryId,
      prNumber,
      endSessions,
      mergeStack,
    }: z.infer<typeof prMergeInput>) => {
      // EXP-711: only forwarded when given, so the tRPC input stays byte-equal
      // to the pre-override shape for every caller that never passes it.
      const endSessionsInput =
        endSessions !== undefined ? { endSessions } : {}
      try {
        const subjects = [
          Boolean(issueId),
          Boolean(issueIds?.length),
          Boolean(repositoryId) || prNumber !== undefined,
        ].filter(Boolean).length
        if (subjects !== 1) {
          throw new Error(
            `Provide exactly one of issueId, issueIds or repositoryId + prNumber`
          )
        }
        if (Boolean(repositoryId) !== (prNumber !== undefined)) {
          throw new Error(`repositoryId and prNumber must be passed together`)
        }

        // EXP-637 decision 6, corrected in EXP-639. A run that merges the PR
        // IT opened must survive its own merge: the durable `merged_own_pr`
        // spare (lib/sessions/merged-own-pr.ts owns the stamp-then-merge
        // order and the revert). Two rules the first cut missed:
        //   * ONLY the run's OWN PR may stamp it. The column is durable, so
        //     stamping it while landing a teammate's PR would also spare the
        //     row from the later merge of its own PR — a run nothing ends.
        //   * A merge that never happened may not leave the stamp (nor the
        //     in_review → running flip) behind.
        const callerSession = await loadCallerSession()
        const stampable =
          callerSession &&
          callerSession.status !== `ended` &&
          !callerSession.mergedOwnPr
            ? callerSession
            : null
        const stampMergedOwnPr = () => stampOwnPr(db, stampable!.id)
        const revertMergedOwnPr = () =>
          revertOwnPr(db, stampable!.id, priorOf(stampable!))

        // EXP-626: the issue-LESS chore PR. repositories.mergePull owns the
        // guards (membership, App config, installation link-gate) and the
        // merge itself; there is no issue row to sync.
        if (repositoryId) {
          // Own-PR test. The merge-driven end that reaches an issue-less row
          // is keyed on the PR `exponential_pr_open` stamped when it parked
          // this run on the PR it opened (EXP-734) — so the spare is for
          // exactly that row shape. But "issue-less run with a PR" is not
          // enough: a chat/batch/action run landing SOMEBODY ELSE'S chore PR
          // would stamp a DURABLE spare that also filters the later merge of
          // its own PR, leaving a run nothing ends. So stamp only when the
          // `repositoryId + prNumber` being merged IS the row's own PR. A
          // test that cannot answer leaves the stamp off: being ended by a
          // merge is recoverable, a run that never ends is not.
          let ownChorePr = false
          if (stampable && stampable.prUrl && stampable.prNumber != null) {
            try {
              const choreRepo = await loadRepositoryForTeam(repositoryId)
              await resolveTeamAccess(user.id, choreRepo.teamId)
              ownChorePr =
                stampable.prNumber === prNumber &&
                repoFromPrUrl(stampable.prUrl) === choreRepo.fullName
            } catch {
              ownChorePr = false
            }
          }
          if (ownChorePr) await stampMergedOwnPr()
          let chore: { merged: boolean; queued?: boolean }
          try {
            chore = await caller(user, request).repositories.mergePull({
              repositoryId,
              prNumber: prNumber!,
              ...(mergeStack ? { mergeStack: true } : {}),
              ...endSessionsInput,
            })
          } catch (e) {
            if (ownChorePr) await revertMergedOwnPr()
            throw e
          }
          // EXP-1165: an enqueued merge has not landed (merged=false).
          const choreQueued = chore.queued === true
          return ok({
            results: [
              {
                repositoryId,
                prNumber: prNumber!,
                merged: chore.merged && !choreQueued,
                ...(choreQueued ? { queued: true } : {}),
              },
            ],
          })
        }

        // Resolve + authorize every issue up front — a scope/membership
        // violation fails the WHOLE call (never a per-item "result").
        const rawIds = issueIds ?? [issueId!]
        const ids: string[] = []
        for (const raw of rawIds) {
          const id = await resolveIssueId(raw, user.id, access)
          if (!ids.includes(id)) ids.push(id)
        }
        const teamIdByIssue = new Map<string, string>()
        for (const id of ids) {
          const issueCtx = await getIssueTeamContext(id)
          assertBoardGranted(access, issueCtx.boardId, issueCtx.teamId)
          await resolveTeamAccess(user.id, issueCtx.teamId)
          teamIdByIssue.set(id, issueCtx.teamId)
        }
        // SLOP-3: mergeStack also lands the open PRs below each target. They
        // may sit on boards this token was never granted: refuse before
        // GitHub sees anything.
        const stackByIssue = new Map<string, StackMember[]>()
        if (mergeStack) {
          for (const id of ids) {
            const teamId = teamIdByIssue.get(id)!
            const chain = await openStackThrough(db, { issueId: id, teamId })
            for (const member of chain) {
              if (member.boardId) assertBoardGranted(access, member.boardId, teamId)
            }
            stackByIssue.set(id, chain)
          }
        }

        // One merge per distinct PR: issues sharing a batch prUrl collapse
        // onto the first listed issue (merging it completes the siblings).
        const rows = await db
          .select({
            id: issues.id,
            identifier: issues.identifier,
            prUrl: issues.prUrl,
            // EXP-639: the own-PR test below — a batch/chore run's row carries
            // the head branch, its issues carry the same one.
            branch: issues.branch,
          })
          .from(issues)
          .where(inArray(issues.id, ids))
        const rowById = new Map(rows.map((row) => [row.id, row]))
        const seenPrUrls = new Set<string>()
        const targets: { id: string; identifier: string }[] = []
        for (const id of ids) {
          const row = rowById.get(id)
          // Unknown row / no linked PR: keep it as a target so the tRPC
          // mutation's own guard produces the precise per-item message.
          if (row?.prUrl) {
            if (seenPrUrls.has(row.prUrl)) continue
            seenPrUrls.add(row.prUrl)
          }
          targets.push({ id, identifier: row?.identifier ?? id })
        }

        // The PR this run owns: the one on the issue it was launched on, or
        // the one on the branch pr_open stamped on a batch/chore row. Matched
        // over every REQUESTED issue rather than the deduped targets, so a
        // batch PR still counts when a sibling issue ended up representing
        // it; a target then owns the merge when it carries that same prUrl.
        const ownPrUrls = new Set<string>()
        if (stampable) {
          for (const row of rows) {
            const own =
              row.id === stampable.issueId ||
              (stampable.prUrl !== null && row.prUrl === stampable.prUrl) ||
              (stampable.branch !== null && row.branch === stampable.branch)
            if (own && row.prUrl) ownPrUrls.add(row.prUrl)
          }
          // A stack merge lands the members below a target too.
          for (const chain of stackByIssue.values()) {
            for (const member of chain) {
              const own =
                member.issueId === stampable.issueId ||
                (stampable.prUrl !== null && member.prUrl === stampable.prUrl) ||
                (stampable.branch !== null && member.branch === stampable.branch)
              if (own) ownPrUrls.add(member.prUrl)
            }
          }
        }
        const ownTargetIds = new Set(
          targets
            .filter((target) => {
              const urls = [
                rowById.get(target.id)?.prUrl,
                ...(stackByIssue.get(target.id) ?? []).map((m) => m.prUrl),
              ]
              return urls.some((url) => Boolean(url && ownPrUrls.has(url)))
            })
            .map((target) => target.id)
        )
        if (ownTargetIds.size > 0) await stampMergedOwnPr()

        // The tRPC mutation owns the guards (open-state, repo-from-prUrl,
        // installation link-gate) and the shared applyPrMergeState writer.
        const trpcCaller = caller(user, request)
        const results: {
          issueId: string
          identifier: string
          merged: boolean
          // FEED-43 R1: GitHub's merge queue holds it; `merged` is false
          // until it lands (the queue may still reject it).
          queued?: boolean
          error?: string
          note?: string
          // mergeStack: the PRs this target's call landed, bottom first.
          stack?: string[]
        }[] = []
        // A queued merge is the run's own success in flight: the spare it
        // stamped stays, or the landing merge would end the run after all.
        const ownMergeEarned = () =>
          results.some(
            (result) =>
              (result.merged || result.queued) && ownTargetIds.has(result.issueId)
          )

        for (const target of targets) {
          try {
            const outcome = await trpcCaller.issues.mergePr({
              issueId: target.id,
              ...endSessionsInput,
              ...(mergeStack ? { mergeStack: true } : {}),
            })
            // FEED-43 R1: an enqueued merge has not landed; say so instead of
            // a `merged: true` the queue may still take back.
            const queued = outcome.queued === true
            results.push({
              issueId: target.id,
              identifier: target.identifier,
              merged: !queued,
              ...(queued ? { queued: true } : {}),
              ...(outcome.note ? { note: outcome.note } : {}),
              ...(outcome.stack
                ? { stack: outcome.stack.map((pr) => pr.identifier) }
                : {}),
            })
          } catch (e) {
            results.push({
              issueId: target.id,
              identifier: target.identifier,
              merged: false,
              error: e instanceof Error ? e.message : String(e),
            })
          }
        }
        // Only a merge that actually landed (or is queued to) earns the spare;
        // an unmergeable PR leaves the row exactly as this call found it.
        if (ownTargetIds.size > 0 && !ownMergeEarned()) {
          // A stack merge that stops part-way reports its target as failed
          // while the members below it landed. The run's own PR may be one
          // of those: the spare then stays (EXP-637).
          const [ownLanded] = await db
            .select({ id: issues.id })
            .from(issues)
            .where(
              and(
                inArray(issues.prUrl, [...ownPrUrls]),
                eq(issues.prState, `merged`)
              )
            )
            .limit(1)
          if (!ownLanded) await revertMergedOwnPr()
        }
        return ok({ results })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_pr_merge`,
    {
      description: `Squash-merge open PRs via the GitHub App (never 'gh'). Pass EXACTLY ONE of 'issueId', 'issueIds' (one merge per distinct prUrl) or 'repositoryId' + 'prNumber' (a PR with no issue). Linked issues flip to merged and the team's PR-merge status (default 'done'); their live sessions end unless the team setting is off or 'endSessions' overrides it; YOUR OWN session keeps running. 'mergeStack' lands a stack member + every open PR beneath it. Each results[] element: 'merged' + optional 'error' or 'queued' (merge queue) + its issueId/identifier or repositoryId/prNumber; one unmergeable PR never blocks the rest. Stale base: exponential_pr_retarget first. Idempotent.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: prMergeInput,
    },
    prMerge
  )

  server.registerTool(
    `exponential_pr_retarget`,
    {
      description: `Change the base branch of an issue's open PR via the GitHub App. Use it when a merge is rejected because the base is stale (e.g. based on an already-merged parent PR). Omit 'base' for the repo's default branch. Then rebase onto the new base, push with --force-with-lease, and call exponential_pr_merge.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        issueId: z.string().min(1),
        base: z.string().min(1).max(255).optional(),
      }),
    },
    async ({ issueId, base }) => {
      try {
        const id = await resolveIssueId(issueId, user.id, access)
        const issueCtx = await getIssueTeamContext(id)
        assertBoardGranted(access, issueCtx.boardId, issueCtx.teamId)
        await resolveTeamAccess(user.id, issueCtx.teamId)

        // The tRPC mutation owns the guards (open-state, repo-from-prUrl,
        // installation link-gate) and the default-branch fill-in.
        const result = await caller(user, request).issues.retargetPr({
          issueId: id,
          base,
        })
        noteAgentIssueActivity(id, user.id)
        return ok({ ok: true, base: result.base })
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-1139: rewrite an open PR's title and/or body. `pr_open` refuses to
  // reopen a PR that exists (FEED-59), and agents hold no `gh` — so a later
  // commit that changed the scope used to leave the GitHub description
  // stale for good. Same subject forms as pr_merge (issueId / issueIds /
  // repositoryId + prNumber); with NO subject the X-Exp-Session-Id header
  // names the run, whose own PR is edited (its issue's, its batch's, or the
  // chore PR pr_open stamped on the row). The tRPC twins own the guards.
  const prUpdateInput = strictInput({
    issueId: z.string().min(1).optional(),
    issueIds: z.array(z.string().min(1)).min(1).max(30).optional(),
    repositoryId: uuidString.optional(),
    prNumber: z.number().int().positive().optional(),
    title: z.string().trim().min(1).max(255).optional(),
    body: z.string().max(60_000).optional(),
  })
  server.registerTool(
    `exponential_pr_update`,
    {
      description: `Rewrite the title and/or body of an open PR via the GitHub App: the fix for a description later commits made stale (pr_open never edits an existing PR). Subjects as exponential_pr_merge: EXACTLY ONE of 'issueId', 'issueIds' (one update per distinct PR) or 'repositoryId' + 'prNumber'; omit all for this run's own PR, whose body is its Guide ('body' ignored: edit exponential_sessions_guide). Pass 'title', 'body' (max 60000) or both; an omitted field keeps its value. results[]: issueId/identifier or repositoryId/prNumber, 'updated', optional 'error'. Accepts identifiers ("MET-12").`,
      inputSchema: prUpdateInput,
    },
    async ({ issueId, issueIds, repositoryId, prNumber, title, body }) => {
      try {
        if (title === undefined && body === undefined) {
          throw new Error(`Pass a title, a body, or both.`)
        }
        let fields: { title?: string; body?: string } = {
          ...(title !== undefined ? { title } : {}),
          ...(body !== undefined ? { body } : {}),
        }
        const subjects = [
          Boolean(issueId),
          Boolean(issueIds?.length),
          Boolean(repositoryId) || prNumber !== undefined,
        ].filter(Boolean).length
        if (subjects > 1) {
          throw new Error(
            `Provide exactly one of issueId, issueIds or repositoryId + prNumber`
          )
        }
        if (Boolean(repositoryId) !== (prNumber !== undefined)) {
          throw new Error(`repositoryId and prNumber must be passed together`)
        }

        let rawIds: string[] | null = issueIds ?? (issueId ? [issueId] : null)
        let chore: { repositoryId: string; prNumber: number } | null =
          repositoryId ? { repositoryId, prNumber: prNumber! } : null

        // No subject: the run's OWN PR, off the session header.
        if (!rawIds && !chore) {
          const callerSession = await loadCallerSession()
          if (!callerSession) {
            throw new Error(
              `Provide issueId, issueIds or repositoryId + prNumber: no run header names a pull request here.`
            )
          }
          // EXP-1154: this run's PR body IS its report; 'body' never lands.
          if (body !== undefined && (await runHasReportBody(callerSession.id))) {
            if (title === undefined) {
              return ok({
                results: [],
                note: `This run's PR body is its Guide: edit it with exponential_sessions_guide (each text write re-syncs the PR).`,
              })
            }
            fields = { title }
          }
          if (callerSession.issueId) {
            rawIds = [callerSession.issueId]
          } else if (
            callerSession.prUrl &&
            callerSession.prNumber != null &&
            callerSession.teamId
          ) {
            // The chore PR pr_open parked this issue-less run on (EXP-734).
            const repoFullName = repoFromPrUrl(callerSession.prUrl)
            if (!repoFullName) {
              throw new Error(`The run's pull request URL is not a GitHub PR URL`)
            }
            const repo = await loadRepositoryByFullName(
              callerSession.teamId,
              repoFullName
            )
            chore = { repositoryId: repo.id, prNumber: callerSession.prNumber }
          } else if (callerSession.branch && callerSession.teamId) {
            // A batch run: its issues share the branch pr_open stamped on
            // the row, and the PR on it.
            const batchRows = await db
              .select({ id: issues.id })
              .from(issues)
              .where(
                and(
                  eq(issues.teamId, callerSession.teamId),
                  eq(issues.branch, callerSession.branch),
                  isNotNull(issues.prUrl)
                )
              )
            if (batchRows.length === 0) {
              throw new Error(
                `This run has no pull request yet. Open one with exponential_pr_open, or name the PR (issueId, issueIds or repositoryId + prNumber).`
              )
            }
            rawIds = batchRows.map((row) => row.id)
          } else {
            throw new Error(
              `This run has no pull request yet. Open one with exponential_pr_open, or name the PR (issueId, issueIds or repositoryId + prNumber).`
            )
          }
        }

        if (chore) {
          const choreRepo = await loadRepositoryForTeam(chore.repositoryId)
          assertTeamFullyGranted(access, choreRepo.teamId)
          await resolveTeamAccess(user.id, choreRepo.teamId)
          await caller(user, request).repositories.updatePull({
            repositoryId: chore.repositoryId,
            prNumber: chore.prNumber,
            ...fields,
          })
          return ok({
            results: [
              {
                repositoryId: chore.repositoryId,
                prNumber: chore.prNumber,
                updated: true,
              },
            ],
          })
        }

        // Resolve + authorize every issue up front — a scope/membership
        // violation fails the WHOLE call (never a per-item "result").
        const ids: string[] = []
        for (const raw of rawIds!) {
          const id = await resolveIssueId(raw, user.id, access)
          if (!ids.includes(id)) ids.push(id)
        }
        for (const id of ids) {
          const issueCtx = await getIssueTeamContext(id)
          assertBoardGranted(access, issueCtx.boardId, issueCtx.teamId)
          await resolveTeamAccess(user.id, issueCtx.teamId)
        }

        // One update per distinct PR: issues sharing a batch prUrl collapse
        // onto the first listed issue (like pr_merge).
        const rows = await db
          .select({
            id: issues.id,
            identifier: issues.identifier,
            prUrl: issues.prUrl,
          })
          .from(issues)
          .where(inArray(issues.id, ids))
        const rowById = new Map(rows.map((row) => [row.id, row]))
        const seenPrUrls = new Set<string>()
        const targets: { id: string; identifier: string }[] = []
        for (const id of ids) {
          const row = rowById.get(id)
          if (row?.prUrl) {
            if (seenPrUrls.has(row.prUrl)) continue
            seenPrUrls.add(row.prUrl)
          }
          targets.push({ id, identifier: row?.identifier ?? id })
        }

        const trpcCaller = caller(user, request)
        const results: {
          issueId: string
          identifier: string
          updated: boolean
          url?: string
          error?: string
        }[] = []
        for (const target of targets) {
          try {
            const updated = await trpcCaller.issues.updatePr({
              issueId: target.id,
              ...fields,
            })
            results.push({
              issueId: target.id,
              identifier: target.identifier,
              updated: true,
              url: updated.url,
            })
          } catch (e) {
            results.push({
              issueId: target.id,
              identifier: target.identifier,
              updated: false,
              error: e instanceof Error ? e.message : String(e),
            })
          }
        }
        return ok({ results })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Sessions (EXP-637)
  // -----------------------------------------------------------------------

  // EXP-679 / EXP-1222: every run of the caller's gets the close-out tool
  // (gates.sessionsEnd = the header's run is the caller's). An unattended run
  // calls it LAST; a person-started one only when the person asks it to end
  // the run (the description and the instructions say so).
  if (gates.sessionsEnd) {
    server.registerTool(
      `exponential_sessions_end`,
      {
        description: `Ends this run. Unattended runs: call it LAST after exponential_pr_open with the worktree clean; attended runs: only when the person asks. 'summary' = one paragraph of what you did (finished, stopped for a human or changed nothing), reported to whoever started this run (not stored). Merging your own PR never ends it; this call does.`,
        _meta: ALWAYS_LOAD_META,
        inputSchema: strictInput({
          summary: z.string().min(1).max(4_000),
        }),
      },
      async ({ summary }) => {
        try {
          if (!sessionId) {
            return err(
              new Error(
                `No coding session: exponential_sessions_end only works inside a session started by the Exponential launcher (missing X-Exp-Session-Id).`
              )
            )
          }
          // Ownership is enforced inside endSessionByAgent (owner or host),
          // which also makes a repeated call idempotent instead of blanking
          // an earlier close-out.
          const result = await endSessionByAgent(db, sessionId, user.id, {
            summary,
          })
          // EXP-700: a just-ended agent-started child reports into its live
          // parent's channel. Only a FIRST real end notifies — alreadyEnded
          // (retries, lost races) never does.
          let reportedToParent = false
          if (result.status === `ended` && !result.alreadyEnded) {
            const { delivered } = await notifyParentOfChildEnd(db, sessionId, {
              summary,
              endedBy: `agent`,
            })
            reportedToParent = delivered
            // EXP-1146: an ended run may complete its yolo tree. Fire and
            // forget — the close-out never waits on GitHub.
            void maybeMergeYoloTree(sessionId)
          }
          return ok({ ...result, reportedToParent })
        } catch (e) {
          return err(e)
        }
      }
    )
  }

  // EXP-700: only an agent-started run (started_reason='agent') can ask its
  // starter a question — the gate does NOT wait for `parent_session_id`,
  // which the parent stamps only after its sessions_start poll returns; the
  // handler below re-checks the linkage. NON-blocking on purpose — agent
  // CLIs time out long-held tool calls — so the answer arrives later as an
  // injected user message, the same rail a human steers with. EXP-1089:
  // registered for EVERY run of the caller's; `to: 'user'` works from any of
  // them, the parent target still needs a starter.
  if (gates.askParent) {
    server.registerTool(
      `exponential_sessions_ask_parent`,
      {
        description: `Ask a question only your starter or the person can answer. 'to': 'parent' (default; the run or MCP client that started this; no live starter run = parked here) or 'user' (the run's owner; parks yours as needing input and notifies them). Non-blocking: on success STOP and end your turn; the answer arrives as a user message. If it fails, finish; note the question in your summary.`,
        _meta: ALWAYS_LOAD_META,
        inputSchema: strictInput({
          question: z.string().min(1).max(4_000),
          to: z.enum([`parent`, `user`]).default(`parent`),
        }),
      },
      async ({ question, to }) => {
        const fallback = `Do not wait for an answer: finish your work, then call exponential_sessions_end and include the open question in your summary.`
        try {
          if (!sessionId) {
            return err(
              new Error(
                `No coding session: exponential_sessions_ask_parent only works inside a session started by the Exponential launcher (missing X-Exp-Session-Id).`
              )
            )
          }
          // The gate is context hygiene; re-check ownership and the linkage.
          const child = await loadChildParentContext(db, sessionId)
          if (
            !child ||
            (child.userId !== user.id && child.hostUserId !== user.id)
          ) {
            return err(new Error(`This run has no live starter to ask.`))
          }

          // EXP-897: park the question. The run parks as needing input
          // (every client surfaces that, and the caption IS the question),
          // the question itself lands on the row (`pending_question`), and
          // the run's owner gets the existing agent_message inbox row + push.
          // The answer comes back as a normal user message in THIS run's own
          // composer (a person) or via exponential_sessions_message (an MCP
          // starter, which reads it off sessions_get / sessions_messages).
          const parkQuestion = async (): Promise<Error | null> => {
            const caption = question.slice(0, 160)
            const askedAt = new Date()
            await db
              .update(codingSessions)
              .set({
                needsInput: true,
                agentCaption: caption,
                pendingQuestion: { question, askedAt: askedAt.toISOString() },
                updatedAt: askedAt,
              })
              .where(eq(codingSessions.id, sessionId))
            const [row] = await db
              .select({
                teamId: codingSessions.teamId,
                userId: codingSessions.userId,
              })
              .from(codingSessions)
              .where(eq(codingSessions.id, sessionId))
              .limit(1)
            if (!row?.teamId) {
              return new Error(`This run has no team to notify in.`)
            }
            await sendAgentMessage({
              teamId: row.teamId,
              senderUserId: row.userId,
              // ONE person: the run's owner — a run's question is not the
              // team's inbox.
              recipientIds: [row.userId],
              title: `${child.issueIdentifier ?? sessionId.slice(0, 8)} asks`,
              body: question,
            })
            return null
          }

          if (to === `user`) {
            const failed = await parkQuestion()
            if (failed) return err(failed)
            return ok({
              delivered: true,
              to: `user`,
              note: `Asked the person. Stop working NOW and end your turn; their answer arrives as a user message in this session.`,
            })
          }

          // EXP-1216: no live parent RUN — an MCP client started this one
          // (no linkage), or the parent chain ended. The starter still exists
          // somewhere, so the question parks on the row exactly like
          // `to: 'user'` instead of failing; whoever started the run reads
          // it and answers through exponential_sessions_message.
          const parkForStarter = async () => {
            const failed = await parkQuestion()
            if (failed) return err(failed)
            return ok({
              delivered: false,
              parked: true,
              to: `starter`,
              note: `No live starter run to relay to, so the question is parked on this run and its owner is notified: your starter reads it with exponential_sessions_get (pendingQuestion) or exponential_sessions_messages and answers with exponential_sessions_message. Stop working NOW and end your turn; the answer arrives as a user message.`,
            })
          }
          if (child.startedReason !== `agent` || !child.parentSessionId) {
            return await parkForStarter()
          }
          // EXP-906: a parent that resumed (account switch) under a new id is
          // still listening there — its live successor takes the question.
          const targetSessionId = await resolveLiveParentSessionId(
            db,
            child
          ).catch(() => null)
          if (!targetSessionId) return await parkForStarter()
          const config = getSteerRelayConfig()
          if (!config) {
            return err(
              new Error(
                `The steer relay is not configured, so the question cannot be delivered. ${fallback}`
              )
            )
          }
          const { delivered } = await relayPostInput(
            config,
            targetSessionId,
            formatChildQuestion(child, question)
          )
          if (!delivered) {
            return err(
              new Error(
                `The question could not be delivered to your starter (its session is not reachable). ${fallback}`
              )
            )
          }
          return ok({
            delivered: true,
            note: `Question delivered. Stop working NOW and end your turn; the answer will arrive as a user message. After acting on it, still finish with exponential_sessions_end.`,
          })
        } catch (e) {
          return err(e)
        }
      }
    )
  }

  // EXP-879: the run publishes PICTURES of its own work. Same header-run
  // ownership as the close-out (owner or host), but no started_reason
  // condition — an attended run's screenshots are exactly as useful as an
  // automation's. Bytes never come through MCP: the tool mints a signed,
  // ten-minute upload URL bound to (session, topic, label, user) and hands the
  // agent a curl line, so a 3 MB PNG never lands in the context window.
  // EXP-1251: the tool is the GUIDE (`exponential_sessions_guide`); the old
  // name stays registered as a deferred alias for shipped CLIs and daemons,
  // routed to the same handler. Every write is stamped `at` (EXP-1245), a
  // text may scope its topic to one PR (`prUrl`), and listed files the
  // branch/PR diff lacks come back in the answer (`missingFromDiff`).
  if (gates.sessionResults) {
    const guideInput = strictInput({
      topic: z.string().trim().min(1).max(SESSION_RESULT_TEXT_MAX),
      label: z.string().trim().min(1).max(SESSION_RESULT_TEXT_MAX).optional(),
      text: z.string().max(SESSION_RESULT_REPORT_MAX).optional(),
      // Limits checked in the handler: the schema is always-loaded bytes.
      files: z.array(z.string()).optional(),
      prUrl: z.string().optional(),
      remove: z.boolean().optional(),
    })
    const guideHandler = async ({
      topic,
      label,
      text,
      files,
      prUrl,
      remove,
    }: {
      topic: string
      label?: string
      text?: string
      files?: string[]
      prUrl?: string
      remove?: boolean
    }) => {
        try {
          if (!sessionId) {
            return err(
              new Error(
                `No coding session: exponential_sessions_guide only works inside a session started by the Exponential launcher (missing X-Exp-Session-Id).`
              )
            )
          }
          // EXP-1154: files belong to a topic's text (its report section).
          if (files !== undefined && (text === undefined || remove)) {
            return err(
              new Error(
                `files rides a topic's text: pass it with text (the section it belongs to).`
              )
            )
          }
          // EXP-1251: so does the topic's PR.
          if (prUrl !== undefined && (text === undefined || remove)) {
            return err(
              new Error(`prUrl rides a topic's text: pass it with text.`)
            )
          }
          if (prUrl !== undefined && prUrl.length > SESSION_RESULT_PR_URL_MAX) {
            return err(
              new Error(`prUrl takes at most ${SESSION_RESULT_PR_URL_MAX} characters.`)
            )
          }
          if (
            files &&
            (files.length > SESSION_RESULT_FILES_MAX ||
              files.some((path) => path.length > SESSION_RESULT_FILE_PATH_MAX))
          ) {
            return err(
              new Error(
                `files takes at most ${SESSION_RESULT_FILES_MAX} paths of at most ${SESSION_RESULT_FILE_PATH_MAX} characters each.`
              )
            )
          }
          const [row] = await db
            .select({
              id: codingSessions.id,
              userId: codingSessions.userId,
              hostUserId: codingSessions.hostUserId,
              status: codingSessions.status,
              results: codingSessions.results,
              prUrl: codingSessions.prUrl,
              branch: codingSessions.branch,
              boardId: codingSessions.boardId,
            })
            .from(codingSessions)
            .where(eq(codingSessions.id, sessionId))
            .limit(1)
          if (!row) return err(new Error(`Session not found`))
          if (row.userId !== user.id && row.hostUserId !== user.id) {
            return err(new Error(`This is not your run.`))
          }

          // Removal works at ANY status: a run that already ended still owns
          // its pictures, and a wrong one has to be retractable.
          if (remove) {
            // The column is a jsonb read-modify-write, so it runs under the
            // same `FOR UPDATE` row lock the upload route takes: the read
            // above only settled ownership, and an upload racing this remove
            // would otherwise have one of them overwrite the other's array.
            const outcome = await db.transaction(async (tx) => {
              const [locked] = await tx
                .select({ results: codingSessions.results })
                .from(codingSessions)
                .where(eq(codingSessions.id, sessionId))
                .limit(1)
                .for(`update`)
              if (!locked) throw new Error(`Session not found`)
              const removeText = text !== undefined && !label
              const { results, removedAttachmentIds } = removeSessionResults(
                locked.results,
                { topic, label: label ?? null, text: removeText }
              )
              const textGone =
                (locked.results ?? []).length - results.length >
                removedAttachmentIds.length
              if (removedAttachmentIds.length === 0 && !textGone) {
                return {
                  results: locked.results,
                  removedAttachmentIds,
                  textGone,
                  gone: [] as Array<{ storageKey: string | null }>,
                }
              }
              // The column first: it is what every client renders, so a
              // picture is off the wire before its bytes go. Rows and objects
              // follow.
              await tx
                .update(codingSessions)
                .set({ results, updatedAt: new Date() })
                .where(eq(codingSessions.id, sessionId))
              if (removedAttachmentIds.length === 0) {
                return {
                  results,
                  removedAttachmentIds,
                  textGone,
                  gone: [] as Array<{ storageKey: string | null }>,
                }
              }
              const gone = await tx
                .delete(sessionAttachments)
                .where(inArray(sessionAttachments.id, removedAttachmentIds))
                .returning({ storageKey: sessionAttachments.storageKey })
              return {
                results,
                removedAttachmentIds,
                textGone,
                gone: gone ?? [],
              }
            })
            const { results, removedAttachmentIds, textGone, gone } = outcome
            // EXP-1154: a removed text changes the report, so the PR body
            // (decided under the row lock, not off the unlocked pre-read).
            if (textGone) await syncRunPrBody(sessionId, { removal: true })
            if (removedAttachmentIds.length === 0) {
              return ok({
                removed:
                  (row.results ?? []).length - (results ?? []).length > 0 ? 1 : 0,
                topic,
                label: label ?? null,
                results: resultsSummary(results),
              })
            }
            // Only once the swap is durable: the removed pictures' bytes go.
            for (const attachment of gone) {
              if (!attachment?.storageKey) continue
              try {
                await deleteObject(attachment.storageKey)
              } catch (deleteError) {
                console.error(
                  `Failed to delete a removed session result object`,
                  deleteError
                )
              }
            }
            return ok({
              removed: removedAttachmentIds.length,
              topic,
              label: label ?? null,
              results: resultsSummary(results),
            })
          }

          if (!label && text === undefined) {
            return err(
              new Error(
                `Pass text (the topic's report) and/or label: label is required to publish a picture (one picture inside the topic, e.g. web/ios/android). Pass remove: true to delete the whole topic instead.`
              )
            )
          }
          // The STATUS gate lives here, not on the upload route: a token
          // minted while the run was live stays good for its ten minutes, so
          // a screenshot in flight survives the run ending.
          if (row.status === `ended`) {
            return err(
              new Error(
                `This run has ended, so it can no longer publish results.`
              )
            )
          }
          // EXP-933: the report text lands right here, under the same row
          // lock the upload route and remove take (jsonb read-modify-write).
          let current = row.results
          let prSync: `synced` | `skipped` | `failed` = `skipped`
          let missingFromDiff: string[] | null = null
          if (text !== undefined) {
            const trimmed = text.trim()
            if (!trimmed) {
              return err(
                new Error(
                  `text is empty. To delete a topic's text pass remove: true with text: ''.`
                )
              )
            }
            const written = await db.transaction(async (tx) => {
              const [locked] = await tx
                .select({ results: codingSessions.results })
                .from(codingSessions)
                .where(eq(codingSessions.id, sessionId))
                .limit(1)
                .for(`update`)
              if (!locked) throw new Error(`Session not found`)
              const next = upsertSessionResultText(
                locked.results,
                topic,
                trimmed,
                files,
                { at: Date.now(), prUrl }
              )
              if (!next) return null
              await tx
                .update(codingSessions)
                .set({ results: next, updatedAt: new Date() })
                .where(eq(codingSessions.id, sessionId))
              return next
            })
            if (!written) {
              return err(
                new Error(
                  `This run's results are full (${SESSION_RESULTS_MAX} entries, ${SESSION_RESULTS_REPORT_TOTAL_MAX} characters of text or ${SESSION_RESULTS_FILES_TOTAL_MAX} files in all). Shorten the text or remove a topic first.`
                )
              )
            }
            current = written
            // EXP-1154: the report IS the PR body; an open PR follows it.
            prSync = await syncRunPrBody(sessionId)
            // EXP-1251: listed files the branch/PR diff does not have.
            const listed = files ? cleanSessionResultFiles(files) : []
            if (listed.length > 0) {
              const entry = written.find((r) => isTextEntry(r) && r.topic === topic)
              const diff = await loadGuideDiff(
                { prUrl: row.prUrl, branch: row.branch, boardId: row.boardId },
                entry?.prUrl ?? null
              )
              const missing = diff ? missingGuideFiles(listed, diff) : []
              if (missing.length > 0) missingFromDiff = missing
            }
            if (!label) {
              return ok({
                topic,
                text: trimmed.length,
                ...(prSync === `synced` ? { pr: `synced` } : {}),
                ...(missingFromDiff ? { missingFromDiff, note: MISSING_NOTE } : {}),
                results: resultsSummary(current),
              })
            }
          }
          if (!label) throw new Error(`unreachable: label checked above`)
          // Fail here rather than after the agent took (and uploaded) a
          // screenshot; the upload route re-checks under its row lock.
          const published = current ?? []
          const replaces = published.some(
            (result) => result?.topic === topic && result?.label === label
          )
          if (!replaces && published.length >= SESSION_RESULTS_MAX) {
            return err(
              new Error(
                `This run already published ${SESSION_RESULTS_MAX} results. Remove one first (remove: true with its topic and label).`
              )
            )
          }
          const { token, expiresAt } = mintSessionResultToken({
            sessionId,
            topic,
            label,
            userId: user.id,
          })
          // Same origin rule as attachments_get: behind a TLS-terminating
          // proxy the request origin is plain http, which a bare `curl -F`
          // would post to a redirect.
          const origin = process.env.BETTER_AUTH_URL
            ? appBaseUrl()
            : new URL(request.url).origin
          const uploadUrl = `${origin}/api/session-results/${token}`
          return ok({
            uploadUrl,
            expiresAt: expiresAt.toISOString(),
            curl: `curl -sS --retry 4 -F file=@screenshot.png "${uploadUrl}"`,
            topic,
            label,
            ...(prSync === `synced` ? { pr: `synced` } : {}),
            ...(missingFromDiff ? { missingFromDiff, note: MISSING_NOTE } : {}),
            results: resultsSummary(current),
          })
        } catch (e) {
          return err(e)
        }
      }
    server.registerTool(
      `exponential_sessions_guide`,
      {
        description: GUIDE_TOOL_DESCRIPTION,
        _meta: ALWAYS_LOAD_META,
        inputSchema: guideInput,
      },
      guideHandler
    )
    server.registerTool(
      `exponential_sessions_results`,
      {
        description: `Old name of exponential_sessions_guide; same input and answer.`,
        inputSchema: guideInput,
      },
      guideHandler
    )
  }

  // EXP-1172: the EARLY form of sessions_guide: one picture into the same
  // `coding_sessions.results` list (topic default `Progress`) with `inline:
  // true` + the caption, so the run's transcript renders it at this call (the
  // answer's `id` = the attachment id, which the engine's preview carries)
  // and the Guide folds it under "Earlier". ONE write behind both
  // tools (`publishSessionResultPicture`): `dataBase64` lands now, `file`
  // mints the same HMAC upload grant with a pre-allocated id.
  if (gates.sessionResults) {
    server.registerTool(
      `exponential_sessions_show`,
      {
        description: `Show a screenshot in your run's transcript now (when a picture helps): file = a local image path, answers a curl line to run; or dataBase64 + contentType. text = caption. Also filed under Guide (topic default 'Progress', folded under Earlier).`,
        _meta: ALWAYS_LOAD_META,
        inputSchema: strictInput({
          file: z.string().trim().min(1).max(1024).optional(),
          dataBase64: z.string().min(1).optional(),
          contentType: z.string().max(100).optional(),
          topic: z.string().trim().min(1).max(SESSION_RESULT_TEXT_MAX).optional(),
          label: z.string().trim().min(1).max(SESSION_RESULT_TEXT_MAX).optional(),
          text: z.string().max(SESSION_RESULT_CAPTION_MAX).optional(),
        }),
      },
      async ({ file, dataBase64, contentType, topic: topicInput, label, text }) => {
        try {
          if (!sessionId) {
            return err(
              new Error(
                `No coding session: exponential_sessions_show only works inside a session started by the Exponential launcher (missing X-Exp-Session-Id).`
              )
            )
          }
          if ((file === undefined) === (dataBase64 === undefined)) {
            return err(new Error(`Pass exactly one of file or dataBase64.`))
          }
          if (dataBase64 !== undefined && !contentType) {
            return err(new Error(`dataBase64 needs contentType (image/png, image/jpeg or image/webp).`))
          }
          const [row] = await db
            .select({
              id: codingSessions.id,
              teamId: codingSessions.teamId,
              userId: codingSessions.userId,
              hostUserId: codingSessions.hostUserId,
              status: codingSessions.status,
              results: codingSessions.results,
            })
            .from(codingSessions)
            .where(eq(codingSessions.id, sessionId))
            .limit(1)
          if (!row?.teamId) return err(new Error(`Session not found`))
          if (row.userId !== user.id && row.hostUserId !== user.id) {
            return err(new Error(`This is not your run.`))
          }
          if (row.status === `ended`) {
            return err(new Error(`This run has ended, so it can no longer show pictures.`))
          }
          const topic = topicInput ?? SESSION_SHOW_DEFAULT_TOPIC
          const caption = text?.trim() || null
          const published = row.results ?? []
          const replaces =
            label !== undefined &&
            published.some((result) => result?.topic === topic && result?.label === label)
          if (!replaces && published.length >= SESSION_RESULTS_MAX) {
            return err(
              new Error(
                `This run already published ${SESSION_RESULTS_MAX} results. Remove one first (exponential_sessions_guide with remove: true).`
              )
            )
          }

          if (dataBase64 !== undefined) {
            const prepared = await prepareSessionImageBytes(
              {
                filename: `show.${contentType?.split(`/`)[1] ?? `png`}`,
                contentType: contentType ?? ``,
                body: new Uint8Array(Buffer.from(dataBase64, `base64`)),
              },
              { teamId: row.teamId, sessionId },
              undefined,
              { imagesOnly: true }
            )
            const written = await publishSessionResultPicture(
              {
                sessionId,
                teamId: row.teamId,
                topic,
                label: label ?? null,
                uploaderId: user.id,
                inline: { caption },
              },
              prepared
            )
            return ok({
              id: prepared.attachmentId,
              topic,
              label: written.label,
              results: resultsSummary(written.results),
            })
          }

          const attachmentId = crypto.randomUUID()
          const { token, expiresAt } = mintSessionResultToken({
            sessionId,
            topic,
            label: label ?? ``,
            userId: user.id,
            show: { attachmentId, caption },
          })
          const origin = process.env.BETTER_AUTH_URL
            ? appBaseUrl()
            : new URL(request.url).origin
          const uploadUrl = `${origin}/api/session-results/${token}`
          const path = `'${(file ?? ``).replace(/'/g, `'\\''`)}'`
          return ok({
            id: attachmentId,
            uploadUrl,
            expiresAt: expiresAt.toISOString(),
            curl: `curl -sS --retry 4 -F file=@${path} "${uploadUrl}"`,
            topic,
            results: resultsSummary(published),
          })
        } catch (e) {
          return err(e)
        }
      }
    )
  }

  // EXP-988/EXP-936: the run asks its HOST to compact its context. Same gate
  // as sessions_guide: the tool acts on the caller's own run, so a caller
  // with no run of its own (a human's MCP client) never sees it. The request
  // is relayed as a `compact_request` steer frame and executed by the device
  // at the next turn boundary; the handler file owns ownership, the refusal
  // codes and the relay hop.
  if (gates.sessionResults) {
    server.registerTool(
      `exponential_sessions_compact`,
      {
        description: `Ask the host to compact this run's context at the next turn boundary. reason is logged on the run; keep names what the summary must preserve (open threads, decisions, file paths). Returns accepted, or refusedBecause: too_early (under half the context used), cooldown (compacted within the last 20 turns), not_own_session, unsupported_agent. Only your own run.`,
        inputSchema: strictInput({
          reason: z.string().trim().min(1).max(500),
          keep: z.string().trim().min(1).max(2_000).optional(),
        }),
      },
      async ({ reason, keep }) => {
        try {
          if (!sessionId) {
            return err(
              new Error(
                `No coding session: exponential_sessions_compact only works inside a session started by the Exponential launcher (missing X-Exp-Session-Id).`
              )
            )
          }
          return ok(
            await requestSessionCompaction({
              sessionId,
              userId: user.id,
              reason,
              keep,
            })
          )
        } catch (e) {
          return err(e)
        }
      }
    )
  }

  // EXP-660: the session read side. No tRPC list/get exists (clients read the
  // Electric shape), so these are direct reads over the SAME predicate the
  // shape uses: the caller's teams minus trashed/archived boards.
  server.registerTool(
    `exponential_sessions_list`,
    {
      annotations: READ_ONLY,
      description: `List coding sessions (newest first) across your teams or one team: status (in_review = PR open, still live), agentBusy (working now), issue, action, branch, device, blocked (usage-wall refusal, see exponential_sessions_get), parentSessionId (the run that started it), endedBy. mine = runs you started or host.`,
      // EXP-1183: the MCP Apps run list (lib/mcp/apps.ts).
      _meta: appMeta(`runs`),
      inputSchema: strictInput({
        teamId: uuidString.optional(),
        status: z.enum([`running`, `in_review`, `ended`]).optional(),
        mine: z.boolean().default(false),
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      }),
    },
    async ({ teamId, status, mine, limit, offset }) => {
      try {
        let teamIds: string[]
        if (teamId) {
          assertTeamVisible(access, teamId)
          await resolveTeamAccess(user.id, teamId)
          teamIds = [teamId]
        } else {
          teamIds = filterVisibleTeamIds(access, await getUserTeamIds(user.id))
          if (teamIds.length === 0) return ok([])
        }
        // EXP-639: a board-confined grant sees THAT board's runs, not the
        // team's other boards' — team visibility alone is the host-team read
        // the grant hands out for aux lookups, never a licence to list. The
        // ONE encoding lives in scope.ts (grantScopeFilter/isRowGranted); the
        // owner columns are what keeps the board-less runs such a grant may
        // START (a batch spanning its boards) readable by their starter.
        const grantFilter = grantScopeFilter(access, {
          boardCol: codingSessions.boardId,
          teamCol: codingSessions.teamId,
          ownerCols: [codingSessions.userId, codingSessions.hostUserId],
          userId: user.id,
        })
        if (grantFilter === GRANT_MATCHES_NOTHING) return ok([])
        const rows = await db
          .select(sessionColumns)
          .from(codingSessions)
          .leftJoin(issues, eq(issues.id, codingSessions.issueId))
          .where(
            and(
              inArray(codingSessions.teamId, teamIds),
              isNull(codingSessions.boardDeletedAt),
              isNull(codingSessions.boardArchivedAt),
              grantFilter,
              status ? eq(codingSessions.status, status) : undefined,
              mine
                ? or(
                    eq(codingSessions.userId, user.id),
                    eq(codingSessions.hostUserId, user.id)
                  )
                : undefined
            )
          )
          .orderBy(desc(codingSessions.startedAt))
          .limit(limit)
          .offset(offset)
        return ok(rows)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_sessions_get`,
    {
      annotations: READ_ONLY,
      description: `Get one coding session by id. Poll it after exponential_sessions_start: status running → in_review (PR open, still live) → ended; endedBy = who ended it; agentBusy = working now. ackedAt = the device's liveness ack, stamped seconds after launch; null for minutes = the launch died. blocked is set only when the agent itself REFUSED a call at its usage wall (never for a usage warning): blocked.window (session = 5h, weekly, model) and blocked.resetsAt describe the SAME window; the run stays running and clears it on its next successful turn. pendingQuestion = the question it parked (needsInput); answer with exponential_sessions_message. waitForIdle: hold the call until the turn ends, it asks, hits its wall or ends (timeoutS, default 60, max 120; a run idle at the call first gets 10s to start the turn your message began); answers waited + timedOut. What it said: exponential_sessions_messages.`,
      inputSchema: strictInput({
        id: uuidString,
        waitForIdle: z.boolean().optional(),
        timeoutS: z
          .number()
          .int()
          .min(1)
          .max(SESSION_WAIT_MAX_S)
          .default(SESSION_WAIT_DEFAULT_S),
      }),
      _meta: appMeta(`run`),
    },
    async ({ id, waitForIdle, timeoutS }) => {
      try {
        // `hostUserId` is read for the grant predicate only — it is a
        // server-only column and never reaches the response.
        const loadRow = async () => {
          const [found] = await db
            .select({
              ...sessionColumns,
              hostUserId: codingSessions.hostUserId,
              // EXP-879: the pictures THIS run published. Read here only —
              // the list tool would ship every run's whole array on every
              // page.
              results: codingSessions.results,
            })
            .from(codingSessions)
            .leftJoin(issues, eq(issues.id, codingSessions.issueId))
            .where(
              and(
                eq(codingSessions.id, id),
                isNull(codingSessions.boardDeletedAt),
                isNull(codingSessions.boardArchivedAt)
              )
            )
            .limit(1)
          return found
        }
        let row = await loadRow()
        if (!row) throw new Error(`Session not found`)
        // EXP-639: the grant confines every read — a connection consented to
        // one board must not read the run its teammate (or it, from another
        // client) started on a sibling board. Board-less runs of the caller's
        // own stay readable inside a visible team, because such a grant can
        // START them. Denied reads as not-found, like a trashed board's row.
        if (!isRowGranted(access, row, user.id)) {
          throw new Error(`Session not found`)
        }
        // Published pictures come back as readable URLs, never as the raw
        // attachment ids the column stores.
        const resultsOrigin = process.env.BETTER_AUTH_URL
          ? appBaseUrl()
          : new URL(request.url).origin
        if (row.userId !== user.id) {
          if (!row.teamId) throw new Error(`Session not found`)
          await resolveTeamAccess(user.id, row.teamId)
        }
        // EXP-1216: a starter that just messaged the run waits HERE for the
        // reply instead of polling: the row is re-read every 2s until it
        // settles or the timeout passes. Access was decided on the first
        // read; the id never changes.
        let waited = false
        let timedOut = false
        if (waitForIdle) {
          const deadline =
            Date.now() + (timeoutS ?? SESSION_WAIT_DEFAULT_S) * 1000
          // Idle on the first read: the message just sent may not have
          // reached the agent yet. Give the turn a grace window to start
          // (agentBusy flips, or the row moves on) before trusting "idle".
          if (sessionIdleButLive(row)) {
            const graceEnd = Math.min(deadline, Date.now() + SESSION_WAIT_GRACE_MS)
            const firstUpdatedAt = rowTime(row.updatedAt)
            while (true) {
              const left = graceEnd - Date.now()
              if (left <= 0) break
              await sleep(Math.min(SESSION_WAIT_POLL_MS, left))
              waited = true
              const next = await loadRow()
              if (!next) throw new Error(`Session not found`)
              row = next
              const updatedAt = rowTime(row.updatedAt)
              if (
                !sessionIdleButLive(row) ||
                (updatedAt !== null &&
                  firstUpdatedAt !== null &&
                  updatedAt > firstUpdatedAt)
              ) {
                break
              }
            }
          }
          while (!sessionSettled(row)) {
            const left = deadline - Date.now()
            if (left <= 0) {
              timedOut = true
              break
            }
            await sleep(Math.min(SESSION_WAIT_POLL_MS, left))
            waited = true
            const next = await loadRow()
            if (!next) throw new Error(`Session not found`)
            row = next
          }
        }
        const { hostUserId: _hostUserId, results, ...session } = row
        // EXP-1183: the run's page in the app (the MCP Apps view's "Open").
        const [team] = row.teamId
          ? await db
              .select({ slug: teams.slug })
              .from(teams)
              .where(eq(teams.id, row.teamId))
              .limit(1)
          : []
        return ok({
          ...session,
          ...(waitForIdle ? { waited, timedOut } : {}),
          url: team
            ? `${resultsOrigin}/t/${encodeURIComponent(team.slug)}/sessions/${row.id}`
            : null,
          // EXP-933: a topic's report text rides beside its pictures. A text
          // entry has no attachment, so it carries only `topic` + `text`, no
          // `label`/`url`/`attachmentId`; a picture's `label` is emitted
          // when set.
          results: (results ?? []).map((result) =>
            isTextEntry(result)
              ? {
                  topic: result.topic,
                  text: result.text ?? ``,
                  // EXP-1183: the paths the topic touched (the Guide's file
                  // rows, here and in the MCP Apps run view).
                  ...(Array.isArray(result.files) && result.files.length > 0
                    ? { files: result.files }
                    : {}),
                }
              : {
                  topic: result.topic,
                  ...(typeof result.label === `string` ? { label: result.label } : {}),
                  url: `${resultsOrigin}/api/attachments/${result.attachmentId}`,
                }
          ),
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-1216: what a run SAID. Transcripts never reach the database (they
  // live on the run's device), so the read joins the run's relay room as a
  // viewer (lib/steer-transcript.ts) and projects the top-level transcript
  // (lib/steer-transcript-project.ts). Owner or host only, like the steer
  // ticket it mints (EXP-312): a teammate sees the status, never the words.
  server.registerTool(
    `exponential_sessions_messages`,
    {
      annotations: READ_ONLY,
      description: `Read what a coding session you own or host said: its top-level transcript (user, assistant, tool, question; subagents left out), ascending seq. since = the nextSince you got last time (only newer messages come back; growing prose comes back whole), limit = the newest N after it (default 50). truncated = older messages were cut. live = the run has not ended. Streams off the run's device via the relay: errors when remote steer is off, or the device is offline or has no history. Wait for a reply with exponential_sessions_get waitForIdle.`,
      inputSchema: strictInput({
        id: uuidString,
        since: z.number().int().min(0).optional(),
        limit: z.number().int().min(1).max(200).default(50),
      }),
    },
    async ({ id, since, limit }) => {
      try {
        const [row] = await db
          .select({
            id: codingSessions.id,
            teamId: codingSessions.teamId,
            boardId: codingSessions.boardId,
            userId: codingSessions.userId,
            hostUserId: codingSessions.hostUserId,
            status: codingSessions.status,
            deviceId: codingSessions.deviceId,
          })
          .from(codingSessions)
          .where(
            and(
              eq(codingSessions.id, id),
              isNull(codingSessions.boardDeletedAt),
              isNull(codingSessions.boardArchivedAt)
            )
          )
          .limit(1)
        if (!row) throw new Error(`Session not found`)
        // Same grant predicate as sessions_get (EXP-639): out of grant reads
        // as not found.
        if (!isRowGranted(access, row, user.id)) {
          throw new Error(`Session not found`)
        }
        if (row.userId !== user.id && row.hostUserId !== user.id) {
          throw new Error(
            `Only the session owner or host can read its transcript`
          )
        }
        // steer.mintTicket's rule: a requester on someone else's shared
        // device must still be a member of the run's team.
        if (
          row.userId === user.id &&
          row.hostUserId !== null &&
          row.hostUserId !== user.id
        ) {
          await assertTeamMember(user.id, row.teamId)
        }
        const config = getSteerRelayConfig()
        if (!config) {
          throw new Error(
            `Remote steer is off on this instance (no steer relay), so run transcripts cannot be read. Use exponential_sessions_get.`
          )
        }
        const ended = row.status === `ended`
        let read
        try {
          read = await readRunTranscript(config, {
            sessionId: row.id,
            teamId: row.teamId ?? ``,
            ownerUserId: user.id,
            // Ended only (steer.mintTicket, EXP-773): the relay asks THIS
            // device to republish its journal.
            ...(ended && row.deviceId
              ? {
                  deviceId: row.deviceId,
                  deviceOwnerId: row.hostUserId ?? row.userId,
                }
              : {}),
          })
        } catch (e) {
          if (e instanceof TranscriptReadError) throw new Error(e.message)
          throw e
        }
        const all = projectTranscript(read.events)
        const newer =
          since === undefined ? all : all.filter((m) => m.seq > since)
        const messages = newer.slice(-limit)
        const newest =
          all.length > 0 ? Math.max(...all.map((m) => m.seq)) : null
        return ok({
          id: row.id,
          live: !ended,
          messages,
          lastSeq: read.lastSeq,
          // Never hand back a cursor below the one passed in (a stale or
          // restarted seq would otherwise re-deliver consumed messages).
          nextSince:
            newest === null
              ? (since ?? null)
              : since === undefined
                ? newest
                : Math.max(newest, since),
          // The relay keeps a tail; older pages exist only on the device.
          truncated:
            newer.length > messages.length ||
            (read.truncated && (since === undefined || since < read.firstSeq)),
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_sessions_kill`,
    {
      description: `Abort a live coding session you own or host: the row flips to ended (endedBy user) and the device tears the agent down. Idempotent. Never your own run — it ends on its own exit or close-out.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        // The desktop reads its own row's →ended edge as the kill switch, so
        // an agent killing itself would vanish mid-call without its
        // close-out — refuse and point at the proper exit.
        if (sessionId && id === sessionId) {
          throw new Error(
            `That is your own session: finish your work and exit instead of killing it.`
          )
        }
        if (!access.full) {
          // Same grant predicate as the read side (EXP-639): killing a run
          // on a board this connection was never granted is out of scope,
          // even inside a team it can otherwise see.
          const [row] = await db
            .select({
              teamId: codingSessions.teamId,
              boardId: codingSessions.boardId,
              userId: codingSessions.userId,
              hostUserId: codingSessions.hostUserId,
            })
            .from(codingSessions)
            .where(eq(codingSessions.id, id))
            .limit(1)
          if (!row) throw new Error(`Session not found`)
          if (!isRowGranted(access, row, user.id)) {
            throw new Error(`Session not found`)
          }
        }
        // Owner-or-host, idempotency and the best-effort relay kill all live
        // in steer.killSession; its row is projected — never returned raw.
        const result = await caller(user, request).steer.killSession({
          sessionId: id,
        })
        // EXP-700: a killed run is an agent-started child that will never
        // send its close-out — tell a live parent instead of leaving it
        // waiting forever. `txId` is set only by the call that actually
        // flipped the row, so a repeated (idempotent) kill notifies once.
        // Best-effort: internally caught and relay-timeout bounded.
        if (result.txId != null) {
          await notifyParentOfChildEnd(db, id, {
            summary: null,
            endedBy: `user`,
          })
        }
        const note = await killedWorktreeNote(result.session)
        return ok({
          ok: true,
          id,
          status: result.session.status,
          endedAt: result.session.endedAt,
          ...(note ? { note } : {}),
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-700: send text into a live session's agent — the parent's half of
  // the ask/answer rail (a child asks via exponential_sessions_ask_parent),
  // and a generic owner-scoped steer. Unconditional and deferred on purpose:
  // a header-less expu_ orchestrator must be able to answer runs it started.
  server.registerTool(
    `exponential_sessions_message`,
    {
      description: `Send text into a live coding session you own or host, as user input prefixed with its source: answer a child's exponential_sessions_ask_parent question (id = the child's UUID) or steer a run you started. Never your own session. queued = mid-turn, consumed = a turn started; consumed false = nobody reads it: kill, resumeSessionId.`,
      inputSchema: strictInput({
        id: uuidString,
        message: z.string().min(1).max(4_000),
      }),
    },
    async ({ id: targetId, message }) => {
      try {
        if (sessionId && targetId === sessionId) {
          throw new Error(
            `That is your own session: you cannot message yourself.`
          )
        }
        const [row] = await db
          .select({
            teamId: codingSessions.teamId,
            boardId: codingSessions.boardId,
            userId: codingSessions.userId,
            hostUserId: codingSessions.hostUserId,
            status: codingSessions.status,
            parentSessionId: codingSessions.parentSessionId,
            // FEED-60: mid-turn before the injection = the text queues.
            agentBusy: codingSessions.agentBusy,
          })
          .from(codingSessions)
          .where(eq(codingSessions.id, targetId))
          .limit(1)
        if (!row) throw new Error(`Session not found`)
        // Same grant predicate as kill (EXP-639): out-of-grant reads as
        // not found, never as forbidden.
        if (!access.full && !isRowGranted(access, row, user.id)) {
          throw new Error(`Session not found`)
        }
        if (row.userId !== user.id && row.hostUserId !== user.id) {
          throw new Error(`Only the session owner or host can message it`)
        }
        if (!(PARENT_LIVE_STATUSES as readonly string[]).includes(row.status)) {
          throw new Error(`Session is not live`)
        }
        // A parent answering its own child gets the answer prefix the ask
        // told the child to expect; every other caller is "your starter".
        const text =
          sessionId && row.parentSessionId === sessionId
            ? formatParentAnswer(sessionId, message)
            : formatStarterMessage(message)
        const config = getSteerRelayConfig()
        if (!config) throw new Error(`The steer relay is not configured`)
        const { delivered } = await relayPostInput(config, targetId, text)
        if (!delivered) {
          throw new Error(
            `Not delivered: the session's device is not connected to the relay. The run may still be starting or its device offline; retry, or fall back to exponential_sessions_get.`
          )
        }
        // EXP-1065: a delivered message IS the answer to the run's open
        // question (`ask_parent`): off the row.
        const { answerPendingQuestion } = await import(`@/lib/sessions/answer-pending-question`)
        await answerPendingQuestion(db, targetId)
        // FEED-60: `delivered` only means the relay found the run's device
        // socket. Say whether the agent actually picked the text up.
        if (row.agentBusy) {
          return ok({
            ok: true,
            id: targetId,
            delivered: true,
            queued: true,
            note: `The agent is mid-turn; the message lands when it reads it (claude replays it into the running turn).`,
          })
        }
        const consumed = await waitForAgentTurn(targetId)
        if (consumed) {
          return ok({ ok: true, id: targetId, delivered: true, consumed: true })
        }
        return ok({
          ok: true,
          id: targetId,
          delivered: true,
          consumed: false,
          note: `The run's agent did not start a turn within ${MESSAGE_CONSUME_WAIT_MS / 1000}s: the text reached its device but nothing is reading it. Kill the run (exponential_sessions_kill) and start it again with resumeSessionId (the resumed run continues the transcript), then message the new run.`,
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-552: remote start over the steer rails, the way the Start-coding
  // dialog and the mobile clients do it. steer.startSession validates the
  // one-of rule, the per-agent model/effort vocabulary, the device's caps and
  // installed agents, and resolves repos; a 404 from the relay means the
  // device is offline. The device then creates the coding_sessions row
  // itself, so the tool waits briefly for it to appear and hands back its id.
  server.registerTool(
    `exponential_sessions_start`,
    {
      description: `Start a coding session on an ONLINE device (exponential_devices_list); offline = refused. One subject: issueId (UUID/identifier), issueIds (one batch PR), actionId (+teamId for builtins, inputs) or resumeSessionId (ended run; + account = switch a live claude run's account). account = a profile id from agentAccounts.<agent>.profiles[]. prompt = free text (REQUIRED for builtin:chat / builtin:create-action). A follow-up run bases on your branch: name it in \`prompt\`, the child opens with \`pr_open{base}\`. Track it with exponential_sessions_get. A child started from a run is unattended: its question, finish or usage wall (wait it out) arrives as '[Exponential child run ...]' input; answer with exponential_sessions_message. Read its report before merging.`,
      inputSchema: strictInput({
        deviceId: z.string().min(1).max(128),
        issueId: z.string().min(1).optional(),
        issueIds: z.array(z.string().min(1)).min(1).max(30).optional(),
        actionId: z.string().min(1).optional(),
        teamId: uuidString.optional(),
        inputs: z.record(z.string(), z.string()).optional(),
        resumeSessionId: uuidString.optional(),
        agent: z.enum(codingAgentValues).optional(),
        model: z.string().max(64).optional(),
        effort: z.string().max(32).optional(),
        planMode: z.boolean().optional(),
        ultracode: z.boolean().optional(),
        allowRateLimited: z.boolean().optional(),
        prompt: z.string().max(MAX_START_PROMPT).optional(),
        // EXP-906: the agent account profile on the target device — the
        // same field steer.startSession takes (absent = the machine's last
        // used login, `system` = the ambient one). Without it an orchestrator
        // whose last used profile is walled had to route around this tool
        // (and lose the parent link) to launch on another account.
        account: z.string().min(1).max(64).optional(),
      }),
    },
    async (input) => {
      try {
        const startedAfter = new Date()
        let issueId: string | undefined
        let issueIds: string[] | undefined
        // The row the device will create, by subject — what the poll below
        // keys on besides user, device and freshness.
        let match: SQL | undefined
        if (input.issueId) {
          issueId = await resolveIssueId(input.issueId, user.id, access)
          const ctx = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctx.boardId, ctx.teamId)
          match = eq(codingSessions.issueId, issueId)
        } else if (input.issueIds) {
          // EXP-707: identifiers accepted here like pr_open/pr_merge.
          issueIds = await Promise.all(
            input.issueIds.map((id) => resolveIssueId(id, user.id, access))
          )
          const contexts = await Promise.all(
            issueIds.map((id) => getIssueTeamContext(id))
          )
          for (const ctx of contexts) {
            assertBoardGranted(access, ctx.boardId, ctx.teamId)
          }
          match = batchStartRowMatch(
            [...new Set(contexts.map((ctx) => ctx.teamId))],
            issueIds
          )
        } else if (input.actionId) {
          if (isBuiltinActionId(input.actionId)) {
            // The router requires teamId here; a builtin run is an issue-less
            // row carrying the action name snapshot but no action FK.
            if (input.teamId) assertTeamFullyGranted(access, input.teamId)
            match = and(
              input.teamId ? eq(codingSessions.teamId, input.teamId) : undefined,
              isNull(codingSessions.issueId),
              isNotNull(codingSessions.actionName)
            )
          } else {
            const action = await getActionContext(input.actionId)
            assertTeamFullyGranted(access, action.teamId)
            match = eq(codingSessions.actionId, input.actionId)
          }
        } else if (input.resumeSessionId) {
          // Same predicate as the read side: a board-confined grant may
          // relaunch a run it can also get/list/kill — including the
          // board-less ones it started itself (steer.startSession keeps the
          // resume owner-only on top).
          const [row] = await db
            .select({
              teamId: codingSessions.teamId,
              boardId: codingSessions.boardId,
              userId: codingSessions.userId,
              hostUserId: codingSessions.hostUserId,
            })
            .from(codingSessions)
            .where(eq(codingSessions.id, input.resumeSessionId))
            .limit(1)
          if (!row) throw new Error(`Session not found`)
          if (!isRowGranted(access, row, user.id)) {
            throw new Error(`Session not found`)
          }
          match = eq(codingSessions.resumedFromId, input.resumeSessionId)
        } else {
          throw new Error(
            `Exactly one of issueId, issueIds, actionId or resumeSessionId is required`
          )
        }

        const started = await caller(user, request).steer.startSession({
          ...input,
          issueId,
          issueIds,
          // EXP-679: a run started from inside a run is that run's child.
          ...(sessionId ? { parentSessionId: sessionId } : {}),
        })

        // The relay accepted the frame; the desktop registers the run via
        // codingSessions.start moments later. Wait for it so the caller can
        // track the run by id instead of guessing from a list.
        const deadline = Date.now() + SESSION_START_POLL_MS
        let session: Record<string, unknown> | null = null
        for (;;) {
          const [row] = await db
            .select(sessionColumns)
            .from(codingSessions)
            .leftJoin(issues, eq(issues.id, codingSessions.issueId))
            .where(
              and(
                eq(codingSessions.userId, user.id),
                eq(codingSessions.deviceId, input.deviceId),
                eq(codingSessions.status, `running`),
                gte(codingSessions.createdAt, startedAfter),
                match
              )
            )
            .orderBy(desc(codingSessions.createdAt))
            .limit(1)
          if (row) {
            // EXP-679: the device creates the row, so the parent link is
            // stamped here — history only, never worth failing the start.
            if (sessionId) {
              try {
                await stampChildOfRun(row, sessionId)
              } catch {
                // ignored
              }
            }
            session = row
            break
          }
          // FEED-63: the device reported why it could not launch the run.
          const failure = startFailureMessage(started, input.deviceId, user.id)
          if (failure) return err(new Error(failure))
          if (Date.now() >= deadline) break
          await sleep(SESSION_START_POLL_STEP_MS)
        }
        // FEED-57/46: no row = the device took no run off the frame (it
        // refused or dropped it). Never an `ok` with a null id: the caller
        // would wait on a run that does not exist.
        if (!session) {
          const failure = startFailureMessage(started, input.deviceId, user.id)
          return err(
            new Error(failure ?? noRunReportedMessage(input.deviceId))
          )
        }
        return ok({
          ok: true,
          deviceId: input.deviceId,
          sessionId: session.id as string,
          session,
          // EXP-700: a child started from inside a run reports back
          // event-based (every supported device brands it agent-started —
          // steer.startSession refuses a host that does not).
          ...(sessionId && session
            ? {
                note: `The child reports into this session as a bracketed [Exponential child run ...] user message when it finishes, asks a question, or hits a real usage wall ('... is rate limited (<window> window) until <resetsAt>' — wait until resetsAt, it is not a failure); answer questions with exponential_sessions_message. Poll exponential_sessions_get only as a fallback.`,
              }
            : {}),
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Devices (EXP-660: the picker for exponential_sessions_start)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_devices_list`,
    {
      annotations: READ_ONLY,
      description: `List your registered machines (desktop app / CLI daemon), plus servers teammates shared with teamId. Pick an online device whose agents includes the agent you want; caps must include resume-run to resume an ended run. agentUsage.<agent> = its last used login: windows[] (percent + resetsAt), fetchedAt = when those numbers were read, stale: true = the last refresh failed and they are as old as fetchedAt; agentUsageAt = when the device last reported. A session running on another account moves that account's own row under agentAccounts.<agent>.profiles[].usage instead. A live session refreshes only the account it runs on, per turn; once it ends — or a window's resetsAt passes — that login returns to the polled cadence. doctor = its readiness report: items[] {key, state ok|action|missing|off|error, detail, action} (null = older build).`,
      // EXP-1183: the MCP Apps devices view (lib/mcp/apps.ts).
      _meta: appMeta(`devices`),
      inputSchema: strictInput({
        teamId: uuidString.optional(),
        ...pageInput,
      }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        // Shared rows name teammates' machines and agents — team-level
        // operational data, gated like actions_list, and readable only by a
        // member of that team.
        if (teamId) {
          assertTeamFullyGranted(access, teamId)
          await assertTeamMember(user.id, teamId)
        }

        // Own rows plus the team's shared servers — the ONE encoding of
        // that query lives in the devices router (visibleDeviceRows).
        const { rows, ownerNames } = await visibleDeviceRows(
          db,
          user.id,
          teamId
        )
        // `online` is last_seen_at freshness (contract onlineWindowSeconds),
        // not relay presence — the same rule every synced client applies.
        const list = composeDeviceList(
          rows,
          ownerNames,
          new Date(),
          user.id,
          teamId
        )

        return ok(
          page(list, limit, offset).map((device) => ({
            deviceId: device.deviceId,
            label: device.deviceLabel,
            kind: device.kind,
            platform: device.platform ?? null,
            online: device.online,
            lastSeenAt: device.lastSeenAt,
            agents: device.agents,
            unauthedAgents: device.unauthedAgents,
            caps: device.caps,
            version: device.version,
            sharedTeamIds: device.sharedTeamIds,
            isDefault: device.isDefault,
            // EXP-484: per-agent sign-in status and usage windows as the
            // machine last probed them (absent on builds without the
            // collector).
            agentAccounts: device.agentAccounts ?? null,
            agentUsage: device.agentUsage ?? null,
            agentUsageAt: device.agentUsageAt ?? null,
            // EXP-1196: the readiness report (device-doctor.json).
            doctor: device.doctor ?? null,
            ...(device.owner ? { owner: device.owner } : {}),
          }))
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-1199: sign an agent account in on one of YOUR machines — the
  // clients' Add account / Sign in, over the same device commands.
  server.registerTool(
    `exponential_devices_account_login`,
    {
      description: `Sign an agent account in on one of your own machines (exponential_devices_list: online, caps has agent-login). The machine runs the agent CLI's own login; only the sign-in URL and the code you type travel, the credential never leaves the machine. Start: {deviceId, agent} adds an account (name = its label, default "<agent> account N"; the default login while it is signed out), or profileId re-signs an existing login (agentAccounts.<agent>.profiles[].id, e.g. one that needs a re-login). Returns status url + url (+ code for codex: enter it at url). Claude: the browser then shows a code, call again with {deviceId, agent, code}. status pending = the machine has not answered yet, call again with commandId. The login appears in exponential_devices_list after the machine's next heartbeat.`,
      inputSchema: strictInput({
        deviceId: z.string().min(1).max(128),
        agent: z.enum(codingAgentValues),
        name: z.string().trim().min(1).max(64).optional(),
        profileId: z.string().min(1).max(64).optional(),
        code: z.string().trim().min(1).max(512).optional(),
        commandId: uuidString.optional(),
      }),
    },
    async (input) => {
      try {
        // A machine's logins sit outside any selectable grant — full access only.
        assertFullAccess(access)
        const trpc = caller(user, request)
        return ok(
          await deviceAccountLogin(input, {
            loadAccounts: async (deviceId) => {
              const [row] = await db
                .select({ agentAccounts: devices.agentAccounts })
                .from(devices)
                .where(
                  and(eq(devices.userId, user.id), eq(devices.deviceId, deviceId))
                )
                .limit(1)
              return row ? (row.agentAccounts ?? null) : undefined
            },
            createCommand: (command) => trpc.devices.createCommand(command),
            getCommand: (commandId) => trpc.devices.getCommand({ commandId }),
          })
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Comments (edit / delete)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_comments_update`,
    {
      description: `Edit the body of an existing comment (by its UUID). Only the comment's author can edit it. Body is plain text; the edit stamps editedAt.`,
      inputSchema: strictInput({
        id: uuidString,
        body: z.string().trim().min(1).max(10_000).describe(`Plain GFM text`),
        attachmentIds: z.array(uuidString).max(10).optional(),
      }),
    },
    async ({ id, body, attachmentIds }) => {
      try {
        if (!access.full) {
          const ctxIssue = await getCommentIssueContext(id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).comments.update({
          id,
          body,
          ...(attachmentIds ? { attachmentIds } : {}),
        })
        return ok(result.comment)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_comments_delete`,
    {
      description: `Permanently delete a comment (by its UUID). Only the comment's author can delete it.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const ctxIssue = await getCommentIssueContext(id)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).comments.delete({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Subscriptions (follow / unfollow an issue)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_issues_subscribe`,
    {
      description: `Subscribe the MCP user to an issue (by UUID or human identifier, e.g. "MET-12") so they receive its notifications. Idempotent.`,
      inputSchema: strictInput({ issueId: z.string().min(1) }),
    },
    async ({ issueId: issueIdInput }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).subscriptions.subscribe({ issueId })
        return ok({ ok: true, issueId, subscribed: true })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_issues_unsubscribe`,
    {
      description: `Unsubscribe the MCP user from an issue (UUID or identifier). Suppresses auto-resubscribe until they act on the issue again.`,
      inputSchema: strictInput({ issueId: z.string().min(1) }),
    },
    async ({ issueId: issueIdInput }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).subscriptions.unsubscribe({ issueId })
        return ok({ ok: true, issueId, subscribed: false })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Notifications (inbox)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_notifications_list`,
    {
      annotations: READ_ONLY,
      description: `List the MCP user's own notifications, newest first. Set unreadOnly to show only those not yet read.`,
      // EXP-1183: the MCP Apps inbox (lib/mcp/apps.ts).
      _meta: appMeta(`inbox`),
      inputSchema: strictInput({
        unreadOnly: z.boolean().default(false),
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      }),
    },
    async ({ unreadOnly, limit, offset }) => {
      try {
        // Mirror the synced notifications shape: rows from trashed or
        // archived boards are hidden via the trigger-maintained mirrors
        // (issue-less rows keep NULL mirrors and always pass).
        const conditions = [
          eq(notifications.userId, user.id),
          isNull(notifications.boardDeletedAt),
          isNull(notifications.boardArchivedAt),
        ]
        if (unreadOnly) conditions.push(isNull(notifications.readAt))
        if (access.full) {
          const rows = await db
            .select(notificationWireColumns)
            .from(notifications)
            .where(and(...conditions))
            .orderBy(desc(notifications.createdAt))
            .limit(limit)
            .offset(offset)
          return ok(rows)
        }
        // Scoped connection: the inbox spans every team, so join through
        // the notification's issue and keep only granted boards (rows
        // without an issue stay private). The grant filter runs in SQL,
        // BEFORE limit/offset — a post-limit JS filter under-fills pages and
        // makes offset pagination skip in-scope notifications.
        // No owner columns: the inner join already drops issue-less rows,
        // so there is no board-less arm to admit here.
        const grantFilter = grantScopeFilter(access, {
          boardCol: issues.boardId,
          teamCol: boards.teamId,
        })
        if (grantFilter === GRANT_MATCHES_NOTHING) return ok([])
        if (grantFilter) conditions.push(grantFilter)
        const rows = await db
          .select({ notification: notificationWireColumns })
          .from(notifications)
          .innerJoin(issues, eq(notifications.issueId, issues.id))
          .innerJoin(boards, eq(issues.boardId, boards.id))
          .where(and(...conditions))
          .orderBy(desc(notifications.createdAt))
          .limit(limit)
          .offset(offset)
        return ok(rows.map((r) => r.notification))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_notifications_mark_read`,
    {
      description: `Mark one notification read by id, or all unread ones with all=true. Only the MCP user's own notifications are affected.`,
      inputSchema: strictInput({
        id: uuidString.optional(),
        all: z.boolean().default(false),
      }),
    },
    async ({ id, all }) => {
      try {
        if (all) {
          // Marking the whole inbox read touches every team.
          assertFullAccess(access)
          await caller(user, request).notifications.markAllRead()
          return ok({ ok: true, marked: `all` })
        }
        if (!id) {
          throw new Error(`Pass a notification id, or all=true.`)
        }
        if (!access.full) {
          const [row] = await db
            .select({ issueId: notifications.issueId })
            .from(notifications)
            .where(
              and(eq(notifications.id, id), eq(notifications.userId, user.id))
            )
            .limit(1)
          if (!row?.issueId) throw new Error(`Notification not found`)
          const ctxIssue = await getIssueTeamContext(row.issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        await caller(user, request).notifications.markRead({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-801: an agent pings a team member — or its own user — with an inbox
  // row plus a push. Synchronous so the agent learns who got it; a recipient
  // who switched off "messages from teammates' agents" is reported as
  // declined, never written. Per-sender token bucket: the tool is spammable
  // by construction (a loop of sends is one bad prompt away).
  server.registerTool(
    `exponential_notifications_send`,
    {
      description: `Notify people (inbox row + push): a long task finished, a decision is needed, or someone asked to be pinged. recipients = team members' user ids or emails, default yourself. issueId (UUID or identifier) = what the row opens: that issue's Results, which every member may see; default this run's issue. teamId defaults to the issue's team or this run's. A member who turned off messages from teammates' agents is reported as declined; your own user always receives.`,
      inputSchema: strictInput({
        teamId: uuidString.optional(),
        recipients: z.array(z.string().min(1).max(320)).min(1).max(20).optional(),
        title: z.string().min(1).max(120),
        body: z.string().max(2000).optional(),
        issueId: z.string().min(1).max(64).optional(),
      }),
    },
    async ({ teamId: requestedTeamId, recipients: requestedRecipients, title, body, issueId: requestedIssueId }) => {
      try {
        // EXP-933: the target is the AGENT's choice (an issue every recipient
        // may open, never the personal run); unnamed, a run bound to an issue
        // links that issue's Results. The team follows the target, then the
        // run, so a run needs neither id to ping its own person.
        let targetIssueId: string | null = null
        let issueTeamId: string | null = null
        if (requestedIssueId) {
          targetIssueId = await resolveIssueId(requestedIssueId, user.id, access)
        }
        const caller =
          !requestedIssueId || !requestedTeamId ? await loadCallerSession() : null
        if (
          !targetIssueId &&
          caller?.issueId &&
          (!requestedTeamId || requestedTeamId === caller.teamId)
        ) {
          targetIssueId = caller.issueId
        }
        if (targetIssueId) {
          const [target] = await db
            .select({ teamId: boards.teamId })
            .from(issues)
            .innerJoin(boards, eq(boards.id, issues.boardId))
            .where(and(eq(issues.id, targetIssueId), boardVisible()))
            .limit(1)
          if (!target) throw new Error(`Issue not found`)
          issueTeamId = target.teamId
        }
        const teamId = requestedTeamId ?? issueTeamId ?? caller?.teamId ?? null
        if (!teamId) {
          throw new Error(
            `Pass teamId (or issueId): outside a coding session there is no team to default to.`
          )
        }
        if (issueTeamId && issueTeamId !== teamId) {
          throw new Error(`The target issue is not in this team.`)
        }
        const recipients = requestedRecipients ?? [user.id]
        // A team-level WRITE (it pushes to every named member), so it takes
        // the full team grant like invites/actions, not visibility.
        assertTeamFullyGranted(access, teamId)
        await resolveTeamAccess(user.id, teamId)
        const memberRows = await db
          .select({ id: users.id, email: users.email })
          .from(teamMembers)
          .innerJoin(users, eq(users.id, teamMembers.userId))
          .where(eq(teamMembers.teamId, teamId))
        const byId = new Map(memberRows.map((row) => [row.id, row]))
        const byEmail = new Map(
          memberRows.map((row) => [row.email.toLowerCase(), row])
        )
        const unknown: string[] = []
        const resolved = new Map<string, { id: string; email: string }>()
        for (const raw of recipients) {
          const key = raw.trim()
          const member = byId.get(key) ?? byEmail.get(key.toLowerCase())
          if (member) resolved.set(member.id, member)
          else unknown.push(key)
        }
        if (resolved.size === 0) {
          throw new Error(
            `No recipient is a member of this team (use exponential_members_list for ids and emails).`
          )
        }
        // Taken AFTER validation so a rejected call burns no burst token.
        const limit = agentMessageLimiter.tryTake(user.id)
        if (!limit.ok) {
          return err(
            new Error(
              `Too many messages — retry in ${limit.retryAfterSeconds}s`
            )
          )
        }
        const outcome = await sendAgentMessage({
          teamId,
          senderUserId: user.id,
          recipientIds: [...resolved.keys()],
          title: title.trim(),
          body: body?.trim() || null,
          issueId: targetIssueId,
        })
        const describe = (ids: string[]) =>
          ids.map((id) => ({ id, email: resolved.get(id)?.email ?? null }))
        return ok({
          ok: outcome.delivered.length > 0,
          delivered: describe(outcome.delivered),
          declined: describe(outcome.declined),
          deduped: describe(outcome.deduped),
          notMembers: describe(outcome.notMembers),
          unknown,
          opens: targetIssueId ? { issueId: targetIssueId, face: `results` } : null,
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Members (resolve assignees)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_members_list`,
    {
      annotations: READ_ONLY,
      description: `List the members of a team. id is the USER id (use it for assigneeId); memberId is the team_members row id (what teamMembers.updateRole/remove take).`,
      inputSchema: strictInput({
        teamId: uuidString,
        limit: z.number().int().min(1).max(200).default(50),
        offset: z.number().int().min(0).default(0),
      }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        assertTeamVisible(access, teamId)
        await resolveTeamAccess(user.id, teamId)
        const rows = await db
          .select({
            id: users.id,
            memberId: teamMembers.id,
            name: users.name,
            email: users.email,
            image: users.image,
            role: teamMembers.role,
          })
          .from(teamMembers)
          .innerJoin(users, eq(users.id, teamMembers.userId))
          .where(eq(teamMembers.teamId, teamId))
          .orderBy(asc(users.name))
          .limit(limit)
          .offset(offset)
        return ok(rows)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Repositories
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_repositories_list`,
    {
      annotations: READ_ONLY,
      description: `List the repositories registered in a team, each with the boards it backs. The MCP user must be a member of the team.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        assertTeamVisible(access, teamId)
        const result = await caller(user, request).repositories.list({
          teamId,
        })
        if (access.full) return ok(page(result, limit, offset))
        // Each repo rides with the boards it backs — a board-scoped
        // grant must not enumerate ungranted sibling boards through them.
        return ok(
          page(
            result.map((repo) => ({
              ...repo,
              boards: repo.boards.filter((p) =>
                isBoardGranted(access, p.id, teamId)
              ),
            })),
            limit,
            offset
          )
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_repositories_add`,
    {
      description: `Register a GitHub repository ("owner/name") in a team so boards can be backed by it. Any member; the repo must be one YOUR GitHub connection grants (team settings → Repositories) — connecting shares it with the team.`,
      inputSchema: strictInput({
        teamId: uuidString,
        fullName: z
          .string()
          .min(1)
          .max(255)
          .regex(/^[^/\s]+\/[^/\s]+$/, `Expected "owner/name"`),
        defaultBranch: z.string().min(1).max(255).optional(),
        private: z.boolean().optional(),
        installationId: z.number().int().optional(),
      }),
    },
    async (input) => {
      try {
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).repositories.add(input)
        return ok(result.repository)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_repositories_branch_diff`,
    {
      annotations: READ_ONLY,
      description: `Get the diff of an issue's exp/<IDENTIFIER> branch against the repo's default branch (UUID or identifier). Returns null when the branch was never pushed. Team members only.`,
      inputSchema: strictInput({ issueId: z.string().min(1) }),
    },
    async ({ issueId: issueIdInput }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).repositories.branchDiff({
          issueId,
        })
        return ok(result)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Actions (per-team reusable prompts, EXP-253)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_actions_list`,
    {
      annotations: READ_ONLY,
      description: `List a team's actions: reusable markdown prompts run as agent sessions on a member's device, each with its triggers (schedule or event, bound to a device). Team members only.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        // FULL team grant even for the read: action bodies are locally
        // executed operational prompts, not board-workflow aux data — a
        // board-confined OAuth token has no business reading them (the
        // run_configs precedent confined list the same way).
        if (!access.full) assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).actions.list({ teamId })
        // EXP-539: actions.list stopped appending the virtual builtins
        // (native clients construct them locally); agents still need them
        // listed, so this tool appends the three listed ones (FEED-50:
        // + Tidy up, which yields to a team's own "Tidy up" row, SLOP-2).
        return ok(
          page(
            [
              ...result.actions,
              builtinCreateAction(teamId),
              builtinFixConflictsAction(teamId),
              ...(hasOwnTidyUpAction(result.actions)
                ? []
                : [builtinTidyUpAction(teamId)]),
            ],
            limit,
            offset
          )
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_actions_create`,
    {
      description: `Create a team action (owner only). body = the markdown prompt an agent runs locally; repositoryId = its repo; icon = a curated icon name; inputs = pick fields (repo/board/pr/icon) for the prompt; promptPlaceholder = the composer's hint.`,
      _meta: ALWAYS_LOAD_META,
      inputSchema: strictInput({
        teamId: uuidString,
        name: z.string().min(1).max(255),
        description: z.string().nullable().optional(),
        icon: boardIconEnumSchema.nullable().optional(),
        repositoryId: uuidString.nullable().optional(),
        body: z.string().min(1),
        inputs: actionInputsSchema.optional(),
        promptPlaceholder: z.string().max(200).nullable().optional(),
        // FEED-73: undescribed (create is always-loaded, 10k budget);
        // actions_update says what it is, the router caps it at 16.
        mcpServerIds: z.array(uuidString).optional(),
      }),
    },
    async (input) => {
      try {
        if (!access.full) assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).actions.create(input)
        return ok(result.action)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_actions_update`,
    {
      description: `Update an action by UUID (owner only); pass only fields to change. icon: null clears; inputs and triggers: whole-array replace (send every trigger you keep, with its id). A trigger = {deviceId, enabled?, agent?, account?, model?, effort?} plus {kind:schedule,interval:daily|weekly|monthly,minuteOfDay,weekday?,dayOfMonth?} or {kind:event,event:created|status_changed|assignee_changed|label_added|priority_changed|pr_opened|pr_merged,filters?}. account = an agent profile id there (needs agent). An enabled trigger needs every input optional. mcpServerIds = MCP servers its runs use; shared ones join as <server>-as-<member>.`,
      inputSchema: strictInput({
        id: uuidString,
        name: z.string().min(1).max(255).optional(),
        description: z.string().nullable().optional(),
        icon: boardIconEnumSchema.nullable().optional(),
        repositoryId: uuidString.nullable().optional(),
        body: z.string().min(1).optional(),
        inputs: actionInputsSchema.optional(),
        promptPlaceholder: z.string().max(200).nullable().optional(),
        // Loose for the MCP context budget; the strict schema validates
        // below (and again in the router — single source).
        triggers: z.array(z.record(z.string(), z.unknown())).optional(),
        // FEED-73: whole-array replace; the router caps it at 16 and keeps
        // it in-team (described in the tool sentence for the budget).
        mcpServerIds: z.array(uuidString).optional(),
        sortOrder: z.number().finite().optional(),
      }),
    },
    async (input) => {
      try {
        if (!access.full) {
          const action = await getActionContext(input.id)
          assertTeamFullyGranted(access, action.teamId)
        }
        const result = await caller(user, request).actions.update({
          ...input,
          triggers:
            input.triggers === undefined
              ? undefined
              : actionTriggersSchema.parse(input.triggers),
        })
        return ok(result.action)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_actions_delete`,
    {
      description: `Delete an action by its UUID. Live runs keep their action_name label and degrade to batch-shaped rows. Team owner only.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const action = await getActionContext(id)
          assertTeamFullyGranted(access, action.teamId)
        }
        await caller(user, request).actions.delete({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Pull request changed files
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_issues_pr_files`,
    {
      annotations: READ_ONLY,
      description: `List the changed files (with patches and add/delete counts) of the issue's linked pull request (UUID or identifier). Empty list when no PR is linked. Team members only.`,
      inputSchema: strictInput({ issueId: z.string().min(1) }),
    },
    async ({ issueId: issueIdInput }) => {
      try {
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        if (!access.full) {
          const ctxIssue = await getIssueTeamContext(issueId)
          assertBoardGranted(access, ctxIssue.boardId, ctxIssue.teamId)
        }
        const result = await caller(user, request).issues.prFiles({ issueId })
        return ok(result)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Boards (delete / retarget repository)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_boards_delete`,
    {
      description: `Move a board to the trash (owner only; by its UUID). Purged with all issues after 48 hours; owners can restore from web settings before then.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(id)
          assertBoardGranted(access, board.id, board.teamId)
        }
        await caller(user, request).boards.delete({ boardId: id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_boards_set_repository`,
    {
      description: `Point a board (by its UUID) at a different registered repository (both must be in the same team), or pass repositoryId: null to detach it. Owner/admin only. Existing worktrees keep working; new coding sessions use the new repo. The board's branch pin resets unless defaultBranch is passed.`,
      inputSchema: strictInput({
        id: uuidString,
        repositoryId: uuidString.nullable(),
        defaultBranch: z.string().min(1).max(255).optional(),
      }),
    },
    async ({ id, repositoryId, defaultBranch }) => {
      try {
        if (!access.full) {
          const board = await getBoardTeamId(id)
          // Retargeting widens the token's GitHub reach to ANY repo in the
          // team registry (pr_open / pr_files / branch_diff then reach
          // the new repo through the granted board's issues) — so this is
          // a team-registry mutation, gated like repositories_add, not
          // a board-scoped one.
          assertTeamFullyGranted(access, board.teamId)
        }
        const result = await caller(user, request).boards.setRepository({
          boardId: id,
          repositoryId,
          defaultBranch,
        })
        return ok(result.board)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Teams (create / update)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_teams_create`,
    {
      description: `Create a new team owned by the MCP user (a unique slug is derived from the name).`,
      inputSchema: strictInput({
        name: z.string().min(1).max(255),
        iconUrl: z.string().url().max(2048).optional(),
      }),
    },
    async (input) => {
      try {
        // A new team is outside any selectable grant — full access only.
        assertFullAccess(access)
        const result = await caller(user, request).teams.create(input)
        return ok(result.team)
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_teams_update`,
    {
      description: `Update a team's name, icon, estimate scale or yolo mode (by its UUID). Team owner only. Teams are always private.`,
      inputSchema: strictInput({
        id: uuidString,
        name: z.string().min(1).max(255).optional(),
        iconUrl: z.string().url().max(2048).nullable().optional(),
        estimationType: z.enum(issueEstimationValues).optional(),
        // EXP-1105: true = every PR exponential_pr_open opens merges at once.
        yoloMode: z.boolean().optional(),
      }),
    },
    async ({ id, ...rest }) => {
      try {
        assertTeamFullyGranted(access, id)
        const result = await caller(user, request).teams.update({
          teamId: id,
          ...rest,
        })
        return ok(result.team)
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Team invites (owner-gated)
  // -----------------------------------------------------------------------

  server.registerTool(
    `exponential_invites_create`,
    {
      description: `Create an invite link for a team, returning the token to share. Owner only. Pass email to have the server mail the link (emailDelivered reports the attempt); an email invite also adds the person to the team at once as a placeholder member (memberUserId, assignable now; their content carries over when they join). name labels that member.`,
      inputSchema: strictInput({
        teamId: uuidString,
        role: z.enum([`owner`, `member`]).default(`member`),
        email: z.string().email().max(255).optional(),
        name: z.string().trim().max(180).optional(),
      }),
    },
    async (input) => {
      try {
        assertTeamFullyGranted(access, input.teamId)
        const result = await caller(user, request).teamInvites.create(input)
        return ok({
          invite: result.invite,
          token: result.token,
          emailDelivered: result.emailDelivered,
          memberUserId: result.memberUserId,
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_invites_list`,
    {
      annotations: READ_ONLY,
      description: `List the pending (unaccepted) invites for a team. The MCP user must be a member of the team.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        assertTeamFullyGranted(access, teamId)
        const result = await caller(user, request).teamInvites.list({
          teamId,
        })
        return ok(page(result.invites, limit, offset))
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_invites_revoke`,
    {
      description: `Revoke a pending invite by its UUID. Owner only.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const [invite] = await db
            .select({ teamId: teamInvites.teamId })
            .from(teamInvites)
            .where(eq(teamInvites.id, id))
            .limit(1)
          if (!invite) throw new Error(`Invite not found`)
          assertTeamFullyGranted(access, invite.teamId)
        }
        await caller(user, request).teamInvites.revoke({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Team MCP servers (EXP-792): the ONE registry a coding run's extra MCP
  // servers come from. Each member connects once in Settings (the server
  // holds the credential), so the tools list/add/remove and hand back a
  // settings deep link for the connect step — never a credential.
  // -----------------------------------------------------------------------

  /** Settings → MCP servers, auto-starting the connect for `serverId`. */
  const mcpConnectUrl = (teamSlug: string, serverId: string) =>
    `${appBaseUrl()}/t/${encodeURIComponent(teamSlug)}/settings/mcp-servers?connect=${serverId}`

  const loadTeamSlug = async (teamId: string) => {
    const [team] = await db
      .select({ slug: teams.slug })
      .from(teams)
      .where(eq(teams.id, teamId))
      .limit(1)
    if (!team) throw new Error(`Team not found`)
    return team.slug
  }

  server.registerTool(
    `exponential_mcp_servers_list`,
    {
      annotations: READ_ONLY,
      description: `List a team's MCP servers (Linear, Sentry, ...) that coding runs can connect to, with YOUR connection status (connected | not_connected | expired | error | not_needed), who connected and who shared it with the team's action runs, and connectUrl: the settings page where you (a person) connect it. Team members only. Members who haven't shared their connection can be asked with exponential_notifications_send.`,
      inputSchema: strictInput({ teamId: uuidString, ...pageInput }),
    },
    async ({ teamId, limit, offset }) => {
      try {
        assertTeamFullyGranted(access, teamId)
        const rows = await caller(user, request).mcpServers.list({ teamId })
        const slug = await loadTeamSlug(teamId)
        return ok(
          page(rows, limit, offset).map((row) => ({
            id: row.id,
            name: row.name,
            transport: row.transport,
            url: row.url,
            command: row.command,
            auth: row.auth,
            scopes: row.scopes,
            enabledByDefault: row.enabledByDefault,
            connection: row.connection,
            connectedCount: row.connectedCount,
            memberCount: row.memberCount,
            sharedCount: row.sharedCount,
            sharedUserIds: row.sharedUserIds,
            connectedUserIds: row.connectedUserIds,
            connectUrl: row.auth === `none` ? null : mcpConnectUrl(slug, row.id),
          }))
        )
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_mcp_servers_add`,
    {
      description: `Add a remote (https) MCP server to a team's registry, never a repo .mcp.json. Owner only. Probes the URL to detect OAuth; name defaults from the host. Returns the row and connectUrl, where each member connects once.`,
      inputSchema: strictInput({
        teamId: uuidString,
        url: z.string().url().max(2048),
        name: z.string().trim().min(1).max(MAX_MCP_SERVER_NAME).optional(),
      }),
    },
    async ({ teamId, url, name }) => {
      try {
        assertTeamFullyGranted(access, teamId)
        const trpc = caller(user, request)
        const probe = await trpc.mcpServers.probe({ teamId, url })
        if (!probe.reachable || probe.error) {
          throw new Error(
            `Could not add ${url}: ${probe.error ?? `unreachable`}`
          )
        }
        const row = await trpc.mcpServers.create({
          teamId,
          name: name ?? probe.suggestedName,
          transport: `http`,
          url: probe.url,
          auth: probe.auth,
          scopes: probe.scopes,
        })
        const slug = await loadTeamSlug(teamId)
        return ok({
          ...row,
          connectUrl: row.auth === `none` ? null : mcpConnectUrl(slug, row.id),
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_mcp_servers_remove`,
    {
      description: `Remove an MCP server from its team's registry (by UUID); every member's stored connection to it goes too. Owner only.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        if (!access.full) {
          const [row] = await db
            .select({ teamId: mcpServers.teamId })
            .from(mcpServers)
            .where(eq(mcpServers.id, id))
            .limit(1)
          if (!row) throw new Error(`MCP server not found`)
          assertTeamFullyGranted(access, row.teamId)
        }
        await caller(user, request).mcpServers.remove({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // -----------------------------------------------------------------------
  // Attachments upload (base64 payload → S3 → attachments row)
  // -----------------------------------------------------------------------
  // EXP-988/EXP-929: three shapes of ONE tool. `dataBase64` present = the
  // inline path below (unchanged). Absent = a SIGNED upload: the call returns
  // an upload URL + curl line (the sessions_guide shape) and a later call
  // with `attachmentId` alone finalizes the row. The signed halves live in
  // handlers/attachments-upload.ts; the access checks stay here.

  server.registerTool(
    `exponential_attachments_upload`,
    {
      description: `Attach a file to an issue (UUID or identifier). With dataBase64: uploads the bytes now (base64 inflates ~33%). Without it: returns attachmentId, a 10-minute signed uploadUrl and a ready curl line for filename/contentType (+ optional commentId); then call again with attachmentId alone to finalize. Images (png/jpeg/webp/gif/avif, 10 MB) and video/audio (50 MB) return "markdown" to embed; other types (50 MB) land in Files, no markdown, never embed. Storage limits apply.`,
      inputSchema: strictInput({
        issueId: z.string().min(1).optional(),
        filename: z.string().min(1).max(255).optional(),
        contentType: z.string().min(1).max(255).optional(),
        dataBase64: z.string().min(1).optional(),
        alt: z.string().max(500).optional(),
        // EXP-929: the signed path's two extra keys.
        commentId: uuidString.optional(),
        attachmentId: uuidString.optional(),
      }),
    },
    async ({
      issueId: issueIdInput,
      filename: filenameInput,
      contentType: contentTypeInput,
      dataBase64,
      alt,
      commentId,
      // Renamed: the inline path below mints its own `attachmentId`.
      attachmentId: finalizeId,
    }) => {
      try {
        // The finalize call: attachmentId ALONE.
        if (finalizeId !== undefined) {
          if (
            issueIdInput !== undefined ||
            filenameInput !== undefined ||
            contentTypeInput !== undefined ||
            dataBase64 !== undefined ||
            commentId !== undefined
          ) {
            throw new Error(
              `Pass attachmentId alone to finalize a signed upload; the other fields belong to the first call.`
            )
          }
          return ok(
            await finalizeSignedAttachmentUpload({
              attachmentId: finalizeId,
              userId: user.id,
              access,
            })
          )
        }
        if (
          issueIdInput === undefined ||
          filenameInput === undefined ||
          contentTypeInput === undefined
        ) {
          throw new Error(
            `issueId, filename and contentType are required (with dataBase64 to upload now, without it to get a signed upload URL).`
          )
        }
        // Canonicalized (lowercase essence) so the exact-match inline-image
        // classification behaves identically for every stored row.
        const contentType = canonicalizeContentType(contentTypeInput)
        const isImage = isAcceptedImageContentType(contentType)
        // The zod schema only checks length — strip control chars (CRLF would
        // otherwise poison the read path's Content-Disposition header).
        const filename = sanitizeUploadFilename(
          filenameInput,
          isImage ? `image` : `file`
        )
        const issueId = await resolveIssueId(issueIdInput, user.id, access)
        const issueCtx = await assertAttachmentUploadAccess({
          issueId,
          userId: user.id,
          access,
        })

        // The signed path: no bytes in context, an upload URL instead.
        if (dataBase64 === undefined) {
          const origin = process.env.BETTER_AUTH_URL
            ? appBaseUrl()
            : new URL(request.url).origin
          return ok(
            await mintSignedAttachmentUpload({
              issueId,
              teamId: issueCtx.teamId,
              boardId: issueCtx.boardId,
              userId: user.id,
              filename,
              contentType,
              commentId,
              origin,
            })
          )
        }
        if (commentId !== undefined) {
          throw new Error(
            `commentId is for the signed upload; attach an inline upload to a comment through exponential_comments_create's attachmentIds.`
          )
        }

        const body = new Uint8Array(Buffer.from(dataBase64, `base64`))
        if (body.byteLength === 0) {
          throw new Error(`Decoded file is empty. Check the base64 payload.`)
        }
        if (body.byteLength > getMaxUploadBytesForContentType(contentType)) {
          throw new Error(
            isImage
              ? `Images must be ${maxImageUploadBytes / (1024 * 1024)} MB or smaller.`
              : `Files must be ${maxFileUploadBytes / (1024 * 1024)} MB or smaller.`
          )
        }

        await assertWithinStorageLimit(issueCtx.teamId, body.byteLength)

        const attachmentId = crypto.randomUUID()
        const storageKey = buildAttachmentStorageKey(
          issueId,
          attachmentId,
          filename
        )
        const url = buildAttachmentUrl(attachmentId)
        // Inline images and (EXP-824) MP4/MOV media are probed for their
        // size; media also for its duration. A pdf/zip has neither.
        const isMedia = isInlineMediaContentType(contentType)
        const media =
          isMedia && isProbeableVideoContentType(contentType)
            ? getVideoMetadata(body)
            : null
        const dimensions = isImage ? getImageDimensions(body) : media

        await uploadObject({
          body,
          contentLength: body.byteLength,
          contentType,
          key: storageKey,
        })

        try {
          await db.insert(attachments).values({
            id: attachmentId,
            teamId: issueCtx.teamId,
            boardId: issueCtx.boardId,
            issueId,
            uploaderId: user.id,
            filename,
            contentType,
            sizeBytes: body.byteLength,
            storageKey,
            url,
            width: dimensions?.width ?? null,
            height: dimensions?.height ?? null,
            durationMs: media?.durationMs ?? null,
          })
        } catch (error) {
          try {
            await deleteObject(storageKey)
          } catch (deleteError) {
            console.error(
              `Failed to rollback uploaded attachment object`,
              deleteError
            )
          }
          throw error
        }

        return ok({
          id: attachmentId,
          url,
          // Images embed as `![]()`; video/audio (EXP-824) as a plain link
          // on its own paragraph. Other files are NOT markdown-embeddable —
          // they live in the issue's Files list.
          ...(isImage
            ? { markdown: `![${alt ?? ``}](${url})` }
            : isMedia
              ? { markdown: `[${filename}](${url})` }
              : {}),
          filename,
          contentType,
          sizeBytes: body.byteLength,
          width: dimensions?.width ?? null,
          height: dimensions?.height ?? null,
          durationMs: media?.durationMs ?? null,
        })
      } catch (e) {
        return err(e)
      }
    }
  )

  server.registerTool(
    `exponential_attachments_delete`,
    {
      description: `Permanently delete an issue attachment by id (the {id} in /api/attachments/{id}) and reclaim its bytes. Descriptions/comments embedding it are rewritten to *(deleted image: …)* in the same transaction.`,
      inputSchema: strictInput({ id: uuidString }),
    },
    async ({ id }) => {
      try {
        const attachment = await getAttachmentTeamContext(id)
        assertBoardGranted(access, attachment.boardId, attachment.teamId)
        // Membership, rewrite and blob reclamation all live in the router —
        // the MCP surface must never fork that logic.
        await caller(user, request).attachments.delete({ id })
        return ok({ ok: true, id })
      } catch (e) {
        return err(e)
      }
    }
  )

  // EXP-496: vendor bug intake. Registered only where the instance has an
  // in-app feedback widget (cloud — the same gate as the sidebar Feedback
  // button), so self-hosted agents never see the tool. Deliberately NO
  // grant/scope assertion: it writes to the vendor's own feedback board, not
  // to any of the caller's team data.
  const feedbackWidgetKey = buildRuntimeConfig().feedbackWidget?.widgetKey
  if (feedbackWidgetKey) {
    server.registerTool(
      `exponential_report_bug`,
      {
        description: `File a bug report about Exponential itself (the issue tracker — any client, these MCP tools, sync, relays) to the Exponential team. Use it the moment Exponential misbehaves or a tool result misleads you mid-task. Not for issues in the user's own project.`,
        inputSchema: strictInput({
          title: z.string().min(1).max(500),
          description: z.string().min(1).max(10_000),
        }),
      },
      async ({ title, description }) => {
        try {
          const limit = agentBugReportLimiter.tryTake(user.id)
          if (!limit.ok) {
            return err(
              new Error(
                `Too many bug reports — retry in ${limit.retryAfterSeconds}s`
              )
            )
          }
          const result = await createAgentBugReport({
            widgetKey: feedbackWidgetKey,
            reporter: { email: user.email, name: user.name ?? null },
            title,
            description,
            userAgent: request.headers.get(`user-agent`),
          })
          return ok(result)
        } catch (e) {
          return err(e)
        }
      }
    )
  }
}
