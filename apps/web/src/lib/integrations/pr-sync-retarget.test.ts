import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-324: `retargetChildrenOfMergedPr` — after a parent PR merges, its
// open children (PRs based on the merged head branch) are retargeted onto the
// repo default so they never point at a dead branch (the EXP-320 shape).

const h = vi.hoisted(() => ({
  githubAppConfigured: vi.fn(() => true),
  getSteerRelayConfig: vi.fn((): { url: string; secret: string } | null => null),
  updates: [] as Array<Record<string, unknown>>,
  resolveRepoDefaultBranchCached: vi.fn(
    async (): Promise<string | null> => `master`
  ),
  resolveRepoInstallationTokenInfo: vi.fn(async () => ({
    token: `tok`,
    installationId: 77,
    expiresAt: null,
  })),
  listOpenPullsByBase: vi.fn(
    async (): Promise<Array<{ number: number; url: string; headRef: string }>> => []
  ),
  retargetPullRequest: vi.fn(async () => {}),
  // The linked-issue → repo-row lookup (EXP-462 override resolution + the
  // EXP-466 raw-default guard): rows the chainable select mock below
  // resolves with.
  linkedRepoRows: [] as Array<{
    defaultBranch: string
    defaultBranchOverride: string | null
  }>,
}))

// One chainable builder: `.limit()` serves the linked-repo lookup.
vi.mock(`@/db/connection`, () => {
  const chain: Record<string, unknown> = {}
  Object.assign(chain, {
    from: () => chain,
    innerJoin: () => chain,
    leftJoin: () => chain,
    where: () => chain,
    limit: async () => h.linkedRepoRows,
  })
  const db: Record<string, unknown> = {
    select: () => chain,
    update: () => ({
      set: (values: Record<string, unknown>) => ({
        where: async () => {
          h.updates.push(values)
        },
      }),
    }),
  }
  // The open/closed flip writers run in a transaction whose `tx` is the
  // same recorder.
  db.transaction = (fn: (tx: unknown) => Promise<unknown>) => fn(db)
  return { db }
})
vi.mock(`@/lib/integrations/github-pr`, () => ({
  listOpenPullsByBase: h.listOpenPullsByBase,
  retargetPullRequest: h.retargetPullRequest,
}))
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: h.githubAppConfigured,
  resolveRepoDefaultBranchCached: h.resolveRepoDefaultBranchCached,
  resolveRepoInstallationTokenInfo: h.resolveRepoInstallationTokenInfo,
}))
vi.mock(`@/lib/integrations/activity`, () => ({ recordIssueEvent: vi.fn() }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetPrNotify: vi.fn(),
}))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: h.getSteerRelayConfig,
  relayPostKill: vi.fn(),
}))
vi.mock(`@/lib/trpc`, () => ({ generateTxId: vi.fn() }))

import {
  applyPrBaseBranchEdit,
  applyPrClosedState,
  applyPrReopenedState,
  retargetChildrenOfMergedPr,
} from "@/lib/integrations/pr-sync"

const PARENT_PR_URL = `https://github.com/owner/repo/pull/240`

beforeEach(() => {
  vi.clearAllMocks()
  h.githubAppConfigured.mockReturnValue(true)
  h.resolveRepoDefaultBranchCached.mockResolvedValue(`master`)
  h.resolveRepoInstallationTokenInfo.mockResolvedValue({
    token: `tok`,
    installationId: 77,
    expiresAt: null,
  })
  h.listOpenPullsByBase.mockResolvedValue([])
  h.getSteerRelayConfig.mockReturnValue(null)
  h.linkedRepoRows = []
  h.updates.length = 0
})

describe(`retargetChildrenOfMergedPr (EXP-324)`, () => {
  it(`retargets every open child onto the default branch`, async () => {
    h.listOpenPullsByBase.mockResolvedValue([
      {
        number: 241,
        url: `https://github.com/owner/repo/pull/241`,
        headRef: `exp/EXP-320`,
      },
      {
        number: 242,
        url: `https://github.com/owner/repo/pull/242`,
        headRef: `exp/EXP-321`,
      },
    ])

    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `exp/EXP-314`,
    })

    expect(h.listOpenPullsByBase).toHaveBeenCalledWith(
      `owner/repo`,
      `exp/EXP-314`,
      `tok`
    )
    expect(h.retargetPullRequest).toHaveBeenCalledTimes(2)
    expect(h.retargetPullRequest).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 241,
      base: `master`,
      token: `tok`,
    })
    expect(h.retargetPullRequest).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 242,
      base: `master`,
      token: `tok`,
    })
  })

  it(`one failing child never blocks the rest`, async () => {
    const errorSpy = vi
      .spyOn(console, `error`)
      .mockImplementation(() => undefined)
    h.listOpenPullsByBase.mockResolvedValue([
      { number: 241, url: `u1`, headRef: `exp/EXP-320` },
      { number: 242, url: `u2`, headRef: `exp/EXP-321` },
    ])
    h.retargetPullRequest.mockRejectedValueOnce(new Error(`boom`))

    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `exp/EXP-314`,
    })

    expect(h.retargetPullRequest).toHaveBeenCalledTimes(2)
    expect(errorSpy).toHaveBeenCalled()
    errorSpy.mockRestore()
  })

  it(`retargets onto the team's pinned branch when an override is set (EXP-462)`, async () => {
    h.linkedRepoRows = [
      { defaultBranch: `master`, defaultBranchOverride: `develop` },
    ]
    h.listOpenPullsByBase.mockResolvedValue([
      { number: 241, url: `u1`, headRef: `exp/EXP-320` },
    ])

    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `exp/EXP-314`,
    })

    expect(h.retargetPullRequest).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 241,
      base: `develop`,
      token: `tok`,
    })
    // The override answered the question — GitHub is never asked.
    expect(h.resolveRepoDefaultBranchCached).not.toHaveBeenCalled()
  })

  it(`the head-is-default bail compares against the pinned branch, not GitHub's`, async () => {
    h.linkedRepoRows = [
      { defaultBranch: `master`, defaultBranchOverride: `develop` },
    ]
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `develop`,
    })
    expect(h.listOpenPullsByBase).not.toHaveBeenCalled()
    expect(h.retargetPullRequest).not.toHaveBeenCalled()
  })

  it(`with an override pinned, a head equal to the RAW default still bails (EXP-466)`, async () => {
    // Would otherwise sweep every master-based PR (prod/hotfix) onto the pin.
    h.linkedRepoRows = [
      { defaultBranch: `master`, defaultBranchOverride: `develop` },
    ]
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `master`,
    })
    expect(h.listOpenPullsByBase).not.toHaveBeenCalled()
    expect(h.retargetPullRequest).not.toHaveBeenCalled()
    // The row answered both compares — GitHub is still never asked.
    expect(h.resolveRepoDefaultBranchCached).not.toHaveBeenCalled()
  })

  it(`bails when the merged head IS the default branch (would match every default-based PR)`, async () => {
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `master`,
    })
    expect(h.listOpenPullsByBase).not.toHaveBeenCalled()
    expect(h.retargetPullRequest).not.toHaveBeenCalled()
  })

  it(`bails silently on a non-GitHub PR URL`, async () => {
    await retargetChildrenOfMergedPr({
      prUrl: `https://gitlab.com/owner/repo/-/merge_requests/1`,
      headBranch: `exp/EXP-314`,
    })
    expect(h.listOpenPullsByBase).not.toHaveBeenCalled()
  })

  it(`bails silently when the GitHub App is not configured`, async () => {
    h.githubAppConfigured.mockReturnValue(false)
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `exp/EXP-314`,
    })
    expect(h.resolveRepoDefaultBranchCached).not.toHaveBeenCalled()
    expect(h.listOpenPullsByBase).not.toHaveBeenCalled()
  })

  it(`bails silently when the default branch cannot be resolved`, async () => {
    h.resolveRepoDefaultBranchCached.mockResolvedValue(null)
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `exp/EXP-314`,
    })
    expect(h.listOpenPullsByBase).not.toHaveBeenCalled()
  })

  // A follow-up tree forks: every child of the merged root is retargeted,
  // and GitHub's 422 on one of them never blocks its sibling.
  it(`retargets two children of one merged parent, tolerating a 422 on one`, async () => {
    const errorSpy = vi
      .spyOn(console, `error`)
      .mockImplementation(() => undefined)
    h.listOpenPullsByBase.mockResolvedValue([
      { number: 241, url: `https://github.com/owner/repo/pull/241`, headRef: `exp/EXP-320` },
      { number: 242, url: `https://github.com/owner/repo/pull/242`, headRef: `exp/EXP-321` },
    ])
    h.retargetPullRequest.mockRejectedValueOnce(
      Object.assign(new Error(`Validation Failed`), { status: 422 })
    )

    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `exp/EXP-314`,
    })

    expect(h.retargetPullRequest).toHaveBeenCalledTimes(2)
    expect(h.retargetPullRequest).toHaveBeenNthCalledWith(1, {
      repo: `owner/repo`,
      prNumber: 241,
      base: `master`,
      token: `tok`,
    })
    expect(h.retargetPullRequest).toHaveBeenNthCalledWith(2, {
      repo: `owner/repo`,
      prNumber: 242,
      base: `master`,
      token: `tok`,
    })
    // The synced base follows only the retarget that landed (242).
    expect(h.updates).toEqual([{ prBaseBranch: `master` }])
    errorSpy.mockRestore()
  })

  it(`bails silently when no installation token resolves`, async () => {
    h.resolveRepoInstallationTokenInfo.mockResolvedValue(
      null as unknown as { token: string; installationId: number; expiresAt: null }
    )
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `exp/EXP-314`,
    })
    expect(h.listOpenPullsByBase).not.toHaveBeenCalled()
  })
})

// A closed PR targets nothing: its `pr_base_branch` goes with the flip, and
// the reopen leg writes back the base GitHub reports.
describe(`applyPrClosedState / applyPrReopenedState`, () => {
  it(`close clears pr_base_branch with the flip`, async () => {
    await applyPrClosedState({ issueId: `issue-2`, prUrl: PARENT_PR_URL })
    expect(h.updates).toEqual([{ prState: `closed`, prBaseBranch: null }])
  })

  it(`reopen restores the base it was handed, and leaves it cleared otherwise`, async () => {
    await applyPrReopenedState({
      issueId: `issue-2`,
      prUrl: PARENT_PR_URL,
      baseBranch: `exp/EXP-1`,
    })
    await applyPrReopenedState({ issueId: `issue-2`, prUrl: PARENT_PR_URL })
    expect(h.updates).toEqual([
      { prState: `open`, prBaseBranch: `exp/EXP-1` },
      { prState: `open` },
    ])
  })
})

describe(`applyPrBaseBranchEdit`, () => {
  it(`mirrors an edited base onto every issue on the PR`, async () => {
    await applyPrBaseBranchEdit({ prUrl: PARENT_PR_URL, baseRef: `exp/EXP-10` })
    expect(h.updates).toEqual([{ prBaseBranch: `exp/EXP-10` }])
  })

  it(`writes nothing without a base`, async () => {
    await applyPrBaseBranchEdit({ prUrl: PARENT_PR_URL, baseRef: null })
    expect(h.updates).toEqual([])
  })
})
