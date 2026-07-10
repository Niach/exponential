import { beforeEach, describe, expect, it, vi } from "vitest"

// Locks the EXP-50 server half: issues.create in a SOLO workspace (exactly one
// human member) defaults an omitted/null assigneeId to that member; an
// explicit assignee always wins; multi-member workspaces keep the unassigned
// default. Mirrors issues-assignee.test.ts's mocking, but with a functional
// transaction mock so the inserted values are observable.

const h = vi.hoisted(() => ({
  getSoleHumanMemberId: vi.fn(async (): Promise<string | null> => null),
  ensureSubscribed: vi.fn(),
  fireAndForgetAssignmentNotify: vi.fn(),
}))

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/workspace-membership`, async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("@/lib/workspace-membership")>()
  return {
    ...actual,
    getProjectWorkspaceId: vi.fn(async () => ({
      id: `proj-1`,
      workspaceId: `ws-1`,
    })),
    resolveWorkspaceAccess: vi.fn(async () => ({
      kind: `member`,
      workspace: { id: `ws-1` },
      member: { role: `member`, userId: `actor`, workspaceId: `ws-1` },
    })),
    // Explicit-assignee validation is covered by issues-assignee.test.ts.
    assertAssigneeInWorkspace: vi.fn(async () => undefined),
    getSoleHumanMemberId: h.getSoleHumanMemberId,
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
  resolveProjectRepository: vi.fn(),
}))
vi.mock(`@/lib/storage/issue-attachments`, () => ({
  canonicalizeMarkdownImageUrls: vi.fn(),
  extractAttachmentIdsFromDescription: vi.fn(),
  hasMarkdownImages: () => false,
}))
vi.mock(`@/lib/storage/issue-attachment-cleanup`, () => ({
  collectAndDeleteRemovedAttachmentsInTx: vi.fn(),
  collectAndDeleteUnreferencedAttachmentsInTx: vi.fn(),
  collectIssueAttachmentStorageKeysInTx: vi.fn(),
  deleteStorageObjects: vi.fn(),
}))
vi.mock(`@/lib/issue-recurrence`, () => ({
  cloneIssueForRecurrence: vi.fn(),
  copyRecurrenceAttachments: vi.fn(),
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

const PROJECT_ID = `11111111-1111-4111-8111-111111111111`
const ISSUE_ID = `22222222-2222-4222-8222-222222222222`

// Functional transaction mock: records inserted issue values and returns a
// row echoing them (like drizzle's .returning()).
const insertedIssues: Array<Record<string, unknown>> = []
const tx = {
  // generateTxId's `SELECT pg_current_xact_id()` probe.
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

const caller = issuesRouter.createCaller({
  session: { user: { id: `actor` } },
  db: { transaction },
  request: new Request(`http://localhost/`),
} as never)

describe(`issues.create solo-workspace default assignment (EXP-50)`, () => {
  beforeEach(() => {
    insertedIssues.length = 0
    h.getSoleHumanMemberId.mockClear()
    h.getSoleHumanMemberId.mockResolvedValue(null)
    h.ensureSubscribed.mockClear()
    h.fireAndForgetAssignmentNotify.mockClear()
    transaction.mockClear()
  })

  it(`defaults an omitted assignee to the sole human member`, async () => {
    h.getSoleHumanMemberId.mockResolvedValue(`actor`)

    const result = await caller.create({ projectId: PROJECT_ID, title: `Solo` })

    expect(h.getSoleHumanMemberId).toHaveBeenCalledWith(`ws-1`)
    expect(insertedIssues[0]?.assigneeId).toBe(`actor`)
    expect(
      (result as { issue: { assigneeId: string | null } }).issue.assigneeId
    ).toBe(`actor`)
    // The assignee auto-subscribe fires alongside the creator's.
    expect(h.ensureSubscribed).toHaveBeenCalledWith(
      tx,
      expect.objectContaining({ userId: `actor`, source: `assignee` })
    )
    // The notify helper is still invoked with the defaulted assignee; the
    // real implementation self-filters when assignee === actor (asserted in
    // notifications-new-issue.test.ts).
    expect(h.fireAndForgetAssignmentNotify).toHaveBeenCalledWith(
      expect.objectContaining({ actorUserId: `actor`, newAssigneeId: `actor` })
    )
  })

  it(`defaults an explicit null assignee too`, async () => {
    h.getSoleHumanMemberId.mockResolvedValue(`actor`)

    await caller.create({
      projectId: PROJECT_ID,
      title: `Solo null`,
      assigneeId: null,
    })

    expect(insertedIssues[0]?.assigneeId).toBe(`actor`)
  })

  it(`leaves multi-member workspaces unassigned`, async () => {
    const result = await caller.create({
      projectId: PROJECT_ID,
      title: `Team`,
    })

    expect(h.getSoleHumanMemberId).toHaveBeenCalledWith(`ws-1`)
    expect(insertedIssues[0]?.assigneeId).toBeNull()
    expect(
      (result as { issue: { assigneeId: string | null } }).issue.assigneeId
    ).toBeNull()
    expect(h.ensureSubscribed).not.toHaveBeenCalledWith(
      tx,
      expect.objectContaining({ source: `assignee` })
    )
  })

  it(`never overrides an explicit assignee`, async () => {
    h.getSoleHumanMemberId.mockResolvedValue(`someone-else`)

    await caller.create({
      projectId: PROJECT_ID,
      title: `Explicit`,
      assigneeId: `chosen`,
    })

    expect(h.getSoleHumanMemberId).not.toHaveBeenCalled()
    expect(insertedIssues[0]?.assigneeId).toBe(`chosen`)
  })
})
