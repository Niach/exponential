import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1284 — `issues.create({ threadId })`, the thread page's "new issue" and
// MCP `exponential_issues_create{threadId}`. Same two rules as the parentId
// link (issues-parent.test.ts, whose harness this borrows):
//
//   1. the thread link is written INSIDE the create transaction, so the
//      thread item lands under the txId the client awaits;
//   2. the thread is validated BEFORE the transaction opens (it must exist
//      and be the board's team), so a refusal leaves no issue behind.

const h = vi.hoisted(() => ({
  getSoleHumanMemberId: vi.fn(async (): Promise<string | null> => null),
  ensureSubscribed: vi.fn(),
  fireAndForgetAssignmentNotify: vi.fn(),
  insertRelationInTx: vi.fn(async (_tx: unknown, _args: unknown) => null),
  getThread: vi.fn(),
  linkIssueToThread: vi.fn(async (..._args: unknown[]) => ({ added: true })),
}))

vi.mock(`@/lib/threads`, () => ({
  getThread: h.getThread,
  linkIssueToThread: h.linkIssueToThread,
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
const THREAD_ID = `33333333-3333-4333-8333-333333333333`

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

function callerWith(extra: Record<string, unknown> = {}) {
  return issuesRouter.createCaller({
    session: { user: { id: `actor` } },
    db: { transaction },
    request: new Request(`http://localhost/`),
    ...extra,
  } as never)
}
const caller = callerWith()

describe(`issues.create threadId (EXP-1284)`, () => {
  beforeEach(() => {
    insertedIssues.length = 0
    h.getThread.mockReset()
    h.getThread.mockResolvedValue({ id: THREAD_ID, teamId: `ws-1` })
    h.linkIssueToThread.mockClear()
    transaction.mockClear()
  })

  it(`links the new issue into the thread inside the create transaction`, async () => {
    await caller.create({ boardId: BOARD_ID, title: `Filed here`, threadId: THREAD_ID })

    expect(h.linkIssueToThread).toHaveBeenCalledTimes(1)
    expect(h.linkIssueToThread).toHaveBeenCalledWith(
      tx,
      { id: THREAD_ID, teamId: `ws-1` },
      ISSUE_ID,
      { userId: `actor`, source: `user` }
    )
    expect(transaction).toHaveBeenCalledTimes(1)
  })

  it(`stamps an agent's create (MCP) as source agent`, async () => {
    await callerWith({ viaMcp: true }).create({
      boardId: BOARD_ID,
      title: `Filed by the agent`,
      threadId: THREAD_ID,
    })

    expect(h.linkIssueToThread.mock.calls[0]![3]).toEqual({
      userId: `actor`,
      source: `agent`,
    })
  })

  it.each([
    [`another team's thread`, { id: THREAD_ID, teamId: `ws-2` }],
    [`a missing thread`, null],
  ])(`refuses %s before anything is written`, async (_name, thread) => {
    h.getThread.mockResolvedValue(thread)

    await expect(
      caller.create({ boardId: BOARD_ID, title: `Nope`, threadId: THREAD_ID })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })

    expect(transaction).not.toHaveBeenCalled()
    expect(insertedIssues).toHaveLength(0)
    expect(h.linkIssueToThread).not.toHaveBeenCalled()
  })

  it(`never touches threads without a threadId`, async () => {
    await caller.create({ boardId: BOARD_ID, title: `Standalone` })

    expect(h.getThread).not.toHaveBeenCalled()
    expect(h.linkIssueToThread).not.toHaveBeenCalled()
    expect(insertedIssues).toHaveLength(1)
  })
})
