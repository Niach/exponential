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
  diagnoseUnmergeablePr: vi.fn(
    async (): Promise<{ message: string; conflict: boolean } | null> => null
  ),
  // EXP-897: the router merges through the stack-aware entry point.
  mergePullRequestSmart: vi.fn(
    async (
      _opts: Record<string, unknown>
    ): Promise<{
      merged: boolean
      queued: boolean
      sha: string | null
      viaStack: boolean
      stackNumber: number | null
      stackMemberNumbers: number[]
    }> => ({
      merged: true,
      queued: false,
      sha: `abc`,
      viaStack: false,
      stackNumber: null,
      stackMemberNumbers: [241],
    })
  ),
  findStackForPull: vi.fn(
    async (): Promise<{ number: number } | null> => null
  ),
  getPullRequest: vi.fn(async () => ({
    state: `open` as const,
    merged: false,
    draft: false,
    headRef: `exp/EXP-320`,
    baseRef: `master`,
    mergeable: true,
    mergeableState: `clean`,
  })),
  resolveRepoDefaultBranchCached: vi.fn(async (): Promise<string | null> => `master`),
  resolveRepoInstallationTokenInfo: vi.fn(async () => ({
    token: `tok`,
    installationId: 77,
    expiresAt: null,
  })),
  isInstallationLinkedToTeam: vi.fn(async () => true),
  applyPrMergeState: vi.fn(async () => {}),
  endMergedPrSessions: vi.fn(async () => {}),
}))

// membership.ts's getDb() dynamically imports @/db/connection; this mock also
// satisfies lib/trpc.ts's module-scope `db` import without a live Postgres.
vi.mock(`@/db/connection`, () => {
  // EXP-712: boardBranchOverride joins boards → repositories; no board pin.
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
    mergePullRequestSmart: h.mergePullRequestSmart,
    findStackForPull: h.findStackForPull,
    getPullRequest: h.getPullRequest,
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

import { issuesRouter } from "@/lib/trpc/issues"
import {
  classifyPrBase,
  GitHubAsyncMergePending,
  GitHubMergeError,
} from "@/lib/integrations/github-pr"

const ISSUE_ID = `22222222-2222-4222-8222-222222222222`
const PR_URL = `https://github.com/owner/repo/pull/241`

const db = {
  // EXP-897: the router persists `pr_base_branch` / `pr_stack_number`.
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
      message: `'nope' is not a valid base branch on owner/repo: Proposed base branch 'nope' was not found`,
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
      message: `The pull request is merged. Only open pull requests can be retargeted.`,
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

  it(`replaces GitHub's bare "not mergeable" with the stale-base diagnosis (412, not a conflict)`, async () => {
    h.selectQueue.push([mergeRow])
    h.mergePullRequestSmart.mockRejectedValueOnce(
      new GitHubMergeError(405, `Pull Request is not mergeable`)
    )
    h.diagnoseUnmergeablePr.mockResolvedValueOnce({
      conflict: false,
      message: `Pull Request is not mergeable: its base branch 'exp/EXP-314' is the head of already-merged PR #240. Retarget this PR to 'master' (call exponential_pr_retarget), rebase onto origin/master if needed, then retry the merge.`,
    })

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

  // EXP-533: a real content conflict is the ONE case a rebase-and-resolve run
  // fixes, so it (and only it) answers CONFLICT/409.
  it(`answers CONFLICT for a real content conflict`, async () => {
    h.selectQueue.push([mergeRow])
    h.mergePullRequestSmart.mockRejectedValueOnce(
      new GitHubMergeError(405, `Pull Request is not mergeable`)
    )
    h.diagnoseUnmergeablePr.mockResolvedValueOnce({
      conflict: true,
      message: `Pull Request has merge conflicts with 'master': rebase onto origin/master, resolve the conflicts, push with --force-with-lease, then retry the merge.`,
    })

    await expect(caller.mergePr({ issueId: ISSUE_ID })).rejects.toMatchObject({
      code: `CONFLICT`,
      message: expect.stringContaining(`has merge conflicts with`),
    })
  })

  it(`keeps GitHub's message and offers the recovery run when the diagnosis cannot run`, async () => {
    h.selectQueue.push([mergeRow])
    h.mergePullRequestSmart.mockRejectedValueOnce(
      new GitHubMergeError(405, `Pull Request is not mergeable`)
    )
    h.diagnoseUnmergeablePr.mockResolvedValueOnce(null)

    await expect(
      caller.mergePr({ issueId: ISSUE_ID })
    ).rejects.toMatchObject({
      code: `CONFLICT`,
      message: `Pull Request is not mergeable`,
    })
  })

  it(`does not attempt a diagnosis for other 405 messages`, async () => {
    h.selectQueue.push([mergeRow])
    h.mergePullRequestSmart.mockRejectedValueOnce(
      new GitHubMergeError(405, `Squash merges are not allowed on this repository`)
    )

    await expect(caller.mergePr({ issueId: ISSUE_ID })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `Squash merges are not allowed on this repository`,
    })
    expect(h.diagnoseUnmergeablePr).not.toHaveBeenCalled()
  })

  // Inversion (EXP-533): GitHub's 409 is "the head branch moved under us",
  // which no conflict-recovery run addresses.
  it(`maps GitHub's 409 head-changed onto PRECONDITION_FAILED`, async () => {
    h.selectQueue.push([mergeRow])
    h.mergePullRequestSmart.mockRejectedValueOnce(
      new GitHubMergeError(409, `Head branch was modified. Review and try the merge again.`)
    )

    await expect(caller.mergePr({ issueId: ISSUE_ID })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `Head branch changed on GitHub. Refresh and try again.`,
    })
  })
})

// EXP-498: merge ALWAYS closes — the sweep runs unconditionally, on both the
// fresh-merge and the already-merged (webhook-won-the-claim) paths.
describe(`issues.mergePr always ends sessions (EXP-498)`, () => {
  const mergeRow = {
    prNumber: 241,
    prUrl: PR_URL,
    prState: `open`,
    identifier: `EXP-320`,
    title: `Stacked child`,
  }
  const OTHER_ISSUE = `33333333-3333-4333-8333-333333333333`

  it(`sweeps every linked issue's sessions after a fresh merge`, async () => {
    h.selectQueue.push([mergeRow])
    // The linked-issues resolve after the GitHub merge (batch fan-out).
    h.selectQueue.push([{ id: ISSUE_ID }, { id: OTHER_ISSUE }])

    await expect(caller.mergePr({ issueId: ISSUE_ID })).resolves.toEqual({
      merged: true,
    })
    expect(h.applyPrMergeState).toHaveBeenCalledTimes(2)
    expect(h.endMergedPrSessions).toHaveBeenCalledTimes(1)
    // EXP-711: no endSessions override on a plain merge — the team decides.
    expect(h.endMergedPrSessions).toHaveBeenCalledWith(
      [ISSUE_ID, OTHER_ISSUE],
      undefined
    )
  })

  it(`sweeps on the already-merged idempotent path too`, async () => {
    h.selectQueue.push([{ ...mergeRow, prState: `merged` }])
    h.selectQueue.push([{ id: ISSUE_ID }])

    await expect(caller.mergePr({ issueId: ISSUE_ID })).resolves.toEqual({
      merged: true,
    })
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(h.endMergedPrSessions).toHaveBeenCalledWith([ISSUE_ID], undefined)
  })
})

// EXP-897 / FEED-43: the stack-aware merge and retarget paths.
describe(`issues.mergePr on a stack (EXP-897)`, () => {
  const UPPER_ISSUE = `44444444-4444-4444-8444-444444444444`
  const UPPER_PR_URL = `https://github.com/owner/repo/pull/242`
  const entryRow = {
    prNumber: 241,
    prUrl: PR_URL,
    prState: `open`,
    identifier: `EXP-11`,
    title: `Lower`,
    branch: `exp/EXP-11`,
    prBaseBranch: `master`,
    prStackNumber: null as number | null,
  }
  const stackRows = (stackNumber: number | null) => [
    {
      id: ISSUE_ID,
      identifier: `EXP-11`,
      title: `Lower`,
      status: `in_review`,
      branch: `exp/EXP-11`,
      prUrl: PR_URL,
      prNumber: 241,
      prState: `open`,
      prBaseBranch: `master`,
      prStackNumber: stackNumber,
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
      prStackNumber: stackNumber,
    },
  ]
  const cohortRows = [
    { id: ISSUE_ID, prUrl: PR_URL, branch: `exp/EXP-11`, prBaseBranch: `master` },
    {
      id: UPPER_ISSUE,
      prUrl: UPPER_PR_URL,
      branch: `exp/EXP-12`,
      prBaseBranch: `exp/EXP-11`,
    },
  ]

  it(`merges a REAL GitHub stack in one call on its topmost open member`, async () => {
    h.selectQueue.push([{ ...entryRow, prStackNumber: 7 }])
    h.selectQueue.push(stackRows(7))
    h.selectQueue.push(cohortRows)
    h.mergePullRequestSmart.mockResolvedValueOnce({
      merged: true,
      queued: false,
      sha: `abc`,
      viaStack: true,
      stackNumber: 7,
      stackMemberNumbers: [241, 242],
    })

    const result = await caller.mergePr({
      issueId: ISSUE_ID,
      mergeStack: true,
    })

    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
    expect(h.mergePullRequestSmart).toHaveBeenCalledWith(
      expect.objectContaining({ prNumber: 242, knownStackNumber: 7 })
    )
    // Merging the top merged everything below it — both issues complete.
    expect(h.applyPrMergeState).toHaveBeenCalledTimes(2)
    expect(h.endMergedPrSessions).toHaveBeenCalledWith(
      [ISSUE_ID, UPPER_ISSUE],
      undefined
    )
    expect(result.merged).toBe(true)
  })

  it(`refuses the whole stack when one member is a draft`, async () => {
    h.selectQueue.push([{ ...entryRow, prStackNumber: 7 }])
    h.selectQueue.push(stackRows(7))
    h.getPullRequest.mockResolvedValueOnce({
      state: `open` as const,
      merged: false,
      draft: true,
      headRef: `exp/EXP-11`,
      baseRef: `master`,
      mergeable: true,
      mergeableState: `clean`,
    })

    await expect(
      caller.mergePr({ issueId: ISSUE_ID, mergeStack: true })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `Cannot merge the stack: PR #241 (EXP-11) is a draft`,
    })
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
  })

  it(`refuses a member GitHub reports as blocked`, async () => {
    h.selectQueue.push([{ ...entryRow, prStackNumber: 7 }])
    h.selectQueue.push(stackRows(7))
    h.getPullRequest.mockResolvedValueOnce({
      state: `open` as const,
      merged: false,
      draft: false,
      headRef: `exp/EXP-11`,
      baseRef: `master`,
      mergeable: false,
      mergeableState: `blocked`,
    })

    await expect(
      caller.mergePr({ issueId: ISSUE_ID, mergeStack: true })
    ).rejects.toMatchObject({
      message: `Cannot merge the stack: PR #241 (EXP-11) is blocked on GitHub`,
    })
  })

  // No GitHub stack (preview off / built by us alone): merge bottom-up,
  // retargeting each next member onto the stack base first.
  it(`merges a candidate stack bottom-up and retargets as it goes`, async () => {
    h.selectQueue.push([entryRow])
    h.selectQueue.push(stackRows(null))

    await expect(
      caller.mergePr({ issueId: ISSUE_ID, mergeStack: true })
    ).resolves.toMatchObject({
      merged: true,
      note: `Merged 2 pull request(s) bottom-up: #241, #242.`,
    })

    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(2)
    expect(h.mergePullRequestSmart.mock.calls[0]![0]).toMatchObject({
      prNumber: 241,
    })
    expect(h.mergePullRequestSmart.mock.calls[1]![0]).toMatchObject({
      prNumber: 242,
    })
    // The upper PR's base was just squash-merged — retarget before merging it.
    expect(h.retargetPullRequest).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 242,
      base: `master`,
      token: `tok`,
    })
  })

  it(`is idempotent once every member is merged`, async () => {
    h.selectQueue.push([{ ...entryRow, prState: `merged` }])
    h.selectQueue.push(
      stackRows(null).map((row) => ({ ...row, prState: `merged` }))
    )

    await expect(
      caller.mergePr({ issueId: ISSUE_ID, mergeStack: true })
    ).resolves.toEqual({ merged: true })
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(h.endMergedPrSessions).toHaveBeenCalledWith(
      [ISSUE_ID, UPPER_ISSUE],
      undefined
    )
  })

  it(`reports a still-running merge-async job without failing the issue`, async () => {
    h.selectQueue.push([entryRow])
    h.mergePullRequestSmart.mockRejectedValueOnce(
      new GitHubAsyncMergePending(241, 7, `u-1`)
    )

    await expect(caller.mergePr({ issueId: ISSUE_ID })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `GitHub is still merging PR #241 (stack #7). It did not finish within 60s — check the PR on GitHub; the issue completes when the merge lands.`,
    })
  })

  it(`reports an enqueued merge as queued and completes nothing yet`, async () => {
    h.selectQueue.push([entryRow])
    h.mergePullRequestSmart.mockResolvedValueOnce({
      merged: true,
      queued: true,
      sha: null,
      viaStack: true,
      stackNumber: 7,
      stackMemberNumbers: [241],
    })

    await expect(caller.mergePr({ issueId: ISSUE_ID })).resolves.toMatchObject({
      merged: true,
      queued: true,
    })
    expect(h.applyPrMergeState).not.toHaveBeenCalled()
  })
})

describe(`issues.retargetPr on a stack member (FEED-43)`, () => {
  it(`says the stack owns the base instead of "not a valid base branch"`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open`, prStackNumber: 7 },
    ])

    await expect(
      caller.retargetPr({ issueId: ISSUE_ID })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `PR #241 is part of GitHub stack #7 (owner/repo); merge the PR below it or merge it on GitHub; its base is managed by the stack.`,
    })
    expect(h.retargetPullRequest).not.toHaveBeenCalled()
  })

  it(`heals an unrecorded stack from GitHub before refusing`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open`, prStackNumber: null },
    ])
    h.findStackForPull.mockResolvedValueOnce({ number: 9 })

    await expect(
      caller.retargetPr({ issueId: ISSUE_ID })
    ).rejects.toMatchObject({
      message: `PR #241 is part of GitHub stack #9 (owner/repo); merge the PR below it or merge it on GitHub; its base is managed by the stack.`,
    })
    expect(db.update).toHaveBeenCalled()
  })

  it(`still retargets a PR that is in no stack`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open`, prStackNumber: null },
    ])
    await expect(caller.retargetPr({ issueId: ISSUE_ID })).resolves.toEqual({
      retargeted: true,
      base: `master`,
    })
  })
})

describe(`error type sanity`, () => {
  it(`the router rethrows TRPCError instances unchanged`, () => {
    expect(new TRPCError({ code: `NOT_FOUND` })).toBeInstanceOf(TRPCError)
  })
})
