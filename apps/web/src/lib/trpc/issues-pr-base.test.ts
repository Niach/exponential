import { afterAll, beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"

// EXP-324 stacked-PR coverage: `issues.prepareConflictFix` (the fix-conflicts
// launch resolver — heals a dead base and returns the live rebase target),
// `issues.retargetPr` (the agent-facing base change), and the `mergePr` 405
// diagnosis. The named regression here is the EXP-320 shape: a child PR
// stacked on a parent that was squash-merged with its branch left undeleted.

const h = vi.hoisted(() => ({
  // Each ctx.db.select() call consumes the next result set, in call order.
  selectQueue: [] as unknown[][],
  // The `pr_base_branch` writes the router issued (set values, in order).
  updates: [] as Array<Record<string, unknown>>,
  assertIssueAccess: vi.fn(async () => ({
    issueId: `issue-1`,
    boardId: `board-1`,
    teamId: `ws-1`,
  })),
  resolvePrBaseState: vi.fn(),
  retargetPullRequest: vi.fn(async () => {}),
  // EXP-1139: the description rewrite.
  updatePullRequest: vi.fn(async () => {}),
  getIssueTeamContext: vi.fn(async () => ({
    teamId: `ws-1`,
    boardId: `board-1`,
  })),
  assertTeamMember: vi.fn(async () => undefined),
  diagnoseUnmergeablePr: vi.fn(
    async (): Promise<{ message: string; conflict: boolean } | null> => null
  ),
  mergePullRequestSmart: vi.fn(
    async (
      _opts: Record<string, unknown>
    ): Promise<{
      merged: boolean
      queued: boolean
      sha: string | null
      mergedBy: { login: string; type: string; id?: number } | null
    }> => ({ merged: true, queued: false, sha: `abc`, mergedBy: null })
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
  // The awaited EXP-324 heal behind a merge whose parent already merged.
  retargetChildrenOfMergedPr: vi.fn(
    async (_opts: { prUrl: string; headBranch: string; teamId?: string }) => {}
  ),
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
    getIssueTeamContext: h.getIssueTeamContext,
    assertTeamMember: h.assertTeamMember,
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
    getPullRequest: h.getPullRequest,
    closePullRequest: vi.fn(),
    resolvePrBaseState: h.resolvePrBaseState,
    retargetPullRequest: h.retargetPullRequest,
    updatePullRequest: h.updatePullRequest,
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
  applySessionPrState: vi.fn(async () => ({ endedSessionIds: [] })),
  endMergedPrSessions: h.endMergedPrSessions,
  retargetChildrenOfMergedPr: h.retargetChildrenOfMergedPr,
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
  basedOnMergedPr,
  stackedOnMessage,
  stackedOnOpenPr,
} from "@/lib/pr-merge-guard"
import {
  classifyPrBase,
  GitHubAsyncMergePending,
  GitHubMergeError,
} from "@/lib/integrations/github-pr"
import {
  _clearPrActorClaims,
  takePrMergeClaim,
} from "@/lib/integrations/pr-actor-claims"

const ISSUE_ID = `22222222-2222-4222-8222-222222222222`
const PR_URL = `https://github.com/owner/repo/pull/241`

const db = {
  update: vi.fn(() => ({
    set: (values: Record<string, unknown>) => ({
      where: async () => {
        h.updates.push(values)
      },
    }),
  })),
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
  h.updates.length = 0
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
    // The synced base follows the heal.
    // Each write lands on the issue rows and the run rows (EXP-1165).
    expect(h.updates).toEqual([
      { prBaseBranch: `master` },
      { prBaseBranch: `master` },
    ])
  })

  it(`returns the live parent branch as the rebase target for an open parent — no retarget`, async () => {
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
    // GitHub's live base is mirrored opportunistically.
    // Each write lands on the issue rows and the run rows (EXP-1165).
    expect(h.updates).toEqual([
      { prBaseBranch: `exp/EXP-314` },
      { prBaseBranch: `exp/EXP-314` },
    ])
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
    // Each write lands on the issue rows and the run rows (EXP-1165).
    expect(h.updates).toEqual([
      { prBaseBranch: `master` },
      { prBaseBranch: `master` },
    ])
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
    // Persisted for every issue on the PR.
    // Each write lands on the issue rows and the run rows (EXP-1165).
    expect(h.updates).toEqual([
      { prBaseBranch: `master` },
      { prBaseBranch: `master` },
    ])
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
    expect(h.updates).toEqual([])
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

describe(`issues.updatePr (EXP-1139)`, () => {
  it(`PATCHes the given fields against the repo the PR lives in`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open` },
    ])
    const result = await caller.updatePr({
      issueId: ISSUE_ID,
      body: `Closes #EXP-320\n\nNow with the picker.`,
    })
    expect(h.updatePullRequest).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 241,
      title: undefined,
      body: `Closes #EXP-320\n\nNow with the picker.`,
      token: `tok`,
    })
    expect(result).toEqual({ updated: true, url: PR_URL, number: 241 })
  })

  it(`refuses a call that changes nothing`, async () => {
    await expect(
      caller.updatePr({ issueId: ISSUE_ID })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(h.updatePullRequest).not.toHaveBeenCalled()
  })

  it(`refuses a non-open PR`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `merged` },
    ])
    await expect(
      caller.updatePr({ issueId: ISSUE_ID, title: `Late rename` })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `The pull request is merged. Only open pull requests can be edited.`,
    })
    expect(h.updatePullRequest).not.toHaveBeenCalled()
  })

  it(`maps GitHub's 404 onto NOT_FOUND`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open` },
    ])
    h.updatePullRequest.mockRejectedValueOnce(
      new GitHubMergeError(404, `Not Found`)
    )
    await expect(
      caller.updatePr({ issueId: ISSUE_ID, title: `x` })
    ).rejects.toMatchObject({
      code: `NOT_FOUND`,
      message: `Pull request not found on GitHub`,
    })
  })

  it(`refuses a severed installation link`, async () => {
    h.selectQueue.push([
      { prNumber: 241, prUrl: PR_URL, prState: `open` },
    ])
    h.isInstallationLinkedToTeam.mockResolvedValueOnce(false)
    await expect(
      caller.updatePr({ issueId: ISSUE_ID, title: `x` })
    ).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    expect(h.updatePullRequest).not.toHaveBeenCalled()
  })
})

describe(`issues.prDescription (EXP-1139)`, () => {
  it(`reads the title and body off GitHub`, async () => {
    h.selectQueue.push([{ prNumber: 241, prUrl: PR_URL }])
    h.getPullRequest.mockResolvedValueOnce({
      state: `open`,
      merged: false,
      draft: false,
      headRef: `exp/EXP-320`,
      baseRef: `master`,
      mergeable: true,
      mergeableState: `clean`,
      title: `EXP-320: Stacked child`,
      body: `Closes #EXP-320`,
      url: PR_URL,
    } as never)
    const result = await caller.prDescription({ issueId: ISSUE_ID })
    expect(result).toEqual({
      repo: `owner/repo`,
      prNumber: 241,
      url: PR_URL,
      title: `EXP-320: Stacked child`,
      body: `Closes #EXP-320`,
      state: `open`,
    })
    expect(h.getPullRequest).toHaveBeenCalledWith(`owner/repo`, 241, `tok`)
  })

  it(`reports merged as its own state`, async () => {
    h.selectQueue.push([{ prNumber: 241, prUrl: PR_URL }])
    h.getPullRequest.mockResolvedValueOnce({
      state: `closed`,
      merged: true,
      draft: false,
      headRef: `exp/EXP-320`,
      baseRef: `master`,
      mergeable: null,
      mergeableState: null,
      title: `t`,
      body: ``,
      url: PR_URL,
    } as never)
    const result = await caller.prDescription({ issueId: ISSUE_ID })
    expect(result.state).toBe(`merged`)
  })

  it(`answers nulls for an issue without a PR`, async () => {
    h.selectQueue.push([{ prNumber: null, prUrl: null }])
    const result = await caller.prDescription({ issueId: ISSUE_ID })
    expect(result).toEqual({
      repo: null,
      prNumber: null,
      url: null,
      title: null,
      body: null,
      state: null,
    })
    expect(h.getPullRequest).not.toHaveBeenCalled()
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

// FEED-43: the merge-async fallback (the stack merge: issues-merge-stack.test.ts).
describe(`issues.mergePr merge-async outcomes`, () => {
  const entryRow = {
    prNumber: 241,
    prUrl: PR_URL,
    prState: `open`,
    identifier: `EXP-11`,
    title: `Lower`,
  }

  it(`reports a still-running merge-async job without failing the issue`, async () => {
    h.selectQueue.push([entryRow])
    h.mergePullRequestSmart.mockRejectedValueOnce(
      new GitHubAsyncMergePending(241, `u-1`)
    )

    await expect(caller.mergePr({ issueId: ISSUE_ID })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `GitHub is still merging PR #241. It did not finish within 60s — check the PR on GitHub; the issue completes when the merge lands.`,
    })
  })

  it(`reports an enqueued merge as queued, NOT merged, and completes nothing yet`, async () => {
    h.selectQueue.push([entryRow])
    h.mergePullRequestSmart.mockResolvedValueOnce({
      merged: true,
      queued: true,
      sha: null,
      mergedBy: null,
    })

    // FEED-43 R1: the queue may still reject it, so an agent reading
    // `merged: true` would end its run on a merge that never landed.
    await expect(caller.mergePr({ issueId: ISSUE_ID })).resolves.toMatchObject({
      merged: false,
      queued: true,
    })
    expect(h.applyPrMergeState).not.toHaveBeenCalled()
  })

  it(`releases the claim when the merge fails`, async () => {
    _clearPrActorClaims()
    h.selectQueue.push([entryRow])
    h.mergePullRequestSmart.mockRejectedValueOnce(
      new GitHubMergeError(405, `Squash merges are not allowed on this repository`)
    )

    await expect(caller.mergePr({ issueId: ISSUE_ID })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
    })
    expect(takePrMergeClaim(`owner/repo`, 241)).toBeNull()
  })
})

// EXP-1145: a PR whose recorded base is another OPEN PR's head would squash
// INTO that branch: the diff never reaches the default branch while the issue
// flips to Done. Refused before any claim or GitHub call, evaluated per merge
// so a parent-then-child sequence (MCP `pr_merge({issueIds: [root, child]})`,
// the yolo tree merge) passes this guard once the parent landed; the
// merged-parent check (its own suite below) then takes over.
describe(`issues.mergePr on a PR stacked on an open PR (EXP-1145)`, () => {
  const UPPER_ISSUE = `44444444-4444-4444-8444-444444444444`
  const childRow = {
    prNumber: 242,
    prUrl: `https://github.com/owner/repo/pull/242`,
    prState: `open`,
    identifier: `EXP-12`,
    title: `Upper`,
    prBaseBranch: `exp/EXP-11`,
  }

  beforeEach(() => {
    _clearPrActorClaims()
    h.assertIssueAccess.mockResolvedValue({
      issueId: UPPER_ISSUE,
      boardId: `board-1`,
      teamId: `ws-1`,
    })
  })

  it(`refuses the merge, naming the parent, before any claim or GitHub call`, async () => {
    h.selectQueue.push([childRow])
    h.selectQueue.push([{ identifier: `EXP-11` }])

    await expect(caller.mergePr({ issueId: UPPER_ISSUE })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `This pull request is stacked on EXP-11; merge EXP-11 first`,
    })
    expect(stackedOnMessage(`EXP-11`)).toBe(
      `This pull request is stacked on EXP-11; merge EXP-11 first`
    )
    expect(h.resolveRepoInstallationTokenInfo).not.toHaveBeenCalled()
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 242)).toBeNull()
  })

  it(`names an issue-less parent run PR by its number`, async () => {
    h.selectQueue.push([{ ...childRow, prBaseBranch: `exp/chat-1a2b3c4d` }])
    h.selectQueue.push([])
    h.selectQueue.push([{ prNumber: 240 }])

    await expect(caller.mergePr({ issueId: UPPER_ISSUE })).rejects.toMatchObject({
      message: `This pull request is stacked on #240; merge #240 first`,
    })
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
  })

  it(`merges a base nobody's PR ever used, with no heal and no GitHub read`, async () => {
    h.selectQueue.push([{ ...childRow, prBaseBranch: `release/1.x` }])
    // No OPEN PR on the base (issue, run), no MERGED one either.
    h.selectQueue.push([], [], [], [])
    h.selectQueue.push([{ id: UPPER_ISSUE }])

    await expect(caller.mergePr({ issueId: UPPER_ISSUE })).resolves.toEqual({
      merged: true,
    })
    expect(h.retargetChildrenOfMergedPr).not.toHaveBeenCalled()
    expect(h.getPullRequest).not.toHaveBeenCalled()
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
  })

  // The open-parent guard's default-branch exit: a team on `develop` with a
  // `develop → main` release PR open (its row carries branch `develop`).
  it(`merges a plain PR while a release PR is open FROM the team's branch`, async () => {
    h.selectQueue.push([{ ...childRow, prBaseBranch: `develop` }])
    // The release PR's issue has an open PR on head `develop`...
    h.selectQueue.push([{ identifier: `EXP-9` }])
    // ...but `develop` is the branch the team develops on (repo row, pins).
    h.selectQueue.push([
      { id: `repo-1`, defaultBranch: `main`, defaultBranchOverride: `develop` },
    ])
    h.selectQueue.push([])
    // No merged PR on `develop` (issue, run).
    h.selectQueue.push([], [])
    h.selectQueue.push([{ id: UPPER_ISSUE }])

    await expect(caller.mergePr({ issueId: UPPER_ISSUE })).resolves.toEqual({
      merged: true,
    })
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
  })

  it(`never looks up a parent for a PR without a recorded base`, async () => {
    h.selectQueue.push([{ ...childRow, prBaseBranch: null }])
    h.selectQueue.push([{ id: UPPER_ISSUE }])

    await expect(caller.mergePr({ issueId: UPPER_ISSUE })).resolves.toEqual({
      merged: true,
    })
    expect(db.select).toHaveBeenCalledTimes(2)
  })
})

// The parent already MERGED: `applyPrMergeState` only fires the EXP-324 heal
// (never awaits it), so a child merged right behind its root (MCP
// `pr_merge({issueIds: [root, child]})`, a retry after a stack refusal) used
// to reach GitHub while still based on the root's kept branch and was
// squashed INTO it: Done, with the code never on the default branch (the
// EXP-320 incident). The merge now awaits the heal and refuses while GitHub
// still reports the merged branch as the base.
describe(`issues.mergePr on a PR whose parent already merged`, () => {
  const ROOT_ISSUE = `55555555-5555-4555-8555-555555555555`
  const CHILD_ISSUE = `44444444-4444-4444-8444-444444444444`
  const ROOT_PR_URL = `https://github.com/owner/repo/pull/241`
  const rootRow = {
    prNumber: 241,
    prUrl: ROOT_PR_URL,
    prState: `open`,
    identifier: `EXP-11`,
    title: `Lower`,
    prBaseBranch: `master`,
  }
  const childRow = {
    prNumber: 242,
    prUrl: `https://github.com/owner/repo/pull/242`,
    prState: `open`,
    identifier: `EXP-12`,
    title: `Upper`,
    prBaseBranch: `exp/EXP-11`,
  }
  // GitHub's view of the child PR's base, and every call in order.
  let childBase = `exp/EXP-11`
  let log: string[] = []
  const pull = (baseRef: string) => ({
    state: `open` as const,
    merged: false,
    draft: false,
    headRef: `exp/EXP-12`,
    baseRef,
    mergeable: true,
    mergeableState: `clean`,
  })

  /** The child's reads up to the heal: its row, no OPEN PR on its base
   *  (issue, run), the MERGED root on it, no repo row for the team. */
  function queueChildGuard() {
    h.selectQueue.push([childRow], [], [], [{ prUrl: ROOT_PR_URL }], [])
  }

  beforeEach(() => {
    _clearPrActorClaims()
    childBase = `exp/EXP-11`
    log = []
    h.getPullRequest.mockImplementation(async () => pull(childBase))
    h.mergePullRequestSmart.mockImplementation(async (opts) => {
      const into = opts.prNumber === 242 ? childBase : `master`
      log.push(`merge #${opts.prNumber} into ${into}`)
      return { merged: true, queued: false, sha: `abc`, mergedBy: null }
    })
    // A real heal awaits GitHub (a list plus a PATCH) before the base moves.
    h.retargetChildrenOfMergedPr.mockImplementation(async (opts) => {
      await new Promise((resolve) => setTimeout(resolve, 5))
      log.push(`retarget ${opts.headBranch}`)
      childBase = `master`
    })
  })
  // Back to the file's defaults for the suites below.
  afterAll(() => {
    h.getPullRequest.mockReset()
    h.mergePullRequestSmart.mockReset()
    h.retargetChildrenOfMergedPr.mockReset()
  })

  it(`root then child in one sequence never calls the GitHub merge while the child's base is the merged branch`, async () => {
    // Root: its row, no open and no merged PR on `master`, its linked issue.
    h.selectQueue.push([rootRow], [], [], [], [], [{ id: ROOT_ISSUE }])
    queueChildGuard()
    h.selectQueue.push([{ id: CHILD_ISSUE }])

    // The MCP loop: one mergePr after the other, nothing in between. The
    // root's own heal is fire-and-forget and has not run.
    await expect(caller.mergePr({ issueId: ROOT_ISSUE })).resolves.toEqual({
      merged: true,
    })
    await expect(caller.mergePr({ issueId: CHILD_ISSUE })).resolves.toEqual({
      merged: true,
    })

    expect(log).toEqual([
      `merge #241 into master`,
      `retarget exp/EXP-11`,
      `merge #242 into master`,
    ])
    expect(h.retargetChildrenOfMergedPr).toHaveBeenCalledWith({
      prUrl: ROOT_PR_URL,
      headBranch: `exp/EXP-11`,
      teamId: `ws-1`,
    })
    // The recorded base follows GitHub's move.
    // Each write lands on the issue rows and the run rows (EXP-1165).
    expect(h.updates).toEqual([
      { prBaseBranch: `master` },
      { prBaseBranch: `master` },
    ])
  })

  it(`refuses, before any claim, when the heal left the PR on the merged branch`, async () => {
    vi.useFakeTimers()
    try {
      queueChildGuard()
      // The heal bailed silently (no token, a 422, GitHub down).
      h.retargetChildrenOfMergedPr.mockResolvedValue(undefined)

      const pending = caller.mergePr({ issueId: CHILD_ISSUE })
      const settled = expect(pending).rejects.toMatchObject({
        code: `PRECONDITION_FAILED`,
        message: `It is still based on exp/EXP-11; retarget it onto the default branch (exponential_pr_retarget) and merge again`,
      })
      await vi.runAllTimersAsync()
      await settled
      expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
      expect(takePrMergeClaim(`owner/repo`, 242)).toBeNull()
      expect(h.applyPrMergeState).not.toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })

  it(`refuses when the heal throws and the base stayed put`, async () => {
    vi.useFakeTimers()
    const errorSpy = vi.spyOn(console, `error`).mockImplementation(() => undefined)
    try {
      queueChildGuard()
      h.retargetChildrenOfMergedPr.mockRejectedValue(new Error(`boom`))

      const pending = caller.mergePr({ issueId: CHILD_ISSUE })
      const settled = expect(pending).rejects.toMatchObject({
        code: `PRECONDITION_FAILED`,
      })
      await vi.runAllTimersAsync()
      await settled
      expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    } finally {
      errorSpy.mockRestore()
      vi.useRealTimers()
    }
  })

  it(`refuses when GitHub will not show the PR's base`, async () => {
    queueChildGuard()
    h.getPullRequest.mockRejectedValue(new Error(`GitHub returned 502`))

    await expect(caller.mergePr({ issueId: CHILD_ISSUE })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `GitHub did not confirm it left exp/EXP-11, whose pull request already merged; merge again`,
    })
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 242)).toBeNull()
  })

  it(`heals from an issue-less merged parent (a run's PR) too`, async () => {
    const RUN_PR_URL = `https://github.com/owner/repo/pull/240`
    h.selectQueue.push(
      [{ ...childRow, prBaseBranch: `exp/chat-1a2b3c4d` }],
      [],
      [],
      // No issue merged on that head; a run row did.
      [],
      [{ prUrl: RUN_PR_URL }],
      []
    )
    h.selectQueue.push([{ id: CHILD_ISSUE }])
    childBase = `exp/chat-1a2b3c4d`

    await expect(caller.mergePr({ issueId: CHILD_ISSUE })).resolves.toEqual({
      merged: true,
    })
    expect(h.retargetChildrenOfMergedPr).toHaveBeenCalledWith({
      prUrl: RUN_PR_URL,
      headBranch: `exp/chat-1a2b3c4d`,
      teamId: `ws-1`,
    })
    expect(log).toEqual([`retarget exp/chat-1a2b3c4d`, `merge #242 into master`])
  })

  it(`leaves a PR on the team's branch alone though a release PR FROM it merged`, async () => {
    h.selectQueue.push(
      [{ ...childRow, prBaseBranch: `develop` }],
      [],
      [],
      // A `develop → main` release PR merged earlier...
      [{ prUrl: ROOT_PR_URL }],
      // ...but `develop` is the team's pin: no heal, no GitHub read.
      [{ id: `repo-1`, defaultBranch: `main`, defaultBranchOverride: `develop` }],
      []
    )
    h.selectQueue.push([{ id: CHILD_ISSUE }])
    childBase = `develop`

    await expect(caller.mergePr({ issueId: CHILD_ISSUE })).resolves.toEqual({
      merged: true,
    })
    expect(h.retargetChildrenOfMergedPr).not.toHaveBeenCalled()
    expect(h.getPullRequest).not.toHaveBeenCalled()
    expect(log).toEqual([`merge #242 into develop`])
  })
})

// FEED-64: the merge call failed, the PR read merged, by a PERSON on
// github.com. Their merge, their attribution: the claim goes and the state
// write is left to the webhook (`merged_by`, EXP-617). Still `merged: true`.
describe(`issues.mergePr after a merge confirmed from the PR's state (FEED-64)`, () => {
  const plainRow = {
    prNumber: 242,
    prUrl: `https://github.com/owner/repo/pull/242`,
    prState: `open`,
    identifier: `EXP-12`,
    title: `Upper`,
    prBaseBranch: `master`,
  }

  beforeEach(() => {
    _clearPrActorClaims()
  })

  it(`hands a person's merge to the webhook: claim released, nothing written`, async () => {
    h.selectQueue.push([plainRow])
    h.mergePullRequestSmart.mockResolvedValueOnce({
      merged: true,
      queued: false,
      sha: `d6ef0be6e5`,
      mergedBy: { login: `danny`, id: 7, type: `User` },
    })

    await expect(caller.mergePr({ issueId: ISSUE_ID })).resolves.toEqual({
      merged: true,
      note: `PR #242 was already merged on GitHub by danny; its issues complete when the merge webhook lands.`,
    })
    expect(h.applyPrMergeState).not.toHaveBeenCalled()
    expect(h.endMergedPrSessions).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 242)).toBeNull()
  })

  it(`treats our own App's confirmed merge exactly like a normal one`, async () => {
    h.selectQueue.push([plainRow])
    // Nobody's PR, open or merged, has `master` as its head.
    h.selectQueue.push([], [], [], [])
    h.selectQueue.push([{ id: ISSUE_ID }])
    h.mergePullRequestSmart.mockResolvedValueOnce({
      merged: true,
      queued: false,
      sha: `d6ef0be6e5`,
      mergedBy: { login: `exponential[bot]`, type: `Bot` },
    })

    await expect(caller.mergePr({ issueId: ISSUE_ID })).resolves.toEqual({
      merged: true,
    })
    expect(h.applyPrMergeState).toHaveBeenCalledTimes(1)
    // The claim stays for the webhook echo (consumed there, attributed to us).
    expect(takePrMergeClaim(`owner/repo`, 242)).toMatchObject({ userId: `actor` })
  })
})

describe(`stackedOnOpenPr (EXP-1145)`, () => {
  function recordingDb(answers: unknown[][]) {
    const wheres: unknown[] = []
    const select = vi.fn(() => {
      const rows = answers.shift() ?? []
      const p = Promise.resolve(rows) as Promise<unknown[]> &
        Record<string, (arg?: unknown) => unknown>
      for (const m of [`from`, `limit`]) p[m] = () => p
      p.where = (cond?: unknown) => {
        wheres.push(cond)
        return p
      }
      return p
    })
    return { db: { select } as never, select, wheres }
  }
  const developRepo = {
    id: `repo-1`,
    defaultBranch: `main`,
    defaultBranchOverride: `develop`,
  }

  it(`asks for the team issue whose OPEN PR head, in the same repo, is this PR's base`, async () => {
    const { db: fake, wheres } = recordingDb([[{ identifier: `EXP-11` }]])
    await expect(
      stackedOnOpenPr(fake, {
        issueId: ISSUE_ID,
        teamId: `ws-1`,
        repoFullName: `owner/repo`,
        prBaseBranch: `exp/EXP-11`,
      })
    ).resolves.toBe(`EXP-11`)
    // The parent lookup, then the team's repo row (none here).
    expect(wheres).toHaveLength(2)
    const { PgDialect } = await import(`drizzle-orm/pg-core`)
    expect(new PgDialect().sqlToQuery(wheres[1] as never).params).toEqual([
      `ws-1`,
      `owner/repo`,
    ])
    const query = new PgDialect().sqlToQuery(wheres[0] as never)
    expect(query.sql).toContain(`"team_id" =`)
    expect(query.sql).toContain(`"id" <>`)
    expect(query.sql).toContain(`"branch" =`)
    expect(query.sql).toContain(`"pr_state" =`)
    expect(query.sql).toContain(`"pr_url" like`)
    expect(query.params).toEqual([
      `ws-1`,
      ISSUE_ID,
      `exp/EXP-11`,
      `open`,
      `https://github.com/owner/repo/pull/%`,
    ])
  })

  it(`falls back to an issue-less run's open PR on that branch`, async () => {
    const { db: fake, wheres } = recordingDb([[], [{ prNumber: 240 }]])
    await expect(
      stackedOnOpenPr(fake, {
        issueId: ISSUE_ID,
        teamId: `ws-1`,
        repoFullName: `owner/repo`,
        prBaseBranch: `exp/chat-1a2b3c4d`,
      })
    ).resolves.toBe(`#240`)
    const { PgDialect } = await import(`drizzle-orm/pg-core`)
    const query = new PgDialect().sqlToQuery(wheres[1] as never)
    expect(query.sql).toContain(`"coding_sessions"."branch" =`)
    expect(query.params).toEqual([
      `ws-1`,
      `exp/chat-1a2b3c4d`,
      `open`,
      `https://github.com/owner/repo/pull/%`,
    ])
  })

  it(`answers null when nobody's open PR has that head`, async () => {
    const { db: fake } = recordingDb([[], []])
    await expect(
      stackedOnOpenPr(fake, {
        issueId: ISSUE_ID,
        teamId: `ws-1`,
        repoFullName: `owner/repo`,
        prBaseBranch: `master`,
      })
    ).resolves.toBeNull()
  })

  it(`never queries for a PR without a recorded base`, async () => {
    const { db: fake, select } = recordingDb([])
    await expect(
      stackedOnOpenPr(fake, {
        issueId: ISSUE_ID,
        teamId: `ws-1`,
        repoFullName: `owner/repo`,
        prBaseBranch: null,
      })
    ).resolves.toBeNull()
    expect(select).not.toHaveBeenCalled()
  })

  // A team on `develop` opens its `develop → main` release PR through
  // `pr_open`: a row now carries branch `develop` with an OPEN PR, and every
  // plain PR of the repo records base `develop`. None of them is stacked.
  it.each([
    [`the team's pin`, `develop`, [] as unknown[]],
    [`GitHub's stored default`, `main`, [] as unknown[]],
    [`a board's pin`, `release/1.x`, [{ defaultBranch: `release/1.x` }]],
  ])(`a base that is %s is never a parent`, async (_label, base, pins) => {
    const issueParent = recordingDb([[{ identifier: `EXP-9` }], [developRepo], pins])
    await expect(
      stackedOnOpenPr(issueParent.db, {
        issueId: ISSUE_ID,
        teamId: `ws-1`,
        repoFullName: `owner/repo`,
        prBaseBranch: base,
      })
    ).resolves.toBeNull()
    // The same exit for an issue-less run's open PR on that branch.
    const runParent = recordingDb([[], [{ prNumber: 240 }], [developRepo], pins])
    await expect(
      stackedOnOpenPr(runParent.db, {
        issueId: ISSUE_ID,
        teamId: `ws-1`,
        repoFullName: `owner/repo`,
        prBaseBranch: base,
      })
    ).resolves.toBeNull()
  })

  it(`still refuses a real parent in a repo with pins`, async () => {
    const { db: fake } = recordingDb([[{ identifier: `EXP-11` }], [developRepo], []])
    await expect(
      stackedOnOpenPr(fake, {
        issueId: ISSUE_ID,
        teamId: `ws-1`,
        repoFullName: `owner/repo`,
        prBaseBranch: `exp/EXP-11`,
      })
    ).resolves.toBe(`EXP-11`)
  })

  it(`asks nothing about the repo while no PR sits on the base`, async () => {
    const { db: fake, select } = recordingDb([[], []])
    await stackedOnOpenPr(fake, {
      issueId: ISSUE_ID,
      teamId: `ws-1`,
      repoFullName: `owner/repo`,
      prBaseBranch: `develop`,
    })
    expect(select).toHaveBeenCalledTimes(2)
  })
})

describe(`basedOnMergedPr`, () => {
  function recordingDb(answers: unknown[][]) {
    const wheres: unknown[] = []
    const select = vi.fn(() => {
      const rows = answers.shift() ?? []
      const p = Promise.resolve(rows) as Promise<unknown[]> &
        Record<string, (arg?: unknown) => unknown>
      for (const m of [`from`, `limit`]) p[m] = () => p
      p.where = (cond?: unknown) => {
        wheres.push(cond)
        return p
      }
      return p
    })
    return { db: { select } as never, select, wheres }
  }
  const opts = {
    teamId: `ws-1`,
    repoFullName: `owner/repo`,
    prBaseBranch: `exp/EXP-11`,
  }

  it(`finds the team issue whose MERGED PR head, in the same repo, is this PR's base`, async () => {
    const { db: fake, wheres } = recordingDb([[{ prUrl: PR_URL }], []])
    await expect(basedOnMergedPr(fake, opts)).resolves.toEqual({ prUrl: PR_URL })
    const { PgDialect } = await import(`drizzle-orm/pg-core`)
    const query = new PgDialect().sqlToQuery(wheres[0] as never)
    expect(query.sql).toContain(`"branch" =`)
    expect(query.sql).toContain(`"pr_state" =`)
    expect(query.params).toEqual([
      `ws-1`,
      `exp/EXP-11`,
      `merged`,
      `https://github.com/owner/repo/pull/%`,
    ])
  })

  it(`falls back to an issue-less run's merged PR on that branch`, async () => {
    const { db: fake, wheres } = recordingDb([[], [{ prUrl: PR_URL }], []])
    await expect(basedOnMergedPr(fake, opts)).resolves.toEqual({ prUrl: PR_URL })
    const { PgDialect } = await import(`drizzle-orm/pg-core`)
    const query = new PgDialect().sqlToQuery(wheres[1] as never)
    expect(query.sql).toContain(`"coding_sessions"."branch" =`)
    expect(query.params).toEqual([
      `ws-1`,
      `exp/EXP-11`,
      `merged`,
      `https://github.com/owner/repo/pull/%`,
    ])
  })

  it(`answers null when no merged PR has that head, or without a recorded base`, async () => {
    const none = recordingDb([[], []])
    await expect(basedOnMergedPr(none.db, opts)).resolves.toBeNull()
    const noBase = recordingDb([])
    await expect(
      basedOnMergedPr(noBase.db, { ...opts, prBaseBranch: null })
    ).resolves.toBeNull()
    expect(noBase.select).not.toHaveBeenCalled()
  })

  it(`answers null for a base the repo is developed on (a merged release PR from it)`, async () => {
    const { db: fake } = recordingDb([
      [{ prUrl: PR_URL }],
      [{ id: `repo-1`, defaultBranch: `main`, defaultBranchOverride: `develop` }],
      [],
    ])
    await expect(
      basedOnMergedPr(fake, { ...opts, prBaseBranch: `develop` })
    ).resolves.toBeNull()
  })
})

describe(`error type sanity`, () => {
  it(`the router rethrows TRPCError instances unchanged`, () => {
    expect(new TRPCError({ code: `NOT_FOUND` })).toBeInstanceOf(TRPCError)
  })
})
