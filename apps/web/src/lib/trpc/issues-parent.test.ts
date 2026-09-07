import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-760 — `issues.create({ parentId })`, the server half of the inline
// sub-issue composer (and of MCP `exponential_issues_create`). Two rules:
//
//   1. the `parent` relation is written INSIDE the create transaction, so the
//      row the client awaits by txId already carries the link — a second
//      round-trip would let a sub-issue appear un-parented for a beat;
//   2. the parent is validated BEFORE the transaction opens, against the same
//      gate `relations.create` applies (visible board, same team). A refusal
//      must therefore leave no issue behind at all.
//
// Harness lifted from issues-default-assignee.test.ts (functional transaction
// mock so the writes are observable), plus a `select` stub for the pre-tx
// parent lookup.

const h = vi.hoisted(() => ({
  getSoleHumanMemberId: vi.fn(async (): Promise<string | null> => null),
  ensureSubscribed: vi.fn(),
  fireAndForgetAssignmentNotify: vi.fn(),
  insertRelationInTx: vi.fn(async (_tx: unknown, _args: unknown) => null),
  parentRows: [] as Array<{ teamId: string }>,
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/team-membership`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/team-membership")>()
  return {
    ...actual,
    getBoardTeamId: vi.fn(async () => ({ id: `proj-1`, teamId: `ws-1` })),
    resolveTeamAccess: vi.fn(async () => ({
      kind: `member`,
      team: { id: `ws-1` },
      member: { role: `member`, userId: `actor`, teamId: `ws-1` },
    })),
    assertAssigneeInTeam: vi.fn(async () => undefined),
    getSoleHumanMemberId: h.getSoleHumanMemberId,
  }
})

// The relation writer is spied on, but canonicalizeRelation stays REAL — the
// point of the test is that the pair reaches storage in the canonical
// direction (parent = issue_id, child = related_issue_id).
vi.mock(`@/lib/issue-relations`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/issue-relations")>()
  return {
    ...actual,
    insertRelationInTx: h.insertRelationInTx,
    syncReferenceRelations: vi.fn(),
  }
})

// Side-effect-free stubs for issues.ts's remaining module-scope imports.
vi.mock(`@/lib/integrations/github-pr`, () => ({
  fetchPullFiles: vi.fn(),
  mergePullRequest: vi.fn(),
  resolveRepoToken: vi.fn(),
  GitHubMergeError: class extends Error {},
}))
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => false,
  resolveRepoInstallationToken: vi.fn(),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrMergeState: vi.fn(),
}))
vi.mock(`@/lib/trpc/repositories`, () => ({
  resolveBoardRepository: vi.fn(),
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
  fireAndForgetAssignmentNotify: h.fireAndForgetAssignmentNotify,
  fireAndForgetStatusChangeNotify: vi.fn(),
  fireAndForgetReporterResolution: vi.fn(),
}))
vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: h.ensureSubscribed,
}))
vi.mock(`@/lib/integrations/activity`, () => ({
  recordIssueEvent: vi.fn(),
}))

import { issuesRouter } from "@/lib/trpc/issues"

const BOARD_ID = `11111111-1111-4111-8111-111111111111`
const ISSUE_ID = `22222222-2222-4222-8222-222222222222`
const PARENT_ID = `33333333-3333-4333-8333-333333333333`

const insertedIssues: Array<Record<string, unknown>> = []
const tx = {
  execute: vi.fn(async () => ({ rows: [{ txid: `42` }] })),
  insert: vi.fn(() => ({
    values: (values: Record<string, unknown>) => {
      insertedIssues.push(values)
      const returning = async () => [
        { id: ISSUE_ID, identifier: `EXP-1`, ...values },
      ]
      return {
        returning,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        then: (res: any, rej: any) => Promise.resolve().then(res, rej),
      }
    },
  })),
}
const transaction = vi.fn(
  async (fn: (t: typeof tx) => Promise<unknown>) => fn(tx)
)

// The pre-tx parent lookup: select → from → innerJoin → where → limit.
const select = vi.fn(() => ({
  from: () => ({
    innerJoin: () => ({
      where: () => ({ limit: async () => h.parentRows }),
    }),
  }),
}))

const caller = issuesRouter.createCaller({
  session: { user: { id: `actor` } },
  db: { transaction, select },
  request: new Request(`http://localhost/`),
} as never)

describe(`issues.create parentId (EXP-760)`, () => {
  beforeEach(() => {
    insertedIssues.length = 0
    h.parentRows = [{ teamId: `ws-1` }]
    h.getSoleHumanMemberId.mockClear()
    h.getSoleHumanMemberId.mockResolvedValue(null)
    h.ensureSubscribed.mockClear()
    h.insertRelationInTx.mockClear()
    transaction.mockClear()
    select.mockClear()
  })

  it(`writes the canonical parent relation inside the create transaction`, async () => {
    await caller.create({
      boardId: BOARD_ID,
      title: `Child`,
      parentId: PARENT_ID,
    })

    expect(h.insertRelationInTx).toHaveBeenCalledTimes(1)
    expect(h.insertRelationInTx).toHaveBeenCalledWith(tx, {
      // Canonical direction: the PARENT is the issue_id side.
      issueId: PARENT_ID,
      relatedIssueId: ISSUE_ID,
      type: `parent`,
      source: `user`,
      teamId: `ws-1`,
      actorUserId: `actor`,
    })
    // First argument is the transaction handle itself — one txId covers the
    // issue insert and the relation.
    expect(transaction).toHaveBeenCalledTimes(1)
  })

  it(`refuses a parent from another team before anything is written`, async () => {
    h.parentRows = [{ teamId: `ws-2` }]

    await expect(
      caller.create({ boardId: BOARD_ID, title: `Child`, parentId: PARENT_ID })
    ).rejects.toThrow(/same team/i)

    expect(transaction).not.toHaveBeenCalled()
    expect(insertedIssues).toHaveLength(0)
    expect(h.insertRelationInTx).not.toHaveBeenCalled()
  })

  it(`refuses a parent on a hidden board the same way`, async () => {
    // boardVisible() is part of the lookup's WHERE, so a trashed or archived
    // board simply returns no row — indistinguishable from "does not exist",
    // which is exactly the point (no identifier leaks).
    h.parentRows = []

    await expect(
      caller.create({ boardId: BOARD_ID, title: `Child`, parentId: PARENT_ID })
    ).rejects.toThrow(/same team/i)

    expect(transaction).not.toHaveBeenCalled()
    expect(h.insertRelationInTx).not.toHaveBeenCalled()
  })

  it(`never touches relations without a parentId`, async () => {
    await caller.create({ boardId: BOARD_ID, title: `Standalone` })

    expect(select).not.toHaveBeenCalled()
    expect(h.insertRelationInTx).not.toHaveBeenCalled()
    expect(insertedIssues).toHaveLength(1)
  })
})
