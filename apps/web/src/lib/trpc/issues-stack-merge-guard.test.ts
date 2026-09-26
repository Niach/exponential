import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1094 on `issues.mergePr({mergeStack: true})` (Reviews' "Merge stack",
// MCP `pr_merge({mergeStack})`, which routes through the same procedure): the
// entry issue alone was guarded, but a stack lands as a unit, so a plain PR
// stacked on a live workflow node's PR would have landed the node PR beneath
// it. Every member is asked; one covered member refuses the whole merge,
// named, before any claim or GitHub call.

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  assertIssueAccess: vi.fn(async () => ({
    issueId: `issue-1`,
    boardId: `board-1`,
    teamId: `ws-1`,
  })),
  liveWorkflowCoveringPr: vi.fn(
    async (
      _executor: unknown,
      _issueId: string,
      _prUrl: string
    ): Promise<{ workflowId: string; nodeId: string; issueId: string } | null> => null
  ),
  mergePullRequestSmart: vi.fn(async () => ({
    merged: true,
    queued: false,
    sha: `abc`,
    viaStack: false,
    stackNumber: null,
    stackMemberNumbers: [241],
  })),
  getPullRequest: vi.fn(async () => ({
    state: `open` as const,
    merged: false,
    draft: false,
    headRef: `exp/EXP-11`,
    baseRef: `master`,
    mergeable: true,
    mergeableState: `clean`,
  })),
  applyPrMergeState: vi.fn(async () => {}),
  endMergedPrSessions: vi.fn(async () => {}),
}))

vi.mock(`@/db/connection`, () => {
  const tail = { where: () => ({ limit: async () => [] }) }
  return {
    db: {
      select: () => ({
        from: () => ({ ...tail, innerJoin: () => tail }),
      }),
    },
  }
})
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/workflows`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/workflows")>()),
  liveWorkflowCoveringPr: h.liveWorkflowCoveringPr,
}))
vi.mock(`@/lib/team-membership`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/team-membership")>()),
  assertIssueAccess: h.assertIssueAccess,
}))
vi.mock(`@/lib/integrations/github-pr`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-pr")>()),
  fetchPullFiles: vi.fn(),
  mergePullRequestSmart: h.mergePullRequestSmart,
  findStackForPull: vi.fn(async () => null),
  getPullRequest: h.getPullRequest,
  closePullRequest: vi.fn(),
  resolvePrBaseState: vi.fn(),
  retargetPullRequest: vi.fn(async () => {}),
  diagnoseUnmergeablePr: vi.fn(async () => null),
}))
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => true,
  resolveRepoInstallationTokenInfo: async () => ({
    token: `tok`,
    installationId: 77,
    expiresAt: null,
  }),
  resolveRepoDefaultBranchCached: async () => `master`,
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  isInstallationLinkedToTeam: async () => true,
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrClosedState: vi.fn(),
  applyPrMergeState: h.applyPrMergeState,
  endMergedPrSessions: h.endMergedPrSessions,
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

import { WORKFLOW_MERGE_REFUSAL, issuesRouter } from "@/lib/trpc/issues"
import {
  _clearPrActorClaims,
  takePrMergeClaim,
} from "@/lib/integrations/pr-actor-claims"

const LOWER_ISSUE = `22222222-2222-4222-8222-222222222222`
const UPPER_ISSUE = `44444444-4444-4444-8444-444444444444`
const LOWER_PR_URL = `https://github.com/owner/repo/pull/241`
const UPPER_PR_URL = `https://github.com/owner/repo/pull/242`

const db = {
  update: vi.fn(() => ({ set: () => ({ where: async () => undefined }) })),
  select: vi.fn(() => {
    const rows = h.selectQueue.shift() ?? []
    const builder = {
      from: () => builder,
      where: () => builder,
      limit: async () => rows,
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

// The entry is the UPPER, plain PR (EXP-12); the node PR (EXP-11) sits
// beneath it on a candidate stack (base-branch edges only).
const upperEntryRow = {
  prNumber: 242,
  prUrl: UPPER_PR_URL,
  prState: `open`,
  identifier: `EXP-12`,
  title: `Upper`,
  branch: `exp/EXP-12`,
  prBaseBranch: `exp/EXP-11`,
  prStackNumber: null,
}
const stackRows = [
  {
    id: LOWER_ISSUE,
    identifier: `EXP-11`,
    title: `Lower`,
    status: `in_review`,
    branch: `exp/EXP-11`,
    prUrl: LOWER_PR_URL,
    prNumber: 241,
    prState: `open`,
    prBaseBranch: `master`,
    prStackNumber: null,
  },
  {
    id: UPPER_ISSUE,
    identifier: `EXP-12`,
    title: `Upper`,
    status: `in_review`,
    branch: `exp/EXP-12`,
    prUrl: UPPER_PR_URL,
    prNumber: 242,
    prState: `open`,
    prBaseBranch: `exp/EXP-11`,
    prStackNumber: null,
  },
]

beforeEach(() => {
  h.selectQueue.length = 0
  vi.clearAllMocks()
  h.liveWorkflowCoveringPr.mockResolvedValue(null)
  h.assertIssueAccess.mockResolvedValue({
    issueId: UPPER_ISSUE,
    boardId: `board-1`,
    teamId: `ws-1`,
  })
  _clearPrActorClaims()
})

describe(`issues.mergePr({mergeStack}) under a live workflow (EXP-1094)`, () => {
  it(`refuses the stack when a member BENEATH the entry is a live node's PR`, async () => {
    h.selectQueue.push([upperEntryRow])
    h.selectQueue.push(stackRows)
    h.liveWorkflowCoveringPr.mockImplementation(async (_executor, issueId) =>
      issueId === LOWER_ISSUE
        ? { workflowId: `wf-1`, nodeId: `n-1`, issueId: LOWER_ISSUE }
        : null
    )

    await expect(
      caller.mergePr({ issueId: UPPER_ISSUE, mergeStack: true })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `EXP-11's pull request ${WORKFLOW_MERGE_REFUSAL}`,
    })

    // Nothing touched GitHub, nothing was claimed.
    expect(h.getPullRequest).not.toHaveBeenCalled()
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 241)).toBeNull()
    expect(takePrMergeClaim(`owner/repo`, 242)).toBeNull()
  })

  it(`asks about every open member with its own PR url, then merges`, async () => {
    h.selectQueue.push([upperEntryRow])
    h.selectQueue.push(stackRows)

    await expect(
      caller.mergePr({ issueId: UPPER_ISSUE, mergeStack: true })
    ).resolves.toMatchObject({ merged: true })

    // The entry guard (issue path) plus one probe per member, bottom-up.
    const asked = h.liveWorkflowCoveringPr.mock.calls.map(([, issueId, prUrl]) => [issueId, prUrl])
    expect(asked).toEqual([
      [UPPER_ISSUE, UPPER_PR_URL],
      [LOWER_ISSUE, LOWER_PR_URL],
      [UPPER_ISSUE, UPPER_PR_URL],
    ])
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(2)
  })
})
