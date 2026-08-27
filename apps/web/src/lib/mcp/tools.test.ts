import { beforeEach, describe, expect, it, vi } from "vitest"
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
    issues: { prFiles: vi.fn(), retargetPr: vi.fn(), update: vi.fn() },
    boards: { delete: vi.fn(), setRepository: vi.fn() },
    teams: { create: vi.fn(), update: vi.fn() },
    teamInvites: { create: vi.fn(), list: vi.fn(), revoke: vi.fn() },
    attachments: { delete: vi.fn() },
  }

  // A chainable, thenable drizzle query stub. Every builder method returns the
  // same object; awaiting it resolves to `dbRows.current`. `.where(cond)`
  // records the condition so scoping tests can render it back to SQL.
  const dbRows: { current: Array<unknown> } = { current: [] }
  const state: { capturedWhere: unknown } = { capturedWhere: undefined }
  const insertValues = vi.fn(async () => undefined)

  const queryBuilder: Record<string, unknown> = {}
  for (const method of [`from`, `innerJoin`, `orderBy`, `limit`, `offset`]) {
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
vi.mock(`@/lib/integrations/github-pr`, () => ({ createPullRequest: vi.fn() }))
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

import { loadRepositoryForTeam } from "@/lib/trpc/repositories"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { applyPrLifecycleStatusInTx } from "@/lib/integrations/pr-sync"
import { fireAndForgetPrNotify } from "@/lib/integrations/notifications"
import { noteAgentIssueActivity } from "@/lib/integrations/pr-actor-claims"
import { endSessionByAgent } from "@/lib/coding-session-end"
import { createPullRequest } from "@/lib/integrations/github-pr"
import { resolveRepoInstallationTokenInfo } from "@/lib/integrations/github-app"
import { isInstallationLinkedToTeam } from "@/lib/trpc/integrations"
import { registerExponentialTools } from "@/lib/mcp/tools"
import {
  builtinCreateAction,
  builtinFixConflictsAction,
} from "@/lib/builtin-actions"
import { FULL_ACCESS } from "@/lib/mcp/scope"
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
function collectTools(
  user: McpUser = USER,
  sessionId: string | null = null
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
    FULL_ACCESS,
    sessionId
  )
  return tools
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
  assertWithinStorageLimit.mockResolvedValue(undefined)
  insertValues.mockResolvedValue(undefined)
})

// ── Caller-backed tools (delegate → ok/err) ──────────────────────────────────

type Descriptor = {
  tool: string
  pick: () => ReturnType<typeof vi.fn>
  args: Record<string, unknown>
  resolved: unknown
  expected: unknown
  calledWith?: unknown
}

const descriptors: Array<Descriptor> = [
  {
    tool: `exponential_comments_update`,
    pick: () => caller.comments.update,
    args: { id: UUID, bodyText: `edited` },
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
]

describe.each(descriptors)(
  `caller-backed MCP tool $tool`,
  ({ tool: name, pick, args, resolved, expected, calledWith }) => {
    it(`happy path returns the mapped payload`, async () => {
      pick().mockResolvedValue(resolved)
      const result = await tool(name)(args)
      expect(parseOk(result)).toEqual(expected)
      if (calledWith) {
        expect(pick()).toHaveBeenCalledWith(calledWith)
      }
    })

    it(`surfaces a permission denial as an MCP error`, async () => {
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
      {
        id: `b`,
        name: `Todo`,
        category: `unstarted`,
        builtinKey: `todo`,
        sortOrder: 1,
        createdAt: at(`2026-01-01T00:00:00Z`),
      },
      {
        id: `a`,
        name: `QA`,
        category: `started`,
        builtinKey: null,
        sortOrder: 2,
        createdAt: at(`2026-01-02T00:00:00Z`),
      },
      {
        id: `c`,
        name: `In Progress`,
        category: `started`,
        builtinKey: `in_progress`,
        sortOrder: 1,
        createdAt: at(`2026-01-01T00:00:00Z`),
      },
    ]
    const result = await tool(`exponential_statuses_list`)({ teamId: WS })
    expect(parseOk(result)).toEqual([
      {
        id: `b`,
        name: `Todo`,
        category: `unstarted`,
        position: 1,
        builtinKey: `todo`,
      },
      {
        id: `c`,
        name: `In Progress`,
        category: `started`,
        position: 1,
        builtinKey: `in_progress`,
      },
      {
        id: `a`,
        name: `QA`,
        category: `started`,
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
      ) => Promise.resolve([{ status: `todo` }]).then(resolve, reject)
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
  it(`refuses outside a launched session, naming the missing header`, async () => {
    const result = await collectTools(USER, null).get(
      `exponential_sessions_end`
    )!({ summary: `did the thing`, outcome: `done` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`X-Exp-Session-Id`)
    expect(endSessionByAgent).not.toHaveBeenCalled()
  })

  it(`closes the header session out with the summary and outcome`, async () => {
    vi.mocked(endSessionByAgent).mockResolvedValue({
      sessionId: SESSION,
      status: `ended`,
      outcome: `blocked`,
      alreadyEnded: false,
    })

    const result = await collectTools(USER, SESSION).get(
      `exponential_sessions_end`
    )!({ summary: `Stuck on the migration.`, outcome: `blocked` })

    expect(parseOk(result)).toEqual({
      sessionId: SESSION,
      status: `ended`,
      outcome: `blocked`,
      alreadyEnded: false,
    })
    expect(endSessionByAgent).toHaveBeenCalledWith(db, SESSION, `user-1`, {
      summary: `Stuck on the migration.`,
      outcome: `blocked`,
    })
  })

  it(`surfaces a foreign session's refusal as a tool error`, async () => {
    vi.mocked(endSessionByAgent).mockRejectedValue(forbidden())

    const result = await collectTools(USER, SESSION).get(
      `exponential_sessions_end`
    )!({ summary: `s`, outcome: `done` })

    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not allowed here`)
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

  it(`stamps merged_own_pr on the header session BEFORE merging (decision 6)`, async () => {
    const updates: Array<{ set: Record<string, unknown>; where: unknown }> = []
    db.update.mockImplementation(() => ({
      set: (values: Record<string, unknown>) => ({
        where: async (cond: unknown) => {
          updates.push({ set: values, where: cond })
        },
      }),
    }))
    dbRows.current = [
      {
        id: SESSION,
        teamId: WS,
        status: `in_review`,
        userId: `user-1`,
        hostUserId: null,
      },
    ]
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

  it(`stamps nothing without a header session`, async () => {
    const updates: Array<{ set: Record<string, unknown> }> = []
    db.update.mockImplementation(() => ({
      set: (values: Record<string, unknown>) => ({
        where: async () => {
          updates.push({ set: values })
        },
      }),
    }))
    caller.repositories.mergePull.mockResolvedValue({ merged: true })

    await collectTools(USER, null).get(`exponential_pr_merge`)!({
      repositoryId: REPO,
      prNumber: 9,
    })

    expect(updates).toHaveLength(0)
  })
})
