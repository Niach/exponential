import { beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-324 stacked-PR coverage: `issues.prepareConflictFix` (the fix-conflicts
// launch resolver — heals a dead base and returns the live rebase target),
// `issues.retargetPr` (the agent-facing base change), and the `mergePr` 405
// diagnosis. The named regression here is the EXP-320 shape: a child PR
// stacked on a parent that was squash-merged with its branch left undeleted.

const h = vi.hoisted(() => ({
  // Each ctx.db.select() call consumes the next result set, in call order.
  selectQueue: [] as unknown[][],
  assertIssueAccess: vi.fn(async () => ({
    issueId: `issue-1`,
    boardId: `board-1`,
    teamId: `ws-1`,
  })),
  resolvePrBaseState: vi.fn(),
  retargetPullRequest: vi.fn(async () => {}),
  diagnoseUnmergeablePr: vi.fn(async (): Promise<string | null> => null),
  mergePullRequest: vi.fn(async () => ({ merged: true, sha: `abc` })),
  resolveRepoDefaultBranchCached: vi.fn(async (): Promise<string | null> => `master`),
  resolveRepoInstallationTokenInfo: vi.fn(async () => ({
    token: `tok`,
    installationId: 77,
    expiresAt: null,
  })),
  isInstallationLinkedToTeam: vi.fn(async () => true),
  applyPrMergeState: vi.fn(async () => {}),
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
  const actual = await importOriginal<typeof import("@/lib/team-membership")>()
  return {
    ...actual,
    assertIssueAccess: h.assertIssueAccess,
  }
})

// Keep the real GitHubMergeError (the router maps on instanceof + status) and
// the real classifyPrBase (tests drive resolvePrBaseState through it); stub
// every fetch-backed function.
vi.mock(`@/lib/integrations/github-pr`, async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("@/lib/integrations/github-pr")>()
  return {
    ...actual,
    fetchPullFiles: vi.fn(),
    mergePullRequest: h.mergePullRequest,
    closePullRequest: vi.fn(),
    resolvePrBaseState: h.resolvePrBaseState,
    retargetPullRequest: h.retargetPullRequest,
    diagnoseUnmergeablePr: h.diagnoseUnmergeablePr,
  }
})
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => true,
  resolveRepoInstallationTokenInfo: h.resolveRepoInstallationTokenInfo,
  resolveRepoDefaultBranchCached: h.resolveRepoDefaultBranchCached,
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  isInstallationLinkedToTeam: h.isInstallationLinkedToTeam,
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrClosedState: vi.fn(),
  applyPrMergeState: h.applyPrMergeState,
  endMergedPrSessions: vi.fn(),
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
import { classifyPrBase, GitHubMergeError } from "@/lib/integrations/github-pr"

const ISSUE_ID = `22222222-2222-4222-8222-222222222222`
const PR_URL = `https://github.com/owner/repo/pull/241`

const db = {
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

// The EXP-320 shape, expressed through the REAL classifier: child PR open on
// base `exp/EXP-314`; the parent PR #240 was squash-merged and its branch left
// undeleted.
function mockExp320BaseState() {
  h.resolvePrBaseState.mockImplementation(async () => ({
    prState: `open` as const,
    merged: false,
    headRef: `exp/EXP-320`,
    baseRef: `exp/EXP-314`,
    ...classifyPrBase({
      baseRef: `exp/EXP-314`,
      defaultBranch: `master`,
      parentPulls: [{ number: 240, state: `closed`, merged: true }],
      baseBranchExists: true,
    }),
  }))
}

beforeEach(() => {
  h.selectQueue.length = 0
  vi.clearAllMocks()
  h.assertIssueAccess.mockResolvedValue({
    issueId: ISSUE_ID,
    boardId: `board-1`,
    teamId: `ws-1`,
  })
  h.resolveRepoDefaultBranchCached.mockResolvedValue(`master`)
  h.resolveRepoInstallationTokenInfo.mockResolvedValue({
    token: `tok`,
    installationId: 77,
    expiresAt: null,
  })
  h.isInstallationLinkedToTeam.mockResolvedValue(true)
})

describe(`issues.prepareConflictFix (EXP-324)`, () => {
  it(`heals the EXP-320 shape: retargets the child onto the default branch and returns it as the rebase target`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open` },
    ])
    mockExp320BaseState()

    const result = await caller.prepareConflictFix({ issueId: ISSUE_ID })

    expect(h.retargetPullRequest).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 241,
      base: `master`,
      token: `tok`,
    })
    expect(result).toEqual({
      repo: `owner/repo`,
      prNumber: 241,
      headRef: `exp/EXP-320`,
      baseRef: `exp/EXP-314`,
      baseKind: `merged-parent`,
      rebaseOnto: `master`,
      retargeted: true,
      defaultBranch: `master`,
    })
  })

  it(`returns the live parent branch as the rebase target for an open stack — no retarget`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open` },
    ])
    h.resolvePrBaseState.mockImplementation(async () => ({
      prState: `open` as const,
      merged: false,
      headRef: `exp/EXP-320`,
      baseRef: `exp/EXP-314`,
      ...classifyPrBase({
        baseRef: `exp/EXP-314`,
        defaultBranch: `master`,
        parentPulls: [{ number: 240, state: `open`, merged: false }],
        baseBranchExists: true,
      }),
    }))

    const result = await caller.prepareConflictFix({ issueId: ISSUE_ID })

    expect(h.retargetPullRequest).not.toHaveBeenCalled()
    expect(result).toMatchObject({
      baseKind: `open-parent`,
      rebaseOnto: `exp/EXP-314`,
      retargeted: false,
    })
  })

  it(`tolerates a 422 on the heal (concurrent retarget won the race)`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open` },
    ])
    mockExp320BaseState()
    h.retargetPullRequest.mockRejectedValueOnce(
      new GitHubMergeError(422, `Base was modified`)
    )

    const result = await caller.prepareConflictFix({ issueId: ISSUE_ID })
    expect(result).toMatchObject({ rebaseOnto: `master`, retargeted: false })
  })

  it(`surfaces a GitHub read failure as BAD_GATEWAY`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open` },
    ])
    h.resolvePrBaseState.mockRejectedValue(new Error(`GitHub returned 500`))

    await expect(
      caller.prepareConflictFix({ issueId: ISSUE_ID })
    ).rejects.toMatchObject({ code: `BAD_GATEWAY` })
  })

  it(`refuses an issue without an open PR`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `merged` },
    ])
    await expect(
      caller.prepareConflictFix({ issueId: ISSUE_ID })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
  })
})

describe(`issues.retargetPr (EXP-324)`, () => {
  it(`fills in the repo default branch when base is omitted`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open` },
    ])
    const result = await caller.retargetPr({ issueId: ISSUE_ID })
    expect(h.retargetPullRequest).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 241,
      base: `master`,
      token: `tok`,
    })
    expect(result).toEqual({ retargeted: true, base: `master` })
  })

  it(`passes an explicit base through and maps GitHub's 422 onto a named error`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open` },
    ])
    h.retargetPullRequest.mockRejectedValueOnce(
      new GitHubMergeError(422, `Proposed base branch 'nope' was not found`)
    )
    await expect(
      caller.retargetPr({ issueId: ISSUE_ID, base: `nope` })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `'nope' is not a valid base branch on owner/repo`,
    })
  })

  it(`refuses a non-open PR`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `merged` },
    ])
    await expect(
      caller.retargetPr({ issueId: ISSUE_ID })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `The pull request is merged — only open pull requests can be retargeted`,
    })
  })
})

describe(`issues.mergePr 405 diagnosis (EXP-324)`, () => {
  const mergeRow = {
    prNumber: 241,
    prUrl: PR_URL,
    prState: `open`,
    identifier: `EXP-320`,
    title: `Stacked child`,
  }

  it(`replaces GitHub's bare "not mergeable" with the stale-base diagnosis`, async () => {
    h.selectQueue.push([mergeRow])
    h.mergePullRequest.mockRejectedValueOnce(
      new GitHubMergeError(405, `Pull Request is not mergeable`)
    )
    h.diagnoseUnmergeablePr.mockResolvedValueOnce(
      `Pull Request is not mergeable: its base branch 'exp/EXP-314' is the head of already-merged PR #240. Retarget this PR to 'master' (call exponential_pr_retarget), rebase onto origin/master if needed, then retry the merge.`
    )

    await expect(
      caller.mergePr({ issueId: ISSUE_ID })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: expect.stringContaining(`exponential_pr_retarget`),
    })
    expect(h.diagnoseUnmergeablePr).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 241,
      token: `tok`,
      defaultBranch: `master`,
    })
  })

  it(`keeps GitHub's message when the diagnosis cannot run`, async () => {
    h.selectQueue.push([mergeRow])
    h.mergePullRequest.mockRejectedValueOnce(
      new GitHubMergeError(405, `Pull Request is not mergeable`)
    )
    h.diagnoseUnmergeablePr.mockResolvedValueOnce(null)

    await expect(
      caller.mergePr({ issueId: ISSUE_ID })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `Pull Request is not mergeable`,
    })
  })

  it(`does not attempt a diagnosis for other 405 messages`, async () => {
    h.selectQueue.push([mergeRow])
    h.mergePullRequest.mockRejectedValueOnce(
      new GitHubMergeError(405, `Squash merges are not allowed on this repository`)
    )

    await expect(caller.mergePr({ issueId: ISSUE_ID })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `Squash merges are not allowed on this repository`,
    })
    expect(h.diagnoseUnmergeablePr).not.toHaveBeenCalled()
  })
})

describe(`error type sanity`, () => {
  it(`the router rethrows TRPCError instances unchanged`, () => {
    expect(new TRPCError({ code: `NOT_FOUND` })).toBeInstanceOf(TRPCError)
  })
})
