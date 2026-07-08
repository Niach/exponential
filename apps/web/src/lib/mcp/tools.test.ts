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
    repositories: { list: vi.fn(), add: vi.fn(), branchDiff: vi.fn() },
    runConfigs: {
      list: vi.fn(),
      create: vi.fn(),
      update: vi.fn(),
      delete: vi.fn(),
    },
    issues: { prFiles: vi.fn() },
    projects: { delete: vi.fn(), setRepository: vi.fn() },
    workspaces: { create: vi.fn(), update: vi.fn() },
    workspaceInvites: { create: vi.fn(), list: vi.fn(), revoke: vi.fn() },
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

  const db = {
    select: vi.fn(() => queryBuilder),
    insert: vi.fn(() => ({ values: insertValues })),
  }

  const membership = {
    resolveWorkspaceAccess: vi.fn(async () => undefined),
    assertWorkspaceMember: vi.fn(async () => undefined),
    getIssueWorkspaceContext: vi.fn(async () => ({
      workspaceId: `ws-1`,
      projectId: `proj-1`,
    })),
    getProjectWorkspaceId: vi.fn(async () => ({ workspaceId: `ws-1` })),
    getAttachmentWorkspaceContext: vi.fn(async () => ({
      workspaceId: `ws-1`,
      contentType: `image/png`,
      storageKey: `k`,
    })),
    getUserWorkspaceIds: vi.fn(async () => [`ws-1`]),
    getPublicWorkspaceIds: vi.fn(async () => []),
  }

  const uploadObject = vi.fn(async () => undefined)
  const deleteObject = vi.fn(async () => undefined)
  const assertWithinStorageLimit = vi.fn(async () => undefined)

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

vi.mock(`@/lib/workspace-membership`, () => h.membership)

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
}))
vi.mock(`@/lib/integrations/activity`, () => ({ recordIssueEvent: vi.fn() }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetPrNotify: vi.fn(),
}))

import { registerExponentialTools } from "@/lib/mcp/tools"
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
  isAgent: false,
  creemCustomerId: null,
  hadTrial: false,
  onboardingCompletedAt: null,
  createdAt: new Date(),
  updatedAt: new Date(),
} as unknown as McpUser

function collectTools(): Map<string, ToolHandler> {
  const tools = new Map<string, ToolHandler>()
  const fakeServer = {
    registerTool: (name: string, _def: unknown, handler: ToolHandler) => {
      tools.set(name, handler)
    },
  }
  registerExponentialTools(
    fakeServer as never,
    USER,
    new Request(`https://x.test/api/mcp`),
    FULL_ACCESS
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
  membership.resolveWorkspaceAccess.mockResolvedValue(undefined)
  membership.assertWorkspaceMember.mockResolvedValue(undefined)
  membership.getIssueWorkspaceContext.mockResolvedValue({
    workspaceId: `ws-1`,
    projectId: `proj-1`,
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
    args: { workspaceId: WS },
    resolved: [{ id: REPO, fullName: `a/b`, projects: [] }],
    expected: [{ id: REPO, fullName: `a/b`, projects: [] }],
    calledWith: { workspaceId: WS },
  },
  {
    tool: `exponential_repositories_add`,
    pick: () => caller.repositories.add,
    args: { workspaceId: WS, fullName: `a/b` },
    resolved: { repository: { id: REPO, fullName: `a/b` } },
    expected: { id: REPO, fullName: `a/b` },
    calledWith: { workspaceId: WS, fullName: `a/b` },
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
    tool: `exponential_run_configs_list`,
    pick: () => caller.runConfigs.list,
    args: { projectId: PROJ },
    resolved: { configs: [{ id: UUID, name: `dev` }] },
    expected: [{ id: UUID, name: `dev` }],
    calledWith: { projectId: PROJ },
  },
  {
    tool: `exponential_run_configs_create`,
    pick: () => caller.runConfigs.create,
    args: { projectId: PROJ, name: `dev`, argv: [`bun`, `dev`] },
    resolved: { config: { id: UUID, name: `dev`, argv: [`bun`, `dev`] } },
    expected: { id: UUID, name: `dev`, argv: [`bun`, `dev`] },
    calledWith: { projectId: PROJ, name: `dev`, argv: [`bun`, `dev`] },
  },
  {
    tool: `exponential_run_configs_update`,
    pick: () => caller.runConfigs.update,
    args: { id: UUID, name: `staging` },
    resolved: { config: { id: UUID, name: `staging` } },
    expected: { id: UUID, name: `staging` },
    calledWith: { id: UUID, name: `staging` },
  },
  {
    tool: `exponential_run_configs_delete`,
    pick: () => caller.runConfigs.delete,
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
    tool: `exponential_projects_delete`,
    pick: () => caller.projects.delete,
    args: { projectId: PROJ },
    resolved: { ok: true, txId: 1 },
    expected: { ok: true, projectId: PROJ },
    calledWith: { projectId: PROJ },
  },
  {
    tool: `exponential_projects_set_repository`,
    pick: () => caller.projects.setRepository,
    args: { projectId: PROJ, repositoryId: REPO },
    resolved: { project: { id: PROJ, repositoryId: REPO } },
    expected: { id: PROJ, repositoryId: REPO },
    calledWith: { projectId: PROJ, repositoryId: REPO },
  },
  {
    tool: `exponential_workspaces_create`,
    pick: () => caller.workspaces.create,
    args: { name: `New WS` },
    resolved: { workspace: { id: WS, name: `New WS` } },
    expected: { id: WS, name: `New WS` },
    calledWith: { name: `New WS` },
  },
  {
    tool: `exponential_workspaces_update`,
    pick: () => caller.workspaces.update,
    args: { id: WS, name: `Renamed` },
    resolved: { workspace: { id: WS, name: `Renamed` } },
    expected: { id: WS, name: `Renamed` },
    calledWith: { id: WS, name: `Renamed` },
  },
  {
    tool: `exponential_invites_create`,
    pick: () => caller.workspaceInvites.create,
    args: { workspaceId: WS, role: `member` },
    resolved: { invite: { id: INV }, token: `tok-abc` },
    expected: { invite: { id: INV }, token: `tok-abc` },
    calledWith: { workspaceId: WS, role: `member` },
  },
  {
    tool: `exponential_invites_list`,
    pick: () => caller.workspaceInvites.list,
    args: { workspaceId: WS },
    resolved: { invites: [{ id: INV }] },
    expected: [{ id: INV }],
    calledWith: { workspaceId: WS },
  },
  {
    tool: `exponential_invites_revoke`,
    pick: () => caller.workspaceInvites.revoke,
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
    const { sql, params } = new PgDialect().sqlToQuery(state.capturedWhere as never)
    expect(sql).toContain(`user_id`)
    expect(params).toContain(`user-1`)
    // unreadOnly must add the read_at IS NULL predicate.
    expect(sql).toContain(`read_at`)
  })
})

// ── members_list (direct DB read, workspace-gated, agent-excluded) ───────────

describe(`exponential_members_list`, () => {
  it(`returns members and excludes agents by default`, async () => {
    dbRows.current = [
      { id: `user-1`, name: `User One`, role: `owner`, isAgent: false },
    ]
    const result = await tool(`exponential_members_list`)({
      workspaceId: WS,
      includeAgents: false,
    })
    expect(parseOk(result)).toEqual([
      { id: `user-1`, name: `User One`, role: `owner`, isAgent: false },
    ])
    expect(membership.resolveWorkspaceAccess).toHaveBeenCalledWith(
      `user-1`,
      WS
    )
    const { sql, params } = new PgDialect().sqlToQuery(state.capturedWhere as never)
    expect(sql).toContain(`is_agent`)
    expect(params).toContain(false)
  })

  it(`includes agents when includeAgents=true`, async () => {
    dbRows.current = []
    await tool(`exponential_members_list`)({
      workspaceId: WS,
      includeAgents: true,
    })
    const { sql } = new PgDialect().sqlToQuery(state.capturedWhere as never)
    expect(sql).not.toContain(`is_agent`)
  })

  it(`denies when the user is not in the workspace`, async () => {
    membership.resolveWorkspaceAccess.mockRejectedValue(forbidden())
    const result = await tool(`exponential_members_list`)({
      workspaceId: WS,
      includeAgents: false,
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

  it(`rejects a non-image content type before touching storage`, async () => {
    const result = await tool(`exponential_attachments_upload`)({
      ...args,
      contentType: `application/pdf`,
    })
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`Unsupported image type`)
    expect(uploadObject).not.toHaveBeenCalled()
  })

  it(`denies when the user is not a workspace member`, async () => {
    membership.assertWorkspaceMember.mockRejectedValue(forbidden())
    const result = await tool(`exponential_attachments_upload`)(args)
    expect(result.isError).toBe(true)
    expect(result.content[0].text).toContain(`not allowed here`)
    expect(uploadObject).not.toHaveBeenCalled()
    expect(insertValues).not.toHaveBeenCalled()
  })
})
