import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// REV2-27: the duplicate/canonical pairing and the completedAt stamp are
// SERVER invariants, not client conventions — MCP and API callers reach
// issues.create/update with the full status enum. status='duplicate' may only
// exist alongside a canonical duplicateOfId (create refuses it outright; a
// bare status-only update refuses it for an unlinked row), and an issue born
// done/cancelled gets the same completedAt a transition into those states
// stamps. Fake-db harness mirrors issues-bulk.test.ts: FIFO select queue,
// recording insert/update chains, transaction() handing back the same fake.

const h = vi.hoisted(() => ({
  getBoardTeamId: vi.fn(async (..._args: unknown[]) => ({
    id: `board-1`,
    teamId: `ws-1`,
  })),
  resolveTeamAccess: vi.fn(async (..._args: unknown[]) => undefined),
  assertIssueAccess: vi.fn(async (..._args: unknown[]) => ({
    issueId: `issue-1`,
    boardId: `board-1`,
    teamId: `ws-1`,
  })),
  getSoleHumanMemberId: vi.fn(async (..._args: unknown[]) => null),
  assertAssigneeInTeam: vi.fn(async (..._args: unknown[]) => undefined),
  ensureSubscribed: vi.fn(),
  recordIssueEvent: vi.fn(),
  fireAndForgetAssignmentNotify: vi.fn(),
  fireAndForgetStatusChangeNotify: vi.fn(),
  fireAndForgetReporterResolution: vi.fn(),
}))

// lib/trpc.ts + lib/admin.ts import db/auth at module scope; runtime here only
// needs the exports to exist.
vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))

vi.mock(`@/lib/team-membership`, () => ({
  resolveTeamAccess: h.resolveTeamAccess,
  assertAssigneeInTeam: h.assertAssigneeInTeam,
  assertIssueAccess: h.assertIssueAccess,
  assertTeamMember: vi.fn(),
  getIssueTeamContext: vi.fn(),
  getBoardTeamId: h.getBoardTeamId,
  getSoleHumanMemberId: h.getSoleHumanMemberId,
  getUserTeamIds: vi.fn(async () => []),
}))

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
  applyPrMergeState: vi.fn(),
  applyPrClosedState: vi.fn(),
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
  fireAndForgetIssueMentionNotify: vi.fn(),
  fireAndForgetStatusChangeNotify: h.fireAndForgetStatusChangeNotify,
  fireAndForgetReporterResolution: h.fireAndForgetReporterResolution,
}))
vi.mock(`@/lib/integrations/mentions`, () => ({
  resolveMentions: vi.fn(async () => []),
}))
vi.mock(`@/lib/integrations/subscriptions`, () => ({
  ensureSubscribed: h.ensureSubscribed,
}))
vi.mock(`@/lib/integrations/activity`, () => ({
  recordIssueEvent: h.recordIssueEvent,
}))

import { issuesRouter } from "@/lib/trpc/issues"

const BOARD_ID = `11111111-1111-4111-8111-111111111111`
const ISSUE_ID = `22222222-2222-4222-8222-222222222222`
const CANONICAL_ID = `33333333-3333-4333-8333-333333333333`

// FIFO select queue: each select() call resolves the next seeded rows.
const selectQueue: unknown[][] = []

function selectChain(): Promise<unknown[]> & Record<string, () => unknown> {
  const p = Promise.resolve(
    selectQueue.shift() ?? []
  ) as Promise<unknown[]> & Record<string, () => unknown>
  for (const m of [`from`, `where`, `innerJoin`, `limit`, `orderBy`, `for`]) {
    p[m] = () => p
  }
  return p
}

const inserted: Record<string, unknown>[] = []
const updated: Record<string, unknown>[] = []

const fakeDb = {
  select: vi.fn(() => selectChain()),
  insert: (_table: unknown) => ({
    values: (values: Record<string, unknown>) => {
      inserted.push(values)
      return {
        returning: async () => [{ id: ISSUE_ID, assigneeId: null, ...values }],
      }
    },
  }),
  update: (_table: unknown) => ({
    set: (values: Record<string, unknown>) => ({
      where: () => ({
        returning: async () => {
          updated.push(values)
          return [{ id: ISSUE_ID, status: `backlog`, ...values }]
        },
      }),
    }),
  }),
  // generateTxId's `SELECT pg_current_xact_id()` probe.
  execute: vi.fn(async () => ({ rows: [{ txid: `77` }] })),
  transaction: vi.fn(
    async (fn: (tx: typeof fakeDb) => Promise<unknown>): Promise<unknown> =>
      fn(fakeDb)
  ),
}

const caller = issuesRouter.createCaller({
  session: { user: { id: `actor` } },
  db: fakeDb,
  request: new Request(`http://localhost/`),
} as never)

function currentIssue(
  overrides: Record<string, unknown> = {}
): Record<string, unknown> {
  return {
    description: null,
    status: `todo`,
    boardId: `board-1`,
    title: `Issue`,
    priority: `none`,
    assigneeId: null,
    duplicateOfId: null,
    ...overrides,
  }
}

async function rejectionOf(promise: Promise<unknown>): Promise<unknown> {
  return promise.then(
    () => undefined,
    (e: unknown) => e
  )
}

beforeEach(() => {
  selectQueue.length = 0
  inserted.length = 0
  updated.length = 0
  fakeDb.select.mockClear()
  fakeDb.execute.mockClear()
  fakeDb.transaction.mockClear()
  h.recordIssueEvent.mockClear()
  h.getSoleHumanMemberId.mockResolvedValue(null)
})

describe(`issues.create duplicate + completedAt invariants (REV2-27)`, () => {
  it(`refuses status 'duplicate' — a new issue has no canonical to pair with`, async () => {
    const error = await rejectionOf(
      caller.create({
        boardId: BOARD_ID,
        title: `Dupe`,
        status: `duplicate`,
      })
    )

    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect(fakeDb.transaction).not.toHaveBeenCalled()
    expect(inserted).toHaveLength(0)
  })

  it(`stamps completedAt when the issue is born done or cancelled`, async () => {
    await caller.create({ boardId: BOARD_ID, title: `Done`, status: `done` })
    await caller.create({
      boardId: BOARD_ID,
      title: `Cancelled`,
      status: `cancelled`,
    })

    expect(inserted.map((values) => values.status)).toEqual([
      `done`,
      `cancelled`,
    ])
    for (const values of inserted) {
      expect(values.completedAt).toBeInstanceOf(Date)
    }
  })

  it(`leaves completedAt null for a non-terminal initial status`, async () => {
    await caller.create({ boardId: BOARD_ID, title: `Open` })
    await caller.create({
      boardId: BOARD_ID,
      title: `In progress`,
      status: `in_progress`,
    })

    expect(inserted.map((values) => values.status)).toEqual([
      `backlog`,
      `in_progress`,
    ])
    for (const values of inserted) {
      expect(values.completedAt).toBeNull()
    }
  })
})

describe(`issues.update bare duplicate status (REV2-27)`, () => {
  it(`refuses status 'duplicate' with no canonical link on an unlinked issue`, async () => {
    selectQueue.push([currentIssue()])

    const error = await rejectionOf(
      caller.update({ id: ISSUE_ID, status: `duplicate` })
    )

    expect(error).toBeInstanceOf(TRPCError)
    expect((error as TRPCError).code).toBe(`BAD_REQUEST`)
    expect((error as TRPCError).message).toBe(
      `Duplicate requires a canonical issue`
    )
    expect(updated).toHaveLength(0)
  })

  it(`allows a bare 'duplicate' restatement on an already-linked issue`, async () => {
    selectQueue.push([
      currentIssue({ status: `duplicate`, duplicateOfId: CANONICAL_ID }),
    ])

    await caller.update({ id: ISSUE_ID, status: `duplicate` })

    expect(updated).toHaveLength(1)
    // Redundant terminal write — the original completion time stands.
    expect(updated[0]!.completedAt).toBeUndefined()
    expect(updated[0]!.duplicateOfId).toBeUndefined()
  })

  it(`still marks a duplicate when the canonical issue comes with it`, async () => {
    selectQueue.push([currentIssue()])
    selectQueue.push([{ teamId: `ws-1` }]) // canonical lookup

    await caller.update({
      id: ISSUE_ID,
      status: `duplicate`,
      duplicateOfId: CANONICAL_ID,
    })

    expect(updated).toHaveLength(1)
    expect(updated[0]!.status).toBe(`duplicate`)
    expect(updated[0]!.duplicateOfId).toBe(CANONICAL_ID)
    expect(updated[0]!.completedAt).toBeInstanceOf(Date)
  })

  it(`still unmarks a duplicate when duplicateOfId is explicitly null`, async () => {
    selectQueue.push([
      currentIssue({ status: `duplicate`, duplicateOfId: CANONICAL_ID }),
    ])

    await caller.update({
      id: ISSUE_ID,
      status: `duplicate`,
      duplicateOfId: null,
    })

    expect(updated).toHaveLength(1)
    expect(updated[0]!.status).toBe(`backlog`)
    expect(updated[0]!.duplicateOfId).toBeNull()
    expect(updated[0]!.completedAt).toBeNull()
  })
})
