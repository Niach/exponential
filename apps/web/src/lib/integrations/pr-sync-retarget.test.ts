import { beforeEach, describe, expect, it, vi } from "vitest"
import { getTableName } from "drizzle-orm"

// EXP-324: `retargetChildrenOfMergedPr` — after a parent PR merges, its
// open children (PRs based on the merged head branch) are retargeted onto the
// repo default so they never point at a dead branch (the EXP-320 shape).

const h = vi.hoisted(() => ({
  githubAppConfigured: vi.fn(() => true),
  getSteerRelayConfig: vi.fn((): { url: string; secret: string } | null => null),
  updates: [] as Array<Record<string, unknown>>,
  // EXP-1165: which table each of those writes hit, in order.
  updatedTables: [] as string[],
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
  // EXP-1248: a child's native `stack` field (null = a plain base chain).
  fetchPullStack: vi.fn(
    async (_opts: { prNumber: number }): Promise<{ number: number } | null> => null
  ),
  // The linked-issue → repo-row lookup (EXP-462 override resolution + the
  // EXP-466 raw-default guard): rows the chainable select mock below
  // resolves with.
  linkedRepoRows: [] as Array<{
    defaultBranch: string
    defaultBranchOverride: string | null
  }>,
  // Every select WITHOUT a join, in call order: the session flip's "does an
  // issue carry this PR" read, then the team's repo row and its board pins
  // (an issue-less merged PR). Empty once drained.
  plainSelects: [] as unknown[][],
  // What the session flip's `.returning()` yields.
  flipped: [] as Array<{ id: string; branch: string | null; teamId: string }>,
}))

// A chainable builder per select: a JOINED one is the linked-repo lookup
// (`.limit()` serves `linkedRepoRows`), a plain one drains `plainSelects`.
vi.mock(`@/db/connection`, () => {
  const select = () => {
    let joined = false
    let rows: unknown[] | null = null
    const answer = () => {
      if (joined) return h.linkedRepoRows
      rows ??= h.plainSelects.shift() ?? []
      return rows
    }
    const chain: Record<string, unknown> = {}
    Object.assign(chain, {
      from: () => chain,
      innerJoin: () => {
        joined = true
        return chain
      },
      where: () => chain,
      limit: async () => answer(),
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      then: (res: any, rej: any) => Promise.resolve(answer()).then(res, rej),
    })
    return chain
  }
  const db: Record<string, unknown> = {
    select,
    update: (table: unknown) => ({
      set: (values: Record<string, unknown>) => ({
        where: () => {
          h.updates.push(values)
          h.updatedTables.push(getTableName(table as never))
          return Object.assign(Promise.resolve(), {
            returning: async () => h.flipped,
          })
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
  fetchPullStack: h.fetchPullStack,
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
  applySessionPrState,
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
  h.fetchPullStack.mockResolvedValue(null)
  h.getSteerRelayConfig.mockReturnValue(null)
  h.linkedRepoRows = []
  h.plainSelects = []
  h.flipped = []
  h.updates.length = 0
  h.updatedTables.length = 0
})

describe(`retargetChildrenOfMergedPr (EXP-324)`, () => {
  it(`EXP-1248: skips a child GitHub holds in a native stack, retargets the plain one`, async () => {
    h.listOpenPullsByBase.mockResolvedValue([
      { number: 241, url: `https://github.com/owner/repo/pull/241`, headRef: `exp/EXP-1` },
      { number: 242, url: `https://github.com/owner/repo/pull/242`, headRef: `exp/EXP-2` },
    ])
    h.fetchPullStack.mockImplementation(async ({ prNumber }) =>
      prNumber === 241 ? { number: 7 } : null
    )
    await retargetChildrenOfMergedPr({ prUrl: PARENT_PR_URL, headBranch: `exp/EXP-320` })
    expect(h.retargetPullRequest).toHaveBeenCalledTimes(1)
    expect(h.retargetPullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ prNumber: 242, base: `master` })
    )
  })

  it(`EXP-1248: a stack read that fails retargets like before`, async () => {
    h.listOpenPullsByBase.mockResolvedValue([
      { number: 241, url: `https://github.com/owner/repo/pull/241`, headRef: `exp/EXP-1` },
    ])
    h.fetchPullStack.mockRejectedValue(new Error(`GitHub returned 502`))
    vi.spyOn(console, `error`).mockImplementation(() => {})
    await retargetChildrenOfMergedPr({ prUrl: PARENT_PR_URL, headBranch: `exp/EXP-320` })
    expect(h.retargetPullRequest).toHaveBeenCalledTimes(1)
  })

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
    // The synced base follows only the retarget that landed (242), on the
    // issue rows and the run rows carrying it.
    expect(h.updates).toEqual([
      { prBaseBranch: `master` },
      { prBaseBranch: `master` },
    ])
    expect(h.updatedTables).toEqual([`issues`, `coding_sessions`])
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

// An issue-less merged PR (a chat/action run's) has no linked issue to reach
// the team's repo row through, so the run's team does: the same EXP-466 /
// EXP-712 guards and the team's pin as the retarget target. The team here
// develops on `develop` while GitHub's default is `main`.
describe(`retargetChildrenOfMergedPr for an issue-less PR`, () => {
  const pinnedRepo = {
    id: `repo-1`,
    defaultBranch: `main`,
    defaultBranchOverride: `develop`,
  }

  beforeEach(() => {
    h.resolveRepoDefaultBranchCached.mockResolvedValue(`main`)
    h.listOpenPullsByBase.mockResolvedValue([
      { number: 241, url: `https://github.com/owner/repo/pull/241`, headRef: `exp/EXP-320` },
    ])
  })

  it(`never sweeps the team's branch: a merged develop → main promotion PR retargets nothing`, async () => {
    h.plainSelects = [[pinnedRepo], []]
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `develop`,
      teamId: `ws-1`,
    })
    expect(h.plainSelects).toEqual([])
    expect(h.listOpenPullsByBase).not.toHaveBeenCalled()
    expect(h.retargetPullRequest).not.toHaveBeenCalled()
  })

  it(`bails on a head equal to the stored GitHub default or to a board pin`, async () => {
    h.plainSelects = [[pinnedRepo], []]
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `main`,
      teamId: `ws-1`,
    })
    h.plainSelects = [[pinnedRepo], [{ defaultBranch: `release/1.x` }]]
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `release/1.x`,
      teamId: `ws-1`,
    })
    expect(h.listOpenPullsByBase).not.toHaveBeenCalled()
    expect(h.retargetPullRequest).not.toHaveBeenCalled()
  })

  it(`retargets a chat run's children onto the team's pin, not GitHub's default`, async () => {
    h.plainSelects = [[pinnedRepo], []]
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `exp/chat-1a2b3c4d`,
      teamId: `ws-1`,
    })
    expect(h.listOpenPullsByBase).toHaveBeenCalledWith(
      `owner/repo`,
      `exp/chat-1a2b3c4d`,
      `tok`
    )
    expect(h.retargetPullRequest).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 241,
      base: `develop`,
      token: `tok`,
    })
    expect(h.resolveRepoDefaultBranchCached).not.toHaveBeenCalled()
    expect(h.updates).toEqual([
      { prBaseBranch: `develop` },
      { prBaseBranch: `develop` },
    ])
  })

  it(`falls back to GitHub's default when the team has no row for the repo`, async () => {
    h.plainSelects = [[]]
    await retargetChildrenOfMergedPr({
      prUrl: PARENT_PR_URL,
      headBranch: `exp/chat-1a2b3c4d`,
      teamId: `ws-1`,
    })
    expect(h.retargetPullRequest).toHaveBeenCalledWith(
      expect.objectContaining({ base: `main` })
    )
  })

  // The session flip is the caller: the flipped run row carries the team.
  it(`applySessionPrState hands the run's team to the retarget (promotion PR)`, async () => {
    h.flipped = [{ id: `sess-1`, branch: `develop`, teamId: `ws-1` }]
    // No issue carries the PR, then the team's repo row and its board pins.
    h.plainSelects = [[], [pinnedRepo], []]

    await applySessionPrState({
      prUrl: PARENT_PR_URL,
      state: `merged`,
      endSessions: false,
    })
    await vi.waitFor(() => expect(h.plainSelects).toEqual([]))
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(h.listOpenPullsByBase).not.toHaveBeenCalled()
    expect(h.retargetPullRequest).not.toHaveBeenCalled()
  })

  it(`applySessionPrState retargets a chat run's children onto the pin`, async () => {
    h.flipped = [{ id: `sess-1`, branch: `exp/chat-1a2b3c4d`, teamId: `ws-1` }]
    h.plainSelects = [[], [pinnedRepo], []]

    await applySessionPrState({
      prUrl: PARENT_PR_URL,
      state: `merged`,
      endSessions: false,
    })

    await vi.waitFor(() =>
      expect(h.retargetPullRequest).toHaveBeenCalledWith({
        repo: `owner/repo`,
        prNumber: 241,
        base: `develop`,
        token: `tok`,
      })
    )
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
  it(`mirrors an edited base onto every issue and run row on the PR`, async () => {
    await applyPrBaseBranchEdit({ prUrl: PARENT_PR_URL, baseRef: `exp/EXP-10` })
    expect(h.updates).toEqual([
      { prBaseBranch: `exp/EXP-10` },
      { prBaseBranch: `exp/EXP-10` },
    ])
    // EXP-1165: an issue-less run's PR records its base on the run row.
    expect(h.updatedTables).toEqual([`issues`, `coding_sessions`])
  })

  it(`writes nothing without a base`, async () => {
    await applyPrBaseBranchEdit({ prUrl: PARENT_PR_URL, baseRef: null })
    expect(h.updates).toEqual([])
  })
})
