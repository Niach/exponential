import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"
import { PgDialect } from "drizzle-orm/pg-core"

// Locks the EXP-264 point-read contract. `issues.get` is the fallback the
// native clients hit when a push tap targets an issue the Electric shape has
// not delivered yet, so its response shape is a cross-client contract:
// `{ issue, labelIds, teamId }` where `issue` carries EXACTLY the issues
// shape's column allowlist — no server-only scoping columns — and `teamId`
// stays top-level (clients need it for their denormalized issue_labels rows).

const h = vi.hoisted(() => ({
  // Each ctx.db.select() call consumes the next result set, in call order.
  selectQueue: [] as unknown[][],
  // Projections passed to select(), and the clause each where() received.
  selectProjections: [] as Array<Record<string, unknown>>,
  whereArgs: [] as unknown[],
  getUserTeamIds: vi.fn(async () => [`ws-1`]),
  assertIssueAccess: vi.fn(async () => ({
    issueId: `issue-1`,
    boardId: `board-1`,
    teamId: `ws-1`,
  })),
}))

// membership.ts's getDb() dynamically imports @/db/connection; this mock also
// satisfies lib/trpc.ts's module-scope `db` import without a live Postgres.
vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: () => ({ where: () => ({ limit: async () => [] }) }),
    }),
  },
}))

vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/team-membership`, async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("@/lib/team-membership")>()
  return {
    ...actual,
    getUserTeamIds: h.getUserTeamIds,
    assertIssueAccess: h.assertIssueAccess,
  }
})

// Side-effect-free stubs for issues.ts's remaining module-scope imports.
vi.mock(`@/lib/integrations/github-pr`, () => ({
  fetchPullFiles: vi.fn(),
  mergePullRequest: vi.fn(),
  closePullRequest: vi.fn(),
  GitHubMergeError: class extends Error {},
}))
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => false,
  resolveRepoInstallationTokenInfo: vi.fn(),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrClosedState: vi.fn(),
  applyPrMergeState: vi.fn(),
}))
vi.mock(`@/lib/storage/issue-attachments`, () => ({
  canonicalizeMarkdownImageUrls: vi.fn(),
  extractAttachmentIdsFromDescription: vi.fn(),
  hasMarkdownImages: () => false,
}))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  collectIssueAttachmentStorageKeysInTx: vi.fn(),
  deleteStorageObjects: vi.fn(),
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetAssignmentNotify: vi.fn(),
  fireAndForgetIssueMentionNotify: vi.fn(),
  fireAndForgetStatusChangeNotify: vi.fn(),
  fireAndForgetReporterResolution: vi.fn(),
}))
vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: vi.fn(),
}))
vi.mock(`@/lib/integrations/activity`, () => ({
  recordIssueEvent: vi.fn(),
}))

import { issuesRouter } from "@/lib/trpc/issues"

const ISSUE_ID = `22222222-2222-4222-8222-222222222222`

// The camelCase twin of ISSUE_COLUMNS in routes/api/shapes/issues.ts — the
// projection `get` must return so a client can merge the row into its synced
// store verbatim. Both lists move together.
const SHAPE_COLUMNS = [
  `id`,
  `boardId`,
  `number`,
  `identifier`,
  `title`,
  `description`,
  `status`,
  `priority`,
  `assigneeId`,
  `creatorId`,
  `source`,
  `dueDate`,
  `sortOrder`,
  `completedAt`,
  `duplicateOfId`,
  `prUrl`,
  `prNumber`,
  `prState`,
  `branch`,
  `prMergedAt`,
  `createdAt`,
  `updatedAt`,
]

const issueRow = {
  id: ISSUE_ID,
  boardId: `board-1`,
  identifier: `EXP-42`,
  title: `Login button unresponsive`,
}

const db = {
  select: vi.fn((projection: Record<string, unknown>) => {
    h.selectProjections.push(projection)
    const rows = h.selectQueue.shift() ?? []
    const builder = {
      from: () => builder,
      where: (clause: unknown) => {
        h.whereArgs.push(clause)
        return builder
      },
      orderBy: () => builder,
      limit: async () => rows,
      // Awaited without .limit() (the label enumeration).
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      then: (res: any, rej: any) => Promise.resolve(rows).then(res, rej),
    }
    return builder
  }),
}

const caller = issuesRouter.createCaller({
  session: { user: { id: `actor` } },
  db,
  request: new Request(`http://localhost/`),
} as never)

// Serialize a captured drizzle clause so its bound params can be asserted.
function paramsOf(clause: unknown): unknown[] {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return new PgDialect().sqlToQuery(clause as any).params
}

describe(`issues.get (EXP-264)`, () => {
  beforeEach(() => {
    h.selectQueue.length = 0
    h.selectProjections.length = 0
    h.whereArgs.length = 0
    h.getUserTeamIds.mockClear()
    h.assertIssueAccess.mockClear()
    h.getUserTeamIds.mockResolvedValue([`ws-1`])
    h.assertIssueAccess.mockResolvedValue({
      issueId: ISSUE_ID,
      boardId: `board-1`,
      teamId: `ws-1`,
    })
    db.select.mockClear()
  })

  it(`reads a UUID straight through the read access check`, async () => {
    h.selectQueue.push([issueRow], [{ labelId: `l-1` }, { labelId: `l-2` }])

    const result = await caller.get({ id: ISSUE_ID })

    expect(h.assertIssueAccess).toHaveBeenCalledWith(`actor`, ISSUE_ID, `read`)
    // A UUID needs no identifier resolution — no team enumeration at all.
    expect(h.getUserTeamIds).not.toHaveBeenCalled()
    expect(result).toEqual({
      issue: issueRow,
      labelIds: [`l-1`, `l-2`],
      teamId: `ws-1`,
    })
  })

  it(`projects exactly the issues-shape column allowlist`, async () => {
    h.selectQueue.push([issueRow], [])

    await caller.get({ id: ISSUE_ID })

    const projection = h.selectProjections[0]
    expect(Object.keys(projection)).toEqual(SHAPE_COLUMNS)
    // The REV2-5 scoping columns are server-only — native issue schemas do not
    // carry them and they must never ride inside the row.
    expect(projection).not.toHaveProperty(`teamId`)
    expect(projection).not.toHaveProperty(`boardDeletedAt`)
  })

  it(`resolves a human identifier team-scoped, uppercased, trash-aware`, async () => {
    h.getUserTeamIds.mockResolvedValue([`ws-1`, `ws-2`])
    h.selectQueue.push([{ id: ISSUE_ID }], [issueRow], [])

    const result = await caller.get({ id: `exp-42` })

    expect(h.getUserTeamIds).toHaveBeenCalledWith(`actor`)
    const params = paramsOf(h.whereArgs[0])
    // Identifiers are stored uppercase; the lookup normalizes.
    expect(params).toContain(`EXP-42`)
    expect(params).not.toContain(`exp-42`)
    // Team-scoped — a foreign team's identifier can never resolve here.
    expect(params).toEqual(expect.arrayContaining([`ws-1`, `ws-2`]))
    // The resolved UUID is what the access check and row read run against.
    expect(h.assertIssueAccess).toHaveBeenCalledWith(`actor`, ISSUE_ID, `read`)
    expect(result.issue).toEqual(issueRow)
  })

  it(`404s an identifier that matches nothing in the caller's teams`, async () => {
    h.selectQueue.push([])

    const error = await caller.get({ id: `EXP-999` }).then(
      () => undefined,
      (e: unknown) => e
    )

    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
    expect((error as TRPCError).message).toBe(`Issue not found`)
    expect(h.assertIssueAccess).not.toHaveBeenCalled()
  })

  it(`404s an identifier when the caller has no teams, without querying`, async () => {
    h.getUserTeamIds.mockResolvedValue([])

    const error = await caller.get({ id: `EXP-1` }).then(
      () => undefined,
      (e: unknown) => e
    )

    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
    expect(db.select).not.toHaveBeenCalled()
  })

  it(`propagates the access check's rejection (foreign-team UUID probe)`, async () => {
    h.assertIssueAccess.mockRejectedValue(
      new TRPCError({ code: `FORBIDDEN`, message: `Not a team member` })
    )

    const error = await caller.get({ id: ISSUE_ID }).then(
      () => undefined,
      (e: unknown) => e
    )

    expect((error as TRPCError).code).toBe(`FORBIDDEN`)
    // Nothing was read: the guard runs before the row select.
    expect(db.select).not.toHaveBeenCalled()
  })

  it(`404s when the row vanished between the access check and the read`, async () => {
    h.selectQueue.push([])

    const error = await caller.get({ id: ISSUE_ID }).then(
      () => undefined,
      (e: unknown) => e
    )

    expect((error as TRPCError).code).toBe(`NOT_FOUND`)
  })
})
