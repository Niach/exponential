import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { PgDialect } from "drizzle-orm/pg-core"

// ── Mocks ────────────────────────────────────────────────────────────────────
// tools.ts talks to the DB two ways: (1) direct drizzle for reads, (2) the tRPC
// caller (appRouter.createCaller) for writes. We mock both so the handlers run
// without a real Postgres/S3, and drive them through a fake McpServer that just
// captures each tool's callback.

// Shared mock state must be defined via vi.hoisted so the (hoisted) vi.mock
// factories below can reference it without TDZ errors.
const h = vi.hoisted(() => {
  const caller = {
    comments: { create: vi.fn(), update: vi.fn(), delete: vi.fn() },
    subscriptions: { subscribe: vi.fn(), unsubscribe: vi.fn() },
    notifications: { markRead: vi.fn(), markAllRead: vi.fn() },
    repositories: { list: vi.fn(), add: vi.fn() },
    prompts: { list: vi.fn(), get: vi.fn(), create: vi.fn(), update: vi.fn(), delete: vi.fn() },
    agents: { list: vi.fn() },
    issues: {
      // EXP-684: statusId passthrough on create.
      create: vi.fn(),
      update: vi.fn(),
    },
    boards: { delete: vi.fn(), setRepository: vi.fn() },
    teams: { create: vi.fn(), update: vi.fn() },
    teamInvites: { create: vi.fn(), list: vi.fn(), revoke: vi.fn() },
    // FEED-108: the by-hand placeholder member.
    teamMembers: { addPlaceholder: vi.fn() },
    attachments: { delete: vi.fn() },
    // EXP-660: the deferred families.
    statuses: { create: vi.fn(), update: vi.fn(), delete: vi.fn() },
    // EXP-1199: the device commands behind devices_account_login.
    devices: { createCommand: vi.fn(), getCommand: vi.fn() },
  }

  // A chainable, thenable drizzle query stub. Every builder method returns the
  // same object; awaiting it resolves to `dbRows.current`. `.where(cond)`
  // records the condition so scoping tests can render it back to SQL.
  const dbRows: { current: Array<unknown> } = { current: [] }
  const state: { capturedWhere: unknown } = { capturedWhere: undefined }
  const insertValues = vi.fn(async () => undefined)

  const queryBuilder: Record<string, unknown> = {}
  for (const method of [
    `from`,
    `innerJoin`,
    `leftJoin`,
    `orderBy`,
    `limit`,
    `offset`,
  ]) {
    queryBuilder[method] = vi.fn(() => queryBuilder)
  }
  queryBuilder.where = vi.fn((cond: unknown) => {
    state.capturedWhere = cond
    return queryBuilder
  })
  ;(queryBuilder as { then: unknown }).then = (
    resolve: (v: unknown) => unknown,
    reject: (e: unknown) => unknown
  ) => Promise.resolve(dbRows.current).then(resolve, reject)

  // EXP-637: pr_merge stamps the header session's merged_own_pr spare outside
  // any transaction, right before the merge.
  // EXP-879: sessions_results writes the run's results column, so the values
  // an update SETS are captured too.
  const updateSet = vi.fn()
  const dbUpdate = vi.fn(
    (): {
      set: (values: Record<string, unknown>) => {
        where: (cond?: unknown) => Promise<unknown>
      }
    } => ({
      set: (values: Record<string, unknown>) => {
        updateSet(values)
        return { where: async () => undefined }
      },
    })
  )
  // EXP-879: the removal path deletes session_attachments rows and reads back
  // their storage keys so the objects can go too.
  const deleteReturning: { current: Array<{ storageKey: string }> } = {
    current: [],
  }
  const dbDelete = vi.fn(() => ({
    where: () => ({ returning: async () => deleteReturning.current }),
  }))

  // EXP-897: the recursive session-tree walks (loadSessionChain /
  // loadSessionDepths / loadSubtreeSessionIds) go through raw SQL.
  const executeRows: { current: Array<Record<string, unknown>> } = { current: [] }
  const db = {
    select: vi.fn(() => queryBuilder),
    insert: vi.fn(() => ({ values: insertValues })),
    update: dbUpdate,
    delete: dbDelete,
    transaction: vi.fn(),
    execute: vi.fn(async () => ({ rows: executeRows.current })),
  }

  const membership = {
    resolveTeamAccess: vi.fn(async () => undefined),
    assertTeamMember: vi.fn(async () => undefined),
    getIssueTeamContext: vi.fn(async () => ({
      teamId: `ws-1`,
      boardId: `proj-1`,
    })),
    getBoardTeamId: vi.fn(async () => ({ teamId: `ws-1` })),
    getAttachmentTeamContext: vi.fn(async () => ({
      teamId: `ws-1`,
      boardId: `proj-1`,
      contentType: `image/png`,
      filename: `shot.png`,
      sizeBytes: 4,
      storageKey: `k`,
    })),
    getUserTeamIds: vi.fn(async () => [`ws-1`]),
    getPublicTeamIds: vi.fn(async () => []),
  }

  const uploadObject = vi.fn(async () => undefined)
  const deleteObject = vi.fn(async () => undefined)
  const getObject = vi.fn()
  const assertWithinStorageLimit = vi.fn(async () => undefined)
  const createAgentBugReport = vi.fn(async () => ({
    issueId: `bug-issue-1`,
    identifier: `EXP-1`,
  }))


  // EXP-1291: the session tree's core and the PR model — the tools add the
  // grant checks and resolve the caller; behaviour lives behind these.
  const sessionsCore = {
    sendSession: vi.fn(),
    loadSessionForUser: vi.fn(),
    assertCanSteer: vi.fn(async () => undefined),
    getSessionTree: vi.fn(),
    listSessions: vi.fn(),
    stopSession: vi.fn(),
    endSession: vi.fn(),
    linkIssues: vi.fn(),
    unlinkIssue: vi.fn(),
    writeGuide: vi.fn(),
  }
  const prs = {
    openPullRequest: vi.fn(),
    resolvePullRequest: vi.fn(),
    mergePullRequest: vi.fn(),
    closePullRequestTx: vi.fn(),
    retargetPullRequest: vi.fn(),
    updatePullRequest: vi.fn(),
    unlinkPullRequestIssue: vi.fn(),
    pullRequestFiles: vi.fn(),
  }

  // NEW-19: the widget submission read and the reporter fallback.
  const reporter = {
    loadWidgetSubmissions: vi.fn(async () => [] as unknown[]),
    findIssueForReporter: vi.fn(async () => null as unknown),
    postReporterReply: vi.fn(),
    listReporterComments: vi.fn(),
    reporterIssueView: vi.fn(),
  }

  return {
    caller,
    reporter,
    sessionsCore,
    prs,
    dbRows,
    executeRows,
    state,
    insertValues,
    updateSet,
    deleteReturning,
    dbDelete,
    db,
    membership,
    uploadObject,
    deleteObject,
    getObject,
    assertWithinStorageLimit,
    createAgentBugReport,
  }
})

const {
  caller,
  sessionsCore,
  prs,
  dbRows,
  state,
  insertValues,
  db,
  membership,
  uploadObject,
  assertWithinStorageLimit,
} = h

vi.mock(`@/routes/api/trpc/$`, () => ({
  appRouter: { createCaller: vi.fn(() => h.caller) },
}))

vi.mock(`@/db/connection`, () => ({ db: h.db }))

vi.mock(`@/lib/team-membership`, () => h.membership)

vi.mock(`@/lib/storage`, () => ({
  uploadObject: h.uploadObject,
  deleteObject: h.deleteObject,
  getObject: h.getObject,
  headObject: vi.fn(),
}))

// EXP-704: attachments_get mints real signed download tokens.
vi.stubEnv(`BETTER_AUTH_SECRET`, `mcp-tools-test-secret`)

vi.mock(`@/lib/storage/image-dimensions`, () => ({
  getImageDimensions: vi.fn(() => ({ width: 12, height: 8 })),
}))

vi.mock(`@/lib/billing`, () => ({
  assertWithinStorageLimit: h.assertWithinStorageLimit,
}))

// FEED-66: the issue resolver stays real (it runs on the db stub).
vi.mock(`@/lib/integrations/github-app`, () => ({
  resolveRepoInstallationToken: vi.fn(),
  resolveRepoInstallationTokenInfo: vi.fn(),
  // SLOP-7: lib/auth/index.ts reads the App's OAuth client at import time.
  githubOAuthClient: () => null,
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  isInstallationLinkedToTeam: vi.fn(),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrLifecycleStatusInTx: vi.fn(),
}))
vi.mock(`@/lib/integrations/pr-actor-claims`, () => ({
  // EXP-617: the write tools record "this user's agent touched this issue"
  // so a later PR fan-out can keep them out of it.
  noteAgentIssueActivity: vi.fn(),
}))
vi.mock(`@/lib/integrations/activity`, () => ({ recordIssueEvent: vi.fn() }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetPrNotify: vi.fn(),
}))
vi.mock(`@/lib/widget/agent-report`, () => ({
  createAgentBugReport: h.createAgentBugReport,
}))
vi.mock(`@/lib/widget/submission`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  loadWidgetSubmissions: h.reporter.loadWidgetSubmissions,
}))
vi.mock(`@/lib/reporter/service`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  findIssueForReporter: h.reporter.findIssueForReporter,
}))
vi.mock(`@/lib/reporter/reply`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  postReporterReply: h.reporter.postReporterReply,
}))
vi.mock(`@/lib/mcp/handlers/reporter-view`, () => ({
  listReporterComments: h.reporter.listReporterComments,
  reporterIssueView: h.reporter.reporterIssueView,
}))
vi.mock(`@/lib/trpc/repositories`, () => ({
  loadRepositoryForTeam: vi.fn(),
}))
// EXP-936: the relay hop behind sessions_compact.
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  getSteerRelayConfig: vi.fn(),
  relayPostCompact: vi.fn(),
}))
vi.mock(`@/lib/sessions/send`, () => ({ sendSession: h.sessionsCore.sendSession }))
vi.mock(`@/lib/sessions/access`, () => ({
  loadSessionForUser: h.sessionsCore.loadSessionForUser,
  assertCanSteer: h.sessionsCore.assertCanSteer,
  // NEW-1: the host is derived from the devices rows; the mocked rows carry
  // it as `hostUserId` so the cases read as before.
  isSessionPrincipal: async (userId: string, row: { userId: string; hostUserId?: string | null }) =>
    row.userId === userId || row.hostUserId === userId,
  sessionDeviceOwner: async (row: { userId: string; hostUserId?: string | null }) =>
    row.hostUserId ?? row.userId,
}))
vi.mock(`@/lib/sessions/tree`, () => ({
  getSessionTree: h.sessionsCore.getSessionTree,
  listSessions: h.sessionsCore.listSessions,
}))
vi.mock(`@/lib/sessions/lifecycle`, () => ({
  stopSession: h.sessionsCore.stopSession,
  endSession: h.sessionsCore.endSession,
}))
vi.mock(`@/lib/sessions/issues`, () => ({
  linkIssues: h.sessionsCore.linkIssues,
  unlinkIssue: h.sessionsCore.unlinkIssue,
}))
vi.mock(`@/lib/sessions/guide`, () => ({ writeGuide: h.sessionsCore.writeGuide }))
vi.mock(`@/lib/pull-requests`, () => h.prs)
// EXP-1280: the card builders stay real (the validation under test); only
// the DB write is stubbed.
const postAskCard = vi.hoisted(() => vi.fn())
const postSurfaceCard = vi.hoisted(() => vi.fn())
vi.mock(`@/lib/sessions/plan`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/sessions/plan")>()),
  postAskCard,
  postSurfaceCard,
}))

import { loadRepositoryForTeam } from "@/lib/trpc/repositories"
import { noteAgentIssueActivity } from "@/lib/integrations/pr-actor-claims"
import { getSteerRelayConfig, relayPostCompact } from "@/lib/steer"
import { registerExponentialTools, GUIDE_PICTURE_NEXT_STEP } from "@/lib/mcp/tools"
import { FULL_ACCESS, type McpAccess } from "@/lib/mcp/scope"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "@/lib/mcp/gates"
import { displayRowNames, legacyNameOfRow } from "@/lib/mcp/nouns"
import type { McpUser } from "@/lib/mcp/server"
import { contract } from "@exp/domain-contract"
import { PLAN_CARD_COPY } from "@exp/domain-contract/plan-card"

// ── Harness ──────────────────────────────────────────────────────────────────

type ToolResult = {
  isError?: boolean
  content: Array<{ type: string; text?: string; data?: string }>
}
type ToolHandler = (
  args: Record<string, unknown>,
  extra?: unknown
) => Promise<ToolResult>

const USER: McpUser = {
  id: `user-1`,
  email: `u@example.com`,
  name: `User One`,
  image: null,
  emailVerified: true,
  isAdmin: false,
  creemCustomerId: null,
  hadTrial: false,
  onboardingCompletedAt: null,
  createdAt: new Date(),
  updatedAt: new Date(),
} as unknown as McpUser

// EXP-637: `sessionId` is what `routes/api/mcp.ts` parsed off the launcher's
// X-Exp-Session-Id header — null for every caller that is not a launched agent.
// EXP-660: `gates` defaults to the full surface (what the route resolves per
// caller); `access` lets scoping tests hand in a confined OAuth grant.
function collectTools(
  user: McpUser = USER,
  sessionId: string | null = null,
  gates: McpToolGates = ALL_MCP_TOOL_GATES,
  access: McpAccess = FULL_ACCESS
): Map<string, ToolHandler> {
  const tools = new Map<string, ToolHandler>()
  const fakeServer = {
    registerTool: (name: string, _def: unknown, handler: ToolHandler) => {
      tools.set(name, handler)
    },
  }
  registerExponentialTools(
    fakeServer as never,
    user,
    new Request(`https://x.test/api/mcp`, {
      headers: { "user-agent": `claude-code/test` },
    }),
    access,
    sessionId,
    gates
  )
  return tools
}

/** The registration DEFS (not the handlers) — the input schemas, so a test
 * can assert what an argument list is allowed to carry. Since EXP-705 every
 * inputSchema is a strict z.object INSTANCE, not a raw shape. */
function collectToolDefs(
  gates: McpToolGates = ALL_MCP_TOOL_GATES
): Map<string, { inputSchema?: z.ZodType; description?: string }> {
  const defs = new Map<
    string,
    { inputSchema?: z.ZodType; description?: string }
  >()
  const fakeServer = {
    registerTool: (
      name: string,
      def: { inputSchema?: z.ZodType; description?: string }
    ) => {
      defs.set(name, def)
    },
  }
  registerExponentialTools(
    fakeServer as never,
    USER,
    new Request(`https://x.test/api/mcp`),
    FULL_ACCESS,
    SESSION,
    gates
  )
  return defs
}

const tools = collectTools()
function tool(name: string): ToolHandler {
  const handler = tools.get(name)
  if (!handler) throw new Error(`tool not registered: ${name}`)
  return handler
}

function parseOk(result: ToolResult): unknown {
  expect(result.isError).toBeFalsy()
  return JSON.parse(result.content[0].text ?? `null`)
}

const UUID = `11111111-1111-1111-1111-111111111111`
const WS = `22222222-2222-2222-2222-222222222222`
const PROJ = `33333333-3333-3333-3333-333333333333`
const REPO = `44444444-4444-4444-4444-444444444444`
const INV = `55555555-5555-5555-5555-555555555555`
// EXP-660 fixtures.
const STATUS = `77777777-7777-7777-7777-777777777777`
// EXP-637: the X-Exp-Session-Id the launcher injects (a `sessions` row since
// EXP-1291). An identifier, never a credential: ownership is re-checked.
const SESSION = `66666666-6666-4666-8666-666666666666`
// EXP-1291 fixtures: a thread root, a child session, a message, a PR row.
const ROOT = `aaaaaaaa-0000-4000-8000-000000000001`
const CHILD = `aaaaaaaa-0000-4000-8000-000000000002`
const MSG = `aaaaaaaa-0000-4000-8000-000000000003`
const PR_ID = `aaaaaaaa-0000-4000-8000-000000000004`
// RFC-valid ids for inputs the sessions.send zod (z.string().uuid()) checks.
const TEAM_V4 = `aaaaaaaa-0000-4000-8000-000000000005`
const ISSUE_V4 = `aaaaaaaa-0000-4000-8000-000000000006`

function renderWhere(): { sql: string; params: unknown[] } {
  const query = new PgDialect().sqlToQuery(state.capturedWhere as never)
  return { sql: query.sql, params: query.params }
}

const SCOPED_TO_WS: McpAccess = {
  full: false,
  fullTeamIds: new Set([WS]),
  grantedBoardIds: new Set(),
  visibleTeamIds: new Set([WS]),
}

const forbidden = () =>
  new TRPCError({ code: `FORBIDDEN`, message: `not allowed here` })

beforeEach(() => {
  vi.clearAllMocks()
  dbRows.current = []
  state.capturedWhere = undefined
  for (const [, methods] of Object.entries(caller)) {
    for (const fn of Object.values(methods)) {
      ;(fn as ReturnType<typeof vi.fn>).mockReset()
    }
  }
  // Restore default "allowed" behavior after clearAllMocks wiped implementations.
  membership.resolveTeamAccess.mockResolvedValue(undefined)
  membership.assertTeamMember.mockResolvedValue(undefined)
  membership.getIssueTeamContext.mockResolvedValue({
    teamId: `ws-1`,
    boardId: `proj-1`,
  })
  membership.getUserTeamIds.mockResolvedValue([`ws-1`])
  assertWithinStorageLimit.mockResolvedValue(undefined)
  insertValues.mockResolvedValue(undefined)
  // EXP-936: relay off by default; individual tests arm it.
  vi.mocked(getSteerRelayConfig).mockReturnValue(null)
  sessionsCore.assertCanSteer.mockResolvedValue(undefined)
  db.transaction.mockImplementation(async (fn: (tx: unknown) => unknown) => fn(db))
  h.reporter.loadWidgetSubmissions.mockResolvedValue([])
  h.reporter.findIssueForReporter.mockResolvedValue(null)
})

// ── Caller-backed tools (delegate → ok/err) ──────────────────────────────────

type Descriptor = {
  tool: string
  pick: () => ReturnType<typeof vi.fn>
  args: Record<string, unknown>
  resolved: unknown
  expected: unknown
  calledWith?: unknown
  // EXP-660: rows the drizzle stub serves for the tool's own context lookup
  // (every select resolves to the same rows, so tools keep it to ONE).
  rows?: Array<unknown>
}

const descriptors: Array<Descriptor> = [
  // SLOP-4: `audience` rides through to the router; the result carries the
  // comment plus `reporterEmailed` (null = a team comment).
  {
    tool: `exponential_comments_create`,
    pick: () => caller.comments.create,
    args: { issueId: UUID, body: `We shipped a fix.`, audience: `reporter` },
    resolved: {
      comment: { id: UUID, body: `We shipped a fix.`, audience: `reporter` },
      reporterEmailed: true,
      txId: 1,
    },
    expected: {
      id: UUID,
      body: `We shipped a fix.`,
      audience: `reporter`,
      reporterEmailed: true,
    },
    calledWith: { issueId: UUID, body: `We shipped a fix.`, audience: `reporter` },
  },
  // EXP-1241: the issue's identifier rides beside the comment row (the run's
  // comment card links `<issue>?comment=<id>` off it).
  {
    tool: `exponential_comments_create`,
    pick: () => caller.comments.create,
    args: { issueId: UUID, body: `Confirmed.` },
    resolved: {
      comment: { id: UUID, issueId: UUID, body: `Confirmed.`, source: `mcp` },
      reporterEmailed: null,
      txId: 1,
    },
    expected: {
      id: UUID,
      issueId: UUID,
      body: `Confirmed.`,
      source: `mcp`,
      issueIdentifier: `EXP-42`,
      reporterEmailed: null,
    },
    rows: [{ identifier: `EXP-42` }],
  },
  {
    tool: `exponential_comments_update`,
    pick: () => caller.comments.update,
    args: { id: UUID, body: `edited` },
    resolved: { comment: { id: UUID, body: `edited` } },
    expected: { id: UUID, body: `edited` },
    calledWith: { id: UUID, body: `edited` },
  },
  {
    tool: `exponential_comments_delete`,
    pick: () => caller.comments.delete,
    args: { id: UUID },
    resolved: { txId: 1 },
    expected: { ok: true, id: UUID },
    calledWith: { id: UUID },
  },
  {
    tool: `exponential_issues_subscribe`,
    pick: () => caller.subscriptions.subscribe,
    args: { issueId: UUID },
    resolved: { txId: 1 },
    expected: { ok: true, issueId: UUID, subscribed: true },
    calledWith: { issueId: UUID },
  },
  {
    tool: `exponential_issues_unsubscribe`,
    pick: () => caller.subscriptions.unsubscribe,
    args: { issueId: UUID },
    resolved: { txId: 1 },
    expected: { ok: true, issueId: UUID, subscribed: false },
    calledWith: { issueId: UUID },
  },
  {
    tool: `exponential_notifications_mark_read`,
    pick: () => caller.notifications.markRead,
    args: { id: UUID },
    resolved: { txId: 1 },
    expected: { ok: true, id: UUID },
    calledWith: { id: UUID },
  },
  {
    tool: `exponential_repositories_list`,
    pick: () => caller.repositories.list,
    args: { teamId: WS },
    resolved: [{ id: REPO, fullName: `a/b`, boards: [] }],
    expected: [{ id: REPO, fullName: `a/b`, boards: [] }],
    calledWith: { teamId: WS },
  },
  {
    tool: `exponential_repositories_add`,
    pick: () => caller.repositories.add,
    args: { teamId: WS, fullName: `a/b` },
    resolved: { repository: { id: REPO, fullName: `a/b` } },
    expected: { id: REPO, fullName: `a/b` },
    calledWith: { teamId: WS, fullName: `a/b` },
  },
  {
    tool: `exponential_boards_delete`,
    pick: () => caller.boards.delete,
    args: { id: PROJ },
    resolved: { ok: true, txId: 1 },
    expected: { ok: true, id: PROJ },
    calledWith: { boardId: PROJ },
  },
  {
    tool: `exponential_boards_set_repository`,
    pick: () => caller.boards.setRepository,
    args: { id: PROJ, repositoryId: REPO },
    resolved: { board: { id: PROJ, repositoryId: REPO } },
    expected: { id: PROJ, repositoryId: REPO },
    calledWith: { boardId: PROJ, repositoryId: REPO },
  },
  {
    tool: `exponential_teams_create`,
    pick: () => caller.teams.create,
    args: { name: `New WS` },
    resolved: { team: { id: WS, name: `New WS` } },
    expected: { id: WS, name: `New WS` },
    calledWith: { name: `New WS` },
  },
  {
    tool: `exponential_teams_update`,
    pick: () => caller.teams.update,
    args: { id: WS, name: `Renamed` },
    resolved: { team: { id: WS, name: `Renamed` } },
    expected: { id: WS, name: `Renamed` },
    calledWith: { teamId: WS, name: `Renamed` },
  },
  {
    tool: `exponential_invites_create`,
    pick: () => caller.teamInvites.create,
    args: { teamId: WS, role: `member` },
    resolved: {
      invite: { id: INV },
      token: `tok-abc`,
      emailDelivered: null,
      memberUserId: null,
    },
    expected: {
      invite: { id: INV },
      token: `tok-abc`,
      emailDelivered: null,
      memberUserId: null,
    },
    calledWith: { teamId: WS, role: `member` },
  },
  // FEED-108: the member row comes back bare (the preview's `member@` ref
  // resolves to its id + name); txId stays server-side.
  {
    tool: `exponential_members_add`,
    pick: () => caller.teamMembers.addPlaceholder,
    args: { teamId: WS, email: `hannes@example.com`, name: `Hannes` },
    resolved: {
      member: {
        id: `u-ph`,
        memberId: `m-ph`,
        name: `Hannes`,
        email: `hannes@example.com`,
        role: `member`,
        placeholder: true,
      },
      txId: 7,
    },
    expected: {
      id: `u-ph`,
      memberId: `m-ph`,
      name: `Hannes`,
      email: `hannes@example.com`,
      role: `member`,
      placeholder: true,
    },
    calledWith: { teamId: WS, email: `hannes@example.com`, name: `Hannes` },
  },
  {
    tool: `exponential_invites_list`,
    pick: () => caller.teamInvites.list,
    args: { teamId: WS },
    resolved: { invites: [{ id: INV }] },
    expected: [{ id: INV }],
    calledWith: { teamId: WS },
  },
  {
    tool: `exponential_invites_revoke`,
    pick: () => caller.teamInvites.revoke,
    args: { id: INV },
    resolved: { ok: true },
    expected: { ok: true, id: INV },
    calledWith: { id: INV },
  },
  // ── EXP-660: statuses ──
  {
    tool: `exponential_statuses_create`,
    pick: () => caller.statuses.create,
    args: { teamId: WS, category: `started`, name: `QA`, color: `#ff8800` },
    resolved: { txId: 1, status: { id: STATUS, name: `QA` } },
    expected: { id: STATUS, name: `QA` },
    calledWith: { teamId: WS, category: `started`, name: `QA`, color: `#ff8800` },
  },
  {
    tool: `exponential_statuses_update`,
    pick: () => caller.statuses.update,
    rows: [{ teamId: WS }],
    args: { id: STATUS, name: `QA 2` },
    resolved: { txId: 1, status: { id: STATUS, name: `QA 2` } },
    expected: { id: STATUS, name: `QA 2` },
    calledWith: { teamId: WS, statusId: STATUS, name: `QA 2` },
  },
  {
    tool: `exponential_statuses_delete`,
    pick: () => caller.statuses.delete,
    rows: [{ teamId: WS }],
    args: { id: STATUS, reassignToId: UUID },
    resolved: { txId: 1, reassigned: 3, reassignedToId: UUID },
    expected: { ok: true, id: STATUS, reassigned: 3, reassignedToId: UUID },
    calledWith: { teamId: WS, statusId: STATUS, reassignToId: UUID },
  },
  // ── NEW-2: prompts (actions became saved prompts), agents ──
  {
    tool: `exponential_prompts_list`,
    pick: () => caller.prompts.list,
    args: { teamId: WS },
    resolved: { prompts: [{ id: UUID, name: `Code review` }] },
    expected: [{ id: UUID, name: `Code review` }],
    calledWith: { teamId: WS },
  },
  {
    tool: `exponential_prompts_get`,
    pick: () => caller.prompts.get,
    args: { id: UUID },
    resolved: { prompt: { id: UUID, name: `Code review`, body: `# Review` } },
    expected: { id: UUID, name: `Code review`, body: `# Review` },
    calledWith: { id: UUID },
  },
  {
    tool: `exponential_prompts_create`,
    pick: () => caller.prompts.create,
    args: { teamId: WS, name: `Release all`, body: `# Release`, icon: `rocket` },
    resolved: { txId: 1, prompt: { id: UUID, name: `Release all`, body: `# Release` } },
    expected: { id: UUID, name: `Release all`, body: `# Release` },
    calledWith: { teamId: WS, name: `Release all`, body: `# Release`, icon: `rocket` },
  },
  {
    tool: `exponential_prompts_update`,
    pick: () => caller.prompts.update,
    args: { id: UUID, body: `# Release v2` },
    resolved: { txId: 1, prompt: { id: UUID, name: `Release all`, body: `# Release v2` } },
    expected: { id: UUID, name: `Release all`, body: `# Release v2` },
    calledWith: { id: UUID, body: `# Release v2` },
  },
  {
    tool: `exponential_prompts_delete`,
    pick: () => caller.prompts.delete,
    args: { id: UUID },
    resolved: { txId: 1, ok: true, id: UUID },
    expected: { ok: true, id: UUID },
    calledWith: { id: UUID },
  },
  {
    tool: `exponential_agents_list`,
    pick: () => caller.agents.list,
    args: { teamId: WS },
    resolved: { agents: [{ id: UUID, name: `Triage` }] },
    expected: [{ id: UUID, name: `Triage` }],
    calledWith: { teamId: WS },
  },
]

describe.each(descriptors)(
  `caller-backed MCP tool $tool`,
  ({ tool: name, pick, args, resolved, expected, calledWith, rows }) => {
    it(`happy path returns the mapped payload`, async () => {
      if (rows) dbRows.current = rows
      pick().mockResolvedValue(resolved)
      const result = await tool(name)(args)
      expect(parseOk(result)).toEqual(expected)
      if (calledWith) {
        expect(pick()).toHaveBeenCalledWith(calledWith)
      }
    })

    it(`surfaces a permission denial as an MCP error`, async () => {
      if (rows) dbRows.current = rows
      pick().mockRejectedValue(forbidden())
      const result = await tool(name)(args)
      expect(result.isError).toBe(true)
      expect(result.content[0].text).toContain(`not allowed here`)
    })
  }
)

// ── prompts writes: the app's owner rule + validation, a confined grant ─────

describe(`exponential_prompts_create / update / delete`, () => {
  const OTHER_TEAM = `88888888-8888-8888-8888-888888888888`

  it(`validates name and body lengths like the router (255 / 64KB, not blank)`, () => {
    const defs = collectToolDefs()
    const create = defs.get(`exponential_prompts_create`)!.inputSchema!
    const update = defs.get(`exponential_prompts_update`)!.inputSchema!
    expect(create.safeParse({ teamId: WS, name: `Release all`, body: `# Release`, icon: `rocket` }).success).toBe(true)
    for (const args of [
      { teamId: WS, name: ``, body: `x` },
      { teamId: WS, name: `n`.repeat(256), body: `x` },
      { teamId: WS, name: `n`, body: `` },
      { teamId: WS, name: `n`, body: `b`.repeat(64 * 1024 + 1) },
      { teamId: WS, name: `n`, body: `x`, icon: `not-an-icon` },
      { teamId: WS, name: `n`, body: `x`, inputs: [] },
    ]) {
      expect(create.safeParse(args).success, JSON.stringify(args).slice(0, 80)).toBe(false)
    }
    expect(update.safeParse({ id: UUID, icon: null }).success).toBe(true)
    expect(update.safeParse({ id: UUID, name: `n`.repeat(256) }).success).toBe(false)
    expect(update.safeParse({ id: UUID, sortOrder: 1 }).success).toBe(false)
  })

  it(`refuses a grant confined to another team before any write`, async () => {
    const scoped = collectTools(USER, null, ALL_MCP_TOOL_GATES, SCOPED_TO_WS)
    const create = await scoped.get(`exponential_prompts_create`)!({ teamId: OTHER_TEAM, name: `n`, body: `b` })
    expect(create.isError).toBe(true)
    expect(create.content[0].text).toContain(`not granted access`)

    // A prompt id that resolves to another team is refused the same way.
    dbRows.current = [{ teamId: OTHER_TEAM }]
    for (const [name, args] of [
      [`exponential_prompts_update`, { id: UUID, name: `n` }],
      [`exponential_prompts_delete`, { id: UUID }],
    ] as const) {
      const result = await scoped.get(name)!(args)
      expect(result.isError, name).toBe(true)
      expect(result.content[0].text, name).toContain(`not granted access`)
    }
    expect(caller.prompts.create).not.toHaveBeenCalled()
    expect(caller.prompts.update).not.toHaveBeenCalled()
    expect(caller.prompts.delete).not.toHaveBeenCalled()

    // Its own team passes the grant and reaches the owner-gated router.
    dbRows.current = [{ teamId: WS }]
    caller.prompts.delete.mockResolvedValue({ txId: 1, ok: true, id: UUID })
    expect(parseOk(await scoped.get(`exponential_prompts_delete`)!({ id: UUID }))).toEqual({ ok: true, id: UUID })
  })
})

// ── notifications_mark_read: all + validation modes ──────────────────────────

describe(`exponential_notifications_mark_read modes`, () => {
  it(`marks all when all=true`, async () => {
    caller.notifications.markAllRead.mockResolvedValue({ txId: 1 })
    const result = await tool(`exponential_notifications_mark_read`)({
      all: true,
    })
    expect(parseOk(result)).toEqual({ ok: true, marked: `all` })
    expect(caller.notifications.markAllRead).toHaveBeenCalledTimes(1)
    expect(caller.notifications.markRead).not.toHaveBeenCalled()
  })

  it(`errors when neither id nor all is given`, async () => {
    const result = await tool(`exponential_notifications_mark_read`)({})
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`notification id`)
  })
})

// ── notifications_list (direct DB read, self-scoped) ─────────────────────────

describe(`exponential_notifications_list`, () => {
  it(`returns the caller's notifications`, async () => {
    dbRows.current = [{ id: `n1`, userId: `user-1` }]
    const result = await tool(`exponential_notifications_list`)({
      unreadOnly: false,
      limit: 50,
      offset: 0,
    })
    expect(parseOk(result)).toEqual([{ id: `n1`, userId: `user-1` }])
  })

  it(`scopes the query to the authenticated user (no cross-user leak)`, async () => {
    await tool(`exponential_notifications_list`)({
      unreadOnly: true,
      limit: 50,
      offset: 0,
    })
    const { sql, params } = new PgDialect().sqlToQuery(
      state.capturedWhere as never
    )
    expect(sql).toContain(`user_id`)
    expect(params).toContain(`user-1`)
    // unreadOnly must add the read_at IS NULL predicate.
    expect(sql).toContain(`read_at`)
  })

  it(`hides trashed and archived boards like the synced shape (EXP-517)`, async () => {
    await tool(`exponential_notifications_list`)({
      unreadOnly: false,
      limit: 50,
      offset: 0,
    })
    const { sql } = new PgDialect().sqlToQuery(state.capturedWhere as never)
    expect(sql).toContain(`"board_deleted_at" is null`)
    expect(sql).toContain(`"board_archived_at" is null`)
  })
})

// ── teams_get (direct DB read, projected) ───────────────────────────────

describe(`exponential_teams_get`, () => {
  // REV2-67: server-only team columns (comp_tier) must stay behind the same
  // allowlist the teams shape pins — a full-row select() leaked them to every
  // member and to any team-scoped OAuth token.
  it(`projects the synced contract columns only`, async () => {
    dbRows.current = [{ id: WS, name: `Acme`, slug: `acme` }]
    await tool(`exponential_teams_get`)({ id: WS })
    const projection = (db.select.mock.calls[0] as unknown[])?.[0] as Record<
      string,
      unknown
    >
    expect(Object.keys(projection).sort()).toEqual([
      `createdAt`,
      `endSessionsOnMerge`,
      `estimationType`,
      `iconUrl`,
      `id`,
      `name`,
      `prMergedAutomation`,
      `prMergedStatusId`,
      `prOpenedAutomation`,
      `prOpenedStatusId`,
      `slug`,
      `updatedAt`,
      `yoloMode`,
    ])
  })
})

// ── members_list (direct DB read, team-gated) ───────────────────────────

describe(`exponential_members_list`, () => {
  it(`returns the team members`, async () => {
    dbRows.current = [{ id: `user-1`, name: `User One`, role: `owner` }]
    const result = await tool(`exponential_members_list`)({
      teamId: WS,
    })
    expect(parseOk(result)).toEqual([
      { id: `user-1`, name: `User One`, role: `owner` },
    ])
    expect(membership.resolveTeamAccess).toHaveBeenCalledWith(`user-1`, WS)
    const { sql } = new PgDialect().sqlToQuery(state.capturedWhere as never)
    expect(sql).not.toContain(`is_agent`)
  })

  it(`denies when the user is not in the team`, async () => {
    membership.resolveTeamAccess.mockRejectedValue(forbidden())
    const result = await tool(`exponential_members_list`)({
      teamId: WS,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not allowed here`)
    expect(db.select).not.toHaveBeenCalled()
  })
})

// ── attachments_upload (base64 image → S3 + attachments row) ─────────────────

describe(`exponential_attachments_upload`, () => {
  const args = {
    issueId: UUID,
    filename: `shot.png`,
    contentType: `image/png`,
    dataBase64: Buffer.from(`fake-png-bytes`).toString(`base64`),
    alt: `a shot`,
  }

  it(`uploads and returns the canonical markdown form`, async () => {
    const result = await tool(`exponential_attachments_upload`)(args)
    const payload = parseOk(result) as {
      id: string
      url: string
      markdown: string
      width: number
    }
    expect(payload.url).toBe(`/api/attachments/${payload.id}`)
    expect(payload.markdown).toBe(`![a shot](/api/attachments/${payload.id})`)
    expect(payload.width).toBe(12)
    expect(uploadObject).toHaveBeenCalledTimes(1)
    expect(insertValues).toHaveBeenCalledTimes(1)
    expect(assertWithinStorageLimit).toHaveBeenCalledWith(
      `ws-1`,
      expect.any(Number)
    )
  })

  // EXP-297: any content type is accepted now. Non-images attach to the
  // issue's Files list and deliberately get NO markdown field (embedding them
  // would break the description round-trip guard) and no probed dimensions.
  it(`accepts a non-image content type without markdown or dimensions`, async () => {
    const result = await tool(`exponential_attachments_upload`)({
      ...args,
      filename: `spec.pdf`,
      contentType: `application/pdf`,
    })
    const payload = parseOk(result) as {
      id: string
      markdown?: string
      width: number | null
      height: number | null
    }
    expect(payload.markdown).toBeUndefined()
    expect(payload.width).toBeNull()
    expect(payload.height).toBeNull()
    expect(uploadObject).toHaveBeenCalledTimes(1)
    expect(insertValues).toHaveBeenCalledTimes(1)
  })

  it(`rejects an empty payload before touching storage`, async () => {
    const result = await tool(`exponential_attachments_upload`)({
      ...args,
      dataBase64: ``,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`empty`)
    expect(uploadObject).not.toHaveBeenCalled()
  })

  it(`denies when the user is not a team member`, async () => {
    membership.assertTeamMember.mockRejectedValue(forbidden())
    const result = await tool(`exponential_attachments_upload`)(args)
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not allowed here`)
    expect(uploadObject).not.toHaveBeenCalled()
    expect(insertValues).not.toHaveBeenCalled()
  })
})

// ── EXP-988: the contract's three-shape upload + the two new tools ───────────

describe(`exponential_attachments_upload signed path (EXP-988/EXP-929)`, () => {
  it(`without dataBase64 checks access first, then mints a signed upload URL with a curl line`, async () => {
    const result = await tool(`exponential_attachments_upload`)({
      issueId: UUID,
      filename: `shot.png`,
      contentType: `image/png`,
    })
    const payload = parseOk(result) as {
      attachmentId: string
      uploadUrl: string
      expiresAt: string
      curl: string
    }
    expect(membership.assertTeamMember).toHaveBeenCalledWith(USER.id, `ws-1`)
    // The sessions_results grant shape: the bytes never cross MCP, and no
    // row exists until the PUT lands (EXP-929).
    expect(
      payload.uploadUrl.startsWith(`https://x.test/api/attachment-uploads/`)
    ).toBe(true)
    expect(payload.curl).toBe(`curl -sS --retry 4 -T 'shot.png' "${payload.uploadUrl}"`)
    expect(Date.parse(payload.expiresAt)).toBeGreaterThan(Date.now())
    expect(payload.attachmentId).toMatch(/^[0-9a-f-]{36}$/)
    expect(uploadObject).not.toHaveBeenCalled()
    expect(insertValues).not.toHaveBeenCalled()
  })

  it(`denies the signed path to a non-member before minting anything`, async () => {
    membership.assertTeamMember.mockRejectedValue(forbidden())
    const result = await tool(`exponential_attachments_upload`)({
      issueId: UUID,
      filename: `shot.png`,
      contentType: `image/png`,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not allowed here`)
  })

  it(`attachmentId alone is the finalize call`, async () => {
    // No row for the id yet: the handler tells the agent to run the curl
    // line first (the rest of finalize is covered in handlers/).
    dbRows.current = []
    const result = await tool(`exponential_attachments_upload`)({
      attachmentId: UUID,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`No upload has landed`)
    expect(membership.getIssueTeamContext).not.toHaveBeenCalled()
  })

  it(`refuses attachmentId mixed with first-call fields`, async () => {
    const result = await tool(`exponential_attachments_upload`)({
      attachmentId: UUID,
      issueId: UUID,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`attachmentId alone`)
  })

  it(`still requires issueId, filename and contentType on a first call`, async () => {
    const result = await tool(`exponential_attachments_upload`)({
      issueId: UUID,
      filename: `shot.png`,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`contentType are required`)
  })

  it(`keeps commentId off the inline path`, async () => {
    const result = await tool(`exponential_attachments_upload`)({
      issueId: UUID,
      filename: `shot.png`,
      contentType: `image/png`,
      dataBase64: Buffer.from(`x`).toString(`base64`),
      commentId: UUID,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`attachmentIds`)
    expect(uploadObject).not.toHaveBeenCalled()
  })
})

describe(`exponential_attachments_list (EXP-988/EXP-979)`, () => {
  it(`applies attachments_get's access rule, then reaches the handler`, async () => {
    const result = await tool(`exponential_attachments_list`)({
      issueId: UUID,
      limit: 50,
      offset: 0,
    })
    expect(result.isError).toBeFalsy()
    expect(JSON.parse(result.content[0].text ?? `null`)).toEqual({
      attachments: [],
      total: 0,
    })
    expect(membership.getIssueTeamContext).toHaveBeenCalledWith(UUID)
    expect(membership.resolveTeamAccess).toHaveBeenCalledWith(USER.id, `ws-1`)
    // The query is scoped to the resolved issue.
    expect(renderWhere().sql).toContain(`"attachments"."issue_id" = $1`)
  })

  it(`denies a non-member`, async () => {
    membership.resolveTeamAccess.mockRejectedValue(forbidden())
    const result = await tool(`exponential_attachments_list`)({
      issueId: UUID,
      limit: 50,
      offset: 0,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not allowed here`)
  })
})

describe(`exponential_sessions_compact (EXP-988/EXP-936)`, () => {
  it(`needs a session header`, async () => {
    const headerless = collectTools()
    const handler = headerless.get(`exponential_sessions_compact`)
    expect(handler).toBeDefined()
    const result = await handler!({ reason: `long` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`X-Exp-Session-Id`)
  })

  it(`relays the header run's ask and answers with the host's verdict`, async () => {
    dbRows.current = [
      {
        id: SESSION,
        userId: USER.id,
        hostUserId: null,
        status: `running`,
        agent: `claude`,
      },
    ]
    vi.mocked(getSteerRelayConfig).mockReturnValue({
      url: `wss://relay.test`,
      secret: `s`,
    })
    vi.mocked(relayPostCompact).mockResolvedValue({
      delivered: true,
      accepted: false,
      refusedBecause: `too_early`,
    })
    const inRun = collectTools(USER, SESSION)
    const result = await inRun.get(`exponential_sessions_compact`)!({
      reason: `long`,
      keep: `open threads`,
    })
    expect(parseOk(result)).toEqual({
      accepted: false,
      refusedBecause: `too_early`,
    })
    expect(relayPostCompact).toHaveBeenCalledWith(
      { url: `wss://relay.test`, secret: `s` },
      SESSION,
      `open threads`
    )
  })
})

// ── attachments_delete (delegates to the attachments router) ─────────────────

describe(`exponential_attachments_delete`, () => {
  it(`delegates to the router so the rewrite/reclaim logic is never forked`, async () => {
    caller.attachments.delete.mockResolvedValue({ txId: 7 })
    const result = await tool(`exponential_attachments_delete`)({ id: UUID })
    expect(parseOk(result)).toEqual({ ok: true, id: UUID })
    expect(caller.attachments.delete).toHaveBeenCalledWith({ id: UUID })
  })

  it(`surfaces the router's authorization failure`, async () => {
    caller.attachments.delete.mockRejectedValue(forbidden())
    const result = await tool(`exponential_attachments_delete`)({ id: UUID })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not allowed here`)
  })
})

// ── attachments_get (EXP-704: every content type via signed download URL) ────

describe(`exponential_attachments_get`, () => {
  const bytes = (s: string) => ({
    Body: { transformToByteArray: async () => new TextEncoder().encode(s) },
  })

  // The URL origin prefers BETTER_AUTH_URL; these cases exercise the
  // request-origin fallback, so keep the var unset for them.
  const envBase = process.env.BETTER_AUTH_URL
  beforeEach(() => {
    delete process.env.BETTER_AUTH_URL
  })
  afterEach(() => {
    if (envBase === undefined) delete process.env.BETTER_AUTH_URL
    else process.env.BETTER_AUTH_URL = envBase
  })

  it(`returns metadata + a signed downloadUrl for a non-image (xlsx)`, async () => {
    membership.getAttachmentTeamContext.mockResolvedValue({
      teamId: WS,
      boardId: PROJ,
      contentType: `application/vnd.openxmlformats-officedocument.spreadsheetml.sheet`,
      filename: `report.xlsx`,
      sizeBytes: 123_456,
      storageKey: `k`,
    })
    const result = await tool(`exponential_attachments_get`)({ id: UUID })
    const payload = parseOk(result) as Record<string, unknown>
    expect(payload).toMatchObject({
      id: UUID,
      filename: `report.xlsx`,
      contentType: `application/vnd.openxmlformats-officedocument.spreadsheetml.sheet`,
      sizeBytes: 123_456,
    })
    expect(payload.downloadUrl).toMatch(
      new RegExp(`^https://x\\.test/api/attachments/${UUID}\\?token=.+`)
    )
    expect(Date.parse(payload.expiresAt as string)).toBeGreaterThan(Date.now())
    // No base64-blob-in-context: the bytes ride the URL, not the payload.
    expect(h.getObject).not.toHaveBeenCalled()
    expect(payload.text).toBeUndefined()
    expect(membership.resolveTeamAccess).toHaveBeenCalledWith(USER.id, WS)
  })

  it(`keeps the inline image block AND adds the metadata payload`, async () => {
    membership.getAttachmentTeamContext.mockResolvedValue({
      teamId: WS,
      boardId: PROJ,
      contentType: `image/png`,
      filename: `shot.png`,
      sizeBytes: 4,
      storageKey: `k`,
    })
    h.getObject.mockResolvedValue(bytes(`png!`))
    const result = await tool(`exponential_attachments_get`)({ id: UUID })
    expect(result.isError).toBeFalsy()
    expect(result.content[0]).toMatchObject({
      type: `image`,
      mimeType: `image/png`,
      data: Buffer.from(`png!`).toString(`base64`),
    })
    const payload = JSON.parse(result.content[1].text!) as Record<
      string,
      unknown
    >
    expect(payload.downloadUrl).toContain(`?token=`)
    // EXP-854: what was inlined is stated in the payload. These bytes are not
    // a decodable image, so the bound falls back to the ORIGINAL rather than
    // dropping the picture (EXP-511) and says so.
    expect(payload.inline).toEqual({
      mimeType: `image/png`,
      bytes: 4,
      downscaled: false,
    })
  })

  it(`inlines small text files next to the URL`, async () => {
    membership.getAttachmentTeamContext.mockResolvedValue({
      teamId: WS,
      boardId: PROJ,
      contentType: `text/csv`,
      filename: `data.csv`,
      sizeBytes: 10,
      storageKey: `k`,
    })
    h.getObject.mockResolvedValue(bytes(`a,b\n1,2\n`))
    const result = await tool(`exponential_attachments_get`)({ id: UUID })
    const payload = parseOk(result) as Record<string, unknown>
    expect(payload.text).toBe(`a,b\n1,2\n`)
    expect(payload.downloadUrl).toContain(`?token=`)
  })

  it(`builds the downloadUrl from BETTER_AUTH_URL, not the request origin`, async () => {
    // Behind a TLS-terminating proxy the request Bun sees is plain HTTP, so
    // the request origin would hand agents an http:// URL that redirects.
    const previous = process.env.BETTER_AUTH_URL
    process.env.BETTER_AUTH_URL = `https://app.example.com/`
    try {
      membership.getAttachmentTeamContext.mockResolvedValue({
        teamId: WS,
        boardId: PROJ,
        contentType: `application/pdf`,
        filename: `spec.pdf`,
        sizeBytes: 999,
        storageKey: `k`,
      })
      const result = await tool(`exponential_attachments_get`)({ id: UUID })
      const payload = parseOk(result) as Record<string, unknown>
      expect(payload.downloadUrl).toMatch(
        new RegExp(`^https://app\\.example\\.com/api/attachments/${UUID}\\?token=.+`)
      )
    } finally {
      if (previous === undefined) delete process.env.BETTER_AUTH_URL
      else process.env.BETTER_AUTH_URL = previous
    }
  })

  it(`skips inline text above the size cap`, async () => {
    membership.getAttachmentTeamContext.mockResolvedValue({
      teamId: WS,
      boardId: PROJ,
      contentType: `text/plain`,
      filename: `big.txt`,
      sizeBytes: 40 * 1024,
      storageKey: `k`,
    })
    const result = await tool(`exponential_attachments_get`)({ id: UUID })
    const payload = parseOk(result) as Record<string, unknown>
    expect(payload.text).toBeUndefined()
    expect(h.getObject).not.toHaveBeenCalled()
    expect(payload.downloadUrl).toContain(`?token=`)
  })
})

// ── statuses (EXP-238: custom statuses over MCP) ─────────────────────────────

describe(`exponential_statuses_list`, () => {
  it(`returns contract-ordered rows with per-category positions`, async () => {
    const at = (iso: string) => new Date(iso)
    // Deliberately shuffled: the tool must order by category display order
    // (backlog, unstarted, started, …), then sortOrder, createdAt, id.
    dbRows.current = [
      // EXP-685 retired the Todo builtin, so `unstarted` is a customs-only
      // category now — this row has no builtinKey.
      {
        id: `b`,
        name: `Triage`,
        category: `unstarted`,
        color: `#6b7280`,
        builtinKey: null,
        sortOrder: 1,
        createdAt: at(`2026-01-01T00:00:00Z`),
      },
      {
        id: `a`,
        name: `QA`,
        category: `started`,
        color: `#ff8800`,
        builtinKey: null,
        sortOrder: 2,
        createdAt: at(`2026-01-02T00:00:00Z`),
      },
      {
        id: `c`,
        name: `In Progress`,
        category: `started`,
        color: `#f59e0b`,
        builtinKey: `in_progress`,
        sortOrder: 1,
        createdAt: at(`2026-01-01T00:00:00Z`),
      },
    ]
    const result = await tool(`exponential_statuses_list`)({ teamId: WS })
    expect(parseOk(result)).toEqual([
      {
        id: `b`,
        name: `Triage`,
        category: `unstarted`,
        color: `#6b7280`,
        position: 1,
        builtinKey: null,
      },
      {
        id: `c`,
        name: `In Progress`,
        category: `started`,
        color: `#f59e0b`,
        position: 1,
        builtinKey: `in_progress`,
      },
      {
        id: `a`,
        name: `QA`,
        category: `started`,
        color: `#ff8800`,
        position: 2,
        builtinKey: null,
      },
    ])
  })

  it(`denies when the user is not in the team`, async () => {
    membership.resolveTeamAccess.mockRejectedValue(forbidden())
    const result = await tool(`exponential_statuses_list`)({ teamId: WS })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not allowed here`)
  })
})

describe(`exponential_issues_update statusId passthrough`, () => {
  it(`forwards statusId to the issues router untouched`, async () => {
    caller.issues.update.mockResolvedValue({
      issue: { id: UUID, statusId: PROJ },
    })
    const result = await tool(`exponential_issues_update`)({
      id: UUID,
      statusId: PROJ,
    })
    expect(parseOk(result)).toEqual({ id: UUID, statusId: PROJ })
    expect(caller.issues.update).toHaveBeenCalledWith({
      id: UUID,
      statusId: PROJ,
      description: undefined,
    })
  })
})

// ── EXP-684: exponential_issues_list filters ─────────────────────────────
// The sweep an automation runs ("created in the last day on boards A+B, not
// done/cancelled, no labels") must be ONE call, so every predicate renders
// server-side. The where clause is rendered back to SQL and inspected.

describe(`exponential_issues_list filters (EXP-684)`, () => {
  const LABEL = `aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa`
  const LABEL2 = `bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb`
  const BOARD2 = `cccccccc-cccc-cccc-cccc-cccccccccccc`

  function whereSql() {
    return new PgDialect().sqlToQuery(state.capturedWhere as never)
  }
  function orderBySql() {
    const builder = db.select() as unknown as {
      orderBy: ReturnType<typeof vi.fn>
    }
    const call = builder.orderBy.mock.calls.at(-1) as Array<unknown>
    return call.map(
      (expr) => new PgDialect().sqlToQuery(expr as never).sql
    )
  }
  const list = (args: Record<string, unknown>) =>
    tool(`exponential_issues_list`)({
      sort: `-createdAt`,
      limit: 50,
      offset: 0,
      ...args,
    })

  it(`expresses the auto-label sweep in one query`, async () => {
    await list({
      boardIds: [PROJ, BOARD2],
      createdAfter: `2026-08-29T10:00:00Z`,
      excludeStatus: [`done`, `cancelled`],
      unlabeled: true,
    })
    // Every named board is access-checked like the single-board form.
    expect(membership.getBoardTeamId).toHaveBeenCalledTimes(2)
    expect(membership.resolveTeamAccess).toHaveBeenCalledTimes(2)
    const { sql, params } = whereSql()
    expect(params).toContain(PROJ)
    expect(params).toContain(BOARD2)
    expect(sql).toContain(`"created_at" >= `)
    expect(params).toContain(`2026-08-29T10:00:00.000Z`)
    expect(sql).toContain(`"status" not in (`)
    expect(params).toEqual(expect.arrayContaining([`done`, `cancelled`]))
    expect(sql).toMatch(
      /not exists \(select 1 from "issue_labels" where "issue_labels"\."issue_id" = "issues"\."id"\)/
    )
  })

  // SLOP-4: widget-filed reports vs member-created issues.
  it(`filters by source`, async () => {
    await list({ boardId: PROJ, source: `widget` })
    const { sql, params } = whereSql()
    expect(sql).toContain(`"source" = `)
    expect(params).toContain(`widget`)
  })

  it(`filters custom statuses by row id and by category`, async () => {
    await list({
      boardId: PROJ,
      statusId: [STATUS],
      statusCategory: [`unstarted`],
      excludeStatusCategory: [`completed`, `cancelled`],
      excludeStatusId: [UUID],
    })
    const { sql, params } = whereSql()
    expect(sql).toContain(`"issues"."status_id" in (`)
    expect(params).toContain(STATUS)
    // Category filters resolve through issue_statuses, never the anchor enum
    // (a custom "Ideas" anchors to backlog but lives in unstarted).
    expect(sql).toMatch(
      /"issues"\."status_id" in \(select "issue_statuses"\."id" from "issue_statuses" where "issue_statuses"\."category" in \(\$\d+\)\)/
    )
    expect(sql).toMatch(
      /"issues"\."status_id" not in \(select "issue_statuses"\."id" from "issue_statuses" where "issue_statuses"\."category" in \(\$\d+, \$\d+\)\)/
    )
    expect(params).toEqual(
      expect.arrayContaining([`unstarted`, `completed`, `cancelled`, UUID])
    )
    // An exclude never drops a row whose status_id is NULL.
    expect(sql).toContain(`"issues"."status_id" is null or `)
  })

  it(`matches labels any-of, all-of, and the updated range`, async () => {
    await list({
      boardId: PROJ,
      labelIds: [LABEL, LABEL2, LABEL2],
      labelMatch: `all`,
      updatedAfter: `2026-08-01`,
      updatedBefore: `2026-08-30T23:59:59Z`,
    })
    let q = whereSql()
    // Duplicates in labelIds collapse so the distinct count still matches.
    expect(q.sql).toMatch(
      /\(select count\(distinct "issue_labels"\."label_id"\) from "issue_labels" where "issue_labels"\."issue_id" = "issues"\."id" and "issue_labels"\."label_id" in \(\$\d+, \$\d+\)\) = \$\d+/
    )
    expect(q.params).toContain(2)
    expect(q.sql).toContain(`"updated_at" >= `)
    expect(q.sql).toContain(`"updated_at" <= `)
    expect(q.params).toContain(`2026-08-01T00:00:00.000Z`)

    await list({ boardId: PROJ, labelIds: [LABEL] })
    q = whereSql()
    expect(q.sql).toMatch(
      /exists \(select 1 from "issue_labels" where "issue_labels"\."issue_id" = "issues"\."id" and "issue_labels"\."label_id" in \(\$\d+\)\)/
    )
    expect(q.params).toContain(LABEL)
  })

  it(`filters on comment presence and author`, async () => {
    await list({
      boardId: PROJ,
      hasComments: false,
      notCommentedBy: `user-1`,
      commentedBy: `user-2`,
    })
    const { sql, params } = whereSql()
    expect(sql).toMatch(
      /not exists \(select 1 from "comments" where "comments"\."issue_id" = "issues"\."id"\)/
    )
    expect(sql).toMatch(
      /not exists \(select 1 from "comments" where "comments"\."issue_id" = "issues"\."id" and "comments"\."author_id" = \$\d+\)/
    )
    expect(sql).toMatch(
      /(?<!not )exists \(select 1 from "comments" where "comments"\."issue_id" = "issues"\."id" and "comments"\."author_id" = \$\d+\)/
    )
    expect(params).toEqual(expect.arrayContaining([`user-1`, `user-2`]))
  })

  it(`sorts by the requested field with a -prefix for descending`, async () => {
    await list({ boardId: PROJ, sort: `updatedAt` })
    expect(orderBySql()[0]).toBe(`"issues"."updated_at" asc`)
    await list({ boardId: PROJ, sort: `-priority` })
    const [first] = orderBySql()
    expect(first).toContain(`case "issues"."priority" when 'urgent' then 4`)
    expect(first).toMatch(/ desc$/)
    await list({ boardId: PROJ })
    expect(orderBySql()).toEqual([
      `"issues"."created_at" desc`,
      `"issues"."created_at" desc`,
      `"issues"."id" desc`,
    ])
  })

  // EXP-847: a listing is about OPEN work, and its rows are a SUMMARY.
  it(`hides closed issues unless asked, and says so on the way out`, async () => {
    await list({ boardId: PROJ })
    let q = whereSql()
    // The status ROW's category decides (customs included), with the
    // dual-written anchor covering a NULL status_id.
    expect(q.sql).toMatch(
      /"issues"\."status_id" not in \(select "issue_statuses"\."id" from "issue_statuses" where "issue_statuses"\."category" in \(\$\d+, \$\d+, \$\d+\)\)/
    )
    expect(q.params).toEqual(
      expect.arrayContaining([`completed`, `cancelled`, `duplicate`])
    )
    expect(q.sql).toContain(`"status" not in (`)
    expect(q.params).toEqual(expect.arrayContaining([`done`, `duplicate`]))

    // includeClosed asks for them back…
    await list({ boardId: PROJ, includeClosed: true })
    q = whereSql()
    expect(q.params).not.toContain(`completed`)
    // …and so does ANY explicit status filter.
    await list({ boardId: PROJ, status: [`done`] })
    q = whereSql()
    expect(q.sql).not.toContain(`not in (select "issue_statuses"."id"`)
    await list({ boardId: PROJ, statusCategory: [`completed`] })
    expect(whereSql().sql).not.toContain(`"status" not in (`)
    await list({ boardId: PROJ, statusId: [STATUS] })
    expect(whereSql().sql).not.toContain(`"status" not in (`)
  })

  it(`cuts list descriptions to 200 chars (issues_get keeps the full text)`, async () => {
    const long = `x`.repeat(250)
    dbRows.current = [
      { id: UUID, identifier: `MET-1`, description: long },
      { id: PROJ, identifier: `MET-2`, description: `short` },
      { id: STATUS, identifier: `MET-3`, description: null },
    ]
    const rows = parseOk(await list({ boardId: PROJ })) as Array<{
      description: string | null
    }>
    expect(rows[0].description).toBe(`${`x`.repeat(200)}…`)
    expect(rows[1].description).toBe(`short`)
    expect(rows[2].description).toBeNull()
  })

  it(`pages up to 1000 rows`, () => {
    const schema = collectToolDefs().get(`exponential_issues_list`)!
      .inputSchema! as z.ZodType<{ limit?: number }>
    expect(schema.parse({}).limit).toBe(50)
    expect(schema.parse({ limit: 1000 }).limit).toBe(1000)
    expect(schema.safeParse({ limit: 1001 }).success).toBe(false)
  })

  it(`validates the budget-trimmed (enum-free) inputs at runtime`, () => {
    const def = collectToolDefs().get(`exponential_issues_list`)!
    const schema = def.inputSchema! as z.ZodType<
      { sort?: string } & Record<string, unknown>
    >
    const parsed = schema.parse({
      excludeStatus: [`done`],
      excludeStatusCategory: [`completed`],
      priority: [`urgent`],
      sort: `-updatedAt`,
      createdAfter: `2026-08-29`,
      dueBefore: `2026-09-01`,
    })
    expect(parsed.sort).toBe(`-updatedAt`)
    expect(schema.parse({}).sort).toBe(`-createdAt`)
    expect(schema.safeParse({ excludeStatus: [`nope`] }).success).toBe(false)
    // EXP-847: status/statusCategory lost their inline enums for the budget —
    // they still refuse an invented value.
    expect(schema.safeParse({ status: [`done`] }).success).toBe(true)
    expect(schema.safeParse({ status: [`nope`] }).success).toBe(false)
    expect(schema.safeParse({ statusCategory: [`completed`] }).success).toBe(
      true
    )
    expect(schema.safeParse({ statusCategory: [`done`] }).success).toBe(false)
    expect(schema.safeParse({ excludeStatusCategory: [`done`] }).success).toBe(
      false
    )
    expect(schema.safeParse({ priority: [`p1`] }).success).toBe(false)
    expect(schema.safeParse({ sort: `title` }).success).toBe(false)
    expect(schema.safeParse({ createdAfter: `yesterday` }).success).toBe(false)
    expect(schema.safeParse({ dueAfter: `2026-8-1` }).success).toBe(false)
    expect(schema.safeParse({ search: `` }).success).toBe(false)
  })
})

describe(`exponential_issues_create statusId passthrough (EXP-684)`, () => {
  it(`forwards statusId so an issue can be created in a custom status`, async () => {
    caller.issues.create.mockResolvedValue({
      issue: { id: UUID, statusId: STATUS },
    })
    const result = await tool(`exponential_issues_create`)({
      boardId: PROJ,
      title: `Idea`,
      statusId: STATUS,
    })
    expect(parseOk(result)).toEqual({ id: UUID, statusId: STATUS })
    expect(caller.issues.create).toHaveBeenCalledWith({
      boardId: PROJ,
      title: `Idea`,
      statusId: STATUS,
      description: undefined,
    })
  })

  // EXP-760: filing a sub-issue is ONE call — the tool forwards `parentId` and
  // the router writes the `parent` relation in the create transaction, so an
  // agent never has to follow up with issue_relations_add.
  it(`forwards parentId so an issue can be filed as a sub-issue`, async () => {
    caller.issues.create.mockResolvedValue({
      issue: { id: UUID, identifier: `EXP-2` },
    })
    const result = await tool(`exponential_issues_create`)({
      boardId: PROJ,
      title: `Child`,
      parentId: WS,
    })
    expect(parseOk(result)).toEqual({ id: UUID, identifier: `EXP-2` })
    expect(caller.issues.create).toHaveBeenCalledWith({
      boardId: PROJ,
      title: `Child`,
      parentId: WS,
      description: undefined,
    })
  })
})

// ── EXP-496: exponential_report_bug ──────────────────────────────────────────
// Cloud-only vendor bug intake — registration is gated on the instance having
// an in-app feedback widget (buildRuntimeConfig().feedbackWidget).

describe(`exponential_report_bug`, () => {
  it(`is not registered without a feedback widget (self-hosted default)`, () => {
    // The module-level collectTools() above ran with CLOUD_INSTANCE unset.
    expect(tools.has(`exponential_report_bug`)).toBe(false)
  })

  it(`files the report as the MCP user via createAgentBugReport`, async () => {
    vi.stubEnv(`CLOUD_INSTANCE`, `true`)
    try {
      const cloudTools = collectTools()
      const handler = cloudTools.get(`exponential_report_bug`)
      expect(handler).toBeDefined()
      const result = await handler!({
        title: `Sync loop stuck`,
        description: `Steps: …`,
      })
      // NEW-19: the reporter's way back — the magic-link page and the
      // MCP reads that open the issue to them.
      expect(parseOk(result)).toEqual({
        issueId: `bug-issue-1`,
        identifier: `EXP-1`,
        next: expect.stringContaining(`EXP-1`),
      })
      expect(h.createAgentBugReport).toHaveBeenCalledWith({
        widgetKey: expect.stringMatching(/^expw_/),
        reporter: { email: `u@example.com`, name: `User One` },
        title: `Sync loop stuck`,
        description: `Steps: …`,
        userAgent: `claude-code/test`,
      })
    } finally {
      vi.unstubAllEnvs()
    }
  })

  it(`rate-limits per user without calling the intake`, async () => {
    vi.stubEnv(`CLOUD_INSTANCE`, `true`)
    try {
      // Fresh user id → fresh token bucket (the limiter is module-scoped).
      const user = { ...(USER as object), id: `rate-limit-user` } as McpUser
      const handler = collectTools(user).get(`exponential_report_bug`)!
      // Burst capacity is 3; the 4th call must fail without reaching intake.
      for (let i = 0; i < 3; i += 1) {
        const result = await handler({ title: `t`, description: `d` })
        expect(result.isError).toBeFalsy()
      }
      h.createAgentBugReport.mockClear()
      const limited = await handler({ title: `t`, description: `d` })
      expect(limited.isError).toBe(true)
      expect(limited.content[0].text).toContain(`Too many bug reports`)
      expect(h.createAgentBugReport).not.toHaveBeenCalled()
    } finally {
      vi.unstubAllEnvs()
    }
  })
})

describe(`exponential_statuses_list color`, () => {
  it(`passes each row's color through`, async () => {
    dbRows.current = [
      {
        id: `a`,
        name: `QA`,
        category: `started`,
        color: `#ff8800`,
        builtinKey: null,
        sortOrder: 1,
        createdAt: new Date(`2026-01-01T00:00:00Z`),
      },
    ]
    const result = await tool(`exponential_statuses_list`)({ teamId: WS })
    expect(parseOk(result)).toEqual([
      {
        id: `a`,
        name: `QA`,
        category: `started`,
        color: `#ff8800`,
        position: 1,
        builtinKey: null,
      },
    ])
  })
})

// EXP-639: the tool reads the devices ROWS directly — `devices.list` (tRPC +
// relay presence) is gone, so `online` is last_seen_at freshness against the
// contract window, exactly like every synced client computes it.
describe(`exponential_devices_list`, () => {
  const deviceRow = (over: Record<string, unknown> = {}) => ({
    id: `row-1`,
    userId: `user-1`,
    deviceId: `mac-1`,
    label: `Mac`,
    kind: `desktop`,
    platform: `macos`,
    version: `1.2.3`,
    agents: [`claude`],
    unauthedAgents: [],
    caps: [`actions`, `resume-run`],
    launchDefaults: null,
    updateRequestedAt: null,
    activeSessions: 0,
    lastSeenAt: new Date(),
    sharedTeamIds: [],
    isDefault: true,
    ...over,
  })

  it(`projects the caller's own rows and derives online from last_seen_at`, async () => {
    dbRows.current = [deviceRow()]
    const result = await tool(`exponential_devices_list`)({})
    expect(parseOk(result)).toEqual([
      {
        deviceId: `mac-1`,
        label: `Mac`,
        kind: `desktop`,
        platform: `macos`,
        online: true,
        lastSeenAt: (dbRows.current[0] as { lastSeenAt: Date }).lastSeenAt.toISOString(),
        agents: [`claude`],
        unauthedAgents: [],
        caps: [`actions`, `resume-run`],
        version: `1.2.3`,
        sharedTeamIds: [],
        isDefault: true,
        // EXP-484: null until the machine's collector reports.
        agentAccounts: null,
        agentUsage: null,
        agentUsageAt: null,
        // EXP-1196: null until the device reports a doctor.
        doctor: null,
      },
    ])
    // No teamId ⇒ no shared join, so nothing is gated on team membership.
    expect(membership.assertTeamMember).not.toHaveBeenCalled()
  })

  it(`carries the device's readiness report (EXP-1196)`, async () => {
    const doctor = {
      checkedAt: `2026-10-05T19:00:00.000Z`,
      items: [{ key: `git`, group: `required`, state: `ok`, detail: `2.55.0` }],
    }
    dbRows.current = [deviceRow({ doctor })]
    const [device] = parseOk(
      await tool(`exponential_devices_list`)({})
    ) as Array<{ doctor: unknown }>
    expect(device!.doctor).toEqual(doctor)
  })

  it(`reads a row past the window as offline`, async () => {
    dbRows.current = [
      deviceRow({ lastSeenAt: new Date(Date.now() - 10 * 60_000) }),
    ]
    const [device] = parseOk(
      await tool(`exponential_devices_list`)({})
    ) as Array<{ online: boolean }>
    expect(device!.online).toBe(false)
  })

  it(`gates a teamId on both the OAuth grant and live membership`, async () => {
    const scoped = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_WS
    ).get(`exponential_devices_list`)!({ teamId: PROJ })
    expect(scoped.isError).toBe(true)
    expect(db.select).not.toHaveBeenCalled()

    membership.assertTeamMember.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN`, message: `not a member` })
    )
    const denied = await tool(`exponential_devices_list`)({ teamId: WS })
    expect(denied.isError).toBe(true)
    expect(denied.content[0].text).toContain(`not a member`)
    expect(db.select).not.toHaveBeenCalled()
  })
})

// EXP-1199: a sign-in on the user's own machine is no team's or board's to
// grant, so only a full-access connection may queue one.
describe(`exponential_devices_account_login`, () => {
  const args = { deviceId: `mac-1`, agent: `claude`, code: `abc#123` }

  it(`refuses a grant-scoped token before any device command`, async () => {
    const result = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_WS
    ).get(`exponential_devices_account_login`)!(args)
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not granted access`)
    expect(db.select).not.toHaveBeenCalled()
    expect(caller.devices.createCommand).not.toHaveBeenCalled()
    expect(caller.devices.getCommand).not.toHaveBeenCalled()
  })

  it(`queues the command for a full-access connection`, async () => {
    caller.devices.createCommand.mockResolvedValue({ id: UUID })
    caller.devices.getCommand.mockResolvedValue({
      id: UUID,
      kind: `agent_login_code`,
      status: `done`,
      result: null,
    })
    const result = await tool(`exponential_devices_account_login`)(args)
    expect(parseOk(result)).toMatchObject({
      status: `signing_in`,
      commandId: UUID,
    })
    expect(caller.devices.createCommand).toHaveBeenCalledWith({
      deviceId: `mac-1`,
      kind: `agent_login_code`,
      agent: `claude`,
      code: `abc#123`,
    })
  })
})

// EXP-846: the drift gate between the REGISTERED tool surface and the
// contract's display rows. A feed row captions an Exponential tool call from
// `expToolDisplay` (`lib/agent-feed.ts` + the three native mirrors), so a tool
// the contract does not know renders as its RAW wire name
// (`mcp__exponential__exponential_whatever`) on all four clients at once.
// Equality BOTH ways: a new tool has to land in the contract, and a row whose
// tool was renamed or retired has to go with it.
describe(`expToolDisplay covers the whole tool surface (EXP-846)`, () => {
  it(`matches the registered exponential_* tools name for name`, () => {
    const prefix = contract.expToolDisplay.prefix
    // The WIDEST surface: every gate open plus the cloud-only
    // `exponential_report_bug` — a row the contract must describe too, since
    // cloud agents call it.
    vi.stubEnv(`CLOUD_INSTANCE`, `true`)
    // NEW-4: every per-operation tool must still be reachable as an op of a
    // noun (nouns.ts routes it), and the contract describes the row
    // `<noun>_<op>` the engine composes for a call.
    const names = (() => {
      try {
        return [...collectToolDefs().keys()]
      } finally {
        vi.unstubAllEnvs()
      }
    })()
    // Every tool this server registers is `exponential_*` — the prefix IS the
    // match rule the clients apply, so a bare name could never be captioned.
    expect(names.filter((name) => !name.startsWith(prefix))).toEqual([])
    const rows = displayRowNames()
    const routed = rows.map((row) => legacyNameOfRow(row)).sort()
    expect(routed).toEqual([...names].sort())
    const registered = [...rows].sort()
    const described = contract.expToolDisplay.tools
      .map((row) => row.name)
      .sort()
    expect(registered).toEqual(described)
  })

  // EXP-862: the same rows carry the SETTINGS copy — the built-in tools group
  // of the MCP servers page (web + desktop) renders title + blurb, with the
  // wire name only as a tooltip. A row without copy would render blank there,
  // so every tool has both, they stay short enough for one line, and they keep
  // the multi-client no-em-dash rule.
  it(`gives every tool a title and a blurb`, () => {
    for (const row of contract.expToolDisplay.tools) {
      expect(row.title.trim().length, row.name).toBeGreaterThan(0)
      expect(row.blurb.trim().length, row.name).toBeGreaterThan(0)
      expect(row.title.length, row.name).toBeLessThanOrEqual(60)
      expect(row.blurb.length, row.name).toBeLessThanOrEqual(120)
      expect(`${row.title}${row.blurb}`.includes(`—`), row.name).toBe(false)
    }
    // Titles are the list's labels: two rows sharing one would read as a
    // duplicate entry.
    const titles = contract.expToolDisplay.tools.map((row) => row.title)
    expect(new Set(titles).size).toBe(titles.length)
  })

  // EXP-948: a run of consecutive calls to the SAME tool renders the row's
  // PLURAL copy ("Read 3 issues"), so both forms exist, both carry the `{n}`
  // the client substitutes, and they keep the no-em-dash rule.
  it(`gives every tool a plural caption for a run of calls`, () => {
    for (const row of contract.expToolDisplay.tools) {
      expect(row.progressiveMany.includes(`{n}`), row.name).toBe(true)
      expect(row.doneMany.includes(`{n}`), row.name).toBe(true)
      expect(row.progressiveMany.length, row.name).toBeLessThanOrEqual(60)
      expect(row.doneMany.length, row.name).toBeLessThanOrEqual(60)
      expect(
        `${row.progressiveMany}${row.doneMany}`.includes(`—`),
        row.name
      ).toBe(false)
    }
  })
})

// SLOP-4: a reporter reply emails an outside address, so only a person's
// full-access key may send one: never a confined OAuth grant, never an
// unattended session (EXP-1291: a child, or a root another session spawned).
// A person's own session keeps it.
describe(`exponential_comments_create audience reporter`, () => {
  const args = { issueId: UUID, body: `Fixed.`, audience: `reporter` }
  const ownSession = (over: Record<string, unknown> = {}) => ({
    id: SESSION,
    teamId: `ws-1`,
    rootId: SESSION,
    parentId: null,
    userId: `user-1`,
    hostUserId: null,
    rootAgentId: null,
    watched: null,
    ...over,
  })

  it(`refuses a board-confined OAuth grant`, async () => {
    const confined: McpAccess = {
      full: false,
      fullTeamIds: new Set(),
      grantedBoardIds: new Set([`proj-1`]),
      visibleTeamIds: new Set([`ws-1`]),
    }
    const result = await collectTools(USER, null, ALL_MCP_TOOL_GATES, confined)
      .get(`exponential_comments_create`)!(args)
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`confined`)
    expect(caller.comments.create).not.toHaveBeenCalled()
  })

  it.each([
    [`a child session`, { parentId: UUID }],
    [`a spawned root`, { watched: true }],
  ])(`refuses an unattended session (%s)`, async (_label, over) => {
    dbRows.current = [ownSession(over)]
    const result = await collectTools(USER, SESSION).get(
      `exponential_comments_create`
    )!(args)
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`unattended`)
    expect(caller.comments.create).not.toHaveBeenCalled()
  })

  it(`keeps it for a person's own session, and a team comment for anyone`, async () => {
    caller.comments.create.mockResolvedValue({
      comment: { id: UUID },
      reporterEmailed: true,
    })
    dbRows.current = [ownSession()]
    const attended = await collectTools(USER, SESSION).get(
      `exponential_comments_create`
    )!(args)
    expect(attended.isError).toBeFalsy()

    dbRows.current = [ownSession({ parentId: UUID })]
    const team = await collectTools(USER, SESSION).get(
      `exponential_comments_create`
    )!({ issueId: UUID, body: `Note.` })
    expect(team.isError).toBeFalsy()
    expect(caller.comments.create).toHaveBeenCalledTimes(2)
  })
})

describe(`MCP Apps bindings (EXP-1212)`, () => {
  function uiBoundTools(sessionId: string | null): string[] {
    const bound: string[] = []
    const fakeServer = {
      registerTool: (name: string, def: { _meta?: { ui?: unknown } }) => {
        if (def._meta?.ui) bound.push(name)
      },
    }
    registerExponentialTools(
      fakeServer as never,
      USER,
      new Request(`https://x.test/api/mcp`),
      FULL_ACCESS,
      sessionId
    )
    return bound.sort()
  }

  it(`binds the views for an external caller`, () => {
    expect(uiBoundTools(null)).toEqual([
      `exponential_devices_list`,
      `exponential_issues_show`,
      `exponential_notifications_list`,
    ])
  })

  it(`binds none inside a session`, () => {
    expect(uiBoundTools(SESSION)).toEqual([])
  })
})

// ── EXP-1291: the v2 agent layer ─────────────────────────────────────────────
// The session tree's core (lib/sessions/**) and the PR model
// (lib/pull-requests.ts) are mocked: these cases lock what the TOOLS add —
// the caller's session from the header, identifier resolution, the OAuth
// grant checks and the result projection.

/** The row loadCallerSession / the gates read for the header session. */
const callerRow = (over: Record<string, unknown> = {}) => ({
  id: SESSION,
  teamId: WS,
  rootId: ROOT,
  parentId: null,
  userId: USER.id,
  hostUserId: null,
  rootAgentId: null,
  watched: null,
  ...over,
})
const sessionRow = (over: Record<string, unknown> = {}) => ({
  id: SESSION,
  teamId: WS,
  rootId: ROOT,
  userId: USER.id,
  shared: false,
  ...over,
})
const teamConfined = (teamIds: string[] = []): McpAccess => ({
  full: false,
  fullTeamIds: new Set(teamIds),
  grantedBoardIds: new Set(),
  visibleTeamIds: new Set(teamIds),
})

describe(`retired tools (EXP-1291)`, () => {
  it(`registers none of the v1 session, thread and notify tools`, () => {
    const names = [...collectToolDefs().keys()]
    for (const name of [
      `exponential_sessions_start`,
      `exponential_sessions_message`,
      `exponential_sessions_messages`,
      `exponential_sessions_ask_parent`,
      `exponential_sessions_guide`,
      `exponential_sessions_show`,
      `exponential_sessions_results`,
      `exponential_sessions_kill`,
      `exponential_threads_get`,
      `exponential_threads_update`,
      `exponential_threads_link`,
      `exponential_threads_unlink`,
      `exponential_notifications_send`,
      `exponential_issues_pr_files`,
      `exponential_repositories_branch_diff`,
      `exponential_actions_create`,
      `exponential_actions_update`,
      `exponential_actions_list`,
      `exponential_actions_get`,
      `exponential_actions_upsert`,
      `exponential_actions_delete`,
    ]) {
      expect(names, name).not.toContain(name)
    }
  })
})

describe(`exponential_sessions_send`, () => {
  beforeEach(() => {
    sessionsCore.sendSession.mockResolvedValue({
      sessionId: CHILD,
      messageId: MSG,
      txid: 9,
    })
  })

  it(`passes the input through, names the caller's session, drops the txid`, async () => {
    dbRows.current = [callerRow()]
    const result = await collectTools(USER, SESSION).get(`exponential_sessions_send`)!({
      parentId: SESSION,
      deviceId: `mac-1`,
      text: `Fix the flaky test`,
    })
    expect(parseOk(result)).toEqual({ sessionId: CHILD, messageId: MSG })
    expect(sessionsCore.sendSession).toHaveBeenCalledWith({
      userId: USER.id,
      input: { parentId: SESSION, deviceId: `mac-1`, text: `Fix the flaky test` },
      callerSessionId: SESSION,
      // MCP = the one caller whose session may speak as itself.
      viaMcp: true,
    })
  })

  it(`resolves issue identifiers before the core sees them`, async () => {
    dbRows.current = [{ id: ISSUE_V4 }]
    const result = await collectTools().get(`exponential_sessions_send`)!({
      teamId: TEAM_V4,
      deviceId: `mac-1`,
      issueIds: [`EXP-12`],
    })
    expect(result.isError).toBeFalsy()
    expect(sessionsCore.sendSession).toHaveBeenCalledWith(
      expect.objectContaining({
        input: { teamId: TEAM_V4, deviceId: `mac-1`, issueIds: [ISSUE_V4] },
        callerSessionId: null,
      })
    )
  })

  it(`applies the sessions.send rules (a message needs text)`, async () => {
    const result = await tool(`exponential_sessions_send`)({ sessionId: SESSION })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`a message needs text, attachments, issues, an answer, an account or a device`)
    expect(sessionsCore.sendSession).not.toHaveBeenCalled()
  })

  it(`validates a new agent's facets strictly`, async () => {
    const result = await tool(`exponential_sessions_send`)({
      teamId: WS,
      deviceId: `mac-1`,
      agent: { name: `Triage`, icon: `not-an-icon`, color: `#ff8800` },
    })
    expect(result.isError).toBe(true)
    expect(sessionsCore.sendSession).not.toHaveBeenCalled()
  })

  it(`holds a confined grant to the target session's team`, async () => {
    sessionsCore.loadSessionForUser.mockResolvedValue(sessionRow({ teamId: WS }))
    const outside = await collectTools(USER, null, ALL_MCP_TOOL_GATES, teamConfined())
      .get(`exponential_sessions_send`)!({ sessionId: SESSION, text: `hi` })
    expect(outside.isError).toBe(true)
    const inside = await collectTools(USER, null, ALL_MCP_TOOL_GATES, teamConfined([WS]))
      .get(`exponential_sessions_send`)!({ sessionId: SESSION, text: `hi` })
    expect(inside.isError).toBeFalsy()
    expect(sessionsCore.sendSession).toHaveBeenCalledTimes(1)
  })

  it(`needs a teamId from a confined caller with no session to default to`, async () => {
    const result = await collectTools(USER, null, ALL_MCP_TOOL_GATES, teamConfined([WS]))
      .get(`exponential_sessions_send`)!({ deviceId: `mac-1`, text: `hi` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`teamId`)
  })
})

describe(`exponential_sessions_get / list`, () => {
  it(`reads the caller's own root by default`, async () => {
    dbRows.current = [callerRow()]
    sessionsCore.getSessionTree.mockResolvedValue({ root: { id: ROOT } })
    const result = await collectTools(USER, SESSION).get(`exponential_sessions_get`)!({})
    expect(parseOk(result)).toEqual({ root: { id: ROOT } })
    expect(sessionsCore.getSessionTree).toHaveBeenCalledWith(USER.id, ROOT, {
      messages: 20,
    })
  })

  it(`reads the tree of a named session (the core resolves its root)`, async () => {
    sessionsCore.getSessionTree.mockResolvedValue({ root: { id: ROOT } })
    await tool(`exponential_sessions_get`)({ id: CHILD, messages: 5 })
    expect(sessionsCore.getSessionTree).toHaveBeenCalledWith(USER.id, CHILD, {
      messages: 5,
    })
  })

  it(`holds a confined grant to the named session's team`, async () => {
    sessionsCore.loadSessionForUser.mockResolvedValue(sessionRow({ id: CHILD, teamId: WS }))
    const result = await collectTools(USER, null, ALL_MCP_TOOL_GATES, teamConfined())
      .get(`exponential_sessions_get`)!({ id: CHILD })
    expect(result.isError).toBe(true)
    expect(sessionsCore.getSessionTree).not.toHaveBeenCalled()
  })

  it(`needs an id without a session header`, async () => {
    const result = await tool(`exponential_sessions_get`)({})
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`X-Exp-Session-Id`)
  })

  it(`lists threads by default and one agent's conversations on request`, async () => {
    sessionsCore.listSessions.mockResolvedValue([{ id: ROOT }])
    await tool(`exponential_sessions_list`)({})
    expect(sessionsCore.listSessions).toHaveBeenCalledWith(USER.id, {
      limit: 50,
      offset: 0,
    })
    await tool(`exponential_sessions_list`)({ agentId: UUID, teamId: WS, live: true })
    expect(sessionsCore.listSessions).toHaveBeenLastCalledWith(USER.id, {
      agentId: UUID,
      teamId: WS,
      live: true,
      limit: 50,
      offset: 0,
    })
  })

  it(`confines a scoped grant's list to a granted team`, async () => {
    const confined = collectTools(USER, null, ALL_MCP_TOOL_GATES, teamConfined([WS]))
    expect((await confined.get(`exponential_sessions_list`)!({})).isError).toBe(true)
    expect(
      (await confined.get(`exponential_sessions_list`)!({ teamId: WS })).isError
    ).toBeFalsy()
  })
})

describe(`exponential_sessions_send stop / sessions_end`, () => {
  it(`NEW-4: stop rides sessions_send and interrupts the named session's turn`, async () => {
    sessionsCore.stopSession.mockResolvedValue({ txid: 1, delivered: true })
    const result = await tool(`exponential_sessions_send`)({ sessionId: CHILD, stop: true })
    expect(parseOk(result)).toEqual({ ok: true, id: CHILD, stopped: true })
    expect(sessionsCore.stopSession).toHaveBeenCalledWith({
      userId: USER.id,
      sessionId: CHILD,
    })
  })

  it(`Chats: a stop the device never got says so`, async () => {
    sessionsCore.stopSession.mockResolvedValue({ txid: 1, delivered: false })
    const result = await tool(`exponential_sessions_send`)({ sessionId: CHILD, stop: true })
    expect(parseOk(result)).toMatchObject({ ok: true, id: CHILD, stopped: false, delivered: false })
  })

  it(`ends the caller's own session with its summary by default`, async () => {
    dbRows.current = [callerRow()]
    sessionsCore.endSession.mockResolvedValue({ txid: 1, endedIds: [SESSION] })
    const result = await collectTools(USER, SESSION).get(`exponential_sessions_end`)!({
      summary: `Shipped the fix.`,
    })
    expect(parseOk(result)).toEqual({ ok: true, id: SESSION })
    expect(sessionsCore.endSession).toHaveBeenCalledWith({
      userId: USER.id,
      sessionId: SESSION,
      summary: `Shipped the fix.`,
    })
  })

  it(`ignores a header naming someone else's session`, async () => {
    dbRows.current = [callerRow({ userId: `other`, hostUserId: null })]
    const result = await collectTools(USER, SESSION).get(`exponential_sessions_end`)!({})
    expect(result.isError).toBe(true)
    expect(sessionsCore.endSession).not.toHaveBeenCalled()
  })

  it(`ends a named session for a person's MCP client`, async () => {
    sessionsCore.endSession.mockResolvedValue({ txid: 1, endedIds: [CHILD] })
    await tool(`exponential_sessions_end`)({ id: CHILD })
    expect(sessionsCore.endSession).toHaveBeenCalledWith({
      userId: USER.id,
      sessionId: CHILD,
      summary: null,
    })
  })
})

describe(`exponential_sessions_link / unlink`, () => {
  it(`links resolved issues to the caller's root as an agent link`, async () => {
    dbRows.current = [callerRow({ teamId: `ws-1` })]
    sessionsCore.linkIssues.mockResolvedValue({ linked: [UUID] })
    const result = await collectTools(USER, SESSION).get(`exponential_sessions_link`)!({
      issueIds: [UUID],
    })
    // NEW-1: the caller's OWN session holds the rows.
    expect(parseOk(result)).toEqual({ sessionId: SESSION, issueIds: [UUID], linked: [UUID] })
    expect(sessionsCore.linkIssues).toHaveBeenCalledWith(db, {
      sessionId: SESSION,
      issueIds: [UUID],
      source: `agent`,
      addedBy: USER.id,
      addedBySessionId: SESSION,
    })
    expect(noteAgentIssueActivity).toHaveBeenCalledWith(UUID, USER.id)
  })

  it(`writes on the named session after the steer check`, async () => {
    sessionsCore.loadSessionForUser.mockResolvedValue(
      sessionRow({ id: CHILD, rootId: ROOT, teamId: `ws-1` })
    )
    sessionsCore.linkIssues.mockResolvedValue({ linked: [UUID] })
    await tool(`exponential_sessions_link`)({ issueIds: [UUID], sessionId: CHILD })
    expect(sessionsCore.assertCanSteer).toHaveBeenCalled()
    expect(sessionsCore.linkIssues).toHaveBeenCalledWith(
      db,
      expect.objectContaining({ sessionId: CHILD, source: `user`, addedBySessionId: null })
    )
  })

  it(`refuses an issue of another team`, async () => {
    sessionsCore.loadSessionForUser.mockResolvedValue(
      sessionRow({ id: CHILD, rootId: ROOT, teamId: WS })
    )
    // getIssueTeamContext answers ws-1 by default.
    const result = await tool(`exponential_sessions_link`)({
      issueIds: [UUID],
      sessionId: CHILD,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not in this session's team`)
    expect(sessionsCore.linkIssues).not.toHaveBeenCalled()
  })

  it(`unlinks from the caller's own session`, async () => {
    dbRows.current = [callerRow({ teamId: `ws-1` })]
    sessionsCore.unlinkIssue.mockResolvedValue({ unlinked: true })
    const result = await collectTools(USER, SESSION).get(`exponential_sessions_unlink`)!({
      issueId: UUID,
    })
    expect(parseOk(result)).toEqual({ sessionId: SESSION, issueId: UUID, unlinked: true })
    expect(sessionsCore.unlinkIssue).toHaveBeenCalledWith(db, {
      sessionId: SESSION,
      issueId: UUID,
      removedBy: USER.id,
      removedBySessionId: SESSION,
    })
  })

  it(`needs a sessionId without a session header`, async () => {
    const result = await tool(`exponential_sessions_link`)({ issueIds: [UUID] })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`sessionId`)
  })
})

describe(`exponential_issues_create sessionId (EXP-1291)`, () => {
  beforeEach(() => {
    caller.issues.create.mockResolvedValue({ issue: { id: UUID, identifier: `EXP-9` } })
    sessionsCore.linkIssues.mockResolvedValue({ linked: [UUID] })
  })

  it(`files into the caller's own thread by default, as a filed agent link`, async () => {
    dbRows.current = [callerRow({ teamId: `ws-1` })]
    const result = await collectTools(USER, SESSION).get(`exponential_issues_create`)!({
      boardId: PROJ,
      title: `Follow-up`,
    })
    // NEW-1: the filer holds the issue.
    expect(parseOk(result)).toEqual({ id: UUID, identifier: `EXP-9`, sessionId: SESSION })
    expect(sessionsCore.linkIssues).toHaveBeenCalledWith(db, {
      sessionId: SESSION,
      issueIds: [UUID],
      source: `agent`,
      addedBy: USER.id,
      addedBySessionId: SESSION,
      filed: true,
    })
  })

  it(`files into the named session after the steer check`, async () => {
    sessionsCore.loadSessionForUser.mockResolvedValue(
      sessionRow({ id: CHILD, rootId: ROOT, teamId: `ws-1` })
    )
    await tool(`exponential_issues_create`)({
      boardId: PROJ,
      title: `Follow-up`,
      sessionId: CHILD,
    })
    expect(sessionsCore.assertCanSteer).toHaveBeenCalled()
    expect(sessionsCore.linkIssues).toHaveBeenCalledWith(
      db,
      expect.objectContaining({ sessionId: CHILD, filed: true, addedBySessionId: null })
    )
  })

  it(`refuses a named session of another team before filing anything`, async () => {
    sessionsCore.loadSessionForUser.mockResolvedValue(sessionRow({ teamId: WS }))
    const result = await tool(`exponential_issues_create`)({
      boardId: PROJ,
      title: `Follow-up`,
      sessionId: CHILD,
    })
    expect(result.isError).toBe(true)
    expect(caller.issues.create).not.toHaveBeenCalled()
  })

  it(`never files into a team agent's channel or another team's thread`, async () => {
    for (const over of [{ teamId: `ws-1`, rootAgentId: UUID }, { teamId: WS }]) {
      dbRows.current = [callerRow(over)]
      const result = await collectTools(USER, SESSION).get(`exponential_issues_create`)!({
        boardId: PROJ,
        title: `Follow-up`,
      })
      expect(parseOk(result)).toEqual({ id: UUID, identifier: `EXP-9` })
    }
    expect(sessionsCore.linkIssues).not.toHaveBeenCalled()
  })

  it(`reports a failed link as a warning on the created issue`, async () => {
    dbRows.current = [callerRow({ teamId: `ws-1` })]
    sessionsCore.linkIssues.mockRejectedValue(new Error(`board trashed`))
    const result = await collectTools(USER, SESSION).get(`exponential_issues_create`)!({
      boardId: PROJ,
      title: `Follow-up`,
    })
    const payload = parseOk(result) as Record<string, unknown>
    expect(payload.id).toBe(UUID)
    expect(payload.warning).toContain(`exponential_sessions_link`)
  })
})

describe(`exponential_guide_write`, () => {
  it(`writes the caller's session's Guide and hands back the upload step`, async () => {
    dbRows.current = [callerRow()]
    sessionsCore.writeGuide.mockResolvedValue({
      topic: `Summary`,
      uploadUrl: `https://x.test/u`,
      curl: `curl -T shot.png https://x.test/u`,
      results: [{ topic: `Summary`, text: 120 }],
    })
    const result = await collectTools(USER, SESSION).get(`exponential_guide_write`)!({
      topic: `Summary`,
      text: `Fixed it.`,
      files: [`src/a.ts`],
      label: `Before`,
    })
    expect(parseOk(result)).toMatchObject({
      topic: `Summary`,
      curl: `curl -T shot.png https://x.test/u`,
      next: GUIDE_PICTURE_NEXT_STEP,
    })
    expect(sessionsCore.writeGuide).toHaveBeenCalledWith({
      userId: USER.id,
      sessionId: SESSION,
      topic: `Summary`,
      text: `Fixed it.`,
      files: [`src/a.ts`],
      prUrl: null,
      remove: false,
      label: `Before`,
    })
  })

  it(`registers only inside a session`, () => {
    const headerless = collectToolDefs({ inSession: false, unattended: false, agent: false })
    expect(headerless.has(`exponential_guide_write`)).toBe(false)
    expect(headerless.has(`exponential_sessions_compact`)).toBe(false)
    expect(collectToolDefs().has(`exponential_guide_write`)).toBe(true)
  })

  it(`refuses a header that is not the caller's own session`, async () => {
    dbRows.current = []
    const result = await collectTools(USER, SESSION).get(`exponential_guide_write`)!({
      topic: `Summary`,
      text: `x`,
    })
    expect(result.isError).toBe(true)
    expect(sessionsCore.writeGuide).not.toHaveBeenCalled()
  })
})

describe(`pull request tools (EXP-1291)`, () => {
  const PR = {
    id: PR_ID,
    teamId: `ws-1`,
    number: 7,
    url: `https://github.com/acme/app/pull/7`,
  }
  beforeEach(() => {
    prs.resolvePullRequest.mockResolvedValue(PR)
  })

  it(`pr_open resolves identifiers and names the caller's session`, async () => {
    dbRows.current = [callerRow()]
    prs.openPullRequest.mockResolvedValue({
      pullRequestId: PR_ID,
      url: PR.url,
      number: 7,
      base: `master`,
      head: `exp/EXP-12`,
    })
    const tools = collectTools(USER, SESSION)
    dbRows.current = [callerRow()]
    const result = await tools.get(`exponential_pr_open`)!({
      title: `Fix it`,
      issueIds: [UUID],
    })
    expect(parseOk(result)).toMatchObject({ pullRequestId: PR_ID, number: 7 })
    expect(prs.openPullRequest).toHaveBeenCalledWith({
      userId: USER.id,
      callerSessionId: SESSION,
      title: `Fix it`,
      issueIds: [UUID],
    })
    expect(noteAgentIssueActivity).toHaveBeenCalledWith(UUID, USER.id)
  })

  it(`pr_open holds a confined grant to the repository's team`, async () => {
    vi.mocked(loadRepositoryForTeam).mockResolvedValue({ teamId: WS } as never)
    prs.openPullRequest.mockResolvedValue({ pullRequestId: PR_ID })
    const outside = await collectTools(USER, null, ALL_MCP_TOOL_GATES, teamConfined())
      .get(`exponential_pr_open`)!({ title: `Chore`, repositoryId: REPO, head: `chore` })
    expect(outside.isError).toBe(true)
    expect(prs.openPullRequest).not.toHaveBeenCalled()
    const inside = await collectTools(USER, null, ALL_MCP_TOOL_GATES, teamConfined([WS]))
      .get(`exponential_pr_open`)!({ title: `Chore`, repositoryId: REPO, head: `chore` })
    expect(inside.isError).toBeFalsy()
  })

  it(`EXP-1280: pr_open refuses while the caller session plans`, async () => {
    dbRows.current = [callerRow({ planMode: true })]
    const result = await collectTools(USER, SESSION).get(`exponential_pr_open`)!({ title: `Fix it` })
    expect(result.isError).toBe(true)
    expect(result.content[0]!.text).toBe(PLAN_CARD_COPY.planModeGate)
    expect(prs.openPullRequest).not.toHaveBeenCalled()

    dbRows.current = [callerRow({ planMode: false })]
    prs.openPullRequest.mockResolvedValue({ pullRequestId: PR_ID })
    const approved = await collectTools(USER, SESSION).get(`exponential_pr_open`)!({ title: `Fix it` })
    expect(approved.isError).toBeFalsy()
  })

  it(`pr_merge defaults to the caller session's PR and passes stack`, async () => {
    dbRows.current = [callerRow()]
    prs.mergePullRequest.mockResolvedValue({ merged: true, mergedIds: [PR_ID] })
    const result = await collectTools(USER, SESSION).get(`exponential_pr_merge`)!({
      stack: true,
    })
    expect(parseOk(result)).toEqual({
      pullRequestId: PR_ID,
      number: 7,
      url: PR.url,
      merged: true,
      mergedIds: [PR_ID],
    })
    expect(prs.resolvePullRequest).toHaveBeenCalledWith({
      userId: USER.id,
      callerSessionId: SESSION,
    })
    expect(prs.mergePullRequest).toHaveBeenCalledWith({
      userId: USER.id,
      pullRequestId: PR_ID,
      stack: true,
      callerSessionId: SESSION,
    })
  })

  it(`pr_close closes the named PR without merging`, async () => {
    prs.closePullRequestTx.mockResolvedValue({ txid: 1 })
    const result = await tool(`exponential_pr_close`)({ pullRequestId: PR_ID })
    expect(parseOk(result)).toEqual({ pullRequestId: PR_ID, number: 7, url: PR.url, closed: true })
    expect(prs.closePullRequestTx).toHaveBeenCalledWith({ userId: USER.id, pullRequestId: PR_ID })
    expect(prs.mergePullRequest).not.toHaveBeenCalled()
  })

  it(`pr_close holds a confined grant to the PR's team`, async () => {
    const result = await collectTools(USER, null, ALL_MCP_TOOL_GATES, teamConfined([WS]))
      .get(`exponential_pr_close`)!({ pullRequestId: PR_ID })
    expect(result.isError).toBe(true)
    expect(prs.closePullRequestTx).not.toHaveBeenCalled()
  })

  it(`names a PR by issue identifier or by repository + number`, async () => {
    dbRows.current = [{ id: UUID }]
    prs.retargetPullRequest.mockResolvedValue({ base: `master` })
    await tool(`exponential_pr_retarget`)({ issueId: `EXP-12` })
    expect(prs.resolvePullRequest).toHaveBeenLastCalledWith({
      userId: USER.id,
      issueId: UUID,
      callerSessionId: null,
    })
    expect(prs.retargetPullRequest).toHaveBeenCalledWith({
      userId: USER.id,
      pullRequestId: PR_ID,
    })
    prs.pullRequestFiles.mockResolvedValue({ files: [] })
    const files = await tool(`exponential_pr_files`)({ repositoryId: REPO, prNumber: 7 })
    expect(parseOk(files)).toEqual({ pullRequestId: PR_ID, number: 7, url: PR.url, files: [] })
    expect(prs.resolvePullRequest).toHaveBeenLastCalledWith({
      userId: USER.id,
      repositoryId: REPO,
      number: 7,
      callerSessionId: null,
    })
  })

  it(`refuses prNumber without repositoryId and an empty update`, async () => {
    expect((await tool(`exponential_pr_files`)({ prNumber: 7 })).isError).toBe(true)
    expect((await tool(`exponential_pr_update`)({ pullRequestId: PR_ID })).isError).toBe(true)
    expect(prs.resolvePullRequest).not.toHaveBeenCalled()
  })

  it(`pr_update passes only the given fields`, async () => {
    prs.updatePullRequest.mockResolvedValue(undefined)
    const result = await tool(`exponential_pr_update`)({ pullRequestId: PR_ID, draft: false })
    expect(parseOk(result)).toEqual({ pullRequestId: PR_ID, number: 7, url: PR.url })
    expect(prs.updatePullRequest).toHaveBeenCalledWith({
      userId: USER.id,
      pullRequestId: PR_ID,
      draft: false,
    })
  })

  it(`pr_unlink names the PR by number and the issue by identifier, never as a PR selector`, async () => {
    dbRows.current = [{ id: UUID }]
    prs.unlinkPullRequestIssue.mockResolvedValue({ unlinked: true })
    const result = await tool(`exponential_pr_unlink`)({
      repositoryId: REPO,
      prNumber: 7,
      issueId: `EXP-12`,
    })
    expect(parseOk(result)).toEqual({
      pullRequestId: PR_ID,
      number: 7,
      url: PR.url,
      issueId: UUID,
      unlinked: true,
    })
    expect(prs.resolvePullRequest).toHaveBeenLastCalledWith({
      userId: USER.id,
      repositoryId: REPO,
      number: 7,
      callerSessionId: null,
    })
    expect(prs.unlinkPullRequestIssue).toHaveBeenCalledWith({
      userId: USER.id,
      pullRequestId: PR_ID,
      issueId: UUID,
    })
  })

  it(`holds a confined grant to the PR's team`, async () => {
    const result = await collectTools(USER, null, ALL_MCP_TOOL_GATES, teamConfined([WS]))
      .get(`exponential_pr_merge`)!({ pullRequestId: PR_ID })
    expect(result.isError).toBe(true)
    expect(prs.mergePullRequest).not.toHaveBeenCalled()
  })
})

describe(`exponential_attachments_get — a message's file (EXP-1291)`, () => {
  it(`falls back to a conversation attachment the caller's session may see`, async () => {
    vi.stubEnv(`BETTER_AUTH_SECRET`, `mcp-tools-test-secret`)
    membership.getAttachmentTeamContext.mockRejectedValue(
      new TRPCError({ code: `NOT_FOUND`, message: `Attachment not found` })
    )
    dbRows.current = [
      {
        teamId: `ws-1`,
        sessionId: SESSION,
        storageKey: `k`,
        contentType: `application/pdf`,
        filename: `spec.pdf`,
        sizeBytes: 10,
        durationMs: null,
      },
    ]
    sessionsCore.loadSessionForUser.mockResolvedValue(sessionRow())
    const result = await tool(`exponential_attachments_get`)({ id: UUID })
    expect(parseOk(result)).toMatchObject({ id: UUID, filename: `spec.pdf` })
    expect(sessionsCore.loadSessionForUser).toHaveBeenCalledWith(USER.id, SESSION)
  })

  it(`stays NOT_FOUND when no message carries it`, async () => {
    membership.getAttachmentTeamContext.mockRejectedValue(
      new TRPCError({ code: `NOT_FOUND`, message: `Attachment not found` })
    )
    dbRows.current = []
    const result = await tool(`exponential_attachments_get`)({ id: UUID })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`Attachment not found`)
  })
})

// EXP-1280 custom plan mode: the plan / question card the session posts on
// itself. The builders (lib/sessions/plan.ts) run for real; postAskCard (the
// DB write + upload minting) is stubbed.
describe(`exponential_plan_propose / exponential_sessions_ask (EXP-1280)`, () => {
  const POSTED = {
    messageId: MSG,
    uploads: [{ label: `grid`, uploadUrl: `https://x.test/u`, curl: `curl -F file=@grid.png "https://x.test/u"` }],
    next: PLAN_CARD_COPY.proposeNext,
  }
  const surface = {
    root: { id: `r`, component: `Stack`, children: [{ id: `t`, component: `Text`, props: { text: `Hi` } }] },
    data: { name: `x` },
  }
  const propose = (args: Record<string, unknown>) =>
    collectTools(USER, SESSION).get(`exponential_plan_propose`)!(args)
  const ask = (args: Record<string, unknown>) =>
    collectTools(USER, SESSION).get(`exponential_sessions_ask`)!(args)

  beforeEach(() => {
    dbRows.current = [callerRow()]
    postAskCard.mockReset()
    postAskCard.mockResolvedValue(POSTED)
  })

  it(`posts the normalised plan card on the caller's own session`, async () => {
    const result = await propose({
      plan: `  1. Tiles\n\n![Grid](upload:grid)  `,
      pictures: [{ label: `grid`, caption: ` The grid ` }],
      questions: [
        { id: `layout`, prompt: `Layout?`, options: [{ label: `Grid`, picture: `grid` }, { label: `List`, description: `Rows` }] },
      ],
      surface,
    })
    expect(parseOk(result)).toEqual(POSTED)
    expect(postAskCard).toHaveBeenCalledWith({
      userId: USER.id,
      sessionId: SESSION,
      card: {
        kind: `plan`,
        plan: `1. Tiles\n\n![Grid](upload:grid)`,
        pictures: [{ label: `grid`, attachmentId: null, width: null, height: null, caption: `The grid` }],
        questions: [
          {
            id: `layout`,
            prompt: `Layout?`,
            multi: false,
            custom: true,
            options: [
              { label: `Grid`, description: null, picture: `grid` },
              { label: `List`, description: `Rows`, picture: null },
            ],
          },
        ],
        surface,
        decision: null,
        fresh: null,
        feedback: null,
        answers: null,
        surfaceData: null,
      },
    })
  })

  it.each([
    [`a picture label twice`, { pictures: [{ label: `a` }, { label: `a` }] }, `declared twice`],
    [`an undeclared embed`, { plan: `![x](upload:nope)` }, `"nope"`],
    [
      `an undeclared option picture`,
      { questions: [{ id: `q`, prompt: `Q?`, options: [{ label: `A`, picture: `nope` }] }] },
      `questions[0].options[0]`,
    ],
    [
      `a question id twice`,
      { questions: [{ id: `q`, prompt: `Q?`, options: [{ label: `A` }] }, { id: `q`, prompt: `R?`, options: [{ label: `B` }] }] },
      `"q" is used twice`,
    ],
    [
      `an option label twice`,
      { questions: [{ id: `q`, prompt: `Q?`, options: [{ label: `A` }, { label: `A` }] }] },
      `listed twice`,
    ],
    [`an unanswerable question`, { questions: [{ id: `q`, prompt: `Q?`, custom: false, options: [] }] }, `cannot be answered`],
    [`a surface outside the core catalog`, { surface: { root: { id: `r`, component: `Nope` } } }, `unknown component Nope`],
    [`a surface with invalid props`, { surface: { root: { id: `r`, component: `Text` } } }, `props.text: required`],
    [`a surface root that is no node`, { surface: { root: { kind: `x` } } }, `Exponential UI node`],
    [`a malformed surface tree`, { surface: { root: { id: `r`, component: `Stack`, children: `x` } } }, `not a valid`],
    [
      `a surface past the byte cap`,
      { surface: { root: { id: `r`, component: `Text`, props: { text: `x`.repeat(70_000) } } } },
      `the cap is 65536`,
    ],
    [
      `a surface past the component cap`,
      {
        surface: {
          root: {
            id: `r`,
            component: `Stack`,
            children: Array.from({ length: 200 }, (_, i) => ({ id: `t${i}`, component: `Text`, props: { text: `x` } })),
          },
        },
      },
      `201 components`,
    ],
  ])(`refuses %s with a message the agent can act on`, async (_name, args, message) => {
    const result = await propose({ plan: `1. x`, ...args })
    expect(result.isError).toBe(true)
    expect(result.content[0]!.text).toContain(message)
    expect(postAskCard).not.toHaveBeenCalled()
  })

  it(`refuses unknown keys, over-cap lists and bad labels at the schema`, () => {
    const schema = collectToolDefs().get(`exponential_plan_propose`)!.inputSchema!
    expect(schema.safeParse({ plan: `1. x` }).success).toBe(true)
    expect(schema.safeParse({ plan: `1. x`, extra: 1 }).success).toBe(false)
    expect(
      schema.safeParse({ plan: `1. x`, pictures: Array.from({ length: 11 }, (_, i) => ({ label: `p${i}` })) }).success
    ).toBe(false)
    expect(schema.safeParse({ plan: `1. x`, pictures: [{ label: `bad label` }] }).success).toBe(false)
    expect(
      schema.safeParse({ plan: `1. x`, questions: [{ id: `q`, prompt: `Q?`, options: [{ label: `A`, extra: 1 }] }] }).success
    ).toBe(false)
    const askSchema = collectToolDefs().get(`exponential_sessions_ask`)!.inputSchema!
    expect(askSchema.safeParse({ question: `Q?`, options: [{ label: `A` }] }).success).toBe(true)
    expect(askSchema.safeParse({ question: `Q?` }).success).toBe(false)
  })

  it(`surfaces a refused post`, async () => {
    postAskCard.mockRejectedValue(
      new TRPCError({ code: `PRECONDITION_FAILED`, message: `This session has ended` })
    )
    const result = await propose({ plan: `1. x` })
    expect(result.isError).toBe(true)
    expect(result.content[0]!.text).toContain(`This session has ended`)
  })

  it(`refuses a header that is not the caller's own session`, async () => {
    dbRows.current = []
    const result = await propose({ plan: `1. x` })
    expect(result.isError).toBe(true)
    expect(postAskCard).not.toHaveBeenCalled()
  })

  it(`posts one question with the sidecars only when an option carries one`, async () => {
    await ask({ question: ` Which? `, options: [{ label: `A` }, { label: `B` }] })
    expect(postAskCard).toHaveBeenLastCalledWith({
      userId: USER.id,
      sessionId: SESSION,
      card: {
        kind: `question`,
        question: `Which?`,
        options: [`A`, `B`],
        pictures: [],
        multi: false,
        custom: true,
        surface: null,
        answer: null,
        answers: null,
        surfaceData: null,
      },
    })
    const result = await ask({
      question: `Which look?`,
      options: [{ label: `Dark`, picture: `dark`, description: `Night` }, { label: `Light` }],
      multi: true,
      custom: false,
      pictures: [{ label: `dark` }],
      surface,
    })
    expect(parseOk(result)).toEqual(POSTED)
    expect(postAskCard).toHaveBeenLastCalledWith({
      userId: USER.id,
      sessionId: SESSION,
      card: {
        kind: `question`,
        question: `Which look?`,
        options: [`Dark`, `Light`],
        optionDescriptions: [`Night`, null],
        optionPictures: [`dark`, null],
        pictures: [{ label: `dark`, attachmentId: null, width: null, height: null, caption: null }],
        multi: true,
        custom: false,
        surface,
        answer: null,
        answers: null,
        surfaceData: null,
      },
    })
  })

  it(`refuses an undeclared option picture on a question`, async () => {
    const result = await ask({ question: `Q?`, options: [{ label: `A`, picture: `nope` }] })
    expect(result.isError).toBe(true)
    expect(result.content[0]!.text).toContain(`options[0]`)
  })

  it(`registers only inside a session`, () => {
    const headerless = collectToolDefs({ inSession: false, unattended: false, agent: false })
    expect(headerless.has(`exponential_plan_propose`)).toBe(false)
    expect(headerless.has(`exponential_sessions_ask`)).toBe(false)
    const defs = collectToolDefs()
    expect(defs.has(`exponential_plan_propose`)).toBe(true)
    expect(defs.has(`exponential_sessions_ask`)).toBe(true)
  })
})

// Chats (fixtures/chats.json `surfaceCard`): `session_self {op: "show"}`
// posts an inline preview in the caller's own chat, validated like a plan
// card's surface (the builder runs for real; the DB write is stubbed).
describe(`exponential_surface_show (Chats)`, () => {
  const surface = {
    root: { id: `r`, component: `Stack`, children: [{ id: `t`, component: `Text`, props: { text: `Hi` } }] },
  }
  const show = (args: Record<string, unknown>) => collectTools(USER, SESSION).get(`exponential_surface_show`)!(args)

  beforeEach(() => {
    dbRows.current = [callerRow()]
    postSurfaceCard.mockReset()
    postSurfaceCard.mockResolvedValue({ messageId: MSG })
  })

  it(`posts a surface card on the caller's own session`, async () => {
    const result = await show({ surface, title: `  Login screen ` })
    expect(parseOk(result)).toEqual({ messageId: MSG })
    expect(postSurfaceCard).toHaveBeenCalledWith({
      sessionId: SESSION,
      card: { kind: `surface`, surface: { root: surface.root, data: null }, title: `Login screen` },
    })
  })

  it.each([
    [`a surface outside the core catalog`, { root: { id: `r`, component: `Nope` } }, `unknown component Nope`],
    [`a surface root that is no node`, { root: { kind: `x` } }, `Exponential UI node`],
    [`a surface past the byte cap`, { root: { id: `r`, component: `Text`, props: { text: `x`.repeat(70_000) } } }, `the cap is 65536`],
  ])(`refuses %s`, async (_name, bad, message) => {
    const result = await show({ surface: bad })
    expect(result.isError).toBe(true)
    expect(result.content[0]!.text).toContain(message)
    expect(postSurfaceCard).not.toHaveBeenCalled()
  })

  it(`needs a surface and refuses unknown keys`, () => {
    const schema = collectToolDefs().get(`exponential_surface_show`)!.inputSchema!
    expect(schema.safeParse({ surface }).success).toBe(true)
    expect(schema.safeParse({ title: `x` }).success).toBe(false)
    expect(schema.safeParse({ surface, extra: 1 }).success).toBe(false)
  })

  it(`only works inside a session`, async () => {
    dbRows.current = []
    const result = await collectTools(USER, null).get(`exponential_surface_show`)!({ surface })
    expect(result.isError).toBe(true)
    expect(result.content[0]!.text).toContain(`No session`)
  })
})

// ── NEW-19: widget metadata (FEED-116) + the reporter's view (FEED-144) ─────

describe(`widget metadata on issues get/list (NEW-19)`, () => {
  const submission = {
    id: `ws-row-1`,
    issueId: UUID,
    reporterName: `Emma Fischer`,
    reporterEmail: `emma@example.com`,
    reporterExternalId: `cust-42`,
    pageUrl: `https://app.example.com/settings`,
    userAgent: `Mozilla/5.0`,
    viewportWidth: 1440,
    viewportHeight: 900,
    screenWidth: 2560,
    screenHeight: 1440,
    devicePixelRatio: 2,
    customData: { plan: `pro` },
    lastReporterSeenAt: null,
    createdAt: new Date(`2026-10-01T10:00:00Z`),
  }

  it(`get carries the whole "Reported via widget" card`, async () => {
    h.reporter.loadWidgetSubmissions.mockResolvedValue([submission])
    const out = parseOk(
      await tool(`exponential_issues_get`)({ id: UUID })
    ) as { widget: unknown }
    expect(h.reporter.loadWidgetSubmissions).toHaveBeenCalledWith([UUID])
    expect(out.widget).toEqual({
      reporter: {
        name: `Emma Fischer`,
        email: `emma@example.com`,
        externalId: `cust-42`,
      },
      customData: { plan: `pro` },
      pageUrl: `https://app.example.com/settings`,
      viewport: { width: 1440, height: 900, devicePixelRatio: 2 },
      screen: { width: 2560, height: 1440 },
      userAgent: `Mozilla/5.0`,
      lastReporterSeenAt: null,
      submittedAt: `2026-10-01T10:00:00.000Z`,
    })
  })

  it(`get omits widget on a member-created issue`, async () => {
    const out = parseOk(
      await tool(`exponential_issues_get`)({ id: UUID })
    ) as Record<string, unknown>
    expect(out).not.toHaveProperty(`widget`)
  })

  it(`list batches ONE submission read and carries the short form`, async () => {
    dbRows.current = [
      { id: UUID, identifier: `FEED-1`, description: null },
      { id: PROJ, identifier: `FEED-2`, description: null },
    ]
    h.reporter.loadWidgetSubmissions.mockResolvedValue([submission])
    const rows = parseOk(
      await tool(`exponential_issues_list`)({
        boardId: PROJ,
        sort: `-createdAt`,
        limit: 50,
        offset: 0,
      })
    ) as Array<Record<string, unknown>>
    expect(h.reporter.loadWidgetSubmissions).toHaveBeenCalledTimes(1)
    expect(h.reporter.loadWidgetSubmissions).toHaveBeenCalledWith([UUID, PROJ])
    expect(rows[0].widget).toEqual({
      reporter: { name: `Emma Fischer`, email: `emma@example.com` },
      customData: { plan: `pro` },
    })
    expect(rows[1]).not.toHaveProperty(`widget`)
  })
})

describe(`the reporter's view over MCP (NEW-19)`, () => {
  const notMember = () =>
    new TRPCError({ code: `FORBIDDEN`, message: `Not a member of this team` })
  const resolved = {
    issue: {
      id: UUID,
      identifier: `FEED-144`,
      teamId: `vendor-team`,
      boardId: `vendor-board`,
      status: `backlog`,
    },
    submission: { id: `ws-row-1`, reporterName: `User One` },
    teamName: `Exponential`,
    statusCategory: `backlog`,
  }

  it(`a non-member non-reporter keeps "Issue not found" (identifier)`, async () => {
    membership.getUserTeamIds.mockResolvedValue([`ws-1`])
    dbRows.current = []
    const result = await tool(`exponential_issues_get`)({ id: `FEED-144` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toBe(`Issue not found: FEED-144`)
    expect(h.reporter.findIssueForReporter).toHaveBeenCalledWith(
      USER,
      `FEED-144`
    )
    expect(h.reporter.reporterIssueView).not.toHaveBeenCalled()
  })

  it(`a non-member non-reporter keeps "Not a member of this team" (UUID)`, async () => {
    membership.resolveTeamAccess.mockRejectedValue(notMember())
    for (const [name, args] of [
      [`exponential_issues_get`, { id: UUID }],
      [`exponential_comments_list`, { issueId: UUID, limit: 100, offset: 0 }],
      [`exponential_comments_create`, { issueId: UUID, body: `hi` }],
    ] as const) {
      const result = await tool(name)(args)
      expect(result.isError).toBe(true)
      expect(result.content[0].text).toBe(`Not a member of this team`)
    }
    expect(caller.comments.create).not.toHaveBeenCalled()
    expect(h.reporter.postReporterReply).not.toHaveBeenCalled()
  })

  it(`the reporter gets the reporter view from issues get`, async () => {
    dbRows.current = []
    h.reporter.findIssueForReporter.mockResolvedValue(resolved)
    h.reporter.reporterIssueView.mockResolvedValue({
      identifier: `FEED-144`,
      access: `reporter`,
    })
    const out = parseOk(
      await tool(`exponential_issues_get`)({ id: `FEED-144`, commentsLimit: 5 })
    )
    expect(out).toEqual({ identifier: `FEED-144`, access: `reporter` })
    expect(h.reporter.reporterIssueView).toHaveBeenCalledWith(resolved, 5)
    expect(h.reporter.loadWidgetSubmissions).not.toHaveBeenCalled()
  })

  it(`the reporter lists only the reporter conversation`, async () => {
    membership.resolveTeamAccess.mockRejectedValue(notMember())
    h.reporter.findIssueForReporter.mockResolvedValue(resolved)
    h.reporter.listReporterComments.mockResolvedValue([
      { id: `c1`, author: `Support Person`, fromReporter: false, body: `On it` },
    ])
    const out = parseOk(
      await tool(`exponential_comments_list`)({
        issueId: UUID,
        limit: 20,
        offset: 0,
      })
    )
    expect(out).toEqual([
      { id: `c1`, author: `Support Person`, fromReporter: false, body: `On it` },
    ])
    expect(h.reporter.listReporterComments).toHaveBeenCalledWith(resolved, {
      limit: 20,
      offset: 0,
    })
  })

  it(`the reporter's comment lands through the shared reporter reply`, async () => {
    membership.resolveTeamAccess.mockRejectedValue(notMember())
    h.reporter.findIssueForReporter.mockResolvedValue(resolved)
    h.reporter.postReporterReply.mockResolvedValue({
      ok: true,
      message: { id: `c9`, createdAt: new Date(`2026-10-26T12:00:00Z`) },
    })
    const out = parseOk(
      await tool(`exponential_comments_create`)({
        issueId: `FEED-144`,
        body: `  Still broken on 0.14.79  `,
      })
    )
    expect(h.reporter.postReporterReply).toHaveBeenCalledWith(
      resolved,
      `Still broken on 0.14.79`
    )
    expect(caller.comments.create).not.toHaveBeenCalled()
    expect(out).toEqual({
      id: `c9`,
      issueId: UUID,
      issueIdentifier: `FEED-144`,
      source: `reporter`,
      audience: `reporter`,
      createdAt: `2026-10-26T12:00:00.000Z`,
    })
  })

  it(`refuses threads, attachments and team audience for the reporter`, async () => {
    membership.resolveTeamAccess.mockRejectedValue(notMember())
    h.reporter.findIssueForReporter.mockResolvedValue(resolved)
    for (const extra of [
      { parentId: UUID },
      { attachmentIds: [UUID] },
      { audience: `team` },
    ]) {
      const result = await tool(`exponential_comments_create`)({
        issueId: UUID,
        body: `x`,
        ...extra,
      })
      expect(result.isError).toBe(true)
    }
    expect(h.reporter.postReporterReply).not.toHaveBeenCalled()
  })

  it(`answers the per-issue reply bucket`, async () => {
    membership.resolveTeamAccess.mockRejectedValue(notMember())
    h.reporter.findIssueForReporter.mockResolvedValue(resolved)
    h.reporter.postReporterReply.mockResolvedValue({
      ok: false,
      retryAfterSeconds: 120,
    })
    const result = await tool(`exponential_comments_create`)({
      issueId: UUID,
      body: `again`,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`120s`)
  })

  it(`a member never reaches the reporter fallback`, async () => {
    caller.comments.create.mockResolvedValue({
      comment: { id: UUID },
      reporterEmailed: null,
    })
    await tool(`exponential_comments_create`)({ issueId: UUID, body: `team` })
    expect(caller.comments.create).toHaveBeenCalled()
    expect(h.reporter.findIssueForReporter).not.toHaveBeenCalled()
  })

  it(`other ops stay member-only`, async () => {
    membership.resolveTeamAccess.mockRejectedValue(notMember())
    h.reporter.findIssueForReporter.mockResolvedValue(resolved)
    const result = await tool(`exponential_issues_activity`)({
      issueId: UUID,
      limit: 10,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toBe(`Not a member of this team`)
  })
})
