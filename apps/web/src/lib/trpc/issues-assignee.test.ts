import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// Locks the REV-3 fix: issues.create/update must validate a provided
// assigneeId against the issue's workspace membership BEFORE the transaction.
// Without it, any member could assign issues to arbitrary users of the
// instance — ensureSubscribed + fireAndForgetAssignmentNotify would then
// subscribe and push-notify victims in workspaces they never joined
// (cross-tenant notification injection).

// Shared mock state must be defined via vi.hoisted so the (hoisted) vi.mock
// factories below can reference it without TDZ errors.
const h = vi.hoisted(() => {
  const state = {
    // Rows returned by the mocked db select chain — drives the REAL
    // getWorkspaceMember used by the real assertAssigneeInWorkspace.
    memberRows: [] as unknown[],
  }
  const fireAndForgetAssignmentNotify = vi.fn()
  return { state, fireAndForgetAssignmentNotify }
})

// membership.ts's getDb() dynamically imports @/db/connection; this mock also
// satisfies lib/trpc.ts and lib/admin.ts's module-scope `db` imports without a
// live Postgres.
vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({
      from: () => ({
        where: () => ({
          limit: async () => h.state.memberRows,
        }),
      }),
    }),
  },
}))

// lib/trpc.ts imports `auth` at module scope; runtime only needs the export.
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

// Override ONLY the actor-side lookups; the real assertAssigneeInWorkspace
// (the code under test) comes through the spread.
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
    // EXP-50 solo-workspace default-assign is covered by its own test file;
    // here it must stay inert (the db mock can't serve its joined query).
    getSoleHumanMemberId: vi.fn(async () => null),
    assertIssueAccess: vi.fn(async () => ({
      issueId: `issue-1`,
      projectId: `proj-1`,
      workspaceId: `ws-1`,
    })),
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
  ensureSubscribed: vi.fn(),
}))
vi.mock(`@/lib/integrations/activity`, () => ({
  recordIssueEvent: vi.fn(),
}))

import { issuesRouter } from "@/lib/trpc/issues"
import { assertAssigneeInWorkspace } from "@/lib/workspace-membership"

const PROJECT_ID = `11111111-1111-4111-8111-111111111111`
const ISSUE_ID = `22222222-2222-4222-8222-222222222222`

// Sentinel: the transaction body is out of scope here — reaching it proves the
// pre-transaction assignee guard passed.
const transaction = vi.fn(async () => {
  throw new Error(`TX_REACHED`)
})

const caller = issuesRouter.createCaller({
  session: { user: { id: `actor` } },
  db: { transaction },
  request: new Request(`http://localhost/`),
} as never)

const memberRow = { userId: `victim`, workspaceId: `ws-1`, role: `member` }

describe(`issues assignee workspace-membership guard (REV-3)`, () => {
  beforeEach(() => {
    h.state.memberRows = []
    h.fireAndForgetAssignmentNotify.mockClear()
    transaction.mockClear()
  })

  it(`create rejects a non-member assignee with BAD_REQUEST before the transaction`, async () => {
    const error = await caller
      .create({ projectId: PROJECT_ID, title: `Phish`, assigneeId: `outsider` })
      .then(
        () => undefined,
        (e: unknown) => e
      )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toBe(
      `Assignee must be a member of this workspace`
    )
    expect(transaction).not.toHaveBeenCalled()
    expect(h.fireAndForgetAssignmentNotify).not.toHaveBeenCalled()
  })

  it(`update rejects a non-member assignee with BAD_REQUEST before the transaction`, async () => {
    const error = await caller
      .update({ id: ISSUE_ID, assigneeId: `outsider` })
      .then(
        () => undefined,
        (e: unknown) => e
      )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toBe(
      `Assignee must be a member of this workspace`
    )
    expect(transaction).not.toHaveBeenCalled()
    expect(h.fireAndForgetAssignmentNotify).not.toHaveBeenCalled()
  })

  it(`create with a same-workspace member assignee passes the guard`, async () => {
    h.state.memberRows = [memberRow]
    await expect(
      caller.create({
        projectId: PROJECT_ID,
        title: `Legit`,
        assigneeId: `victim`,
      })
    ).rejects.toThrow(`TX_REACHED`)
  })

  it(`create without an assignee needs no membership lookup`, async () => {
    await expect(
      caller.create({ projectId: PROJECT_ID, title: `Unassigned` })
    ).rejects.toThrow(`TX_REACHED`)
  })

  it(`update with assigneeId null (unassign) skips the guard`, async () => {
    await expect(
      caller.update({ id: ISSUE_ID, assigneeId: null })
    ).rejects.toThrow(`TX_REACHED`)
  })

  it(`assertAssigneeInWorkspace rejects non-members and resolves for members`, async () => {
    const error = await assertAssigneeInWorkspace(`x`, `ws-1`).then(
      () => undefined,
      (e: unknown) => e
    )
    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)

    h.state.memberRows = [memberRow]
    await expect(assertAssigneeInWorkspace(`victim`, `ws-1`)).resolves.toBe(
      undefined
    )
  })
})
