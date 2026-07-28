import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-324: `retargetChildrenOfMergedPr` — after a stack parent merges, its
// open children (PRs based on the merged head branch) are retargeted onto the
// repo default so they never point at a dead branch (the EXP-320 shape).

const h = vi.hoisted(() => ({
  githubAppConfigured: vi.fn(() => true),
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
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
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
  getSteerRelayConfig: () => null,
  relayPostKill: vi.fn(),
}))
vi.mock(`@/lib/trpc`, () => ({ generateTxId: vi.fn() }))

import { retargetChildrenOfMergedPr } from "@/lib/integrations/pr-sync"

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
