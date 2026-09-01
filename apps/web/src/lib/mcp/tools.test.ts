import { beforeEach, describe, expect, it, vi } from "vitest"
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
    comments: { update: vi.fn(), delete: vi.fn() },
    subscriptions: { subscribe: vi.fn(), unsubscribe: vi.fn() },
    notifications: { markRead: vi.fn(), markAllRead: vi.fn() },
    repositories: {
      list: vi.fn(),
      add: vi.fn(),
      branchDiff: vi.fn(),
      forIssue: vi.fn(),
      // EXP-626: the issue-less merge path.
      mergePull: vi.fn(),
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
    },
    boards: { delete: vi.fn(), setRepository: vi.fn() },
    teams: { create: vi.fn(), update: vi.fn() },
    teamInvites: { create: vi.fn(), list: vi.fn(), revoke: vi.fn() },
    attachments: { delete: vi.fn() },
    // EXP-660: the deferred families.
    statuses: { create: vi.fn(), update: vi.fn(), delete: vi.fn() },
    automations: { list: vi.fn(), update: vi.fn(), delete: vi.fn() },
    steer: { killSession: vi.fn(), startSession: vi.fn() },
    helpdesk: {
      listThreads: vi.fn(),
      getThread: vi.fn(),
      reply: vi.fn(),
      note: vi.fn(),
      close: vi.fn(),
      reopen: vi.fn(),
      escalate: vi.fn(),
    },
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
  const dbUpdate = vi.fn(
    (): {
      set: (values: Record<string, unknown>) => {
        where: (cond?: unknown) => Promise<unknown>
      }
    } => ({ set: () => ({ where: async () => undefined }) })
  )

  const db = {
    select: vi.fn(() => queryBuilder),
    insert: vi.fn(() => ({ values: insertValues })),
    update: dbUpdate,
    transaction: vi.fn(),
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
      contentType: `image/png`,
      storageKey: `k`,
    })),
    getUserTeamIds: vi.fn(async () => [`ws-1`]),
    getPublicTeamIds: vi.fn(async () => []),
  }

  const uploadObject = vi.fn(async () => undefined)
  const deleteObject = vi.fn(async () => undefined)
  const assertWithinStorageLimit = vi.fn(async () => undefined)
  const createAgentBugReport = vi.fn(async () => ({
    issueId: `bug-issue-1`,
    identifier: `EXP-1`,
  }))

  return {
    caller,
    dbRows,
    state,
    insertValues,
    db,
    membership,
    uploadObject,
    deleteObject,
    assertWithinStorageLimit,
    createAgentBugReport,
  }
})

const {
  caller,
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
  getObject: vi.fn(),
}))

vi.mock(`@/lib/storage/image-dimensions`, () => ({
  getImageDimensions: vi.fn(() => ({ width: 12, height: 8 })),
}))

vi.mock(`@/lib/billing`, () => ({
  assertWithinStorageLimit: h.assertWithinStorageLimit,
}))

// pr_open-only deps — mocked so the module import stays side-effect free.
vi.mock(`@/lib/integrations/github-pr`, () => ({
  createPullRequest: vi.fn(),
  // EXP-639: pr_merge reads a chore PR's head ref to decide whether the
  // caller is merging its OWN PR.
  getPullRequest: vi.fn(),
}))
vi.mock(`@/lib/integrations/github-app`, () => ({
  resolveRepoInstallationToken: vi.fn(),
  resolveRepoInstallationTokenInfo: vi.fn(),
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  isInstallationLinkedToTeam: vi.fn(),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrLifecycleStatusInTx: vi.fn(),
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
}))
vi.mock(`@/lib/widget/agent-report`, () => ({
  createAgentBugReport: h.createAgentBugReport,
}))
// EXP-626/EXP-637: the issue-less PR path and the agent close-out.
vi.mock(`@/lib/trpc/repositories`, () => ({
  loadRepositoryForTeam: vi.fn(),
}))
vi.mock(`@/lib/coding-session-end`, () => ({ endSessionByAgent: vi.fn() }))
// EXP-700: the relay injection rail. Partial mocks — the formatters and the
// one-select lookup stay real (the lookup runs against the drizzle stub), so
// ask_parent/message tests exercise the actual message convention.
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  getSteerRelayConfig: vi.fn(),
  relayPostInput: vi.fn(),
}))
vi.mock(`@/lib/steer-child-messages`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  notifyParentOfChildEnd: vi.fn(),
}))

import { loadRepositoryForTeam } from "@/lib/trpc/repositories"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { applyPrLifecycleStatusInTx } from "@/lib/integrations/pr-sync"
import { fireAndForgetPrNotify } from "@/lib/integrations/notifications"
import { noteAgentIssueActivity } from "@/lib/integrations/pr-actor-claims"
import { endSessionByAgent } from "@/lib/coding-session-end"
import { getSteerRelayConfig, relayPostInput } from "@/lib/steer"
import { notifyParentOfChildEnd } from "@/lib/steer-child-messages"
import {
  createPullRequest,
  getPullRequest,
} from "@/lib/integrations/github-pr"
import { resolveRepoInstallationTokenInfo } from "@/lib/integrations/github-app"
import { isInstallationLinkedToTeam } from "@/lib/trpc/integrations"
import { registerExponentialTools } from "@/lib/mcp/tools"
import {
  builtinCreateAction,
  builtinFixConflictsAction,
} from "@/lib/builtin-actions"
import { FULL_ACCESS, type McpAccess } from "@/lib/mcp/scope"
import { ALL_MCP_TOOL_GATES, type McpToolGates } from "@/lib/mcp/gates"
import type { McpUser } from "@/lib/mcp/server"

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
const THREAD = `88888888-8888-8888-8888-888888888888`
const AUTO = `99999999-9999-9999-9999-999999999999`
const RUN = `66666666-6666-6666-6666-666666666666`
const HELPDESK_ROWS = [{ teamId: WS, helpdeskEnabled: true }]

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
    expected: { retargeted: true, base: `master` },
    calledWith: { issueId: UUID, base: `master` },
  },
  {
    tool: `exponential_boards_delete`,
    pick: () => caller.boards.delete,
    args: { boardId: PROJ },
    resolved: { ok: true, txId: 1 },
    expected: { ok: true, boardId: PROJ },
    calledWith: { boardId: PROJ },
  },
  {
    tool: `exponential_boards_set_repository`,
    pick: () => caller.boards.setRepository,
    args: { boardId: PROJ, repositoryId: REPO },
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
    calledWith: { id: WS, name: `Renamed` },
  },
  {
    tool: `exponential_invites_create`,
    pick: () => caller.teamInvites.create,
    args: { teamId: WS, role: `member` },
    resolved: { invite: { id: INV }, token: `tok-abc` },
    expected: { invite: { id: INV }, token: `tok-abc` },
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
    args: { teamId: WS, statusId: STATUS, name: `QA 2` },
    resolved: { txId: 1 },
    expected: { ok: true, statusId: STATUS },
    calledWith: { teamId: WS, statusId: STATUS, name: `QA 2` },
  },
  {
    tool: `exponential_statuses_delete`,
    pick: () => caller.statuses.delete,
    args: { teamId: WS, statusId: STATUS, reassignToId: UUID },
    resolved: { txId: 1, reassigned: 3, reassignedToId: UUID },
    expected: { ok: true, statusId: STATUS, reassigned: 3, reassignedToId: UUID },
    calledWith: { teamId: WS, statusId: STATUS, reassignToId: UUID },
  },
  // ── EXP-660: automations ──
  {
    tool: `exponential_automations_list`,
    pick: () => caller.automations.list,
    args: { teamId: WS },
    resolved: { automations: [{ id: AUTO, enabled: true }] },
    expected: [{ id: AUTO, enabled: true }],
    calledWith: { teamId: WS },
  },
  {
    tool: `exponential_automations_update`,
    pick: () => caller.automations.update,
    args: { id: AUTO, enabled: false },
    resolved: { automation: { id: AUTO, enabled: false }, txid: 1 },
    expected: { id: AUTO, enabled: false },
    calledWith: { id: AUTO, enabled: false },
  },
  {
    tool: `exponential_automations_toggle`,
    pick: () => caller.automations.update,
    args: { id: AUTO, enabled: true },
    resolved: { automation: { id: AUTO, enabled: true }, txid: 1 },
    expected: { id: AUTO, enabled: true },
    calledWith: { id: AUTO, enabled: true },
  },
  {
    tool: `exponential_automations_delete`,
    pick: () => caller.automations.delete,
    args: { id: AUTO },
    resolved: { ok: true, txid: 1 },
    expected: { ok: true, id: AUTO },
    calledWith: { id: AUTO },
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
      txid: 1,
    },
    expected: { ok: true, id: RUN, status: `ended`, endedAt: null },
    calledWith: { codingSessionId: RUN },
  },
  // ── EXP-660: helpdesk (registered under the default gates) ──
  {
    tool: `exponential_helpdesk_threads_list`,
    pick: () => caller.helpdesk.listThreads,
    rows: [{ helpdeskEnabled: true }],
    args: { teamId: WS, filter: `open`, limit: 50 },
    resolved: [{ id: THREAD, title: `Login broken` }],
    expected: [{ id: THREAD, title: `Login broken` }],
    calledWith: { teamId: WS, filter: `open`, limit: 50 },
  },
  {
    tool: `exponential_helpdesk_threads_get`,
    pick: () => caller.helpdesk.getThread,
    rows: HELPDESK_ROWS,
    args: { threadId: THREAD },
    resolved: { thread: { id: THREAD }, messages: [], linkedIssue: null },
    expected: { thread: { id: THREAD }, messages: [], linkedIssue: null },
    calledWith: { threadId: THREAD },
  },
  {
    tool: `exponential_helpdesk_reply`,
    pick: () => caller.helpdesk.reply,
    rows: HELPDESK_ROWS,
    args: { threadId: THREAD, body: `On it.` },
    resolved: {
      message: { id: UUID },
      reporterEmailed: true,
      reporterViewing: false,
      reopened: false,
    },
    expected: {
      message: { id: UUID },
      reporterEmailed: true,
      reporterViewing: false,
      reopened: false,
    },
    calledWith: { threadId: THREAD, body: `On it.` },
  },
  {
    tool: `exponential_helpdesk_note`,
    pick: () => caller.helpdesk.note,
    rows: HELPDESK_ROWS,
    args: { threadId: THREAD, body: `internal` },
    resolved: { message: { id: UUID, visibility: `internal` } },
    expected: { id: UUID, visibility: `internal` },
    calledWith: { threadId: THREAD, body: `internal` },
  },
  {
    tool: `exponential_helpdesk_close`,
    pick: () => caller.helpdesk.close,
    rows: HELPDESK_ROWS,
    args: { threadId: THREAD },
    resolved: { ok: true },
    expected: { ok: true, threadId: THREAD },
    calledWith: { threadId: THREAD },
  },
  {
    tool: `exponential_helpdesk_reopen`,
    pick: () => caller.helpdesk.reopen,
    rows: HELPDESK_ROWS,
    args: { threadId: THREAD },
    resolved: { ok: true },
    expected: { ok: true, threadId: THREAD },
    calledWith: { threadId: THREAD },
  },
  {
    tool: `exponential_helpdesk_escalate`,
    pick: () => caller.helpdesk.escalate,
    rows: HELPDESK_ROWS,
    args: { threadId: THREAD, boardId: PROJ, title: `Login broken` },
    resolved: { issue: { id: UUID, identifier: `EXP-9` }, txId: 1 },
    expected: { id: UUID, identifier: `EXP-9` },
    calledWith: { threadId: THREAD, boardId: PROJ, title: `Login broken` },
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
      `helpdeskEnabled`,
      `iconUrl`,
      `id`,
      `name`,
      `prMergedAutomation`,
      `prMergedStatusId`,
      `prOpenedAutomation`,
      `prOpenedStatusId`,
      `slug`,
      `updatedAt`,
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

// ── pr_open batch session flip (EXP-194 / EXP-545) ───────────────────────────
// Batch coding sessions carry no issue linkage, so the PR-open flip finds them
// by `issue_id IS NULL` on the caller's running rows. Action runs are issue-less
// too — they must never be flipped to in_review or stamped with the PR branch.
describe(`exponential_pr_open batch session flip`, () => {
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
    vi.mocked(isInstallationLinkedToTeam).mockResolvedValue(true)
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
      ) => Promise.resolve([{ status: `backlog` }]).then(resolve, reject)
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

  it(`excludes ACTION runs from the issue-less flip`, async () => {
    const updates = armPrOpen()
    const result = await tool(`exponential_pr_open`)({
      issueIds: [UUID, PROJ],
      title: `Batch PR`,
      head: `exp/batch-abcd1234`,
    })
    expect(parseOk(result)).toMatchObject({ number: 7 })
    const flip = updates.find((u) => u.set.status === `in_review`)
    expect(flip).toBeDefined()
    // The branch stamp is the batch↔PR linkage — it must not land on an
    // action row, whose `branch` is NULL by contract.
    expect(flip!.set.branch).toBe(`exp/batch-abcd1234`)
    const { sql } = new PgDialect().sqlToQuery(flip!.where as never)
    expect(sql).toContain(`"issue_id" is null`)
    expect(sql).toContain(`"action_id" is null`)
  })
})

// ── EXP-637: the session header ──────────────────────────────────────────────
// The launcher injects X-Exp-Session-Id into the MCP config it writes, so
// every tool call an agent makes names the run it is running inside. That is
// how sessions_end closes out the right row and how pr_open parks the EXACT
// row instead of guessing. The id is an identifier, never a credential:
// ownership is re-checked per tool.
const SESSION = `66666666-6666-4666-8666-666666666666`

describe(`exponential_sessions_end`, () => {
  // EXP-679: the tool only registers for an unattended run, so these cases
  // hand in the gate the route would have resolved for one.
  const UNATTENDED = { helpdesk: true, sessionsEnd: true, askParent: false }

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
      keptOpen: false,
    })

    const result = await collectTools(USER, SESSION, UNATTENDED).get(
      `exponential_sessions_end`
    )!({ summary: `Stuck on the migration.` })

    expect(parseOk(result)).toEqual({
      sessionId: SESSION,
      status: `ended`,
      alreadyEnded: false,
      keptOpen: false,
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
  // close-out (alreadyEnded) or a kept-open person-started run never does.
  it(`does not notify the parent again on a retried close-out`, async () => {
    vi.mocked(endSessionByAgent).mockResolvedValue({
      sessionId: SESSION,
      status: `ended`,
      alreadyEnded: true,
      keptOpen: false,
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

  it(`does not notify the parent for a kept-open run`, async () => {
    vi.mocked(endSessionByAgent).mockResolvedValue({
      sessionId: SESSION,
      status: `running`,
      alreadyEnded: false,
      keptOpen: true,
    })

    const result = await collectTools(USER, SESSION, UNATTENDED).get(
      `exponential_sessions_end`
    )!({ summary: `s` })

    expect(parseOk(result)).toMatchObject({ keptOpen: true })
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

  // EXP-679: a person-started run never gets the tool — the close-out is
  // meaningless there (it would not end the run) and the human is present.
  it(`is not registered for a person-started session`, async () => {
    const tools = collectTools(USER, SESSION, {
      helpdesk: true,
      sessionsEnd: false,
      askParent: false,
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
  const AGENT_CHILD = { helpdesk: true, sessionsEnd: true, askParent: true }
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
      helpdesk: true,
      sessionsEnd: true,
      askParent: false,
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
    )!({ sessionId: SESSION, message: `hi` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`your own session`)
  })

  it(`injects with the starter prefix for a header-less caller`, async () => {
    dbRows.current = [targetRow()]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    vi.mocked(relayPostInput).mockResolvedValue({ delivered: true })

    const result = await collectTools(USER, null).get(
      `exponential_sessions_message`
    )!({ sessionId: TARGET, message: `Use staging.` })

    expect(relayPostInput).toHaveBeenCalledWith(
      RELAY,
      TARGET,
      `[Message from your starter via exponential_sessions_message] Use staging.`
    )
    expect(parseOk(result)).toEqual({
      ok: true,
      sessionId: TARGET,
      delivered: true,
    })
  })

  it(`uses the parent-answer prefix when answering its own child`, async () => {
    dbRows.current = [targetRow({ parentSessionId: SESSION })]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    vi.mocked(relayPostInput).mockResolvedValue({ delivered: true })

    await collectTools(USER, SESSION).get(`exponential_sessions_message`)!({
      sessionId: TARGET,
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
    )!({ sessionId: TARGET, message: `hi` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`owner or host`)
    expect(relayPostInput).not.toHaveBeenCalled()
  })

  it(`refuses an ended session`, async () => {
    dbRows.current = [targetRow({ status: `ended` })]

    const result = await collectTools(USER, null).get(
      `exponential_sessions_message`
    )!({ sessionId: TARGET, message: `hi` })

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
    )!({ sessionId: TARGET, message: `hi` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`Session not found`)
  })

  it(`errors when the relay cannot deliver`, async () => {
    dbRows.current = [targetRow()]
    vi.mocked(getSteerRelayConfig).mockReturnValue(RELAY)
    vi.mocked(relayPostInput).mockResolvedValue({ delivered: false })

    const result = await collectTools(USER, null).get(
      `exponential_sessions_message`
    )!({ sessionId: TARGET, message: `hi` })

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
    vi.mocked(isInstallationLinkedToTeam).mockResolvedValue(true)
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

    expect(updates).toHaveLength(1)
    expect(updates[0]!.set).toMatchObject({
      status: `in_review`,
      branch: `exp/chat-1a2b3c4d`,
      needsInput: false,
    })
    const { sql, params } = new PgDialect().sqlToQuery(
      updates[0]!.where as never
    )
    expect(sql).toContain(`"id" =`)
    expect(params).toContain(SESSION)
    // Never the loose issue-less sweep.
    expect(sql).not.toContain(`"issue_id" is null`)
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

  // The chore-PR own-merge test: repositoryId + prNumber names no branch, so
  // the tool asks GitHub for the PR's head ref. `CHORE_BRANCH` is the branch
  // the caller's own run sits on.
  const CHORE_BRANCH = `exp/chore-1a2b3c4d`
  function stageChorePr(headRef: string): void {
    vi.mocked(loadRepositoryForTeam).mockResolvedValue({
      repositoryId: REPO,
      teamId: WS,
      fullName: `acme/app`,
      defaultBranch: `main`,
    } as never)
    vi.mocked(resolveRepoInstallationTokenInfo).mockResolvedValue({
      token: `ghs_x`,
      installationId: 1,
    } as never)
    vi.mocked(getPullRequest).mockResolvedValue({
      state: `open`,
      merged: false,
      headRef,
      baseRef: `main`,
      mergeable: true,
      mergeableState: `clean`,
    })
  }

  const runRow = (over: Record<string, unknown> = {}) => ({
    id: SESSION,
    teamId: WS,
    issueId: null,
    branch: null,
    status: `in_review`,
    needsInput: false,
    mergedOwnPr: false,
    userId: `user-1`,
    hostUserId: null,
    ...over,
  })

  it(`stamps merged_own_pr on the header session BEFORE merging (decision 6)`, async () => {
    const updates = captureUpdates()
    // An issue-less run parked on the chore branch it opened the PR from —
    // the only row shape the branch-keyed merge sweep can reach.
    dbRows.current = [runRow({ branch: CHORE_BRANCH })]
    stageChorePr(CHORE_BRANCH)
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
    dbRows.current = [runRow({ branch: CHORE_BRANCH })]
    stageChorePr(CHORE_BRANCH)
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
    // No stampable row ⇒ no reason to ask GitHub anything.
    expect(getPullRequest).not.toHaveBeenCalled()
  })

  // The durable spare filters EVERY merge-driven end, so a chat/batch/action
  // run that lands somebody else's chore PR must not get one — it would also
  // survive the merge of its own PR, and then nothing would ever end it.
  it(`spares nothing when an issue-less run lands a FOREIGN chore PR`, async () => {
    const updates = captureUpdates()
    dbRows.current = [runRow({ branch: CHORE_BRANCH })]
    stageChorePr(`exp/somebody-elses-branch`)
    caller.repositories.mergePull.mockResolvedValue({ merged: true })

    await collectTools(USER, SESSION).get(`exponential_pr_merge`)!({
      repositoryId: REPO,
      prNumber: 9,
    })

    expect(caller.repositories.mergePull).toHaveBeenCalled()
    expect(getPullRequest).toHaveBeenCalledWith(`acme/app`, 9, `ghs_x`)
    expect(updates).toHaveLength(0)
  })

  it(`leaves the stamp off when the head-ref lookup fails`, async () => {
    const updates = captureUpdates()
    dbRows.current = [runRow({ branch: CHORE_BRANCH })]
    stageChorePr(CHORE_BRANCH)
    vi.mocked(getPullRequest).mockRejectedValue(new Error(`GitHub returned 502`))
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
]

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
    for (const column of [`id`, `issueId`, `issueIdentifier`, `summary`, `endedBy`, `branch`, `deviceId`]) {
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
    dbRows.current = [{ id: RUN, userId: `user-2`, teamId: WS, status: `ended`, summary: `Shipped` }]
    const result = await tool(`exponential_sessions_get`)({ id: RUN })
    expect(parseOk(result)).toMatchObject({ id: RUN, summary: `Shipped` })
    expect(membership.resolveTeamAccess).toHaveBeenCalledWith(`user-1`, WS)
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
    expect(caller.steer.killSession).toHaveBeenCalledWith({ codingSessionId: UUID })
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
    sharedTeamId: null,
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
        sharedTeamId: null,
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

describe(`exponential_sessions_start`, () => {
  const startedRow = { id: RUN, status: `running`, issueId: UUID, deviceId: `mac-1` }

  it(`resolves an identifier, starts over the steer rails and returns the run`, async () => {
    caller.steer.startSession.mockResolvedValue({ ok: true })
    // Every select resolves to dbRows.current at await time, so stage the
    // rows per call: 1 = boards (resolveIssueId), 2 = the issue by
    // identifier, 3+ = the poll for the device-created row.
    const builder = db.select()
    db.select.mockClear()
    let call = 0
    db.select.mockImplementation(() => {
      call += 1
      dbRows.current =
        call === 1
          ? [{ id: PROJ, teamId: `ws-1` }]
          : call === 2
            ? [{ id: UUID }]
            : [startedRow]
      return builder
    })

    let result: ToolResult
    try {
      result = await tool(`exponential_sessions_start`)({
        deviceId: `mac-1`,
        issueId: `exp-7`,
        agent: `codex`,
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
    expect(caller.steer.startSession).toHaveBeenCalledWith(
      expect.objectContaining({ deviceId: `mac-1`, issueId: UUID, agent: `codex` })
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

  it(`hands back sessionId null when the device never reports the run`, async () => {
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
      expect(parseOk(result)).toEqual({
        ok: true,
        deviceId: `mac-1`,
        sessionId: null,
        session: null,
      })
    } finally {
      vi.useRealTimers()
    }
    expect(caller.steer.startSession).toHaveBeenCalledTimes(1)
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

describe(`exponential_automations_update trigger`, () => {
  it(`rejects a malformed trigger before calling the router`, async () => {
    const result = await tool(`exponential_automations_update`)({
      id: AUTO,
      trigger: { kind: `nope` },
    })
    expect(result.isError).toBe(true)
    expect(caller.automations.update).not.toHaveBeenCalled()
  })

  it(`forwards a parsed schedule trigger`, async () => {
    caller.automations.update.mockResolvedValue({ automation: { id: AUTO }, txid: 1 })
    const trigger = { kind: `schedule`, interval: `daily`, minuteOfDay: 540 }
    const result = await tool(`exponential_automations_update`)({ id: AUTO, trigger })
    expect(parseOk(result)).toEqual({ id: AUTO })
    expect(caller.automations.update).toHaveBeenCalledWith(
      expect.objectContaining({ id: AUTO, trigger })
    )
  })
})

describe(`exponential_helpdesk_* gating`, () => {
  const HELPDESK_TOOLS = [
    `exponential_helpdesk_threads_list`,
    `exponential_helpdesk_threads_get`,
    `exponential_helpdesk_reply`,
    `exponential_helpdesk_note`,
    `exponential_helpdesk_close`,
    `exponential_helpdesk_reopen`,
    `exponential_helpdesk_escalate`,
  ]

  it(`registers the whole family under the default gates and none when off`, () => {
    for (const name of HELPDESK_TOOLS) expect(tools.has(name)).toBe(true)
    const off = collectTools(USER, null, {
      helpdesk: false,
      sessionsEnd: false,
      askParent: false,
    })
    for (const name of [...off.keys()]) {
      expect(name.startsWith(`exponential_helpdesk_`)).toBe(false)
    }
    // Everything else is untouched by the gate.
    expect(off.has(`exponential_sessions_start`)).toBe(true)
  })

  it(`refuses a thread of a team with helpdesk switched off`, async () => {
    dbRows.current = [{ teamId: WS, helpdeskEnabled: false }]
    const result = await tool(`exponential_helpdesk_reply`)({ threadId: THREAD, body: `hi` })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not enabled`)
    expect(caller.helpdesk.reply).not.toHaveBeenCalled()
  })

  it(`refuses listing for a team with helpdesk switched off`, async () => {
    dbRows.current = [{ helpdeskEnabled: false }]
    const result = await tool(`exponential_helpdesk_threads_list`)({
      teamId: WS,
      filter: `open`,
      limit: 50,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not enabled`)
    expect(caller.helpdesk.listThreads).not.toHaveBeenCalled()
  })

  it(`reports an unknown thread as not found`, async () => {
    dbRows.current = []
    const result = await tool(`exponential_helpdesk_close`)({ threadId: THREAD })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`Thread not found`)
  })

  it(`requires a FULL grant on the thread's team even for reads`, async () => {
    dbRows.current = [{ teamId: PROJ, helpdeskEnabled: true }]
    const result = await collectTools(USER, null, ALL_MCP_TOOL_GATES, SCOPED_TO_WS).get(
      `exponential_helpdesk_threads_get`
    )!({ threadId: THREAD })
    expect(result.isError).toBe(true)
    expect(caller.helpdesk.getThread).not.toHaveBeenCalled()
  })
})
