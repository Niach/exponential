import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1291 — `issues.create({ sessionId })`, the thread page's "new issue"
// and MCP `exponential_issues_create`: the issue joins the session's thread
// (its ROOT). Same two rules as the parentId link (issues-parent.test.ts,
// whose harness this borrows):
//
//   1. the thread link is written INSIDE the create transaction, so the
//      session_issues row lands under the txId the client awaits;
//   2. the session is validated BEFORE the transaction opens (visible to the
//      caller and the board's team), so a refusal leaves no issue behind.

const h = vi.hoisted(() => ({
  getSoleHumanMemberId: vi.fn(async (): Promise<string | null> => null),
  ensureSubscribed: vi.fn(),
  fireAndForgetAssignmentNotify: vi.fn(),
  insertRelationInTx: vi.fn(async (_tx: unknown, _args: unknown) => null),
  loadSessionForUser: vi.fn(),
  linkIssues: vi.fn(async (..._args: unknown[]) => ({ linked: [] })),
}))

vi.mock(`@/lib/sessions/access`, () => ({
  loadSessionForUser: h.loadSessionForUser,
}))
vi.mock(`@/lib/sessions/issues`, () => ({
  linkIssues: h.linkIssues,
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
const SESSION_ID = `33333333-3333-4333-8333-333333333333`
const ROOT_ID = `44444444-4444-4444-8444-444444444444`

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

describe(`issues.create sessionId (EXP-1291)`, () => {
  beforeEach(() => {
    insertedIssues.length = 0
    h.loadSessionForUser.mockReset()
    h.loadSessionForUser.mockResolvedValue({ id: SESSION_ID, rootId: ROOT_ID, teamId: `ws-1` })
    h.linkIssues.mockClear()
    transaction.mockClear()
  })

  it(`links the new issue into the session's thread inside the create transaction`, async () => {
    await caller.create({ boardId: BOARD_ID, title: `Filed here`, sessionId: SESSION_ID })

    expect(h.loadSessionForUser).toHaveBeenCalledWith(`actor`, SESSION_ID)
    expect(h.linkIssues).toHaveBeenCalledTimes(1)
    expect(h.linkIssues).toHaveBeenCalledWith(tx, {
      sessionId: SESSION_ID,
      issueIds: [ISSUE_ID],
      source: `user`,
      addedBy: `actor`,
      addedBySessionId: null,
      filed: true,
    })
    expect(transaction).toHaveBeenCalledTimes(1)
  })

  it(`stamps an agent's create (MCP) as source agent`, async () => {
    await callerWith({ viaMcp: true }).create({
      boardId: BOARD_ID,
      title: `Filed by the agent`,
      sessionId: SESSION_ID,
    })

    expect(h.linkIssues.mock.calls[0]![1]).toMatchObject({ source: `agent` })
  })

  it.each([
    [`another team's session`, async () => ({ id: SESSION_ID, rootId: ROOT_ID, teamId: `ws-2` })],
    [`an invisible session`, async () => Promise.reject(new Error(`Session not found`))],
  ])(`refuses %s before anything is written`, async (_name, load) => {
    h.loadSessionForUser.mockImplementation(load as never)

    await expect(
      caller.create({ boardId: BOARD_ID, title: `Nope`, sessionId: SESSION_ID })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })

    expect(transaction).not.toHaveBeenCalled()
    expect(insertedIssues).toHaveLength(0)
    expect(h.linkIssues).not.toHaveBeenCalled()
  })

  it(`never touches threads without a sessionId`, async () => {
    await caller.create({ boardId: BOARD_ID, title: `Standalone` })

    expect(h.loadSessionForUser).not.toHaveBeenCalled()
    expect(h.linkIssues).not.toHaveBeenCalled()
    expect(insertedIssues).toHaveLength(1)
  })
})
