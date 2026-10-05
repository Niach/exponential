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
    repositories: {
      list: vi.fn(),
      add: vi.fn(),
      branchDiff: vi.fn(),
      forIssue: vi.fn(),
      // EXP-626: the issue-less merge path.
      mergePull: vi.fn(),
      // EXP-1139: the issue-less description rewrite.
      updatePull: vi.fn(),
    },
    actions: {
      list: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
    },
    issues: {
      prFiles: vi.fn(),
      retargetPr: vi.fn(),
      // EXP-684: statusId passthrough on create.
      create: vi.fn(),
      update: vi.fn(),
      // EXP-639: the issue path of exponential_pr_merge.
      mergePr: vi.fn(),
      // EXP-1139: the issue path of exponential_pr_update.
      updatePr: vi.fn(),
    },
    boards: { delete: vi.fn(), setRepository: vi.fn() },
    teams: { create: vi.fn(), update: vi.fn() },
    teamInvites: { create: vi.fn(), list: vi.fn(), revoke: vi.fn() },
    attachments: { delete: vi.fn() },
    // EXP-660: the deferred families.
    statuses: { create: vi.fn(), update: vi.fn(), delete: vi.fn() },
    steer: { killSession: vi.fn(), startSession: vi.fn() },
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
    getSessionAttachmentTeamContext: vi.fn(),
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

  return {
    caller,
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
  dbRows,
  executeRows,
  state,
  insertValues,
  updateSet,
  deleteReturning,
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

// pr_open-only deps — mocked so the module import stays side-effect free.
vi.mock(`@/lib/integrations/github-pr`, async (importOriginal) => ({
  PullAlreadyExistsError: (
    await importOriginal<typeof import("@/lib/integrations/github-pr")>()
  ).PullAlreadyExistsError,
  createPullRequest: vi.fn(),
  findOpenPullByHead: vi.fn(),
  branchExists: vi.fn(async () => false),
}))
// FEED-66: the retired-identifier read behind pr_open's head inference; the
// resolver itself stays real (it runs on the db stub like everything else).
vi.mock(`@/lib/issue-resolver`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  retiredIdentifiers: vi.fn(async () => []),
}))
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
vi.mock(`@/lib/issue-relations`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  insertRelationInTx: vi.fn(),
}))
vi.mock(`@/lib/integrations/pr-actor-claims`, () => ({
  claimPrOpen: vi.fn(),
  releasePrOpenClaim: vi.fn(),
  // EXP-617: the write tools record "this user's agent touched this issue" so
  // a later PR fan-out can keep them out of it.
  noteAgentIssueActivity: vi.fn(),
}))
vi.mock(`@/lib/integrations/activity`, () => ({ recordIssueEvent: vi.fn() }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetPrNotify: vi.fn(),
  // EXP-801: the synchronous agent-message fan-out.
  sendAgentMessage: vi.fn(),
}))
vi.mock(`@/lib/widget/agent-report`, () => ({
  createAgentBugReport: h.createAgentBugReport,
}))
// EXP-626/EXP-637: the issue-less PR path and the agent close-out.
vi.mock(`@/lib/trpc/repositories`, () => ({
  loadRepositoryForTeam: vi.fn(),
  // EXP-1139: the header-only chore path of exponential_pr_update.
  loadRepositoryByFullName: vi.fn(),
}))
vi.mock(`@/lib/coding-session-end`, () => ({ endSessionByAgent: vi.fn() }))
// EXP-1146: the tree merge is exercised by its own test; here it is a fake
// whose result pr_open's yolo path reports.
vi.mock(`@/lib/yolo-tree-merge`, () => ({ maybeMergeYoloTree: vi.fn() }))
// EXP-700: the relay injection rail. Partial mocks — the formatters and the
// one-select lookup stay real (the lookup runs against the drizzle stub), so
// ask_parent/message tests exercise the actual message convention.
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  getSteerRelayConfig: vi.fn(),
  relayPostInput: vi.fn(),
  relayPostCompact: vi.fn(),
}))
vi.mock(`@/lib/steer-child-messages`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  notifyParentOfChildEnd: vi.fn(),
}))
// SLOP-3: the stack walk behind pr_merge({mergeStack}); none by default.
vi.mock(`@/lib/pr-merge-guard`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  openStackThrough: vi.fn(async () => []),
}))
// EXP-1154: the PR body follows the run's report; all of its I/O lives in
// one module. Default = no report (pr_open keeps the agent's body).
vi.mock(`@/lib/run-pr-body`, () => ({
  runPrBody: vi.fn(async (_id: string | null, fallback: string | undefined) => ({
    body: fallback,
    fromResults: false,
  })),
  runHasReportBody: vi.fn(async () => false),
  syncRunPrBody: vi.fn(async () => `skipped`),
}))

import {
  loadRepositoryByFullName,
  loadRepositoryForTeam,
} from "@/lib/trpc/repositories"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { applyPrLifecycleStatusInTx } from "@/lib/integrations/pr-sync"
import {
  fireAndForgetPrNotify,
  sendAgentMessage,
} from "@/lib/integrations/notifications"
import { noteAgentIssueActivity } from "@/lib/integrations/pr-actor-claims"
import { endSessionByAgent } from "@/lib/coding-session-end"
import {
  getSteerRelayConfig,
  relayPostCompact,
  relayPostInput,
} from "@/lib/steer"
import { notifyParentOfChildEnd } from "@/lib/steer-child-messages"
import { maybeMergeYoloTree } from "@/lib/yolo-tree-merge"
import { openStackThrough } from "@/lib/pr-merge-guard"
import { runHasReportBody, runPrBody, syncRunPrBody } from "@/lib/run-pr-body"
import { retiredIdentifiers } from "@/lib/issue-resolver"
import {
  branchExists,
  createPullRequest,
  findOpenPullByHead,
  PullAlreadyExistsError,
} from "@/lib/integrations/github-pr"
import { insertRelationInTx } from "@/lib/issue-relations"
import { resolveRepoInstallationTokenInfo } from "@/lib/integrations/github-app"
import { registerExponentialTools } from "@/lib/mcp/tools"
import { verifySessionResultToken } from "@/lib/storage/session-result-token"
import {
  builtinCreateAction,
  builtinFixConflictsAction,
  builtinTidyUpAction,
} from "@/lib/builtin-actions"
import { FULL_ACCESS, type McpAccess } from "@/lib/mcp/scope"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "@/lib/mcp/gates"
import type { McpUser } from "@/lib/mcp/server"
import { contract } from "@exp/domain-contract"
import { mintStartId, recordStartFailure } from "@/lib/start-failures"

// ── Harness ──────────────────────────────────────────────────────────────────

type ToolResult = {
  isError?: boolean
  content: Array<{ type: string; text?: string; data?: string }>
}
type ToolHandler = (args: Record<string, unknown>) => Promise<ToolResult>

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
): Map<string, { inputSchema?: z.ZodType }> {
  const defs = new Map<string, { inputSchema?: z.ZodType }>()
  const fakeServer = {
    registerTool: (name: string, def: { inputSchema?: z.ZodType }) => {
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
const AUTO = `99999999-9999-9999-9999-999999999999`
const RUN = `66666666-6666-6666-6666-666666666666`

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
  // EXP-700: relay off by default; individual tests arm it.
  vi.mocked(getSteerRelayConfig).mockReturnValue(null)
  // FEED-59: pr_open looks for an open PR on its head first; none by default.
  vi.mocked(findOpenPullByHead).mockReset().mockResolvedValue(null)
  vi.mocked(relayPostInput).mockResolvedValue({ delivered: false })
  vi.mocked(notifyParentOfChildEnd).mockResolvedValue({ delivered: false })
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
    tool: `exponential_repositories_branch_diff`,
    pick: () => caller.repositories.branchDiff,
    args: { issueId: UUID },
    resolved: { files: [], prNumber: null },
    expected: { files: [], prNumber: null },
    calledWith: { issueId: UUID },
  },
  {
    tool: `exponential_actions_list`,
    pick: () => caller.actions.list,
    args: { teamId: WS },
    resolved: { actions: [{ id: UUID, name: `Code review` }] },
    // EXP-539: actions.list carries DB rows only; the MCP tool appends the
    // virtual builtins itself so agents still see them.
    expected: [
      { id: UUID, name: `Code review` },
      JSON.parse(JSON.stringify(builtinCreateAction(WS))),
      JSON.parse(JSON.stringify(builtinFixConflictsAction(WS))),
      JSON.parse(JSON.stringify(builtinTidyUpAction(WS))),
    ],
    calledWith: { teamId: WS },
  },
  {
    tool: `exponential_actions_create`,
    pick: () => caller.actions.create,
    args: { teamId: WS, name: `Code review`, body: `# Review the repo` },
    resolved: {
      action: { id: UUID, name: `Code review`, body: `# Review the repo` },
    },
    expected: { id: UUID, name: `Code review`, body: `# Review the repo` },
    calledWith: { teamId: WS, name: `Code review`, body: `# Review the repo` },
  },
  {
    tool: `exponential_actions_update`,
    pick: () => caller.actions.update,
    args: { id: UUID, name: `Nightly review` },
    resolved: { action: { id: UUID, name: `Nightly review` } },
    expected: { id: UUID, name: `Nightly review` },
    calledWith: { id: UUID, name: `Nightly review` },
  },
  {
    tool: `exponential_actions_delete`,
    pick: () => caller.actions.delete,
    args: { id: UUID },
    resolved: { ok: true },
    expected: { ok: true, id: UUID },
    calledWith: { id: UUID },
  },
  {
    tool: `exponential_issues_pr_files`,
    pick: () => caller.issues.prFiles,
    args: { issueId: UUID },
    resolved: { repo: `a/b`, prNumber: 7, files: [] },
    expected: { repo: `a/b`, prNumber: 7, files: [] },
    calledWith: { issueId: UUID },
  },
  {
    tool: `exponential_pr_retarget`,
    pick: () => caller.issues.retargetPr,
    args: { issueId: UUID, base: `master` },
    resolved: { retargeted: true, base: `master` },
    expected: { ok: true, base: `master` },
    calledWith: { issueId: UUID, base: `master` },
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
  // ── EXP-660: sessions / devices ──
  {
    tool: `exponential_sessions_kill`,
    pick: () => caller.steer.killSession,
    args: { id: RUN },
    // The router hands back the FULL row; the tool must project the
    // server-only columns away.
    resolved: {
      session: {
        id: RUN,
        status: `ended`,
        endedAt: null,
        hostUserId: `host-1`,
        mergedOwnPr: true,
      },
      txId: 1,
    },
    expected: { ok: true, id: RUN, status: `ended`, endedAt: null },
    calledWith: { sessionId: RUN },
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

// ── pr_update (EXP-1139) ──────────────────────────────────────────────────────

describe(`exponential_pr_update`, () => {
  const PR_URL = `https://github.com/acme/app/pull/7`
  const runRow = (over: Record<string, unknown> = {}) => ({
    id: SESSION,
    teamId: WS,
    issueId: null,
    branch: null,
    prUrl: null,
    prNumber: null,
    status: `in_review`,
    needsInput: false,
    mergedOwnPr: false,
    userId: `user-1`,
    hostUserId: null,
    ...over,
  })

  beforeEach(() => {
    caller.issues.updatePr.mockResolvedValue({
      updated: true,
      url: PR_URL,
      number: 7,
    })
    caller.repositories.updatePull.mockResolvedValue({ updated: true })
  })

  it(`rewrites an issue's PR through issues.updatePr`, async () => {
    dbRows.current = [{ id: UUID, identifier: `EXP-1`, prUrl: PR_URL }]
    const result = await tool(`exponential_pr_update`)({
      issueId: UUID,
      body: `Closes #EXP-1\n\nNow a searchable picker.`,
    })
    expect(parseOk(result)).toEqual({
      results: [
        { issueId: UUID, identifier: `EXP-1`, updated: true, url: PR_URL },
      ],
    })
    expect(caller.issues.updatePr).toHaveBeenCalledWith({
      issueId: UUID,
      body: `Closes #EXP-1\n\nNow a searchable picker.`,
    })
    // An omitted title is ABSENT from the tRPC input, never `undefined`-as-null.
    expect(
      Object.keys(caller.issues.updatePr.mock.calls[0]![0] as object)
    ).toEqual([`issueId`, `body`])
  })

  it(`updates a batch PR once, on the first listed issue`, async () => {
    const OTHER = `33333333-3333-4333-8333-333333333333`
    dbRows.current = [
      { id: UUID, identifier: `EXP-1`, prUrl: PR_URL },
      { id: OTHER, identifier: `EXP-2`, prUrl: PR_URL },
    ]
    const result = await tool(`exponential_pr_update`)({
      issueIds: [UUID, OTHER],
      title: `EXP-1 + EXP-2: the picker`,
    })
    expect(parseOk(result)).toEqual({
      results: [
        { issueId: UUID, identifier: `EXP-1`, updated: true, url: PR_URL },
      ],
    })
    expect(caller.issues.updatePr).toHaveBeenCalledTimes(1)
  })

  it(`reports a per-issue refusal as a result, not a tool error`, async () => {
    dbRows.current = [{ id: UUID, identifier: `EXP-1`, prUrl: PR_URL }]
    caller.issues.updatePr.mockRejectedValueOnce(
      new TRPCError({
        code: `PRECONDITION_FAILED`,
        message: `The pull request is merged. Only open pull requests can be edited.`,
      })
    )
    const result = await tool(`exponential_pr_update`)({
      issueId: UUID,
      title: `x`,
    })
    expect(result.isError).toBeFalsy()
    expect(parseOk(result)).toEqual({
      results: [
        {
          issueId: UUID,
          identifier: `EXP-1`,
          updated: false,
          error: `The pull request is merged. Only open pull requests can be edited.`,
        },
      ],
    })
  })

  it(`rewrites a chore PR through repositories.updatePull`, async () => {
    vi.mocked(loadRepositoryForTeam).mockResolvedValue({
      repositoryId: REPO,
      teamId: WS,
      fullName: `acme/app`,
      defaultBranch: `main`,
    } as never)
    const result = await tool(`exponential_pr_update`)({
      repositoryId: REPO,
      prNumber: 9,
      title: `chore: bump deps`,
      body: ``,
    })
    expect(parseOk(result)).toEqual({
      results: [{ repositoryId: REPO, prNumber: 9, updated: true }],
    })
    expect(caller.repositories.updatePull).toHaveBeenCalledWith({
      repositoryId: REPO,
      prNumber: 9,
      title: `chore: bump deps`,
      body: ``,
    })
  })

  it(`with no subject edits the header run's own issue PR`, async () => {
    // The drizzle stub serves the same rows to every select: the session
    // lookup and the issue lookup both read a row that carries what they
    // need.
    dbRows.current = [
      { ...runRow({ issueId: UUID }), identifier: `EXP-1`, prUrl: PR_URL },
    ]
    const result = await collectTools(USER, SESSION).get(
      `exponential_pr_update`
    )!({ body: `Scope changed: see the second commit.` })
    expect(parseOk(result)).toMatchObject({
      results: [{ issueId: UUID, updated: true }],
    })
    expect(caller.issues.updatePr).toHaveBeenCalledWith({
      issueId: UUID,
      body: `Scope changed: see the second commit.`,
    })
  })

  it(`with no subject edits the header run's own chore PR`, async () => {
    dbRows.current = [runRow({ branch: `exp/chore-1a2b3c4d`, prUrl: PR_URL, prNumber: 7 })]
    vi.mocked(loadRepositoryByFullName).mockResolvedValue({
      id: REPO,
      teamId: WS,
      fullName: `acme/app`,
    } as never)
    vi.mocked(loadRepositoryForTeam).mockResolvedValue({
      repositoryId: REPO,
      teamId: WS,
      fullName: `acme/app`,
      defaultBranch: `main`,
    } as never)
    const result = await collectTools(USER, SESSION).get(
      `exponential_pr_update`
    )!({ title: `chore: the real scope` })
    expect(parseOk(result)).toEqual({
      results: [{ repositoryId: REPO, prNumber: 7, updated: true }],
    })
    expect(loadRepositoryByFullName).toHaveBeenCalledWith(WS, `acme/app`)
    expect(caller.repositories.updatePull).toHaveBeenCalledWith({
      repositoryId: REPO,
      prNumber: 7,
      title: `chore: the real scope`,
    })
  })

  // EXP-1154: a run's own PR body IS its report.
  it(`with no subject, a body-only call is a no-op once the run has a report`, async () => {
    dbRows.current = [
      { ...runRow({ issueId: UUID }), identifier: `EXP-1`, prUrl: PR_URL },
    ]
    vi.mocked(runHasReportBody).mockResolvedValueOnce(true)
    const result = await collectTools(USER, SESSION).get(
      `exponential_pr_update`
    )!({ body: `Hand-written body` })
    expect(parseOk(result)).toMatchObject({
      results: [],
      note: expect.stringContaining(`exponential_sessions_results`),
    })
    expect(caller.issues.updatePr).not.toHaveBeenCalled()
  })

  it(`with no subject and a report, a title + body call lands only the title`, async () => {
    dbRows.current = [
      { ...runRow({ issueId: UUID }), identifier: `EXP-1`, prUrl: PR_URL },
    ]
    vi.mocked(runHasReportBody).mockResolvedValueOnce(true)
    await collectTools(USER, SESSION).get(`exponential_pr_update`)!({
      title: `New title`,
      body: `Hand-written body`,
    })
    expect(caller.issues.updatePr).toHaveBeenCalledWith({
      issueId: UUID,
      title: `New title`,
    })
  })

  it(`refuses a call with no subject and no run header`, async () => {
    const result = await tool(`exponential_pr_update`)({ title: `x` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`no run header`)
    expect(caller.issues.updatePr).not.toHaveBeenCalled()
  })

  it(`refuses a call that changes nothing`, async () => {
    const result = await tool(`exponential_pr_update`)({ issueId: UUID })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`title, a body, or both`)
  })

  it(`refuses two subjects at once`, async () => {
    const result = await tool(`exponential_pr_update`)({
      issueId: UUID,
      repositoryId: REPO,
      prNumber: 9,
      title: `x`,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`exactly one`)
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

// ── EXP-825 compat: retired text/textarea input defs on the MCP tools ────────
// An old creator run (desktop ≤ 0.14.35) still tells the agent `type: text`;
// the tools accept the def and drop it (lib/action-inputs.ts), seeding the
// composer hint. update forwards to actions.update, which drops against the
// row (the hint seeds only when the row has none).

describe(`exponential_actions_create/update — input kinds`, () => {
  beforeEach(() => {
    caller.actions.create.mockReset()
    caller.actions.update.mockReset()
  })

  it(`forwards a pick schema untouched`, async () => {
    caller.actions.create.mockResolvedValue({
      action: { id: UUID, name: `Release`, inputs: [] },
    })
    const result = await tool(`exponential_actions_create`)({
      teamId: WS,
      name: `Release`,
      body: `# Do the release`,
      inputs: [{ key: `repo`, label: `Repository`, type: `repo` }],
      promptPlaceholder: `Which platforms`,
    })
    expect(parseOk(result)).toMatchObject({ id: UUID, inputs: [] })
    expect(caller.actions.create).toHaveBeenCalledWith({
      teamId: WS,
      name: `Release`,
      body: `# Do the release`,
      inputs: [{ key: `repo`, label: `Repository`, type: `repo` }],
      promptPlaceholder: `Which platforms`,
    })
  })

  it(`update forwards its input array as sent`, async () => {
    caller.actions.update.mockResolvedValue({ action: { id: UUID, inputs: [] } })
    const result = await tool(`exponential_actions_update`)({
      id: UUID,
      inputs: [{ key: `board`, label: `Board`, type: `board` }],
    })
    expect(parseOk(result)).toMatchObject({ id: UUID })
    expect(caller.actions.update).toHaveBeenCalledWith({
      id: UUID,
      inputs: [{ key: `board`, label: `Board`, type: `board` }],
    })
  })

  // EXP-825: free text reaches a run through the start's `prompt`, so the
  // retired `text`/`textarea` kinds are as unknown as any other bogus one.
  it(`rejects unknown and retired input kinds at the schema`, () => {
    const schema = collectToolDefs().get(`exponential_actions_create`)!.inputSchema!
    for (const type of [`number`, `text`, `textarea`]) {
      expect(
        schema.safeParse({
          teamId: WS,
          name: `A`,
          body: `x`,
          inputs: [{ key: `n`, label: `N`, type }],
        }).success
      ).toBe(false)
    }
  })
})

// ── notifications_send (EXP-801) ─────────────────────────────────────────────

describe(`exponential_notifications_send`, () => {
  const send = vi.mocked(sendAgentMessage)
  beforeEach(() => {
    send.mockReset()
    dbRows.current = [
      { id: `user-1`, email: `u@example.com` },
      { id: `user-2`, email: `Two@Example.com` },
    ]
  })

  it(`resolves ids and emails against the team's members and reports each bucket`, async () => {
    send.mockResolvedValue({
      delivered: [`user-2`],
      declined: [],
      notMembers: [],
      deduped: [`user-1`],
    })
    const result = await tool(`exponential_notifications_send`)({
      teamId: WS,
      recipients: [`user-1`, `two@example.com`, `ghost@example.com`],
      title: `  Build finished  `,
      body: `All green.`,
    })
    expect(parseOk(result)).toEqual({
      ok: true,
      delivered: [{ id: `user-2`, email: `Two@Example.com` }],
      declined: [],
      deduped: [{ id: `user-1`, email: `u@example.com` }],
      notMembers: [],
      unknown: [`ghost@example.com`],
      opens: null,
    })
    expect(send).toHaveBeenCalledWith({
      teamId: WS,
      senderUserId: `user-1`,
      recipientIds: [`user-1`, `user-2`],
      title: `Build finished`,
      body: `All green.`,
      issueId: null,
    })
    expect(membership.resolveTeamAccess).toHaveBeenCalledWith(`user-1`, WS)
  })

  // EXP-933: a run pings its own person without looking anyone up.
  it(`defaults recipients to the caller`, async () => {
    send.mockResolvedValue({ delivered: [`user-1`], declined: [], notMembers: [], deduped: [] })
    const result = await tool(`exponential_notifications_send`)({ teamId: WS, title: `Done` })
    expect(parseOk(result)).toMatchObject({ ok: true })
    expect(send).toHaveBeenCalledWith(
      expect.objectContaining({ recipientIds: [`user-1`], issueId: null })
    )
  })

  it(`asks for a team outside a session when none can be derived`, async () => {
    const result = await tool(`exponential_notifications_send`)({ title: `Done` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`Pass teamId`)
    expect(send).not.toHaveBeenCalled()
  })

  it(`targets a named issue and derives the team from it`, async () => {
    const ISSUE = `11111111-2222-4333-8444-555555555555`
    // Every select in this mock answers the same rows: the issue lookup reads
    // `teamId`, the member lookup `id`/`email`.
    dbRows.current = [{ id: `user-1`, email: `u@example.com`, teamId: WS }]
    send.mockResolvedValue({ delivered: [`user-1`], declined: [], notMembers: [], deduped: [] })
    const result = await tool(`exponential_notifications_send`)({ title: `Done`, issueId: ISSUE })
    expect(parseOk(result)).toMatchObject({ opens: { issueId: ISSUE, face: `results` } })
    expect(send).toHaveBeenCalledWith(
      expect.objectContaining({ teamId: WS, issueId: ISSUE, recipientIds: [`user-1`] })
    )
  })

  it(`reports a recipient who blocked teammates' agents as declined, not an error`, async () => {
    send.mockResolvedValue({
      delivered: [],
      declined: [`user-2`],
      notMembers: [],
      deduped: [],
    })
    const result = await tool(`exponential_notifications_send`)({
      teamId: WS,
      recipients: [`user-2`],
      title: `Ping`,
    })
    expect(parseOk(result)).toMatchObject({
      ok: false,
      declined: [{ id: `user-2`, email: `Two@Example.com` }],
    })
  })

  it(`refuses when no recipient is a member, without sending`, async () => {
    const result = await tool(`exponential_notifications_send`)({
      teamId: WS,
      recipients: [`nobody@example.com`],
      title: `Ping`,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`exponential_members_list`)
    expect(send).not.toHaveBeenCalled()
  })

  // A team-level WRITE: a board-confined grant can SEE the host team (aux
  // reads) but may not push to its members.
  it(`refuses a board-confined grant (team visible, not fully granted) before the fan-out`, async () => {
    const result = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_notifications_send`)!({
      teamId: WS,
      recipients: [`user-2`],
      title: `Ping`,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`team-level operations`)
    expect(send).not.toHaveBeenCalled()
    expect(membership.resolveTeamAccess).not.toHaveBeenCalled()
  })

  it(`lets a whole-team grant send`, async () => {
    send.mockResolvedValue({
      delivered: [`user-2`],
      declined: [],
      notMembers: [],
      deduped: [],
    })
    const result = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_WS
    ).get(`exponential_notifications_send`)!({
      teamId: WS,
      recipients: [`user-2`],
      title: `Ping`,
    })
    expect(parseOk(result)).toMatchObject({ ok: true })
  })

  // The per-sender bucket (capacity 10) is charged AFTER validation: a burst
  // of rejected calls must not lock out the next valid one.
  it(`does not burn a burst token on a rejected call`, async () => {
    for (let i = 0; i < 12; i++) {
      const rejected = await tool(`exponential_notifications_send`)({
        teamId: WS,
        recipients: [`nobody@example.com`],
        title: `Ping`,
      })
      expect(rejected.isError).toBe(true)
      expect(rejected.content[0].text).not.toContain(`Too many messages`)
    }
    send.mockResolvedValue({
      delivered: [`user-2`],
      declined: [],
      notMembers: [],
      deduped: [],
    })
    const result = await tool(`exponential_notifications_send`)({
      teamId: WS,
      recipients: [`user-2`],
      title: `Ping`,
    })
    expect(parseOk(result)).toMatchObject({ ok: true })
  })

  it(`denies a non-member sender before touching the fan-out`, async () => {
    membership.resolveTeamAccess.mockRejectedValueOnce(
      new TRPCError({ code: `FORBIDDEN`, message: `Not a member` })
    )
    const result = await tool(`exponential_notifications_send`)({
      teamId: WS,
      recipients: [`user-2`],
      title: `Ping`,
    })
    expect(result.isError).toBe(true)
    expect(send).not.toHaveBeenCalled()
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
    expect(payload.curl).toBe(`curl -sS -T 'shot.png' "${payload.uploadUrl}"`)
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

// ── FEED-57: the batch start poll's row match ────────────────────────────
// Compat cleanup round 25: a batch row with a NULL covered set never matches
// (every device at or above the floor stamps `batch_issue_ids`).

describe(`batchStartRowMatch (FEED-57)`, () => {
  it(`requires the covered set to name the batch, never a NULL set`, async () => {
    const { batchStartRowMatch } = await import(`@/lib/mcp/tools`)
    const { sql, params } = new PgDialect().sqlToQuery(
      batchStartRowMatch([WS], [UUID]) as never
    )
    expect(sql).toContain(`"coding_sessions"."batch_issue_ids" ?| `)
    expect(sql).not.toContain(`"batch_issue_ids" is null`)
    expect(sql).toContain(`"coding_sessions"."action_name" is null`)
    expect(params).toEqual(expect.arrayContaining([WS, [UUID]]))
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
      expect(parseOk(result)).toEqual({
        issueId: `bug-issue-1`,
        identifier: `EXP-1`,
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

// ── pr_open batch session parking (EXP-194 / EXP-545 / EXP-637) ──────────────
// Batch coding sessions carry no issue linkage, so the per-issue PR-open flip
// misses them and pr_open parks the caller's own row instead. The EXP-637
// session header names it EXACTLY; EXP-710 removed the pre-EXP-637 heuristic
// (the caller's issue-less, action-less running rows in the affected teams),
// which two concurrent batch runs by one user in one team could not tell
// apart — so a headerless caller now parks nothing at all.
describe(`exponential_pr_open batch session parking`, () => {
  function armPrOpen(
    issueRow: Record<string, unknown> = { status: `backlog` }
  ): Array<{ set: Record<string, unknown>; where: unknown }> {
    const updates: Array<{ set: Record<string, unknown>; where: unknown }> = []
    caller.repositories.forIssue.mockResolvedValue({
      repositoryId: REPO,
      fullName: `acme/app`,
      defaultBranch: `main`,
    })
    vi.mocked(resolveRepoInstallationTokenInfo).mockResolvedValue({
      token: `tok`,
      installationId: 42,
    } as never)
    vi.mocked(createPullRequest).mockResolvedValue({
      url: `https://github.com/acme/app/pull/7`,
      number: 7,
    } as never)
    db.transaction.mockImplementation(async (fn: (tx: unknown) => unknown) => {
      const txSelect: Record<string, unknown> = {}
      for (const method of [`from`, `where`, `limit`]) {
        txSelect[method] = () => txSelect
      }
      ;(txSelect as { then: unknown }).then = (
        resolve: (v: unknown) => unknown,
        reject: (e: unknown) => unknown
      ) => Promise.resolve([issueRow]).then(resolve, reject)
      return fn({
        select: () => txSelect,
        update: () => ({
          set: (values: Record<string, unknown>) => ({
            where: async (cond: unknown) => {
              updates.push({ set: values, where: cond })
            },
          }),
        }),
      })
    })
    return updates
  }

  // FEED-66: a cross-board move renumbered the issue mid-run (FEED-50 →
  // EXP-1147); the run pushed `exp/FEED-50`, the inferred head named a branch
  // nobody pushed and GitHub answered 422 "head invalid".
  it(`infers the head from a RETIRED identifier when that branch is the one on GitHub`, async () => {
    armPrOpen()
    dbRows.current = [{ identifier: `EXP-1147`, branch: null }]
    vi.mocked(retiredIdentifiers).mockResolvedValueOnce([`FEED-50`])
    vi.mocked(branchExists).mockImplementation(
      async (_repo, branch) => branch === `exp/FEED-50`
    )

    const result = await collectTools(USER, null).get(`exponential_pr_open`)!({
      issueId: UUID,
      title: `Team management`,
    })

    expect(parseOk(result)).toMatchObject({ number: 7 })
    expect(vi.mocked(branchExists).mock.calls.map(([, branch]) => branch)).toEqual([
      `exp/EXP-1147`,
      `exp/FEED-50`,
    ])
    expect(vi.mocked(createPullRequest).mock.calls[0]![0]).toMatchObject({
      head: `exp/FEED-50`,
    })
  })

  it(`costs a never-moved issue no GitHub read and keeps the plain guess`, async () => {
    armPrOpen()
    dbRows.current = [{ identifier: `EXP-1147`, branch: null }]

    await collectTools(USER, null).get(`exponential_pr_open`)!({
      issueId: UUID,
      title: `Team management`,
    })

    expect(branchExists).not.toHaveBeenCalled()
    expect(vi.mocked(createPullRequest).mock.calls[0]![0]).toMatchObject({
      head: `exp/EXP-1147`,
    })
  })

  it(`says the head was inferred when GitHub rejects it with 422`, async () => {
    armPrOpen()
    dbRows.current = [{ identifier: `EXP-1147`, branch: null }]
    vi.mocked(createPullRequest).mockRejectedValueOnce(
      new Error(`GitHub PR create failed (422): {"message":"Validation Failed","errors":[{"field":"head","code":"invalid"}]}`)
    )

    const result = await collectTools(USER, null).get(`exponential_pr_open`)!({
      issueId: UUID,
      title: `Team management`,
    })

    expect(result.isError).toBe(true)
    expect(result.content[0]!.text).toContain(`(422)`)
    expect(result.content[0]!.text).toContain(
      `The head branch 'exp/EXP-1147' was inferred from the issue; pass 'head' with the branch you pushed.`
    )
  })

  it(`parks the EXACT header session, stamping the combined PR and its branch`, async () => {
    const updates = armPrOpen()
    dbRows.current = [
      {
        id: SESSION,
        teamId: WS,
        issueId: null,
        branch: null,
        status: `running`,
        needsInput: false,
        mergedOwnPr: false,
        userId: `user-1`,
        hostUserId: null,
      },
    ]

    const result = await collectTools(USER, SESSION).get(
      `exponential_pr_open`
    )!({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
    })

    expect(parseOk(result)).toMatchObject({ number: 7 })
    const flip = updates.find((u) => u.set.status === `in_review`)
    expect(flip).toBeDefined()
    // The run owns its PR: the row↔PR linkage clients hang Merge on.
    const stamp = updates.find(
      (u) => u.set.prUrl && u.set.branch === `exp/batch-abcd1234` && !(`status` in u.set) && u.set.updatedAt
    )
    expect(stamp!.set).toMatchObject({
      prUrl: `https://github.com/acme/app/pull/7`,
      prNumber: 7,
      prState: `open`,
    })
    for (const update of [flip!, stamp!]) {
      const { sql, params } = new PgDialect().sqlToQuery(update.where as never)
      expect(sql).toContain(`"id" =`)
      expect(params).toContain(SESSION)
      // SLOP-3: only a run of the PR's own team (the issues' team) is stamped.
      expect(sql).toContain(`"team_id" =`)
      expect(params).toContain(`ws-1`)
      // Never the removed heuristic sweep — it could reach an action or chat run.
      expect(sql).not.toContain(`"issue_id" is null`)
      expect(sql).not.toContain(`"action_id" is null`)
    }
  })

  // EXP-1154: the issue path sends the report too.
  it(`sends the body derived from the run's report on the issue path`, async () => {
    armPrOpen()
    vi.mocked(runPrBody).mockResolvedValueOnce({ body: `From the report`, fromResults: true })
    const result = await tool(`exponential_pr_open`)({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
      body: `Agent body`,
    })
    expect(createPullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ body: `From the report` })
    )
    expect(parseOk(result)).toMatchObject({ number: 7, body: `report` })
  })

  it(`parks NOTHING without a session header (EXP-710)`, async () => {
    const updates = armPrOpen()
    const result = await tool(`exponential_pr_open`)({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
    })
    expect(parseOk(result)).toMatchObject({ number: 7 })
    // The issues still move; only the guess at the caller's own run is gone.
    expect(updates.some((u) => u.set.status === `in_review`)).toBe(false)
    expect(updates.some((u) => u.set.branch === `exp/batch-abcd1234`)).toBe(
      true
    )
  })

  // FEED-59: more issues implemented on the batch branch after its PR opened.
  // Looked up BEFORE the create, whatever its base: GitHub's 422 only fires
  // for the same head AND base, so a different base would open a second PR.
  it(`links more issues to the PR already open on head, whatever its base`, async () => {
    const updates = armPrOpen()
    vi.mocked(findOpenPullByHead).mockResolvedValue({
      url: `https://github.com/acme/app/pull/5`,
      number: 5,
      baseRef: `develop`,
    })

    const result = await tool(`exponential_pr_open`)({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
    })

    expect(parseOk(result)).toMatchObject({
      url: `https://github.com/acme/app/pull/5`,
      number: 5,
      base: `develop`,
      reused: true,
    })
    const links = updates.filter((u) => u.set.prUrl)
    expect(links).toHaveLength(2)
    for (const link of links) {
      expect(link.set).toMatchObject({
        prUrl: `https://github.com/acme/app/pull/5`,
        prNumber: 5,
        prState: `open`,
        branch: `exp/batch-abcd1234`,
        // The real base of the existing PR, not the one this call computed.
        prBaseBranch: `develop`,
      })
    }
    expect(applyPrLifecycleStatusInTx).toHaveBeenCalledTimes(2)
    expect(fireAndForgetPrNotify).toHaveBeenCalledTimes(2)
    expect(createPullRequest).not.toHaveBeenCalled()
    expect(findOpenPullByHead).toHaveBeenCalledWith(
      `acme/app`,
      `exp/batch-abcd1234`,
      `tok`,
      undefined,
      undefined
    )
  })

  // EXP-1154: a reused PR's body follows the report, but only the PR the
  // caller's row actually got stamped with.
  it(`re-syncs a reused issue PR's body, pinned to that PR's url`, async () => {
    armPrOpen()
    vi.mocked(findOpenPullByHead).mockResolvedValue({
      url: `https://github.com/acme/app/pull/5`,
      number: 5,
      baseRef: `main`,
    })
    vi.mocked(runPrBody).mockResolvedValueOnce({ body: `From the report`, fromResults: true })
    dbRows.current = [
      {
        id: SESSION,
        teamId: WS,
        issueId: null,
        branch: null,
        status: `running`,
        needsInput: false,
        mergedOwnPr: false,
        userId: `user-1`,
        hostUserId: null,
      },
    ]

    const result = await collectTools(USER, SESSION).get(`exponential_pr_open`)!({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
    })

    expect(parseOk(result)).toMatchObject({ number: 5, reused: true })
    expect(syncRunPrBody).toHaveBeenCalledTimes(1)
    expect(syncRunPrBody).toHaveBeenCalledWith(SESSION, {
      expectPrUrl: `https://github.com/acme/app/pull/5`,
    })
  })

  it(`falls back to the base-filtered lookup when a create races into a 422`, async () => {
    const updates = armPrOpen()
    vi.mocked(createPullRequest).mockRejectedValue(
      new PullAlreadyExistsError(`GitHub PR create failed (422)`)
    )
    vi.mocked(findOpenPullByHead)
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce({
        url: `https://github.com/acme/app/pull/5`,
        number: 5,
        baseRef: `main`,
      })

    const result = await tool(`exponential_pr_open`)({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
    })

    expect(parseOk(result)).toMatchObject({ number: 5, reused: true })
    expect(findOpenPullByHead).toHaveBeenLastCalledWith(
      `acme/app`,
      `exp/batch-abcd1234`,
      `tok`,
      undefined,
      `main`
    )
    expect(updates.filter((u) => u.set.prUrl)).toHaveLength(2)
  })

  it(`surfaces GitHub's own 422 when the fallback lookup fails`, async () => {
    armPrOpen()
    vi.mocked(createPullRequest).mockRejectedValue(
      new PullAlreadyExistsError(`GitHub PR create failed (422): exists`)
    )
    vi.mocked(findOpenPullByHead)
      .mockResolvedValueOnce(null)
      .mockRejectedValueOnce(new Error(`GitHub returned 502 listing open pulls`))

    const result = await tool(`exponential_pr_open`)({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`422`)
    expect(result.content[0].text).not.toContain(`502`)
  })

  it(`re-links an issue whose row still names the PR but is not open`, async () => {
    const updates = armPrOpen({
      status: `done`,
      prUrl: `https://github.com/acme/app/pull/5`,
      prState: `closed`,
    })
    vi.mocked(findOpenPullByHead).mockResolvedValue({
      url: `https://github.com/acme/app/pull/5`,
      number: 5,
      baseRef: `main`,
    })

    await tool(`exponential_pr_open`)({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
    })

    const links = updates.filter((u) => u.set.prUrl)
    expect(links).toHaveLength(2)
    expect(links[0]!.set.prState).toBe(`open`)
  })

  it(`leaves issues already linked to the reused PR untouched`, async () => {
    const updates = armPrOpen({
      status: `in_review`,
      prUrl: `https://github.com/acme/app/pull/5`,
      prState: `open`,
    })
    vi.mocked(createPullRequest).mockRejectedValue(
      new PullAlreadyExistsError(`GitHub PR create failed (422)`)
    )
    vi.mocked(findOpenPullByHead).mockResolvedValue({
      url: `https://github.com/acme/app/pull/5`,
      number: 5,
      baseRef: `main`,
    })

    const result = await tool(`exponential_pr_open`)({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
    })

    expect(parseOk(result)).toMatchObject({ number: 5, reused: true })
    expect(updates.some((u) => u.set.prUrl)).toBe(false)
    expect(recordIssueEvent).not.toHaveBeenCalled()
    expect(applyPrLifecycleStatusInTx).not.toHaveBeenCalled()
    expect(fireAndForgetPrNotify).not.toHaveBeenCalled()
  })

  it(`still fails on any other create error, or when no open PR is found`, async () => {
    armPrOpen()
    vi.mocked(createPullRequest).mockRejectedValue(
      new PullAlreadyExistsError(`GitHub PR create failed (422): exists`)
    )
    vi.mocked(findOpenPullByHead).mockResolvedValue(null)

    const result = await tool(`exponential_pr_open`)({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`422`)

    vi.mocked(createPullRequest).mockRejectedValue(
      new Error(`GitHub PR create failed (422): No commits between`)
    )
    vi.mocked(findOpenPullByHead).mockClear()
    const other = await tool(`exponential_pr_open`)({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
    })
    expect(other.isError).toBe(true)
    // Only the up-front lookup, never the 422 fallback.
    expect(findOpenPullByHead).toHaveBeenCalledTimes(1)
  })
})

// ── EXP-637: the session header ──────────────────────────────────────────────
// The launcher injects X-Exp-Session-Id into the MCP config it writes, so
// every tool call an agent makes names the run it is running inside. That is
// how sessions_end closes out the right row and how pr_open parks the EXACT
// row instead of guessing. The id is an identifier, never a credential:
// ownership is re-checked per tool.
const SESSION = `66666666-6666-4666-8666-666666666666`

// SLOP-4: a reporter reply emails an outside address, so only a person's
// full-access key may send one: never a confined OAuth grant, never an
// unattended run. A person-started run keeps it.
describe(`exponential_comments_create audience reporter`, () => {
  const args = { issueId: UUID, body: `Fixed.`, audience: `reporter` }
  const ownRun = (startedReason: string | null) => ({
    id: SESSION,
    teamId: `ws-1`,
    status: `running`,
    startedReason,
    userId: `user-1`,
    hostUserId: null,
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

  it.each([`schedule`, `event`, `agent`])(
    `refuses an unattended run (%s)`,
    async (startedReason) => {
      dbRows.current = [ownRun(startedReason)]
      const result = await collectTools(USER, SESSION).get(
        `exponential_comments_create`
      )!(args)
      expect(result.isError).toBe(true)
      expect(result.content[0].text).toContain(`unattended`)
      expect(caller.comments.create).not.toHaveBeenCalled()
    }
  )

  it(`keeps it for a person-started run, and a team comment for anyone`, async () => {
    caller.comments.create.mockResolvedValue({
      comment: { id: UUID },
      reporterEmailed: true,
    })
    dbRows.current = [ownRun(null)]
    const attended = await collectTools(USER, SESSION).get(
      `exponential_comments_create`
    )!(args)
    expect(attended.isError).toBeFalsy()

    dbRows.current = [ownRun(`schedule`)]
    const team = await collectTools(USER, SESSION).get(
      `exponential_comments_create`
    )!({ issueId: UUID, body: `Note.` })
    expect(team.isError).toBeFalsy()
    expect(caller.comments.create).toHaveBeenCalledTimes(2)
  })
})

describe(`exponential_sessions_end`, () => {
  // EXP-679: the tool only registers for an unattended run, so these cases
  // hand in the gate the route would have resolved for one.
  const UNATTENDED = { sessionsEnd: true, askParent: false, sessionResults: true }

  it(`refuses outside a launched session, naming the missing header`, async () => {
    const result = await collectTools(USER, null, UNATTENDED).get(
      `exponential_sessions_end`
    )!({ summary: `did the thing` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`X-Exp-Session-Id`)
    expect(endSessionByAgent).not.toHaveBeenCalled()
  })

  it(`closes the header session out with the summary`, async () => {
    vi.mocked(endSessionByAgent).mockResolvedValue({
      sessionId: SESSION,
      status: `ended`,
      alreadyEnded: false,
    })

    const result = await collectTools(USER, SESSION, UNATTENDED).get(
      `exponential_sessions_end`
    )!({ summary: `Stuck on the migration.` })

    expect(parseOk(result)).toEqual({
      sessionId: SESSION,
      status: `ended`,
      alreadyEnded: false,
      reportedToParent: false,
    })
    expect(endSessionByAgent).toHaveBeenCalledWith(db, SESSION, `user-1`, {
      summary: `Stuck on the migration.`,
    })
    // EXP-700: a first real end reports into a live parent (the helper
    // no-ops for parentless runs).
    expect(notifyParentOfChildEnd).toHaveBeenCalledWith(db, SESSION, {
      summary: `Stuck on the migration.`,
      endedBy: `agent`,
    })
  })

  // EXP-700: only the FIRST real end notifies the parent — a retried
  // close-out (alreadyEnded) never does.
  it(`does not notify the parent again on a retried close-out`, async () => {
    vi.mocked(endSessionByAgent).mockResolvedValue({
      sessionId: SESSION,
      status: `ended`,
      alreadyEnded: true,
    })

    const result = await collectTools(USER, SESSION, UNATTENDED).get(
      `exponential_sessions_end`
    )!({ summary: `retry` })

    expect(parseOk(result)).toMatchObject({
      alreadyEnded: true,
      reportedToParent: false,
    })
    expect(notifyParentOfChildEnd).not.toHaveBeenCalled()
  })

  // EXP-705: unknown keys are a hard error everywhere, including the stray
  // `outcome` old pre-EXP-686 builds still send — a loud unrecognized-key
  // rejection the agent can retry, never a silent strip (min-version gates
  // retire those builds).
  it(`rejects an old client's stray outcome argument`, () => {
    const schema = collectToolDefs(UNATTENDED).get(`exponential_sessions_end`)!
      .inputSchema!
    const result = schema.safeParse({ summary: `Shipped it.`, outcome: `done` })
    expect(result.success).toBe(false)
    expect(schema.safeParse({ summary: `Shipped it.` }).success).toBe(true)
  })

  // EXP-679: a person-started run never gets the tool — the human is right
  // there, and a close-out would end a conversation they are still having.
  it(`is not registered for a person-started session`, async () => {
    const tools = collectTools(USER, SESSION, {
      sessionsEnd: false,
      askParent: false,
      sessionResults: true,
    })
    expect(tools.has(`exponential_sessions_end`)).toBe(false)
  })

  it(`surfaces a foreign session's refusal as a tool error`, async () => {
    vi.mocked(endSessionByAgent).mockRejectedValue(forbidden())

    const result = await collectTools(USER, SESSION, UNATTENDED).get(
      `exponential_sessions_end`
    )!({ summary: `s` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not allowed here`)
  })
})

// ── EXP-700: the child's ask rail ────────────────────────────────────────────
describe(`exponential_sessions_ask_parent`, () => {
  const PARENT = `77777777-7777-4777-8777-777777777777`
  const AGENT_CHILD = { sessionsEnd: true, askParent: true, sessionResults: true }
  const RELAY = { url: `https://relay.test`, secret: `s` }

  // The row loadChildParentContext's one select serves: the child, its issue
  // identifier and the joined parent status.
  const childRow = (over: Record<string, unknown> = {}) => ({
    id: SESSION,
    userId: `user-1`,
    hostUserId: null,
    startedReason: `agent`,
    parentSessionId: PARENT,
    actionName: null,
    issueIdentifier: `EXP-12`,
    parentStatus: `running`,
    ...over,
  })

  it(`is not registered without its gate`, () => {
    const tools = collectTools(USER, SESSION, {
      sessionsEnd: true,
      askParent: false,
      sessionResults: true,
    })
    expect(tools.has(`exponential_sessions_ask_parent`)).toBe(false)
  })

  it(`refuses outside a launched session, naming the missing header`, async () => {
    const result = await collectTools(USER, null, AGENT_CHILD).get(
      `exponential_sessions_ask_parent`
    )!({ question: `Which env?` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`X-Exp-Session-Id`)
  })

  it(`delivers the question into the parent's channel and says to wait`, async () => {
    dbRows.current = [childRow()]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    vi.mocked(relayPostInput).mockResolvedValue({ delivered: true })

    const result = await collectTools(USER, SESSION, AGENT_CHILD).get(
      `exponential_sessions_ask_parent`
    )!({ question: `Which env?` })

    expect(relayPostInput).toHaveBeenCalledWith(
      RELAY,
      PARENT,
      `[Exponential child run EXP-12 ${SESSION.slice(0, 8)} asks — reply with exponential_sessions_message sessionId=${SESSION}] Which env?`
    )
    expect(parseOk(result)).toMatchObject({ delivered: true })
    expect((parseOk(result) as { note: string }).note).toContain(
      `end your turn`
    )
  })

  it(`refuses a run without an agent parent linkage`, async () => {
    dbRows.current = [childRow({ startedReason: `schedule`, parentSessionId: null })]

    const result = await collectTools(USER, SESSION, AGENT_CHILD).get(
      `exponential_sessions_ask_parent`
    )!({ question: `q` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`no live starter`)
    expect(relayPostInput).not.toHaveBeenCalled()
  })

  it(`points an orphaned child at its close-out when the parent has ended`, async () => {
    dbRows.current = [childRow({ parentStatus: `ended` })]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)

    const result = await collectTools(USER, SESSION, AGENT_CHILD).get(
      `exponential_sessions_ask_parent`
    )!({ question: `q` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`exponential_sessions_end`)
    expect(relayPostInput).not.toHaveBeenCalled()
  })

  it(`degrades with guidance when the relay cannot deliver`, async () => {
    dbRows.current = [childRow()]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    vi.mocked(relayPostInput).mockResolvedValue({ delivered: false })

    const result = await collectTools(USER, SESSION, AGENT_CHILD).get(
      `exponential_sessions_ask_parent`
    )!({ question: `q` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`exponential_sessions_end`)
  })

  it(`degrades with guidance when the relay is not configured`, async () => {
    dbRows.current = [childRow()]

    const result = await collectTools(USER, SESSION, AGENT_CHILD).get(
      `exponential_sessions_ask_parent`
    )!({ question: `q` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`exponential_sessions_end`)
    expect(relayPostInput).not.toHaveBeenCalled()
  })
})

// ── EXP-1089 / EXP-1065: `to: 'user'` from any run, the question on the row ──
describe(`exponential_sessions_ask_parent — to: 'user' (EXP-1089)`, () => {
  const OWN_RUN = { sessionsEnd: false, askParent: true, sessionResults: true }

  const childRow = (over: Record<string, unknown> = {}) => ({
    id: SESSION,
    userId: `user-1`,
    hostUserId: null,
    startedReason: null,
    parentSessionId: null,
    actionName: null,
    issueIdentifier: `EXP-12`,
    parentStatus: null,
    ...over,
  })

  // One select result per call, in order (the shared builder serves every
  // select the same rows, and this path reads two different tables).
  const selectsInOrder = (...results: unknown[][]) => {
    for (const rows of results) {
      h.db.select.mockImplementationOnce(() => {
        const builder: Record<string, unknown> = {}
        for (const method of [`from`, `innerJoin`, `leftJoin`, `orderBy`, `limit`, `offset`, `where`]) {
          builder[method] = () => builder
        }
        ;(builder as { then: unknown }).then = (
          resolve: (v: unknown) => unknown,
          reject: (e: unknown) => unknown
        ) => Promise.resolve(rows).then(resolve, reject)
        return builder
      })
    }
  }

  beforeEach(() => {
    vi.mocked(sendAgentMessage).mockReset()
    vi.mocked(sendAgentMessage).mockResolvedValue({
      delivered: [`user-1`],
      declined: [],
      notMembers: [],
      deduped: [],
    } as never)
  })

  it(`is registered for a person-started run and asks its owner`, async () => {
    const tools = collectTools(USER, SESSION, OWN_RUN)
    expect(tools.has(`exponential_sessions_ask_parent`)).toBe(true)
    selectsInOrder([childRow()], [{ teamId: WS, userId: `user-1` }])
    const result = await tools.get(`exponential_sessions_ask_parent`)!({
      question: `Which env?`,
      to: `user`,
    })
    expect(parseOk(result)).toMatchObject({ delivered: true, to: `user` })
    expect(updateSet).toHaveBeenCalledWith(
      expect.objectContaining({
        needsInput: true,
        agentCaption: `Which env?`,
        pendingQuestion: { question: `Which env?`, askedAt: expect.any(String) },
      })
    )
    expect(sendAgentMessage).toHaveBeenCalledWith(
      expect.objectContaining({ recipientIds: [`user-1`], title: `EXP-12 asks`, body: `Which env?` })
    )
    expect(insertValues).not.toHaveBeenCalled()
  })

  it(`still refuses a starter target from a run nobody started`, async () => {
    selectsInOrder([childRow()])
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_ask_parent`
    )!({ question: `q` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`no live starter`)
  })

})

// ── EXP-879: the run publishes pictures of its own work ──────────────────────
// EXP-1172: the early form of sessions_results — one inline picture, the
// same grant + write, the attachment id in the answer's `id`.
describe(`exponential_sessions_show`, () => {
  const OWN_RUN = {
    sessionsEnd: false,
    askParent: false,
    sessionResults: true,
  }
  const runRow = (over: Record<string, unknown> = {}) => ({
    id: SESSION,
    teamId: `team-1`,
    userId: `user-1`,
    hostUserId: null,
    status: `running`,
    results: null,
    ...over,
  })

  beforeEach(() => {
    vi.stubEnv(`BETTER_AUTH_SECRET`, `mcp-tools-test-secret`)
    vi.stubEnv(`BETTER_AUTH_URL`, ``)
  })

  it(`rides the sessions_results gate`, () => {
    const closed = collectTools(USER, SESSION, { ...OWN_RUN, sessionResults: false })
    expect(closed.has(`exponential_sessions_show`)).toBe(false)
    expect(collectTools(USER, SESSION, OWN_RUN).has(`exponential_sessions_show`)).toBe(
      true
    )
  })

  it(`refuses outside a launched session and without exactly one source`, async () => {
    const outside = await collectTools(USER, null, OWN_RUN).get(
      `exponential_sessions_show`
    )!({ file: `shot.png` })
    expect(outside.content[0].text).toContain(`X-Exp-Session-Id`)
    dbRows.current = [runRow()]
    const tool = collectTools(USER, SESSION, OWN_RUN).get(`exponential_sessions_show`)!
    expect((await tool({})).content[0].text).toContain(`exactly one of file or dataBase64`)
    expect(
      (await tool({ file: `a.png`, dataBase64: `AA==`, contentType: `image/png` }))
        .content[0].text
    ).toContain(`exactly one`)
    expect((await tool({ dataBase64: `AA==` })).content[0].text).toContain(
      `needs contentType`
    )
  })

  it(`refuses another member's run and an ended one`, async () => {
    dbRows.current = [runRow({ userId: `user-2` })]
    const tool = collectTools(USER, SESSION, OWN_RUN).get(`exponential_sessions_show`)!
    expect((await tool({ file: `a.png` })).content[0].text).toContain(`not your run`)
    dbRows.current = [runRow({ status: `ended` })]
    expect((await tool({ file: `a.png` })).content[0].text).toContain(`ended`)
  })

  it(`answers the pre-allocated id with an inline grant and a quoted curl line`, async () => {
    dbRows.current = [runRow()]
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_show`
    )!({ file: `/tmp/it's here.png`, text: `  The empty state  ` })
    const payload = parseOk(result) as {
      id: string
      uploadUrl: string
      curl: string
      topic: string
      results: unknown
    }
    expect(payload.topic).toBe(`Progress`)
    expect(payload.curl).toBe(
      `curl -sS -F file=@'/tmp/it'\\''s here.png' "${payload.uploadUrl}"`
    )
    const token = payload.uploadUrl.split(`/`).pop()!
    expect(verifySessionResultToken(token)).toMatchObject({
      s: SESSION,
      t: `Progress`,
      l: ``,
      u: `user-1`,
      a: payload.id,
      i: 1,
      c: `The empty state`,
    })
    expect(payload.results).toEqual([])
    expect(h.db.update).not.toHaveBeenCalled()
  })
})

describe(`exponential_sessions_results`, () => {
  // Any run of the caller's gets the tool — attended included, unlike the
  // close-out.
  const OWN_RUN = {
    sessionsEnd: false,
    askParent: false,
    sessionResults: true,
  }
  const picture = (topic: string, label: string, attachmentId: string) => ({
    topic,
    label,
    attachmentId,
    width: 1600,
    height: 900,
  })
  const runRow = (over: Record<string, unknown> = {}) => ({
    id: SESSION,
    userId: `user-1`,
    hostUserId: null,
    status: `running`,
    results: null,
    ...over,
  })

  beforeEach(() => {
    // Real tokens are minted here (the signature IS the upload route's
    // credential); an earlier describe's unstubAllEnvs drops the file-level
    // secret, so re-stub it. The upload URL's origin falls back to the
    // request's when the app has no configured base URL.
    vi.stubEnv(`BETTER_AUTH_SECRET`, `mcp-tools-test-secret`)
    vi.stubEnv(`BETTER_AUTH_URL`, ``)
    deleteReturning.current = []
    // The remove path re-reads the row under `FOR UPDATE` inside a
    // transaction (the same lock the upload route takes) and writes through
    // the tx handle; the locked read answers with the same run row, and the
    // tx's update/delete are the db's own mocks so the assertions below see
    // them.
    h.db.transaction.mockImplementation(async (fn: (tx: unknown) => unknown) => {
      const txSelect: Record<string, unknown> = {}
      for (const method of [`from`, `where`, `limit`, `for`]) {
        txSelect[method] = () => txSelect
      }
      ;(txSelect as { then: unknown }).then = (
        resolve: (v: unknown) => unknown,
        reject: (e: unknown) => unknown
      ) => Promise.resolve(dbRows.current).then(resolve, reject)
      return fn({
        select: () => txSelect,
        update: h.db.update,
        delete: h.db.delete,
      })
    })
  })

  it(`is not registered without its gate`, () => {
    expect(
      collectTools(USER, SESSION, {
        sessionsEnd: true,
        askParent: true,
        sessionResults: false,
      }).has(`exponential_sessions_results`)
    ).toBe(false)
  })

  it(`refuses outside a launched session, naming the missing header`, async () => {
    const result = await collectTools(USER, null, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `chatui`, label: `web` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`X-Exp-Session-Id`)
  })

  it(`refuses a run that is neither owned nor hosted by the caller`, async () => {
    dbRows.current = [runRow({ userId: `user-2`, hostUserId: `user-3` })]
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `chatui`, label: `web` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not your run`)
  })

  it(`requires a label to publish a picture`, async () => {
    dbRows.current = [runRow()]
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `chatui` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`label is required`)
  })

  // The status gate lives HERE, not on the upload route: a token minted while
  // the run was live stays good for its ten minutes.
  it(`refuses to mint a link for an ended run`, async () => {
    dbRows.current = [runRow({ status: `ended` })]
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `chatui`, label: `web` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`ended`)
  })

  it(`mints a scoped upload link with a ready curl line and the run's list`, async () => {
    dbRows.current = [
      runRow({ results: [picture(`nav`, `web`, `att-1`)] }),
    ]
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `chatui`, label: `ios` })
    const payload = parseOk(result) as {
      uploadUrl: string
      curl: string
      expiresAt: string
      topic: string
      label: string
      results: unknown
    }
    expect(payload.uploadUrl.startsWith(`https://x.test/api/session-results/`)).toBe(
      true
    )
    expect(payload.curl).toBe(
      `curl -sS -F file=@screenshot.png "${payload.uploadUrl}"`
    )
    expect(Date.parse(payload.expiresAt)).toBeGreaterThan(Date.now())
    // The token carries the whole scope — it can write that picture and no
    // other.
    const token = payload.uploadUrl.split(`/`).pop()!
    expect(verifySessionResultToken(token)).toMatchObject({
      s: SESSION,
      t: `chatui`,
      l: `ios`,
      u: `user-1`,
    })
    // Every response carries what the run has published so far.
    expect(payload.results).toEqual([{ topic: `nav`, label: `web` }])
    expect(h.db.update).not.toHaveBeenCalled()
  })

  // EXP-933: the run's report text lands directly, no upload link.
  it(`files a topic's report text under the row lock, without a link`, async () => {
    dbRows.current = [runRow({ results: [picture(`nav`, `web`, `att-1`)] })]
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `Summary`, text: `  ## Done\n- shipped #EXP-1  ` })
    expect(parseOk(result)).toEqual({
      topic: `Summary`,
      text: 24,
      results: [
        { topic: `nav`, label: `web` },
        { topic: `Summary`, text: 24 },
      ],
    })
    expect(updateSet).toHaveBeenCalledWith(
      expect.objectContaining({
        results: [
          expect.objectContaining({ attachmentId: `att-1` }),
          expect.objectContaining({ topic: `Summary`, label: null, text: `## Done\n- shipped #EXP-1` }),
        ],
      })
    )
  })

  // EXP-1154: the report IS the PR body.
  it(`re-syncs the run's open PR after a text write and stores the files`, async () => {
    dbRows.current = [runRow({ results: [] })]
    vi.mocked(syncRunPrBody).mockResolvedValueOnce(`synced`)
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `Summary`, text: `Did it`, files: [`apps/web/a.ts`, ` apps/web/a.ts `] })
    expect(parseOk(result)).toMatchObject({ topic: `Summary`, pr: `synced` })
    expect(syncRunPrBody).toHaveBeenCalledWith(SESSION)
    expect(updateSet).toHaveBeenCalledWith(
      expect.objectContaining({
        results: [expect.objectContaining({ topic: `Summary`, files: [`apps/web/a.ts`] })],
      })
    )
  })

  it(`never syncs the PR for a picture`, async () => {
    dbRows.current = [runRow({ results: [] })]
    await collectTools(USER, SESSION, OWN_RUN).get(`exponential_sessions_results`)!({
      topic: `nav`,
      label: `web`,
    })
    expect(syncRunPrBody).not.toHaveBeenCalled()
  })

  it(`refuses files without text`, async () => {
    dbRows.current = [runRow({ results: [] })]
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `nav`, label: `web`, files: [`a.ts`] })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`files rides a topic's text`)
    expect(h.db.update).not.toHaveBeenCalled()
  })

  it(`files text and mints a picture link in one call`, async () => {
    dbRows.current = [runRow({ results: [] })]
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `nav`, text: `The nav`, label: `web` })
    const payload = parseOk(result) as { uploadUrl: string; results: unknown }
    expect(payload.uploadUrl).toContain(`/api/session-results/`)
    expect(payload.results).toEqual([{ topic: `nav`, text: 7 }])
  })

  it(`removes only a topic's text with remove + text ''`, async () => {
    dbRows.current = [
      runRow({
        results: [
          { topic: `nav`, label: null, attachmentId: null, width: null, height: null, text: `x` },
          picture(`nav`, `web`, `att-1`),
        ],
      }),
    ]
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `nav`, text: ``, remove: true })
    expect(parseOk(result)).toMatchObject({ removed: 1, results: [{ topic: `nav`, label: `web` }] })
    expect(h.deleteObject).not.toHaveBeenCalled()
    // EXP-1154: the report changed, so the PR body follows (a last text
    // gone shrinks it to the footer link).
    expect(syncRunPrBody).toHaveBeenCalledWith(SESSION, { removal: true })
  })

  it(`removes one label, reclaiming its row and its object`, async () => {
    dbRows.current = [
      runRow({
        results: [
          picture(`chatui`, `web`, `att-1`),
          picture(`chatui`, `ios`, `att-2`),
        ],
      }),
    ]
    deleteReturning.current = [{ storageKey: `sessions/att-2.png` }]

    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `chatui`, label: `ios`, remove: true })

    expect(parseOk(result)).toEqual({
      removed: 1,
      topic: `chatui`,
      label: `ios`,
      results: [{ topic: `chatui`, label: `web` }],
    })
    expect(updateSet).toHaveBeenCalledWith(
      expect.objectContaining({
        results: [expect.objectContaining({ attachmentId: `att-1` })],
      })
    )
    expect(h.deleteObject).toHaveBeenCalledWith(`sessions/att-2.png`)
    // No text went away under the lock: the PR body is untouched.
    expect(syncRunPrBody).not.toHaveBeenCalled()
  })

  it(`removes a whole topic when no label is given`, async () => {
    dbRows.current = [
      runRow({
        results: [
          picture(`chatui`, `web`, `att-1`),
          picture(`chatui`, `ios`, `att-2`),
          picture(`nav`, `web`, `att-3`),
        ],
      }),
    ]
    deleteReturning.current = [
      { storageKey: `sessions/att-1.png` },
      { storageKey: `sessions/att-2.png` },
    ]

    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `chatui`, remove: true })

    expect(parseOk(result)).toMatchObject({
      removed: 2,
      results: [{ topic: `nav`, label: `web` }],
    })
    expect(h.deleteObject).toHaveBeenCalledTimes(2)
  })

  it(`removes nothing for an unknown topic, and touches no storage`, async () => {
    dbRows.current = [runRow({ results: [picture(`nav`, `web`, `att-1`)] })]
    const result = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `chatui`, remove: true })
    expect(parseOk(result)).toMatchObject({ removed: 0 })
    expect(h.db.update).not.toHaveBeenCalled()
    expect(h.deleteObject).not.toHaveBeenCalled()
  })

  // The cap is refused BEFORE the agent goes off and takes a screenshot; the
  // upload route re-checks it under the row lock.
  it(`refuses a new picture once the run sits at the cap`, async () => {
    const full = Array.from({ length: 60 }, (_, index) =>
      picture(`t`, `l${index}`, `att-${index}`)
    )
    dbRows.current = [runRow({ results: full })]
    const refused = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `chatui`, label: `web` })
    expect(refused.isError).toBe(true)
    expect(refused.content[0].text).toContain(`60`)
    // Replacing one of the 60 is still allowed — the list does not grow.
    const replaced = await collectTools(USER, SESSION, OWN_RUN).get(
      `exponential_sessions_results`
    )!({ topic: `t`, label: `l7` })
    expect(replaced.isError).toBeFalsy()
  })
})

// ── EXP-700: the owner-scoped steer/answer rail ──────────────────────────────
describe(`exponential_sessions_message`, () => {
  const TARGET = `88888888-8888-4888-8888-888888888888`
  const RELAY = { url: `https://relay.test`, secret: `s` }

  const targetRow = (over: Record<string, unknown> = {}) => ({
    teamId: `ws-1`,
    boardId: `proj-1`,
    userId: `user-1`,
    hostUserId: null,
    status: `running`,
    parentSessionId: null,
    ...over,
  })

  it(`refuses messaging your own session`, async () => {
    const result = await collectTools(USER, SESSION).get(
      `exponential_sessions_message`
    )!({ id: SESSION, message: `hi` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`your own session`)
  })

  // FEED-60: the device ticker flips agent_busy once the injected text
  // starts a turn; here it is already set when the poll first reads.
  const deliverAndStartTurn = () =>
    vi.mocked(relayPostInput).mockImplementation(async () => {
      dbRows.current = [targetRow({ agentBusy: true })]
      return { delivered: true }
    })

  it(`injects with the starter prefix for a header-less caller`, async () => {
    dbRows.current = [targetRow()]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    deliverAndStartTurn()

    const result = await collectTools(USER, null).get(
      `exponential_sessions_message`
    )!({ id: TARGET, message: `Use staging.` })

    expect(relayPostInput).toHaveBeenCalledWith(
      RELAY,
      TARGET,
      `[Message from your starter via exponential_sessions_message] Use staging.`
    )
    expect(parseOk(result)).toEqual({
      ok: true,
      id: TARGET,
      delivered: true,
      consumed: true,
    })
  })

  it(`answers queued when the agent is already mid-turn`, async () => {
    dbRows.current = [targetRow({ agentBusy: true })]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    vi.mocked(relayPostInput).mockResolvedValue({ delivered: true })

    const result = await collectTools(USER, null).get(
      `exponential_sessions_message`
    )!({ id: TARGET, message: `hi` })

    expect(parseOk(result)).toEqual({
      ok: true,
      id: TARGET,
      delivered: true,
      queued: true,
      note: `The agent is mid-turn; the message lands when it reads it (claude replays it into the running turn).`,
    })
  })

  it(`answers consumed once the turn starts within the wait`, async () => {
    dbRows.current = [targetRow()]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    vi.mocked(relayPostInput).mockResolvedValue({ delivered: true })
    vi.useFakeTimers()
    try {
      const pending = collectTools(USER, null).get(
        `exponential_sessions_message`
      )!({ id: TARGET, message: `hi` })
      await vi.advanceTimersByTimeAsync(2_000)
      dbRows.current = [targetRow({ agentBusy: true })]
      await vi.advanceTimersByTimeAsync(1_000)
      const result = await pending
      expect(parseOk(result)).toEqual({
        ok: true,
        id: TARGET,
        delivered: true,
        consumed: true,
      })
    } finally {
      vi.useRealTimers()
    }
  })

  it(`answers consumed false when no turn starts within 6s`, async () => {
    dbRows.current = [targetRow()]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    vi.mocked(relayPostInput).mockResolvedValue({ delivered: true })
    vi.useFakeTimers()
    try {
      const pending = collectTools(USER, null).get(
        `exponential_sessions_message`
      )!({ id: TARGET, message: `hi` })
      await vi.advanceTimersByTimeAsync(7_000)
      const result = await pending
      expect(parseOk(result)).toEqual({
        ok: true,
        id: TARGET,
        delivered: true,
        consumed: false,
        note: `The run's agent did not start a turn within 6s: the text reached its device but nothing is reading it. Kill the run (exponential_sessions_kill) and start it again with resumeSessionId (the resumed run continues the transcript), then message the new run.`,
      })
    } finally {
      vi.useRealTimers()
    }
  })

  it(`uses the parent-answer prefix when answering its own child`, async () => {
    dbRows.current = [targetRow({ parentSessionId: SESSION })]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    deliverAndStartTurn()

    await collectTools(USER, SESSION).get(`exponential_sessions_message`)!({
      id: TARGET,
      message: `Use staging.`,
    })

    expect(relayPostInput).toHaveBeenCalledWith(
      RELAY,
      TARGET,
      `[Answer from your parent run ${SESSION.slice(0, 8)} via exponential_sessions_message] Use staging.`
    )
  })

  it(`refuses a session the caller neither owns nor hosts`, async () => {
    dbRows.current = [targetRow({ userId: `other`, hostUserId: `other-2` })]

    const result = await collectTools(USER, null).get(
      `exponential_sessions_message`
    )!({ id: TARGET, message: `hi` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`owner or host`)
    expect(relayPostInput).not.toHaveBeenCalled()
  })

  it(`refuses an ended session`, async () => {
    dbRows.current = [targetRow({ status: `ended` })]

    const result = await collectTools(USER, null).get(
      `exponential_sessions_message`
    )!({ id: TARGET, message: `hi` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not live`)
  })

  it(`hides an out-of-grant session as not found`, async () => {
    dbRows.current = [targetRow()]
    const confined: McpAccess = {
      full: false,
      fullTeamIds: new Set(),
      grantedBoardIds: new Set(),
      visibleTeamIds: new Set(),
    }

    const result = await collectTools(USER, null, ALL_MCP_TOOL_GATES, confined).get(
      `exponential_sessions_message`
    )!({ id: TARGET, message: `hi` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`Session not found`)
  })

  it(`errors when the relay cannot deliver`, async () => {
    dbRows.current = [targetRow()]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    vi.mocked(relayPostInput).mockResolvedValue({ delivered: false })

    const result = await collectTools(USER, null).get(
      `exponential_sessions_message`
    )!({ id: TARGET, message: `hi` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`Not delivered`)
  })
})

// ── EXP-626: a PR with no issue ──────────────────────────────────────────────
describe(`exponential_pr_open — repositoryId path`, () => {
  function armRepoPr(): Array<{ set: Record<string, unknown>; where: unknown }> {
    const updates: Array<{ set: Record<string, unknown>; where: unknown }> = []
    vi.mocked(loadRepositoryForTeam).mockResolvedValue({
      id: REPO,
      teamId: WS,
      fullName: `acme/app`,
      defaultBranch: `main`,
    })
    vi.mocked(resolveRepoInstallationTokenInfo).mockResolvedValue({
      token: `tok`,
      installationId: 42,
    } as never)
    vi.mocked(createPullRequest).mockResolvedValue({
      url: `https://github.com/acme/app/pull/9`,
      number: 9,
    } as never)
    db.transaction.mockImplementation(async (fn: (tx: unknown) => unknown) =>
      fn({
        update: () => ({
          set: (values: Record<string, unknown>) => ({
            where: async (cond: unknown) => {
              updates.push({ set: values, where: cond })
            },
          }),
        }),
      })
    )
    return updates
  }

  it(`opens the PR and links, moves and notifies NOTHING`, async () => {
    armRepoPr()

    const result = await collectTools(USER, null).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/refresh-screenshots-1a2b3c4d`,
      title: `Refresh screenshots`,
    })

    expect(parseOk(result)).toEqual({
      url: `https://github.com/acme/app/pull/9`,
      number: 9,
    })
    expect(createPullRequest).toHaveBeenCalledWith(
      expect.objectContaining({
        repo: `acme/app`,
        head: `exp/refresh-screenshots-1a2b3c4d`,
        base: `main`,
      })
    )
    // No issue exists, so nothing may be recorded against one.
    expect(recordIssueEvent).not.toHaveBeenCalled()
    expect(applyPrLifecycleStatusInTx).not.toHaveBeenCalled()
    expect(fireAndForgetPrNotify).not.toHaveBeenCalled()
    expect(noteAgentIssueActivity).not.toHaveBeenCalled()
    // And without a session header there is no row to park either — the
    // batch heuristic must NOT run here (it would hit an unrelated run).
    expect(db.transaction).not.toHaveBeenCalled()
  })

  // EXP-1105: yolo mode merges what pr_open just opened, through pr_merge.
  it(`leaves the PR open when the team is not in yolo mode`, async () => {
    armRepoPr()
    dbRows.current = [{ yoloMode: false }]

    const result = await collectTools(USER, null).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chore-1a2b3c4d`,
      title: `Chore`,
    })

    expect(parseOk(result)).not.toHaveProperty(`autoMerge`)
    expect(caller.repositories.mergePull).not.toHaveBeenCalled()
  })

  it(`merges the opened PR at once in yolo mode`, async () => {
    armRepoPr()
    dbRows.current = [{ yoloMode: true }]
    caller.repositories.mergePull.mockResolvedValue({ merged: true })

    const result = await collectTools(USER, null).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chore-1a2b3c4d`,
      title: `Chore`,
    })

    expect(caller.repositories.mergePull).toHaveBeenCalledWith({
      repositoryId: REPO,
      prNumber: 9,
    })
    expect(parseOk(result)).toEqual({
      url: `https://github.com/acme/app/pull/9`,
      number: 9,
      autoMerge: { merged: true },
    })
  })

  it(`still reports the opened PR when the yolo merge is refused`, async () => {
    armRepoPr()
    dbRows.current = [{ yoloMode: true }]
    caller.repositories.mergePull.mockRejectedValue(
      new Error(`Required status check "ci" is expected.`)
    )

    const result = await collectTools(USER, null).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chore-1a2b3c4d`,
      title: `Chore`,
    })

    expect(result.isError).toBeUndefined()
    expect(parseOk(result)).toMatchObject({
      url: `https://github.com/acme/app/pull/9`,
      number: 9,
      autoMerge: {
        merged: false,
        error: `Required status check "ci" is expected.`,
      },
    })
  })

  // EXP-1146: with a session header the PR belongs to a run that may still
  // start follow-up runs on this branch — nothing merges while it is busy;
  // the tree merge (fired again on its idle edge) lands it root first.
  it(`defers the yolo merge to the run's tree while the caller is busy`, async () => {
    armRepoPr()
    dbRows.current = [
      { id: SESSION, teamId: WS, status: `running`, userId: `user-1`, hostUserId: null, yoloMode: true },
    ]
    vi.mocked(maybeMergeYoloTree).mockResolvedValue({
      status: `incomplete`,
      blocking: [`Chat`],
    })

    const result = await collectTools(USER, SESSION).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chat-1a2b3c4d`,
      title: `Chore`,
    })

    expect(maybeMergeYoloTree).toHaveBeenCalledWith(SESSION)
    expect(caller.repositories.mergePull).not.toHaveBeenCalled()
    expect(parseOk(result)).toMatchObject({
      number: 9,
      autoMerge: { merged: false, deferred: true },
    })
    expect(
      (parseOk(result) as { autoMerge: { note: string } }).autoMerge.note
    ).toContain(`root first`)
  })

  // EXP-1146: no tree to merge (a run outside yolo mode)
  // means nothing will ever auto-merge this PR — no `deferred` promise.
  it(`returns the plain opened result when the run has no yolo tree`, async () => {
    armRepoPr()
    dbRows.current = [
      { id: SESSION, teamId: WS, status: `running`, userId: `user-1`, hostUserId: null, yoloMode: true },
    ]
    vi.mocked(maybeMergeYoloTree).mockResolvedValue({ status: `not_yolo` })

    const result = await collectTools(USER, SESSION).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chat-1a2b3c4d`,
      title: `Chore`,
    })

    expect(maybeMergeYoloTree).toHaveBeenCalledWith(SESSION)
    expect(caller.repositories.mergePull).not.toHaveBeenCalled()
    expect(parseOk(result)).toEqual({
      url: `https://github.com/acme/app/pull/9`,
      number: 9,
    })
    expect(parseOk(result)).not.toHaveProperty(`autoMerge`)
  })

  it(`reports the caller's own PR merged when its idle tree of one landed at pr_open`, async () => {
    armRepoPr()
    dbRows.current = [
      { id: SESSION, teamId: WS, status: `running`, userId: `user-1`, hostUserId: null, yoloMode: true },
    ]
    vi.mocked(maybeMergeYoloTree).mockResolvedValue({
      status: `merged`,
      mergedNow: 1,
      outcomes: { [SESSION]: { kind: `merged` } },
    })

    const result = await collectTools(USER, SESSION).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chat-1a2b3c4d`,
      title: `Chore`,
    })

    expect(parseOk(result)).toEqual({
      url: `https://github.com/acme/app/pull/9`,
      number: 9,
      autoMerge: { merged: true },
    })
  })

  it(`parks the EXACT header session in review, never a heuristic set`, async () => {
    const updates = armRepoPr()
    dbRows.current = [
      {
        id: SESSION,
        teamId: WS,
        status: `running`,
        userId: `user-1`,
        hostUserId: null,
      },
    ]

    await collectTools(USER, SESSION).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chat-1a2b3c4d`,
      title: `Chore`,
    })

    expect(updates).toHaveLength(2)
    expect(updates[0]!.set).toMatchObject({
      status: `in_review`,
      needsInput: false,
    })
    expect(updates[1]!.set).toMatchObject({
      branch: `exp/chat-1a2b3c4d`,
      // EXP-734: the run IS the link — the PR lands on the session row.
      prUrl: `https://github.com/acme/app/pull/9`,
      prNumber: 9,
      prState: `open`,
      // EXP-1165: with its base, so the merge guards see a stacked run PR.
      prBaseBranch: `main`,
    })
    for (const update of updates) {
      const { sql, params } = new PgDialect().sqlToQuery(update.where as never)
      // The row is pinned by id (never the pre-EXP-637 heuristic sweep).
      expect(sql).toContain(`"id" =`)
      expect(params).toContain(SESSION)
      expect(sql).not.toContain(`"issue_id" is null`)
    }
  })

  // SLOP-3: every run owns the PR it opens, an issue-scoped run's side chore
  // PR included — the url-keyed writers advance it from there.
  it(`stamps an issue-scoped caller's row on the chore path too`, async () => {
    const updates = armRepoPr()
    dbRows.current = [
      {
        id: SESSION,
        teamId: WS,
        issueId: UUID,
        branch: `exp/EXP-1`,
        status: `running`,
        userId: `user-1`,
        hostUserId: null,
      },
    ]

    await collectTools(USER, SESSION).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chat-1a2b3c4d`,
      title: `Chore`,
    })

    expect(updates[1]!.set).toMatchObject({
      prUrl: `https://github.com/acme/app/pull/9`,
      branch: `exp/chat-1a2b3c4d`,
    })
  })

  // SLOP-3: a team-A run opening team B's chore PR must not carry it — B's
  // merge would end the A run. The predicate is the repo's team, so the
  // update matches no row of another team; the PR itself still opens.
  it(`stamps only a caller row of the repository's own team`, async () => {
    const updates = armRepoPr()
    dbRows.current = [
      {
        id: SESSION,
        teamId: `other-team`,
        status: `running`,
        userId: `user-1`,
        hostUserId: null,
      },
    ]

    const result = await collectTools(USER, SESSION).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chat-1a2b3c4d`,
      title: `Chore`,
    })

    expect(parseOk(result)).toMatchObject({ number: 9 })
    for (const update of updates) {
      const { sql, params } = new PgDialect().sqlToQuery(update.where as never)
      expect(sql).toContain(`"team_id" =`)
      expect(params).toContain(WS)
      expect(params).not.toContain(`other-team`)
    }
  })

  it(`ignores a header naming somebody else's run`, async () => {
    const updates = armRepoPr()
    dbRows.current = [
      {
        id: SESSION,
        teamId: WS,
        status: `running`,
        userId: `someone-else`,
        hostUserId: null,
      },
    ]

    await collectTools(USER, SESSION).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chat-1a2b3c4d`,
      title: `Chore`,
    })

    expect(updates).toHaveLength(0)
  })

  // FEED-59: a head that already has an open PR links to it, never a 422.
  it(`parks the caller on the PR already open on head`, async () => {
    const updates = armRepoPr()
    vi.mocked(createPullRequest).mockRejectedValue(
      new PullAlreadyExistsError(`GitHub PR create failed (422)`)
    )
    vi.mocked(findOpenPullByHead).mockResolvedValue({
      url: `https://github.com/acme/app/pull/5`,
      number: 5,
      baseRef: `main`,
    })
    dbRows.current = [
      {
        id: SESSION,
        teamId: WS,
        status: `running`,
        userId: `user-1`,
        hostUserId: null,
      },
    ]

    const result = await collectTools(USER, SESSION).get(
      `exponential_pr_open`
    )!({ repositoryId: REPO, head: `exp/chat-1a2b3c4d`, title: `Chore` })

    expect(parseOk(result)).toMatchObject({
      url: `https://github.com/acme/app/pull/5`,
      number: 5,
      reused: true,
    })
    expect(findOpenPullByHead).toHaveBeenCalledWith(
      `acme/app`,
      `exp/chat-1a2b3c4d`,
      `tok`,
      undefined,
      undefined
    )
    expect(updates[1]!.set).toMatchObject({ prUrl: `https://github.com/acme/app/pull/5`, prNumber: 5 })
  })

  // EXP-1154: the run's report IS the PR body.
  it(`opens the PR with the body derived from the run's report`, async () => {
    armRepoPr()
    dbRows.current = [
      { id: SESSION, teamId: WS, status: `running`, userId: `user-1`, hostUserId: null },
    ]
    vi.mocked(runPrBody).mockResolvedValueOnce({ body: `From the report`, fromResults: true })

    const result = await collectTools(USER, SESSION).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chat-1a2b3c4d`,
      title: `Chore`,
      body: `Agent body`,
    })

    expect(runPrBody).toHaveBeenCalledWith(SESSION, `Agent body`)
    expect(createPullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ body: `From the report` })
    )
    expect(parseOk(result)).toMatchObject({ number: 9, body: `report` })
    // A NEW PR already carries the report: nothing to re-sync.
    expect(syncRunPrBody).not.toHaveBeenCalled()
  })

  it(`re-syncs a reused PR's body to the report`, async () => {
    armRepoPr()
    vi.mocked(findOpenPullByHead).mockResolvedValue({
      url: `https://github.com/acme/app/pull/5`,
      number: 5,
      baseRef: `main`,
    })
    dbRows.current = [
      { id: SESSION, teamId: WS, status: `running`, userId: `user-1`, hostUserId: null },
    ]
    vi.mocked(runPrBody).mockResolvedValueOnce({ body: `From the report`, fromResults: true })

    const result = await collectTools(USER, SESSION).get(`exponential_pr_open`)!({
      repositoryId: REPO,
      head: `exp/chat-1a2b3c4d`,
      title: `Chore`,
    })

    expect(parseOk(result)).toMatchObject({ number: 5, reused: true, body: `report` })
    // Only the PR the row got: never another PR on a skipped stamp.
    expect(syncRunPrBody).toHaveBeenCalledWith(SESSION, {
      expectPrUrl: `https://github.com/acme/app/pull/5`,
    })
  })

  it(`requires head, and refuses more than one subject`, async () => {
    armRepoPr()
    const prOpen = collectTools(USER, null).get(`exponential_pr_open`)!

    const noHead = await prOpen({ repositoryId: REPO, title: `x` })
    expect(noHead.isError).toBe(true)
    expect(noHead.content[0].text).toContain(`'head' is required`)

    const both = await prOpen({
      repositoryId: REPO,
      issueId: UUID,
      head: `exp/x`,
      title: `x`,
    })
    expect(both.isError).toBe(true)
    expect(both.content[0].text).toContain(`exactly one`)
    expect(createPullRequest).not.toHaveBeenCalled()
  })
})

// ── EXP-626/EXP-637: merging without an issue, and surviving your own merge ──
describe(`exponential_pr_merge — repository path and the self-merge spare`, () => {
  it(`delegates a repositoryId + prNumber merge to repositories.mergePull`, async () => {
    caller.repositories.mergePull.mockResolvedValue({ merged: true })

    const result = await collectTools(USER, null).get(
      `exponential_pr_merge`
    )!({ repositoryId: REPO, prNumber: 9 })

    expect(parseOk(result)).toEqual({
      results: [{ repositoryId: REPO, prNumber: 9, merged: true }],
    })
    expect(caller.repositories.mergePull).toHaveBeenCalledWith({
      repositoryId: REPO,
      prNumber: 9,
    })
  })

  // EXP-1165: GitHub's merge queue only took it, nothing landed yet.
  it(`reports a queued chore merge as merged=false, queued`, async () => {
    caller.repositories.mergePull.mockResolvedValue({
      merged: false,
      queued: true,
      note: `GitHub queued the merge of PR #9.`,
    })

    const result = await collectTools(USER, null).get(
      `exponential_pr_merge`
    )!({ repositoryId: REPO, prNumber: 9 })

    expect(parseOk(result)).toEqual({
      results: [{ repositoryId: REPO, prNumber: 9, merged: false, queued: true }],
    })
  })

  it(`refuses repositoryId without prNumber, and a second subject`, async () => {
    const prMerge = collectTools(USER, null).get(`exponential_pr_merge`)!

    const half = await prMerge({ repositoryId: REPO })
    expect(half.isError).toBe(true)
    const both = await prMerge({ repositoryId: REPO, prNumber: 9, issueId: UUID })
    expect(both.isError).toBe(true)
    expect(caller.repositories.mergePull).not.toHaveBeenCalled()
  })

  // Records every db.update() the tool makes: the stamp and, when the merge
  // did not land, its revert.
  function captureUpdates(): Array<{
    set: Record<string, unknown>
    where: unknown
  }> {
    const updates: Array<{ set: Record<string, unknown>; where: unknown }> = []
    db.update.mockImplementation(() => ({
      set: (values: Record<string, unknown>) => ({
        where: async (cond: unknown) => {
          updates.push({ set: values, where: cond })
        },
      }),
    }))
    return updates
  }

  // The drizzle stub serves ONE row set per await, so stage the tool's reads
  // in call order: 1 = the header session (loadCallerSession), 2 = the issues
  // it was asked to merge.
  function stageSelects(staged: Array<Array<unknown>>): () => void {
    const builder = db.select()
    db.select.mockClear()
    let call = 0
    db.select.mockImplementation(() => {
      dbRows.current = staged[call] ?? []
      call += 1
      return builder
    })
    return () => db.select.mockImplementation(() => builder)
  }

  // The chore-PR own-merge test: `repositoryId + prNumber` is matched against
  // the PR `exponential_pr_open` stamped on the caller's own run (EXP-734).
  // `CHORE_BRANCH` is the branch that run sits on; `ownChorePr()` gives it the
  // PR row shape pr_open leaves behind.
  const CHORE_BRANCH = `exp/chore-1a2b3c4d`
  const ownChorePr = (prNumber = 9) => ({
    branch: CHORE_BRANCH,
    prUrl: `https://github.com/acme/app/pull/${prNumber}`,
    prNumber,
  })
  function stageChoreRepo(): void {
    vi.mocked(loadRepositoryForTeam).mockResolvedValue({
      repositoryId: REPO,
      teamId: WS,
      fullName: `acme/app`,
      defaultBranch: `main`,
    } as never)
  }

  const runRow = (over: Record<string, unknown> = {}) => ({
    id: SESSION,
    teamId: WS,
    issueId: null,
    branch: null,
    prUrl: null,
    prNumber: null,
    status: `in_review`,
    needsInput: false,
    mergedOwnPr: false,
    userId: `user-1`,
    hostUserId: null,
    ...over,
  })

  it(`stamps merged_own_pr on the header session BEFORE merging (decision 6)`, async () => {
    const updates = captureUpdates()
    // An issue-less run parked on the chore PR it opened — the only row
    // shape a merge-driven end can reach.
    dbRows.current = [runRow(ownChorePr())]
    stageChoreRepo()
    caller.repositories.mergePull.mockResolvedValue({ merged: true })

    await collectTools(USER, SESSION).get(`exponential_pr_merge`)!({
      repositoryId: REPO,
      prNumber: 9,
    })

    expect(updates).toHaveLength(1)
    expect(updates[0]!.set).toMatchObject({
      mergedOwnPr: true,
      // Back to `running`: the agent is still working, and the badge must
      // not read "in review" after its own PR landed.
      status: `running`,
      needsInput: false,
    })
    const { params } = new PgDialect().sqlToQuery(updates[0]!.where as never)
    expect(params).toContain(SESSION)
  })

  it(`spares nothing when an ISSUE run lands a chore PR (EXP-639)`, async () => {
    const updates = captureUpdates()
    dbRows.current = [runRow({ issueId: UUID })]
    caller.repositories.mergePull.mockResolvedValue({ merged: true })

    await collectTools(USER, SESSION).get(`exponential_pr_merge`)!({
      repositoryId: REPO,
      prNumber: 9,
    })

    expect(caller.repositories.mergePull).toHaveBeenCalled()
    expect(updates).toHaveLength(0)
  })

  it(`reverts the stamp when the chore merge fails (EXP-639)`, async () => {
    const updates = captureUpdates()
    dbRows.current = [runRow(ownChorePr())]
    stageChoreRepo()
    caller.repositories.mergePull.mockRejectedValue(
      new TRPCError({ code: `PRECONDITION_FAILED`, message: `not mergeable` })
    )

    const result = await collectTools(USER, SESSION).get(
      `exponential_pr_merge`
    )!({ repositoryId: REPO, prNumber: 9 })

    expect(result.isError).toBe(true)
    expect(updates).toHaveLength(2)
    expect(updates[1]!.set).toMatchObject({
      mergedOwnPr: false,
      status: `in_review`,
      needsInput: false,
    })
  })

  it(`stamps nothing without a header session`, async () => {
    const updates = captureUpdates()
    caller.repositories.mergePull.mockResolvedValue({ merged: true })

    await collectTools(USER, null).get(`exponential_pr_merge`)!({
      repositoryId: REPO,
      prNumber: 9,
    })

    expect(updates).toHaveLength(0)
    // No stampable row ⇒ no reason to resolve the repo at all.
    expect(loadRepositoryForTeam).not.toHaveBeenCalled()
  })

  // The durable spare filters EVERY merge-driven end, so a chat/batch/action
  // run that lands somebody else's chore PR must not get one — it would also
  // survive the merge of its own PR, and then nothing would ever end it.
  it(`spares nothing when an issue-less run lands a FOREIGN chore PR`, async () => {
    const updates = captureUpdates()
    // The run's own PR is #7; #9 belongs to somebody else.
    dbRows.current = [runRow(ownChorePr(7))]
    stageChoreRepo()
    caller.repositories.mergePull.mockResolvedValue({ merged: true })

    await collectTools(USER, SESSION).get(`exponential_pr_merge`)!({
      repositoryId: REPO,
      prNumber: 9,
    })

    expect(caller.repositories.mergePull).toHaveBeenCalled()
    expect(updates).toHaveLength(0)
  })

  it(`leaves the stamp off when the repo lookup fails`, async () => {
    const updates = captureUpdates()
    dbRows.current = [runRow(ownChorePr())]
    vi.mocked(loadRepositoryForTeam).mockRejectedValue(
      new Error(`GitHub returned 502`)
    )
    caller.repositories.mergePull.mockResolvedValue({ merged: true })

    await collectTools(USER, SESSION).get(`exponential_pr_merge`)!({
      repositoryId: REPO,
      prNumber: 9,
    })

    // Being ended by a merge is recoverable; a run nothing ends is not.
    expect(caller.repositories.mergePull).toHaveBeenCalled()
    expect(updates).toHaveLength(0)
  })

  // EXP-639: `merged_own_pr` is DURABLE — stamping it for a PR the run does
  // not own would also spare the row from the later merge of its own PR, so
  // the issue path matches the merged PR against the run's own issue/branch.
  it(`stamps when the run merges the PR of its OWN issue`, async () => {
    const updates = captureUpdates()
    caller.issues.mergePr.mockResolvedValue({ merged: true })
    const restore = stageSelects([
      [runRow({ issueId: UUID })],
      [
        {
          id: UUID,
          identifier: `MET-1`,
          prUrl: `https://github.com/acme/app/pull/9`,
          branch: `exp/MET-1`,
        },
      ],
    ])

    try {
      const result = await collectTools(USER, SESSION).get(
        `exponential_pr_merge`
      )!({ issueId: UUID })
      expect(parseOk(result)).toMatchObject({
        results: [{ issueId: UUID, identifier: `MET-1`, merged: true }],
      })
    } finally {
      restore()
    }

    expect(updates).toHaveLength(1)
    expect(updates[0]!.set).toMatchObject({
      mergedOwnPr: true,
      status: `running`,
    })
  })

  // FEED-43 R1: a queued merge reports `merged: false, queued: true`; the
  // spare stays, since the landing merge is still the run's own.
  it(`reports an enqueued own-PR merge as queued and keeps the spare`, async () => {
    const updates = captureUpdates()
    caller.issues.mergePr.mockResolvedValue({
      merged: false,
      queued: true,
      note: `GitHub queued the merge of PR #9. Nothing is merged yet; the issue completes when the merge lands.`,
    })
    const restore = stageSelects([
      [runRow({ issueId: UUID })],
      [
        {
          id: UUID,
          identifier: `MET-1`,
          prUrl: `https://github.com/acme/app/pull/9`,
          branch: `exp/MET-1`,
        },
      ],
    ])

    try {
      const result = await collectTools(USER, SESSION).get(
        `exponential_pr_merge`
      )!({ issueId: UUID })
      const ok = parseOk(result) as {
        results: Array<{ merged: boolean; queued?: boolean; note?: string }>
      }
      expect(ok.results).toEqual([
        {
          issueId: UUID,
          identifier: `MET-1`,
          merged: false,
          queued: true,
          note: `GitHub queued the merge of PR #9. Nothing is merged yet; the issue completes when the merge lands.`,
        },
      ])
    } finally {
      restore()
    }

    // The stamp, and NO revert.
    expect(updates).toHaveLength(1)
    expect(updates[0]!.set).toMatchObject({ mergedOwnPr: true })
  })

  it(`stamps when a BATCH run merges the PR on its own branch`, async () => {
    const updates = captureUpdates()
    caller.issues.mergePr.mockResolvedValue({ merged: true })
    const restore = stageSelects([
      [runRow({ branch: `exp/batch-1a2b3c4d` })],
      [
        {
          id: UUID,
          identifier: `MET-1`,
          prUrl: `https://github.com/acme/app/pull/9`,
          branch: `exp/batch-1a2b3c4d`,
        },
      ],
    ])

    try {
      await collectTools(USER, SESSION).get(`exponential_pr_merge`)!({
        issueId: UUID,
      })
    } finally {
      restore()
    }

    expect(updates).toHaveLength(1)
    expect(updates[0]!.set).toMatchObject({ mergedOwnPr: true })
  })

  it(`stamps NOTHING when the run merges somebody else's PR`, async () => {
    const updates = captureUpdates()
    caller.issues.mergePr.mockResolvedValue({ merged: true })
    const restore = stageSelects([
      // A run on its own issue + branch, asked to land an unrelated PR.
      [runRow({ issueId: RUN, branch: `exp/batch-1a2b3c4d` })],
      [
        {
          id: UUID,
          identifier: `MET-1`,
          prUrl: `https://github.com/acme/app/pull/9`,
          branch: `exp/MET-1`,
        },
      ],
    ])

    try {
      await collectTools(USER, SESSION).get(`exponential_pr_merge`)!({
        issueId: UUID,
      })
    } finally {
      restore()
    }

    expect(caller.issues.mergePr).toHaveBeenCalledWith({ issueId: UUID })
    expect(updates).toHaveLength(0)
  })

  // EXP-711: the per-call override of the team's end-sessions-on-merge
  // setting rides the tRPC input on both paths, and only when given.
  it(`forwards endSessions to the tRPC merge on both paths`, async () => {
    captureUpdates()
    caller.issues.mergePr.mockResolvedValue({ merged: true })
    const restore = stageSelects([
      [runRow({ issueId: RUN, branch: `exp/batch-1a2b3c4d` })],
      [
        {
          id: UUID,
          identifier: `MET-1`,
          prUrl: `https://github.com/acme/app/pull/9`,
          branch: `exp/MET-1`,
        },
      ],
    ])
    try {
      await collectTools(USER, SESSION).get(`exponential_pr_merge`)!({
        issueId: UUID,
        endSessions: false,
      })
    } finally {
      restore()
    }
    expect(caller.issues.mergePr).toHaveBeenCalledWith({
      issueId: UUID,
      endSessions: false,
    })

    caller.repositories.mergePull.mockResolvedValue({ merged: true })
    await collectTools(USER, null).get(`exponential_pr_merge`)!({
      repositoryId: REPO,
      prNumber: 9,
      endSessions: true,
    })
    expect(caller.repositories.mergePull).toHaveBeenCalledWith({
      repositoryId: REPO,
      prNumber: 9,
      endSessions: true,
    })
  })

  it(`leaves the row untouched when its own merge fails`, async () => {
    const updates = captureUpdates()
    caller.issues.mergePr.mockRejectedValue(
      new TRPCError({ code: `PRECONDITION_FAILED`, message: `not mergeable` })
    )
    const restore = stageSelects([
      [runRow({ issueId: UUID })],
      [
        {
          id: UUID,
          identifier: `MET-1`,
          prUrl: `https://github.com/acme/app/pull/9`,
          branch: `exp/MET-1`,
        },
      ],
    ])

    try {
      const result = await collectTools(USER, SESSION).get(
        `exponential_pr_merge`
      )!({ issueId: UUID })
      // The per-item failure is a result, never a thrown call.
      expect(parseOk(result)).toMatchObject({
        results: [{ issueId: UUID, merged: false }],
      })
    } finally {
      restore()
    }

    expect(updates).toHaveLength(2)
    expect(updates[0]!.set).toMatchObject({ mergedOwnPr: true })
    expect(updates[1]!.set).toMatchObject({
      mergedOwnPr: false,
      status: `in_review`,
      needsInput: false,
    })
  })

  // A stack merge that stops part-way fails its TARGET, yet the members
  // below landed. The run's own PR among them keeps the spare (EXP-637).
  it(`keeps the stamp when a partial stack merge landed the run's own PR`, async () => {
    const OWN = `77777777-7777-4777-8777-777777777777`
    const stackMember = (n: number, issueId: string) => ({
      issueId,
      identifier: `MET-${n}`,
      boardId: `proj-1`,
      prNumber: n,
      prUrl: `https://github.com/acme/app/pull/${n}`,
      branch: `exp/MET-${n}`,
      prBaseBranch: n === 1 ? `main` : `exp/MET-${n - 1}`,
    })
    const updates = captureUpdates()
    vi.mocked(openStackThrough).mockResolvedValueOnce([
      stackMember(1, `issue-1`),
      stackMember(2, OWN),
      stackMember(3, UUID),
    ])
    caller.issues.mergePr.mockRejectedValue(
      new TRPCError({
        code: `CONFLICT`,
        message: `Merged MET-1, MET-2. MET-3 (#3) stopped the stack: conflicts`,
      })
    )
    const restore = stageSelects([
      // The run sits on the MIDDLE member; it is asked to land the top one.
      [runRow({ issueId: OWN })],
      [
        {
          id: UUID,
          identifier: `MET-3`,
          prUrl: `https://github.com/acme/app/pull/3`,
          branch: `exp/MET-3`,
        },
      ],
      // The re-read before the revert: the run's own PR did land.
      [{ id: OWN }],
    ])

    try {
      const result = await collectTools(USER, SESSION).get(
        `exponential_pr_merge`
      )!({ issueId: UUID, mergeStack: true })
      expect(parseOk(result)).toMatchObject({
        results: [{ issueId: UUID, merged: false }],
      })
    } finally {
      restore()
    }

    // The stamp, and NO revert.
    expect(updates).toHaveLength(1)
    expect(updates[0]!.set).toMatchObject({ mergedOwnPr: true })
    // The re-read asks for the run's OWN PR only, merged.
    const { params } = renderWhere()
    expect(params).toContain(`https://github.com/acme/app/pull/2`)
    expect(params).toContain(`merged`)
    expect(params).not.toContain(`https://github.com/acme/app/pull/3`)
  })

  it(`reverts the stamp when a failed stack merge left the run's own PR open`, async () => {
    const OWN = `77777777-7777-4777-8777-777777777777`
    const updates = captureUpdates()
    vi.mocked(openStackThrough).mockResolvedValueOnce([
      {
        issueId: OWN,
        identifier: `MET-2`,
        boardId: `proj-1`,
        prNumber: 2,
        prUrl: `https://github.com/acme/app/pull/2`,
        branch: `exp/MET-2`,
        prBaseBranch: `main`,
      },
      {
        issueId: UUID,
        identifier: `MET-3`,
        boardId: `proj-1`,
        prNumber: 3,
        prUrl: `https://github.com/acme/app/pull/3`,
        branch: `exp/MET-3`,
        prBaseBranch: `exp/MET-2`,
      },
    ])
    caller.issues.mergePr.mockRejectedValue(
      new TRPCError({ code: `CONFLICT`, message: `MET-2 (#2): conflicts` })
    )
    const restore = stageSelects([
      [runRow({ issueId: OWN })],
      [
        {
          id: UUID,
          identifier: `MET-3`,
          prUrl: `https://github.com/acme/app/pull/3`,
          branch: `exp/MET-3`,
        },
      ],
      // The re-read: nothing of the run's own is merged.
      [],
    ])

    try {
      await collectTools(USER, SESSION).get(`exponential_pr_merge`)!({
        issueId: UUID,
        mergeStack: true,
      })
    } finally {
      restore()
    }

    expect(updates).toHaveLength(2)
    expect(updates[1]!.set).toMatchObject({
      mergedOwnPr: false,
      status: `in_review`,
    })
  })
})

// ── EXP-660: the deferred families ───────────────────────────────────────────

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

// EXP-639: a consent grant confined to ONE board of a team the connection can
// otherwise see (the host team stays visible for label/member aux reads).
const SCOPED_TO_BOARD: McpAccess = {
  full: false,
  fullTeamIds: new Set(),
  grantedBoardIds: new Set([PROJ]),
  visibleTeamIds: new Set([WS]),
}

const SERVER_ONLY_SESSION_COLUMNS = [
  `hostUserId`,
  `mergedOwnPr`,
  `boardDeletedAt`,
  `boardArchivedAt`,
  // EXP-862: the close-out summary is reported to whoever started the run,
  // never stored and never projected.
  `summary`,
]

// SLOP-3: mergeStack rides through to issues.mergePr; the members below the
// target are checked against an OAuth grant before GitHub sees anything.
describe(`exponential_pr_merge mergeStack`, () => {
  const member = (n: number, boardId: string) => ({
    issueId: `issue-${n}`,
    identifier: `MET-${n}`,
    boardId,
    prNumber: n,
    prUrl: `https://github.com/acme/app/pull/${n}`,
    branch: `exp/MET-${n}`,
    prBaseBranch: n === 1 ? `main` : `exp/MET-${n - 1}`,
  })

  it(`passes mergeStack through and reports the landed stack`, async () => {
    vi.mocked(openStackThrough).mockResolvedValueOnce([
      member(1, `proj-1`),
      member(2, `proj-1`),
    ])
    caller.issues.mergePr.mockResolvedValue({
      merged: true,
      stack: [
        { identifier: `MET-1`, prNumber: 1 },
        { identifier: `MET-2`, prNumber: 2 },
      ],
    })

    const result = await collectTools(USER, null).get(`exponential_pr_merge`)!({
      issueId: UUID,
      mergeStack: true,
    })

    expect(caller.issues.mergePr).toHaveBeenCalledWith({
      issueId: UUID,
      mergeStack: true,
    })
    expect(parseOk(result)).toMatchObject({
      results: [{ issueId: UUID, merged: true, stack: [`MET-1`, `MET-2`] }],
    })
  })

  it(`leaves mergeStack off the plain merge`, async () => {
    caller.issues.mergePr.mockResolvedValue({ merged: true })

    await collectTools(USER, null).get(`exponential_pr_merge`)!({ issueId: UUID })

    expect(caller.issues.mergePr).toHaveBeenCalledWith({ issueId: UUID })
    expect(openStackThrough).not.toHaveBeenCalled()
  })

  it(`refuses a stack reaching a board the token was never granted`, async () => {
    vi.mocked(openStackThrough).mockResolvedValueOnce([
      member(1, `proj-secret`),
      member(2, `proj-1`),
    ])
    const scoped: McpAccess = {
      full: false,
      fullTeamIds: new Set(),
      grantedBoardIds: new Set([`proj-1`]),
      visibleTeamIds: new Set([`ws-1`]),
    }

    const result = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      scoped
    ).get(`exponential_pr_merge`)!({ issueId: UUID, mergeStack: true })

    expect(result.isError).toBe(true)
    expect(caller.issues.mergePr).not.toHaveBeenCalled()
  })

  it(`refuses mergeStack on a chore PR`, async () => {
    const result = await collectTools(USER, null).get(`exponential_pr_merge`)!({
      repositoryId: REPO,
      prNumber: 9,
      mergeStack: true,
    })

    expect(result.isError).toBe(true)
    expect(caller.repositories.mergePull).not.toHaveBeenCalled()
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

describe(`exponential_sessions_list`, () => {
  it(`projects the shape allowlist only and scopes like the shape`, async () => {
    dbRows.current = [{ id: RUN, status: `running` }]
    const result = await tool(`exponential_sessions_list`)({
      teamId: WS,
      mine: true,
      status: `running`,
      limit: 50,
      offset: 0,
    })
    expect(parseOk(result)).toEqual([{ id: RUN, status: `running` }])
    expect(membership.resolveTeamAccess).toHaveBeenCalledWith(`user-1`, WS)

    // The stub's select is typed without params; the tool passes the
    // projection object as its first argument.
    const selectCalls = db.select.mock.calls as unknown as Array<
      [Record<string, unknown>]
    >
    const projection = Object.keys(selectCalls[0]![0])
    for (const column of SERVER_ONLY_SESSION_COLUMNS) {
      expect(projection).not.toContain(column)
    }
    // FEED-63: agentBusy/agentCaption = the working signal beside status.
    for (const column of [`id`, `issueId`, `issueIdentifier`, `endedBy`, `branch`, `deviceId`, `prUrl`, `prNumber`, `prState`, `agentBusy`, `agentCaption`, `parentSessionId`]) {
      expect(projection).toContain(column)
    }

    const { sql, params } = renderWhere()
    expect(sql).toContain(`"team_id" in`)
    expect(sql).toContain(`"board_deleted_at" is null`)
    expect(sql).toContain(`"board_archived_at" is null`)
    expect(sql).toContain(`"status" =`)
    // `mine` = started by me OR hosted on my machine; host_user_id is
    // WHERE-only, never projected.
    expect(sql).toContain(`"user_id" =`)
    expect(sql).toContain(`"host_user_id" =`)
    expect(params).toContain(WS)
    expect(params).toContain(`user-1`)
  })

  it(`spans every visible team when teamId is omitted`, async () => {
    membership.getUserTeamIds.mockResolvedValue([WS, PROJ])
    dbRows.current = []
    const result = await tool(`exponential_sessions_list`)({
      mine: false,
      limit: 50,
      offset: 0,
    })
    expect(parseOk(result)).toEqual([])
    const { params } = renderWhere()
    expect(params).toContain(WS)
    expect(params).toContain(PROJ)
    expect(membership.resolveTeamAccess).not.toHaveBeenCalled()
  })

  it(`returns [] without a query when the caller has no teams`, async () => {
    membership.getUserTeamIds.mockResolvedValue([])
    const result = await tool(`exponential_sessions_list`)({
      mine: false,
      limit: 50,
      offset: 0,
    })
    expect(parseOk(result)).toEqual([])
    expect(db.select).not.toHaveBeenCalled()
  })

  // EXP-639: team visibility is what a board grant hands out for aux reads —
  // never a licence to list the team's OTHER boards' runs.
  it(`filters a board-confined grant down to that board, in SQL`, async () => {
    dbRows.current = []
    await collectTools(USER, null, ALL_MCP_TOOL_GATES, SCOPED_TO_BOARD).get(
      `exponential_sessions_list`
    )!({ teamId: WS, mine: false, limit: 50, offset: 0 })

    const { sql, params } = renderWhere()
    expect(sql).toContain(`"board_id" in`)
    expect(params).toContain(PROJ)
  })

  it(`keeps a whole-team grant unfiltered, so issue-less runs still list`, async () => {
    dbRows.current = []
    await collectTools(USER, null, ALL_MCP_TOOL_GATES, SCOPED_TO_WS).get(
      `exponential_sessions_list`
    )!({ teamId: WS, mine: false, limit: 50, offset: 0 })

    // A batch/action/chat row carries board_id NULL — a board predicate would
    // drop it, so a full-team grant must not add one.
    const { sql } = renderWhere()
    expect(sql).not.toContain(`"board_id" in`)
  })

  // The board-less arm: the runs a board grant can start must stay listable
  // by the person who started them.
  it(`admits the caller's own board-less runs under a board grant`, async () => {
    dbRows.current = []
    await collectTools(USER, null, ALL_MCP_TOOL_GATES, SCOPED_TO_BOARD).get(
      `exponential_sessions_list`
    )!({ teamId: WS, mine: false, limit: 50, offset: 0 })

    const { sql, params } = renderWhere()
    expect(sql).toContain(`"board_id" is null`)
    expect(sql).toContain(`"user_id" =`)
    expect(sql).toContain(`"host_user_id" =`)
    expect(params).toContain(`user-1`)
    // Still scoped to the teams the grant makes visible.
    expect(params).toContain(WS)
  })

  it(`returns [] without a query when the grant covers nothing`, async () => {
    const empty: McpAccess = {
      full: false,
      fullTeamIds: new Set(),
      grantedBoardIds: new Set(),
      visibleTeamIds: new Set([WS]),
    }
    const result = await collectTools(USER, null, ALL_MCP_TOOL_GATES, empty).get(
      `exponential_sessions_list`
    )!({ teamId: WS, mine: false, limit: 50, offset: 0 })
    expect(parseOk(result)).toEqual([])
    expect(db.select).not.toHaveBeenCalled()
  })

  it(`denies a non-member and an ungranted team before querying`, async () => {
    membership.resolveTeamAccess.mockRejectedValue(
      new TRPCError({ code: `FORBIDDEN`, message: `not a member` })
    )
    const denied = await tool(`exponential_sessions_list`)({
      teamId: WS,
      mine: false,
      limit: 50,
      offset: 0,
    })
    expect(denied.isError).toBe(true)
    expect(denied.content[0].text).toContain(`not a member`)
    expect(db.select).not.toHaveBeenCalled()

    membership.resolveTeamAccess.mockResolvedValue(undefined)
    const scoped = await collectTools(USER, null, ALL_MCP_TOOL_GATES, SCOPED_TO_WS).get(
      `exponential_sessions_list`
    )!({ teamId: PROJ, mine: false, limit: 50, offset: 0 })
    expect(scoped.isError).toBe(true)
    expect(db.select).not.toHaveBeenCalled()
  })
})

describe(`exponential_sessions_get`, () => {
  it(`returns the projected row and checks membership for a teammate's run`, async () => {
    dbRows.current = [{ id: RUN, userId: `user-2`, teamId: WS, status: `ended`, endedBy: `agent` }]
    const result = await tool(`exponential_sessions_get`)({ id: RUN })
    expect(parseOk(result)).toMatchObject({ id: RUN, endedBy: `agent` })
    expect(membership.resolveTeamAccess).toHaveBeenCalledWith(`user-1`, WS)
  })

  // FEED-63: in_review says "PR open"; agentBusy says "working now".
  it(`carries agentBusy beside the PR state`, async () => {
    dbRows.current = [
      { id: RUN, userId: `user-1`, teamId: WS, status: `in_review`, agentBusy: true },
    ]
    const result = await tool(`exponential_sessions_get`)({ id: RUN })
    expect(parseOk(result)).toMatchObject({ status: `in_review`, agentBusy: true })
    const selectCalls = db.select.mock.calls as unknown as Array<
      [Record<string, unknown>]
    >
    expect(Object.keys(selectCalls[0]![0])).toContain(`agentBusy`)
  })

  it(`skips the membership lookup for the caller's own run`, async () => {
    dbRows.current = [{ id: RUN, userId: `user-1`, teamId: WS, status: `running` }]
    const result = await tool(`exponential_sessions_get`)({ id: RUN })
    expect(parseOk(result)).toMatchObject({ id: RUN })
    expect(membership.resolveTeamAccess).not.toHaveBeenCalled()
  })

  it(`reports a missing (or trashed-board) row as not found`, async () => {
    dbRows.current = []
    const result = await tool(`exponential_sessions_get`)({ id: RUN })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`Session not found`)
  })

  // EXP-639: the grant confines the caller's OWN runs too — a board-confined
  // connection reading a sibling board's run is out of scope, not "mine".
  it(`hides a sibling board's run from a board-confined grant`, async () => {
    dbRows.current = [
      { id: RUN, userId: `user-1`, teamId: WS, boardId: `other-board`, status: `running` },
    ]
    const result = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_sessions_get`)!({ id: RUN })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`Session not found`)
  })

  it(`serves the granted board's run`, async () => {
    dbRows.current = [
      { id: RUN, userId: `user-2`, teamId: WS, boardId: PROJ, status: `running` },
    ]
    const result = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_sessions_get`)!({ id: RUN })
    expect(parseOk(result)).toMatchObject({ id: RUN })
    expect(membership.resolveTeamAccess).toHaveBeenCalledWith(`user-1`, WS)
  })

  // EXP-639: a board grant may START a batch run (its issues all sit on the
  // granted board) and such a row carries board_id NULL — so the caller's OWN
  // board-less runs stay readable inside a visible team. A teammate's do not.
  it(`serves the caller's own issue-less run under a board grant`, async () => {
    dbRows.current = [
      {
        id: RUN,
        userId: `user-1`,
        teamId: WS,
        boardId: null,
        status: `running`,
      },
    ]
    const result = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_sessions_get`)!({ id: RUN })
    expect(parseOk(result)).toMatchObject({ id: RUN })
  })

  it(`hides a teammate's issue-less run from a board grant`, async () => {
    dbRows.current = [
      {
        id: RUN,
        userId: `user-2`,
        hostUserId: `user-3`,
        teamId: WS,
        boardId: null,
        status: `running`,
      },
    ]
    const denied = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_sessions_get`)!({ id: RUN })
    expect(denied.isError).toBe(true)
    expect(denied.content[0].text).toContain(`Session not found`)

    // A whole-team grant sees it (subject to membership, as ever).
    dbRows.current = [
      {
        id: RUN,
        userId: `user-2`,
        teamId: WS,
        boardId: null,
        status: `running`,
      },
    ]
    const allowed = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_WS
    ).get(`exponential_sessions_get`)!({ id: RUN })
    expect(parseOk(allowed)).toMatchObject({ id: RUN })
  })

  it(`never returns the server-only host_user_id`, async () => {
    dbRows.current = [
      { id: RUN, userId: `user-1`, teamId: WS, hostUserId: `user-9` },
    ]
    expect(parseOk(await tool(`exponential_sessions_get`)({ id: RUN })))
      .not.toHaveProperty(`hostUserId`)
  })

  // EXP-933: a text entry has no attachment, so it carries only `topic` +
  // `text` (no attachment fields); pictures keep their label and url.
  it(`a text result carries only topic and text; pictures keep label and url`, async () => {
    dbRows.current = [
      {
        id: RUN,
        userId: `user-1`,
        teamId: WS,
        status: `ended`,
        results: [
          { topic: `nav`, label: `web`, attachmentId: `att-1`, width: 1600, height: 900 },
          { topic: `nav`, label: null, attachmentId: null, width: null, height: null, text: `# Report` },
        ],
      },
    ]
    const payload = parseOk(await tool(`exponential_sessions_get`)({ id: RUN })) as {
      results: Array<Record<string, unknown>>
    }
    expect(payload.results).toEqual([
      {
        topic: `nav`,
        label: `web`,
        url: expect.stringMatching(/\/api\/attachments\/att-1$/),
      },
      { topic: `nav`, text: `# Report` },
    ])
    expect(payload.results[1]).not.toHaveProperty(`label`)
    expect(JSON.stringify(payload.results)).not.toContain(`null`)
  })
})

describe(`exponential_sessions_kill`, () => {
  it(`refuses to kill the caller's own header session`, async () => {
    const result = await collectTools(USER, RUN).get(`exponential_sessions_kill`)!({
      id: RUN,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`your own session`)
    expect(caller.steer.killSession).not.toHaveBeenCalled()
  })

  it(`kills another run from inside a session`, async () => {
    caller.steer.killSession.mockResolvedValue({
      session: { id: UUID, status: `ended`, endedAt: null },
    })
    const result = await collectTools(USER, RUN).get(`exponential_sessions_kill`)!({
      id: UUID,
    })
    expect(parseOk(result)).toEqual({ ok: true, id: UUID, status: `ended`, endedAt: null })
    expect(caller.steer.killSession).toHaveBeenCalledWith({ sessionId: UUID })
  })

  // FEED-63: the device's last worktree report says the kill leaves work
  // behind; the answer names it (best-effort, never failing the kill).
  it(`notes uncommitted work the run's worktree reported`, async () => {
    const reportedAt = new Date(`2026-09-30T10:00:00Z`)
    caller.steer.killSession.mockResolvedValue({
      session: {
        id: UUID,
        status: `ended`,
        endedAt: null,
        userId: `user-1`,
        hostUserId: null,
        deviceId: `mac-1`,
        branch: `exp/EXP-1`,
      },
    })
    dbRows.current = [{ dirty: `tracked`, reportedAt }]
    const result = await collectTools(USER, RUN).get(`exponential_sessions_kill`)!({
      id: UUID,
    })
    expect(parseOk(result)).toMatchObject({
      note: `The run's worktree on its device reported uncommitted changes (tracked, as of 2026-09-30T10:00:00.000Z); the device saves them as a WIP commit on exp/EXP-1 when it tears the run down.`,
    })
    const { params } = renderWhere()
    expect(params).toContain(`mac-1`)
    expect(params).toContain(`exp/EXP-1`)
    expect(params).toContain(`user-1`)
  })

  it(`adds no note for a clean worktree or no report`, async () => {
    caller.steer.killSession.mockResolvedValue({
      session: {
        id: UUID,
        status: `ended`,
        endedAt: null,
        userId: `user-1`,
        deviceId: `mac-1`,
        branch: `exp/EXP-1`,
      },
    })
    dbRows.current = [{ dirty: `clean`, reportedAt: new Date() }]
    const clean = await collectTools(USER, RUN).get(`exponential_sessions_kill`)!({
      id: UUID,
    })
    expect(parseOk(clean)).not.toHaveProperty(`note`)
    dbRows.current = []
    const none = await collectTools(USER, RUN).get(`exponential_sessions_kill`)!({
      id: UUID,
    })
    expect(parseOk(none)).not.toHaveProperty(`note`)
  })

  // EXP-700: a killed child never sends its own close-out, so the kill is
  // what has to tell the parent — otherwise it waits forever.
  it(`tells a live parent that the killed child ended without a report`, async () => {
    caller.steer.killSession.mockResolvedValue({
      session: { id: UUID, status: `ended`, endedAt: null },
      txId: 42,
    })
    await collectTools(USER, RUN).get(`exponential_sessions_kill`)!({
      id: UUID,
    })
    expect(notifyParentOfChildEnd).toHaveBeenCalledWith(db, UUID, {
      summary: null,
      endedBy: `user`,
    })
  })

  // Idempotent kill: the row was already ended (no txId), so the parent was
  // told by whichever path ended it — never twice.
  it(`does not notify again when the run was already ended`, async () => {
    caller.steer.killSession.mockResolvedValue({
      session: { id: UUID, status: `ended`, endedAt: null },
    })
    await collectTools(USER, RUN).get(`exponential_sessions_kill`)!({
      id: UUID,
    })
    expect(notifyParentOfChildEnd).not.toHaveBeenCalled()
  })

  it(`checks the run's team against a scoped grant before delegating`, async () => {
    dbRows.current = [{ teamId: PROJ, boardId: null }]
    const result = await collectTools(USER, null, ALL_MCP_TOOL_GATES, SCOPED_TO_WS).get(
      `exponential_sessions_kill`
    )!({ id: UUID })
    expect(result.isError).toBe(true)
    expect(caller.steer.killSession).not.toHaveBeenCalled()
  })

  // EXP-639: same predicate as the read side — a visible team is not a
  // licence to kill runs on its other boards.
  it(`refuses a sibling board's run under a board-confined grant`, async () => {
    dbRows.current = [{ teamId: WS, boardId: `other-board` }]
    const denied = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_sessions_kill`)!({ id: UUID })
    expect(denied.isError).toBe(true)
    expect(caller.steer.killSession).not.toHaveBeenCalled()

    caller.steer.killSession.mockResolvedValue({
      session: { id: UUID, status: `ended`, endedAt: null },
    })
    dbRows.current = [{ teamId: WS, boardId: PROJ }]
    const allowed = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_sessions_kill`)!({ id: UUID })
    expect(parseOk(allowed)).toMatchObject({ ok: true, id: UUID })
  })

  it(`kills the caller's own board-less run, never a teammate's`, async () => {
    dbRows.current = [
      { teamId: WS, boardId: null, userId: `user-2`, hostUserId: `user-3` },
    ]
    const denied = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_sessions_kill`)!({ id: UUID })
    expect(denied.isError).toBe(true)
    expect(caller.steer.killSession).not.toHaveBeenCalled()

    caller.steer.killSession.mockResolvedValue({
      session: { id: UUID, status: `ended`, endedAt: null },
    })
    dbRows.current = [
      { teamId: WS, boardId: null, userId: `user-1`, hostUserId: null },
    ]
    const allowed = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_sessions_kill`)!({ id: UUID })
    expect(parseOk(allowed)).toMatchObject({ ok: true, id: UUID })
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
      },
    ])
    // No teamId ⇒ no shared join, so nothing is gated on team membership.
    expect(membership.assertTeamMember).not.toHaveBeenCalled()
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

describe(`exponential_sessions_start`, () => {
  const startedRow = { id: RUN, status: `running`, issueId: UUID, deviceId: `mac-1` }

  it(`resolves an identifier, starts over the steer rails and returns the run`, async () => {
    caller.steer.startSession.mockResolvedValue({ ok: true })
    // Every select resolves to dbRows.current at await time, so stage the
    // rows per call: 1 = the issue by identifier (the shared resolver's one
    // select; a FULL-access caller skips the boards lookup), 2+ = the poll
    // for the device-created row.
    const builder = db.select()
    db.select.mockClear()
    let call = 0
    db.select.mockImplementation(() => {
      call += 1
      dbRows.current = call === 1 ? [{ id: UUID }] : [startedRow]
      return builder
    })

    let result: ToolResult
    try {
      result = await tool(`exponential_sessions_start`)({
        deviceId: `mac-1`,
        issueId: `exp-7`,
        agent: `codex`,
        prompt: `Keep the tokens.`,
      })
    } finally {
      db.select.mockImplementation(() => builder)
    }

    expect(parseOk(result)).toEqual({
      ok: true,
      deviceId: `mac-1`,
      sessionId: RUN,
      session: startedRow,
    })
    // EXP-825: the free text rides the start as `prompt`.
    expect(caller.steer.startSession).toHaveBeenCalledWith(
      expect.objectContaining({
        deviceId: `mac-1`,
        issueId: UUID,
        agent: `codex`,
        prompt: `Keep the tokens.`,
      })
    )
    const { sql, params } = renderWhere()
    expect(sql).toContain(`"user_id" =`)
    expect(sql).toContain(`"device_id" =`)
    expect(sql).toContain(`"status" =`)
    expect(sql).toContain(`"created_at" >=`)
    expect(sql).toContain(`"issue_id" =`)
    expect(params).toContain(UUID)
    expect(params).toContain(`mac-1`)
  })

  // EXP-679: a run started from inside a run records the parent, so a chain
  // of agent-started runs is readable after the fact.
  it(`links the child run to the calling session, and only then`, async () => {
    caller.steer.startSession.mockResolvedValue({ ok: true })
    dbRows.current = [{ ...startedRow }]

    await collectTools(USER, RUN).get(`exponential_sessions_start`)!({
      deviceId: `mac-1`,
      issueId: UUID,
    })
    expect(caller.steer.startSession).toHaveBeenCalledWith(
      expect.objectContaining({ parentSessionId: RUN })
    )
    expect(db.update).toHaveBeenCalled()

    caller.steer.startSession.mockClear()
    dbRows.current = [{ ...startedRow }]
    await collectTools(USER, null).get(`exponential_sessions_start`)!({
      deviceId: `mac-1`,
      issueId: UUID,
    })
    expect(caller.steer.startSession.mock.calls[0][0]).not.toHaveProperty(
      `parentSessionId`
    )
  })

  // EXP-906: the profile rides the start like every other option — an
  // orchestrator whose default profile is walled no longer has to route
  // around this tool (and lose the parent link) to launch elsewhere.
  it(`passes the account profile through to the steer start`, async () => {
    caller.steer.startSession.mockResolvedValue({ ok: true })
    dbRows.current = [{ ...startedRow }]

    await collectTools(USER, RUN).get(`exponential_sessions_start`)!({
      deviceId: `mac-1`,
      issueId: UUID,
      account: `dennis`,
    })
    expect(caller.steer.startSession).toHaveBeenCalledWith(
      expect.objectContaining({
        account: `dennis`,
        parentSessionId: RUN,
      })
    )
  })

  // FEED-57/46: a start the device never turned into a run is an ERROR,
  // never `ok` with a null id the caller would wait on.
  it(`errors when the device never reports the run`, async () => {
    caller.steer.startSession.mockResolvedValue({ ok: true })
    dbRows.current = []
    vi.useFakeTimers()
    try {
      const pending = tool(`exponential_sessions_start`)({
        deviceId: `mac-1`,
        issueId: UUID,
      })
      await vi.advanceTimersByTimeAsync(12_000)
      const result = await pending
      expect(result.isError).toBe(true)
      expect(result.content[0].text).toBe(
          `Device mac-1 took the start but reported no run within 10s. Check that it is online (exponential_devices_list), its app or daemon log, and whether a live run already holds the issue (exponential_sessions_list). Check exponential_sessions_list before starting again: the run may still appear.`
      )
    } finally {
      vi.useRealTimers()
    }
    expect(caller.steer.startSession).toHaveBeenCalledTimes(1)
  })

  // FEED-63: the device reported why it could not launch: named at once,
  // never the generic timeout.
  it(`names the device's reported start failure`, async () => {
    const startId = mintStartId(`user-1`)
    caller.steer.startSession.mockResolvedValue({ ok: true, startId })
    dbRows.current = []
    vi.useFakeTimers()
    try {
      const pending = tool(`exponential_sessions_start`)({
        deviceId: `mac-1`,
        issueId: UUID,
      })
      await vi.advanceTimersByTimeAsync(1_000)
      recordStartFailure({
        startId,
        userId: `user-1`,
        reason: `git checkout refused: uncommitted changes`,
      })
      await vi.advanceTimersByTimeAsync(600)
      const result = await pending
      expect(result.isError).toBe(true)
      expect(result.content[0].text).toBe(
        `Device mac-1 could not start the run: git checkout refused: uncommitted changes`
      )
    } finally {
      vi.useRealTimers()
    }
  })

  // FEED-57: a builtin chat/action row (issue-less, action-less, named) the
  // same user starts on the same device must never pass for the batch run.
  it(`matches a batch start's row by its covered set, never a named builtin row`, async () => {
    membership.getIssueTeamContext.mockResolvedValue({ teamId: WS, boardId: PROJ })
    caller.steer.startSession.mockResolvedValue({ ok: true })
    dbRows.current = [{ id: RUN, status: `running` }]
    await tool(`exponential_sessions_start`)({ deviceId: `mac-1`, issueIds: [UUID, RUN] })
    const { sql, params } = renderWhere()
    expect(sql).toContain(`"issue_id" is null`)
    expect(sql).toContain(`"action_id" is null`)
    expect(sql).toContain(`"action_name" is null`)
    expect(sql).toContain(`"batch_issue_ids" ?|`)
    expect(params).toContainEqual([UUID, RUN])
  })

  it(`surfaces an offline device (relay 404) as an MCP error`, async () => {
    caller.steer.startSession.mockRejectedValue(
      new TRPCError({ code: `PRECONDITION_FAILED`, message: `device_offline` })
    )
    const result = await tool(`exponential_sessions_start`)({
      deviceId: `mac-1`,
      issueId: UUID,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`device_offline`)
  })

  it(`refuses an ungranted board before touching the relay`, async () => {
    membership.getIssueTeamContext.mockResolvedValue({ teamId: PROJ, boardId: `other-board` })
    const result = await collectTools(USER, null, ALL_MCP_TOOL_GATES, SCOPED_TO_WS).get(
      `exponential_sessions_start`
    )!({ deviceId: `mac-1`, issueId: UUID })
    expect(result.isError).toBe(true)
    expect(caller.steer.startSession).not.toHaveBeenCalled()
  })

  it(`requires exactly one subject`, async () => {
    const result = await tool(`exponential_sessions_start`)({ deviceId: `mac-1` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`Exactly one of`)
    expect(caller.steer.startSession).not.toHaveBeenCalled()
  })

  // EXP-639: start and read must agree. A board grant may batch the issues on
  // its board — the row that produces carries board_id NULL, and the read side
  // admits it because it is the caller's own (see sessions_get/list/kill).
  it(`lets a board grant batch its own board's issues`, async () => {
    membership.getIssueTeamContext.mockResolvedValue({
      teamId: WS,
      boardId: PROJ,
    })
    caller.steer.startSession.mockResolvedValue({ ok: true })
    dbRows.current = [{ id: RUN, status: `running` }]

    const result = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_sessions_start`)!({
      deviceId: `mac-1`,
      issueIds: [UUID, RUN],
    })

    expect(parseOk(result)).toMatchObject({ ok: true, sessionId: RUN })
    expect(caller.steer.startSession).toHaveBeenCalled()
  })

  // The resume branch used to demand a WHOLE-team grant, which no board grant
  // could ever satisfy for the very runs it had just started.
  it(`resumes the caller's own board-less run under a board grant`, async () => {
    caller.steer.startSession.mockResolvedValue({ ok: true })
    dbRows.current = [
      {
        id: RUN,
        status: `running`,
        teamId: WS,
        boardId: null,
        userId: `user-1`,
        hostUserId: null,
      },
    ]

    const result = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_sessions_start`)!({
      deviceId: `mac-1`,
      resumeSessionId: RUN,
    })

    expect(parseOk(result)).toMatchObject({ ok: true, sessionId: RUN })
  })

  it(`refuses to resume a teammate's board-less run`, async () => {
    dbRows.current = [
      {
        id: RUN,
        teamId: WS,
        boardId: null,
        userId: `user-2`,
        hostUserId: `user-3`,
      },
    ]

    const denied = await collectTools(
      USER,
      null,
      ALL_MCP_TOOL_GATES,
      SCOPED_TO_BOARD
    ).get(`exponential_sessions_start`)!({
      deviceId: `mac-1`,
      resumeSessionId: RUN,
    })

    expect(denied.isError).toBe(true)
    expect(denied.content[0].text).toContain(`Session not found`)
    expect(caller.steer.startSession).not.toHaveBeenCalled()
  })
})

// SLOP-2: an action's triggers ride `actions_update` as a whole array.
describe(`exponential_actions_update triggers`, () => {
  it(`rejects a malformed trigger before calling the router`, async () => {
    const result = await tool(`exponential_actions_update`)({
      id: AUTO,
      triggers: [{ deviceId: `mac-1`, kind: `nope` }],
    })
    expect(result.isError).toBe(true)
    expect(caller.actions.update).not.toHaveBeenCalled()
  })

  it(`forwards the parsed triggers, defaults filled`, async () => {
    caller.actions.update.mockResolvedValue({ action: { id: AUTO }, txId: 1 })
    const result = await tool(`exponential_actions_update`)({
      id: AUTO,
      triggers: [
        { deviceId: `mac-1`, kind: `schedule`, interval: `daily`, minuteOfDay: 540 },
        { deviceId: `mac-1`, enabled: false, kind: `event`, event: `pr_merged` },
      ],
    })
    expect(parseOk(result)).toEqual({ id: AUTO })
    expect(caller.actions.update).toHaveBeenCalledWith({
      id: AUTO,
      triggers: [
        {
          deviceId: `mac-1`,
          enabled: true,
          kind: `schedule`,
          interval: `daily`,
          minuteOfDay: 540,
        },
        {
          deviceId: `mac-1`,
          enabled: false,
          kind: `event`,
          source: `exponential`,
          event: `pr_merged`,
        },
      ],
    })
  })

  it(`leaves the triggers alone when none are passed`, async () => {
    caller.actions.update.mockResolvedValue({ action: { id: AUTO }, txId: 1 })
    await tool(`exponential_actions_update`)({ id: AUTO, name: `Sweep` })
    expect(caller.actions.update).toHaveBeenCalledWith({
      id: AUTO,
      name: `Sweep`,
      triggers: undefined,
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
    const registered = names.map((name) => name.slice(prefix.length)).sort()
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

// ── EXP-897 / FEED-43: stacks over MCP ───────────────────────────────────────
// `pr_open` records the stack edge (and the `blocks` relation behind it),
// `pr_merge` lands a whole chain in one call, `ask_parent` escalates past a
// parent that cannot decide, and the session list nests.
describe(`exponential_pr_open — a follow-up run based on its parent's branch`, () => {
  const LOWER_ISSUE = `aaaaaaaa-1111-4111-8111-111111111111`
  const LOWER_PR_URL = `https://github.com/acme/app/pull/241`

  function armPrOpen(): Array<{ set: Record<string, unknown>; where: unknown }> {
    const updates: Array<{ set: Record<string, unknown>; where: unknown }> = []
    caller.repositories.forIssue.mockResolvedValue({
      repositoryId: REPO,
      fullName: `acme/app`,
      defaultBranch: `main`,
    })
    vi.mocked(resolveRepoInstallationTokenInfo).mockResolvedValue({
      token: `tok`,
      installationId: 42,
    } as never)
    vi.mocked(createPullRequest).mockResolvedValue({
      url: `https://github.com/acme/app/pull/242`,
      number: 242,
    } as never)
    db.transaction.mockImplementation(async (fn: (tx: unknown) => unknown) => {
      const txSelect: Record<string, unknown> = {}
      for (const method of [`from`, `where`, `limit`]) {
        txSelect[method] = () => txSelect
      }
      ;(txSelect as { then: unknown }).then = (
        resolve: (v: unknown) => unknown,
        reject: (e: unknown) => unknown
      ) => Promise.resolve([{ status: `backlog` }]).then(resolve, reject)
      const tx: Record<string, unknown> = {
        select: () => txSelect,
        update: () => ({
          set: (values: Record<string, unknown>) => ({
            where: async (cond: unknown) => {
              updates.push({ set: values, where: cond })
            },
          }),
        }),
      }
      // SLOP-3: the implicit `blocks` write runs in a savepoint.
      tx.transaction = async (inner: (sp: unknown) => unknown) => {
        savepoints.push(tx)
        return inner(tx)
      }
      return fn(tx)
    })
    return updates
  }
  const savepoints: unknown[] = []

  beforeEach(() => {
    dbRows.current = []
    savepoints.length = 0
    vi.mocked(insertRelationInTx).mockResolvedValue(null)
  })

  // SLOP-3: a SQL error aborts the transaction it runs in; the savepoint
  // keeps it off the one carrying the issue link for a PR already open.
  it(`a failed blocks write never aborts the issue link`, async () => {
    const updates = armPrOpen()
    dbRows.current = [
      {
        id: LOWER_ISSUE,
        identifier: `EXP-11`,
        teamId: `ws-1`,
        prNumber: 241,
        prState: `open`,
        prUrl: LOWER_PR_URL,
      },
    ]
    vi.mocked(insertRelationInTx).mockRejectedValueOnce(
      new Error(`duplicate key value violates unique constraint`)
    )
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})

    const result = await tool(`exponential_pr_open`)({
      issueId: UUID,
      title: `Upper`,
      head: `exp/EXP-12`,
      base: `exp/EXP-11`,
    })

    expect(parseOk(result)).toMatchObject({ number: 242 })
    expect(savepoints).toHaveLength(1)
    expect(insertRelationInTx).toHaveBeenCalledTimes(1)
    // The link records the base the PR was opened against.
    expect(
      updates.find((u) => u.set.prUrl === `https://github.com/acme/app/pull/242`)
        ?.set
    ).toMatchObject({ prBaseBranch: `exp/EXP-11` })
    expect(warn).toHaveBeenCalled()
    warn.mockRestore()
  })

  it(`refuses the retired stackOnIssueId`, () => {
    const schema = collectToolDefs().get(`exponential_pr_open`)!.inputSchema!
    expect(
      schema.safeParse({ issueId: UUID, title: `t`, stackOnIssueId: LOWER_ISSUE })
        .success
    ).toBe(false)
  })

  it(`a base that IS a teammate's open PR branch makes that issue block this one`, async () => {
    armPrOpen()
    dbRows.current = [
      {
        id: LOWER_ISSUE,
        identifier: `EXP-11`,
        teamId: `ws-1`,
        prNumber: 241,
        prState: `open`,
        prUrl: LOWER_PR_URL,
      },
    ]

    const result = await tool(`exponential_pr_open`)({
      issueId: UUID,
      title: `Upper`,
      head: `exp/EXP-12`,
      base: `exp/EXP-11`,
    })

    expect(parseOk(result)).toMatchObject({ number: 242, base: `exp/EXP-11` })
    expect(vi.mocked(createPullRequest).mock.calls.at(-1)![0]).toMatchObject({
      base: `exp/EXP-11`,
    })
    expect(insertRelationInTx).toHaveBeenCalledTimes(1)
    expect(insertRelationInTx).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        issueId: LOWER_ISSUE,
        relatedIssueId: UUID,
        type: `blocks`,
        source: `reference`,
      })
    )
  })

  it(`refuses a base that is the branch of an already-merged PR`, async () => {
    armPrOpen()
    dbRows.current = [
      {
        id: LOWER_ISSUE,
        identifier: `EXP-11`,
        teamId: `ws-1`,
        prNumber: 241,
        prState: `merged`,
        prUrl: LOWER_PR_URL,
      },
    ]

    const result = await tool(`exponential_pr_open`)({
      issueId: UUID,
      title: `Upper`,
      head: `exp/EXP-12`,
      base: `exp/EXP-11`,
    })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toBe(
      `'exp/EXP-11' is the branch of merged PR #241 (EXP-11). Rebase onto main and pass no base.`
    )
    expect(createPullRequest).not.toHaveBeenCalled()
  })
})

describe(`exponential_sessions_ask_parent — targets`, () => {
  const AGENT_CHILD = { sessionsEnd: true, askParent: true, sessionResults: true }
  const RELAY = { url: `https://relay.test`, secret: `s` }
  const PARENT = `77777777-7777-4777-8777-777777777777`

  const childRow = (over: Record<string, unknown> = {}) => ({
    id: SESSION,
    userId: `user-1`,
    hostUserId: null,
    teamId: WS,
    startedReason: `agent`,
    parentSessionId: PARENT,
    actionName: null,
    issueIdentifier: `EXP-12`,
    parentStatus: `running`,
    ...over,
  })

  beforeEach(() => {
    executeRows.current = []
    vi.mocked(sendAgentMessage).mockReset()
    vi.mocked(sendAgentMessage).mockResolvedValue({
      delivered: [`user-1`],
      declined: [],
      notMembers: [],
      deduped: [],
    } as never)
  })

  it(`refuses the retired to: 'root'`, () => {
    const schema = collectToolDefs().get(`exponential_sessions_ask_parent`)!
      .inputSchema!
    expect(schema.safeParse({ question: `q`, to: `root` }).success).toBe(false)
    expect(schema.safeParse({ question: `q`, to: `user` }).success).toBe(true)
  })

  it(`to: 'user' parks the run and notifies its owner`, async () => {
    dbRows.current = [childRow()]

    const result = await collectTools(USER, SESSION, AGENT_CHILD).get(
      `exponential_sessions_ask_parent`
    )!({ question: `Ship it without tests?`, to: `user` })

    expect(db.update).toHaveBeenCalled()
    expect(sendAgentMessage).toHaveBeenCalledWith({
      teamId: WS,
      senderUserId: `user-1`,
      recipientIds: [`user-1`],
      title: `EXP-12 asks`,
      body: `Ship it without tests?`,
    })
    expect(parseOk(result)).toMatchObject({ delivered: true, to: `user` })
    // The relay is not involved: the answer comes back in this run's own
    // composer.
    expect(relayPostInput).not.toHaveBeenCalled()
  })

  it(`to: 'parent' stays byte-identical to the pre-EXP-897 message`, async () => {
    dbRows.current = [childRow()]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    vi.mocked(relayPostInput).mockResolvedValue({ delivered: true })

    await collectTools(USER, SESSION, AGENT_CHILD).get(
      `exponential_sessions_ask_parent`
    )!({ question: `Which env?` })

    expect(relayPostInput).toHaveBeenCalledWith(
      RELAY,
      PARENT,
      `[Exponential child run EXP-12 ${SESSION.slice(0, 8)} asks — reply with exponential_sessions_message sessionId=${SESSION}] Which env?`
    )
  })
})
