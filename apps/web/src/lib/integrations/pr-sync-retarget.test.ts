import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-324: `retargetChildrenOfMergedPr` — after a stack parent merges, its
// open children (PRs based on the merged head branch) are retargeted onto the
// repo default so they never point at a dead branch (the EXP-320 shape).

const h = vi.hoisted(() => ({
  githubAppConfigured: vi.fn(() => true),
  getSteerRelayConfig: vi.fn((): { url: string; secret: string } | null => null),
  findStackForPull: vi.fn(
    async (): Promise<{ number: number } | null> => null
  ),
  updates: [] as Array<Record<string, unknown>>,
  relayPostInput: vi.fn(async () => ({ delivered: true })),
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
  // EXP-897: whatever the next flat (non-limit) select resolves with —
  // stacked children, or the live sessions of a foundation's dependants.
  awaitRows: [] as Array<Record<string, unknown>>,
}))

// One chainable builder: `.limit()` serves the linked-repo lookup, awaiting
// the builder directly serves the flat reads (EXP-897's stacked-children and
// live-session queries).
vi.mock(`@/db/connection`, () => {
  const chain: Record<string, unknown> = {}
  Object.assign(chain, {
    from: () => chain,
    innerJoin: () => chain,
    leftJoin: () => chain,
    where: () => chain,
    limit: async () => h.linkedRepoRows,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    then: (res: any, rej: any) => Promise.resolve(h.awaitRows).then(res, rej),
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
  // FEED-43 R1: the open/closed flip writers run in a transaction whose `tx`
  // is the same recorder.
  db.transaction = (fn: (tx: unknown) => Promise<unknown>) => fn(db)
  return { db }
})
vi.mock(`@/lib/integrations/github-pr`, () => ({
  findStackForPull: h.findStackForPull,
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
  relayPostInput: h.relayPostInput,
}))
vi.mock(`@/lib/trpc`, () => ({ generateTxId: vi.fn() }))

import {
  applyPrClosedState,
  applyPrReopenedState,
  foundationChangeMessage,
  notifyStackedChildrenOfFoundationChange,
  refreshPrStackState,
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
  h.relayPostInput.mockResolvedValue({ delivered: true })
  h.linkedRepoRows = []
  h.awaitRows = []
  h.updates.length = 0
  h.findStackForPull.mockResolvedValue(null)
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

  // EXP-897: a real GitHub stack owns its members' bases.
  it(`does nothing when the MERGED PR was itself a GitHub stack member`, async () => {
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `exp/EXP-314`,
      stackNumber: 7,
    })
    expect(h.listOpenPullsByBase).not.toHaveBeenCalled()
    expect(h.retargetPullRequest).not.toHaveBeenCalled()
  })

  it(`skips a child that is a GitHub stack member and retargets the rest`, async () => {
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
    h.awaitRows = [{ prUrl: `https://github.com/owner/repo/pull/241` }]

    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `exp/EXP-314`,
    })

    expect(h.retargetPullRequest).toHaveBeenCalledTimes(1)
    expect(h.retargetPullRequest).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 242,
      base: `master`,
      token: `tok`,
    })
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

// EXP-897: a foundation that gains commits leaves every run stacked on it out
// of date — and only the agent inside those runs can rebase.
describe(`notifyStackedChildrenOfFoundationChange (EXP-897)`, () => {
  const RELAY = { url: `https://relay.test`, secret: `s` }

  it(`names the PR, the branch and the exact recovery, once per live run`, async () => {
    h.getSteerRelayConfig.mockReturnValue(RELAY)
    h.awaitRows = [{ id: `session-1` }, { id: `session-2` }]

    const result = await notifyStackedChildrenOfFoundationChange({
      repoFullName: `owner/repo`,
      headRef: `exp/EXP-10`,
      prNumber: 240,
      prUrl: PARENT_PR_URL,
    })

    expect(result.notified).toEqual([`session-1`, `session-2`])
    expect(h.relayPostInput).toHaveBeenCalledWith(
      RELAY,
      `session-1`,
      `[Exponential] foundation changed — the PR you are stacked on (owner/repo#240, branch exp/EXP-10) got new commits. Rebase onto origin/exp/EXP-10, push with --force-with-lease, then continue.`
    )
  })

  it(`dedupes a force-push storm within the window`, async () => {
    h.getSteerRelayConfig.mockReturnValue(RELAY)
    h.awaitRows = [{ id: `session-dedupe` }]
    const args = {
      repoFullName: `owner/repo`,
      headRef: `exp/EXP-42`,
      prNumber: 240,
      prUrl: PARENT_PR_URL,
    }
    await notifyStackedChildrenOfFoundationChange(args)
    h.awaitRows = [{ id: `session-dedupe` }]
    const second = await notifyStackedChildrenOfFoundationChange(args)
    expect(h.relayPostInput).toHaveBeenCalledTimes(1)
    expect(second.notified).toEqual([])
  })

  it(`does nothing without a relay or without a branch`, async () => {
    h.awaitRows = [{ id: `session-1` }]
    await notifyStackedChildrenOfFoundationChange({
      repoFullName: `owner/repo`,
      headRef: `exp/EXP-10`,
      prNumber: 240,
      prUrl: PARENT_PR_URL,
    })
    expect(h.relayPostInput).not.toHaveBeenCalled()

    h.getSteerRelayConfig.mockReturnValue(RELAY)
    await notifyStackedChildrenOfFoundationChange({
      repoFullName: `owner/repo`,
      headRef: ``,
      prNumber: 240,
      prUrl: PARENT_PR_URL,
    })
    expect(h.relayPostInput).not.toHaveBeenCalled()
  })

  it(`needs the synchronized PR itself: a PR nobody tracks is no foundation`, async () => {
    h.getSteerRelayConfig.mockReturnValue(RELAY)
    h.awaitRows = [{ id: `session-1` }]
    const result = await notifyStackedChildrenOfFoundationChange({
      repoFullName: `owner/repo`,
      headRef: `master`,
      prNumber: 240,
      prUrl: ``,
    })
    expect(result.notified).toEqual([])
    expect(h.relayPostInput).not.toHaveBeenCalled()
  })

  it(`byte-locks the message the agent reads`, () => {
    expect(foundationChangeMessage(`o/r`, 9, `feat/x`)).toBe(
      `[Exponential] foundation changed — the PR you are stacked on (o/r#9, branch feat/x) got new commits. Rebase onto origin/feat/x, push with --force-with-lease, then continue.`
    )
  })
})

// EXP-897: the stack edge can move on github.com alone — the webhook legs ask
// for a re-read.
describe(`refreshPrStackState (EXP-897)`, () => {
  const PR_URL = `https://github.com/owner/repo/pull/241`

  it(`writes the base ref and the stack number GitHub reports`, async () => {
    h.findStackForPull.mockResolvedValue({ number: 7 })
    await refreshPrStackState({
      prUrl: PR_URL,
      repoFullName: `owner/repo`,
      prNumber: 241,
      baseRef: `exp/EXP-10`,
    })
    expect(h.updates).toEqual([
      { prBaseBranch: `exp/EXP-10` },
      { prStackNumber: 7 },
    ])
  })

  it(`clears the stack number when the PR is no longer stacked`, async () => {
    await refreshPrStackState({
      prUrl: PR_URL,
      repoFullName: `owner/repo`,
      prNumber: 241,
      baseRef: `master`,
    })
    expect(h.updates).toContainEqual({ prStackNumber: null })
  })

  // Guessing "not stacked" would route the next merge through the endpoint
  // GitHub refuses (FEED-43).
  it(`leaves the recorded stack alone when the read fails`, async () => {
    h.findStackForPull.mockRejectedValue(new Error(`boom`))
    await refreshPrStackState({
      prUrl: PR_URL,
      repoFullName: `owner/repo`,
      prNumber: 241,
      baseRef: `master`,
    })
    expect(h.updates).toEqual([{ prBaseBranch: `master` }])
  })
})

// FEED-43 R1: a PR that is closed (or merged) is in no stack any more. The
// edge and the stack identity go with it, so the server's stack walk and the
// clients' nesting never hang a chain on a dead member; the reopen leg writes
// the base GitHub reports back.
describe(`applyPrClosedState / applyPrReopenedState clear and restore the stack edge (EXP-897)`, () => {
  it(`close clears pr_base_branch and pr_stack_number with the flip`, async () => {
    await applyPrClosedState({ issueId: `issue-2`, prUrl: PARENT_PR_URL })
    expect(h.updates).toEqual([
      { prState: `closed`, prBaseBranch: null, prStackNumber: null },
    ])
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
