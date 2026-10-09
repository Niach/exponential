import { afterAll, beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1145 on the number-addressed merge path: `mergeRepositoryPull` backs
// `repositories.mergePull` (MCP `pr_merge({repositoryId, prNumber})`) and
// `codingSessions.mergePr` (MCP `pr_merge` with no subject, the run's own PR).
// A PR an issue links records its base (`pr_base_branch`); when that base is
// another OPEN PR's head it is refused before any claim or GitHub call. An
// issue-less PR records no base and merges as before. A base whose PR already
// MERGED awaits the EXP-324 heal and refuses while GitHub still reports it.

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  // The `pr_base_branch` writes (set values, in order).
  updates: [] as Array<Record<string, unknown>>,
  mergePullRequestSmart: vi.fn(
    async (
      _opts: Record<string, unknown>
    ): Promise<{
      merged: boolean
      queued: boolean
      sha: string | null
      mergedBy: null
    }> => ({ merged: true, queued: false, sha: `abc`, mergedBy: null })
  ),
  getPullRequest: vi.fn(),
  // EXP-1165: GitHub's default branch, the guards' exit when the team has no
  // repositories row for the repo.
  resolveRepoDefaultBranchCached: vi.fn(
    async (_repo: string): Promise<string | null> => null
  ),
  applySessionPrState: vi.fn(async () => ({ endedSessionIds: [] })),
  applyPrMergeState: vi.fn(async () => {}),
  retargetChildrenOfMergedPr: vi.fn(
    async (_opts: { prUrl: string; headBranch: string; teamId?: string }) => {}
  ),
  // EXP-1248: stack membership (null = a lone PR) and the run PR's children.
  openStackMember: vi.fn(async (): Promise<unknown> => null),
  openChildPrUrls: vi.fn(async (): Promise<Set<string>> => new Set()),
  ensureGithubStack: vi.fn(async () => ({ number: 7, baseRef: `master`, open: true, pulls: [] })),
  mergeThrough: vi.fn(
    async (
      _opts: Record<string, unknown>
    ): Promise<{ merged: boolean; queued: boolean; sha: string | null; mergedBy: null }> => ({
      merged: true,
      queued: false,
      sha: `abc`,
      mergedBy: null,
    })
  ),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => {
      const rows = h.selectQueue.shift() ?? []
      const builder = {
        from: () => builder,
        where: () => builder,
        orderBy: () => builder,
        limit: async () => rows,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        then: (res: any, rej: any) => Promise.resolve(rows).then(res, rej),
      }
      return builder
    },
    update: () => ({
      set: (values: Record<string, unknown>) => ({
        where: async () => {
          h.updates.push(values)
        },
      }),
    }),
  },
}))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/integrations/github-app`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-app")>()),
  githubAppConfigured: () => true,
  resolveRepoDefaultBranchCached: h.resolveRepoDefaultBranchCached,
  resolveRepoInstallationTokenInfo: async () => ({
    token: `tok`,
    installationId: 77,
    expiresAt: null,
  }),
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  assertRepoInstallationAccess: vi.fn(),
  isInstallationLinkedToTeam: async () => true,
}))
vi.mock(`@/lib/integrations/github-pr`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-pr")>()),
  mergePullRequestSmart: h.mergePullRequestSmart,
  getPullRequest: h.getPullRequest,
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applySessionPrState: h.applySessionPrState,
  applyPrMergeState: h.applyPrMergeState,
  retargetChildrenOfMergedPr: h.retargetChildrenOfMergedPr,
}))
vi.mock(`@/lib/pr-merge-guard`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  openStackMember: h.openStackMember,
  openChildPrUrls: h.openChildPrUrls,
}))
vi.mock(`@/lib/pr-stacks`, () => ({
  ensureGithubStack: h.ensureGithubStack,
  mergeThrough: h.mergeThrough,
}))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: vi.fn(),
  getIssueTeamContext: vi.fn(),
}))

import { mergeRepositoryPull } from "@/lib/trpc/repositories"
import {
  _clearPrActorClaims,
  takePrMergeClaim,
} from "@/lib/integrations/pr-actor-claims"

const repo = {
  id: `repo-1`,
  teamId: `ws-1`,
  fullName: `owner/repo`,
  defaultBranch: `master`,
  defaultBranchOverride: null,
  installationId: 77,
  inaccessibleAt: null,
  sharedByUserId: null,
  archivedAt: null,
}

/** The issue row(s) the PR links, as the first read returns them. */
const onPr = (prBaseBranch: string | null) => ({
  id: `issue-12`,
  identifier: `EXP-12`,
  title: `Upper`,
  prBaseBranch,
})

const pull = (baseRef: string) => ({
  state: `open` as const,
  merged: false,
  draft: false,
  headRef: `exp/EXP-12`,
  baseRef,
  mergeable: true,
  mergeableState: `clean`,
})

beforeEach(() => {
  h.selectQueue.length = 0
  h.updates.length = 0
  vi.clearAllMocks()
  _clearPrActorClaims()
  h.openStackMember.mockResolvedValue(null)
  h.openChildPrUrls.mockResolvedValue(new Set())
})

describe(`mergeRepositoryPull on a PR stacked on an open PR (EXP-1145)`, () => {
  it(`refuses before any claim or GitHub call, naming the parent issue`, async () => {
    h.selectQueue.push([onPr(`exp/EXP-11`)])
    h.selectQueue.push([{ identifier: `EXP-11` }])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `This pull request is stacked on EXP-11; merge EXP-11 first`,
    })
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 242)).toBeNull()
  })

  it(`names an issue-less parent run PR by number`, async () => {
    h.selectQueue.push([onPr(`exp/chat-1a2b3c4d`)])
    h.selectQueue.push([])
    h.selectQueue.push([{ prNumber: 240 }])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true })
    ).rejects.toMatchObject({
      message: `This pull request is stacked on #240; merge #240 first`,
    })
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
  })

  it(`merges a base nobody's PR ever used, with no heal and no GitHub read`, async () => {
    h.selectQueue.push([onPr(`release/1.x`)])
    // No OPEN PR on the base (issue, run), no MERGED one either.
    h.selectQueue.push([], [], [], [])
    h.selectQueue.push([{ id: `issue-12` }])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true })
    ).resolves.toEqual({ merged: true })
    expect(h.retargetChildrenOfMergedPr).not.toHaveBeenCalled()
    expect(h.getPullRequest).not.toHaveBeenCalled()
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
    expect(h.applyPrMergeState).toHaveBeenCalledTimes(1)
  })

  it(`merges an issue-less PR (no recorded base) without a parent lookup`, async () => {
    // No issue on the PR, no run row recording a base, no linked issues.
    h.selectQueue.push([], [], [])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 300, userId: `actor`, viaAgent: true })
    ).resolves.toEqual({ merged: true })
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
    expect(h.applySessionPrState).toHaveBeenCalledTimes(1)
    // No issue, no `IDENT: title` squash title: GitHub's default stands.
    expect(h.mergePullRequestSmart.mock.calls[0][0]).not.toHaveProperty(
      `commitTitle`
    )
  })
})

// The parent already MERGED (the root of a follow-up tree, then this child):
// the heal behind that merge is fire-and-forget, so this merge awaits it and
// refuses while GitHub still reports the merged branch as the base.
describe(`mergeRepositoryPull on a PR whose parent already merged`, () => {
  const ROOT_PR_URL = `https://github.com/owner/repo/pull/241`
  let childBase = `exp/EXP-11`
  let log: string[] = []

  /** The PR's issue, no OPEN PR on its base (issue, run), the MERGED root on
   *  it, no pin on that branch (repo row, board pins). */
  function queueGuard() {
    h.selectQueue.push(
      [onPr(`exp/EXP-11`)],
      [],
      [],
      [{ prUrl: ROOT_PR_URL }],
      [{ id: `repo-1`, defaultBranch: `master`, defaultBranchOverride: null }],
      []
    )
  }

  beforeEach(() => {
    childBase = `exp/EXP-11`
    log = []
    h.getPullRequest.mockImplementation(async () => pull(childBase))
    h.mergePullRequestSmart.mockImplementation(async (opts) => {
      log.push(`merge #${opts.prNumber} into ${childBase}`)
      return { merged: true, queued: false, sha: `abc`, mergedBy: null }
    })
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

  it(`awaits the heal and merges only once GitHub moved the base`, async () => {
    queueGuard()
    h.selectQueue.push([{ id: `issue-12` }])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true })
    ).resolves.toEqual({ merged: true })
    expect(log).toEqual([`retarget exp/EXP-11`, `merge #242 into master`])
    expect(h.retargetChildrenOfMergedPr).toHaveBeenCalledWith({
      prUrl: ROOT_PR_URL,
      headBranch: `exp/EXP-11`,
      teamId: `ws-1`,
    })
    // The issue rows and (EXP-1165) the run rows carrying the PR.
    expect(h.updates).toEqual([
      { prBaseBranch: `master` },
      { prBaseBranch: `master` },
    ])
  })

  it(`refuses, before any claim, when the heal left the PR on the merged branch`, async () => {
    vi.useFakeTimers()
    try {
      queueGuard()
      h.retargetChildrenOfMergedPr.mockResolvedValue(undefined)

      const pending = mergeRepositoryPull({
        repo,
        prNumber: 242,
        userId: `actor`,
        viaAgent: true,
      })
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
})

// FEED-43 R1, like `issues.mergePr`: GitHub's merge queue took the merge and
// may still reject it, so nothing is written (no issue merged or Done, no
// session ended, nobody notified); the webhook completes it when it lands.
describe(`mergeRepositoryPull on a queued merge`, () => {
  it(`reports queued and writes nothing; the claim stays for the landing merge`, async () => {
    h.selectQueue.push([onPr(null)])
    h.mergePullRequestSmart.mockResolvedValueOnce({
      merged: true,
      queued: true,
      sha: null,
      mergedBy: null,
    })

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true })
    ).resolves.toEqual({
      merged: false,
      queued: true,
      note: `GitHub queued the merge of PR #242. Nothing is merged yet; it completes when the merge lands.`,
    })
    expect(h.applySessionPrState).not.toHaveBeenCalled()
    expect(h.applyPrMergeState).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 242)).toMatchObject({ userId: `actor` })
  })
})

describe(`mergeRepositoryPull squash title`, () => {
  it(`names the squash commit after the PR's issue, like issues.mergePr`, async () => {
    // A batch PR merged from its run: two issues, the oldest names it.
    h.selectQueue.push([
      onPr(null),
      { id: `issue-13`, identifier: `EXP-13`, title: `Sibling`, prBaseBranch: null },
    ])
    h.selectQueue.push([{ id: `issue-12` }, { id: `issue-13` }])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true })
    ).resolves.toEqual({ merged: true })
    expect(h.mergePullRequestSmart).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 242,
      token: `tok`,
      commitTitle: `EXP-12: Upper (#242)`,
    })
    expect(h.applyPrMergeState).toHaveBeenCalledTimes(2)
  })
})

// EXP-1165: an issue-less run's PR (a chat or action run) records its base on
// the run row at pr_open, so the same guards hold on `pr_merge({repositoryId,
// prNumber})` and `codingSessions.mergePr`.
describe(`mergeRepositoryPull on an issue-less run PR with a recorded base`, () => {
  it(`refuses a run PR stacked on an open issue PR`, async () => {
    h.selectQueue.push([], [{ prBaseBranch: `exp/EXP-11` }])
    h.selectQueue.push([{ identifier: `EXP-11` }])
    // The base is no branch the repo is developed on.
    h.selectQueue.push([
      { id: `repo-1`, defaultBranch: `master`, defaultBranchOverride: null },
    ])
    h.selectQueue.push([])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 300, userId: `actor`, viaAgent: true })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `This pull request is stacked on EXP-11; merge EXP-11 first`,
    })
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 300)).toBeNull()
  })

  it(`refuses a run PR stacked on another run's open PR`, async () => {
    h.selectQueue.push([], [{ prBaseBranch: `exp/chat-1a2b3c4d` }])
    h.selectQueue.push([], [{ prNumber: 240 }])
    h.selectQueue.push([
      { id: `repo-1`, defaultBranch: `master`, defaultBranchOverride: null },
    ])
    h.selectQueue.push([])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 300, userId: `actor`, viaAgent: true })
    ).rejects.toMatchObject({
      message: `This pull request is stacked on #240; merge #240 first`,
    })
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
  })

  it(`awaits the heal when the run PR's parent already merged`, async () => {
    let childBase = `exp/EXP-11`
    h.getPullRequest.mockImplementation(async () => pull(childBase))
    h.retargetChildrenOfMergedPr.mockImplementationOnce(async () => {
      childBase = `master`
    })
    h.selectQueue.push(
      [],
      [{ prBaseBranch: `exp/EXP-11` }],
      // No OPEN PR on the base (issue, run), the MERGED root on it, no pin.
      [],
      [],
      [{ prUrl: `https://github.com/owner/repo/pull/241` }],
      [{ id: `repo-1`, defaultBranch: `master`, defaultBranchOverride: null }],
      []
    )
    // The linked-issue read after the merge.
    h.selectQueue.push([])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 300, userId: `actor`, viaAgent: true })
    ).resolves.toEqual({ merged: true })
    expect(h.retargetChildrenOfMergedPr).toHaveBeenCalledWith({
      prUrl: `https://github.com/owner/repo/pull/241`,
      headBranch: `exp/EXP-11`,
      teamId: `ws-1`,
    })
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
    h.getPullRequest.mockReset()
  })
})

// EXP-1165: a team with no repositories row for the repo still gets the
// default-branch exit, off GitHub's default branch.
describe(`the merge guard's default-branch exit with no repositories row`, () => {
  it(`merges a PR based on GitHub's default branch while a PR from it is open`, async () => {
    h.resolveRepoDefaultBranchCached.mockResolvedValueOnce(`develop`)
    h.selectQueue.push([onPr(`develop`)])
    // A `develop → main` release PR is open on the base…
    h.selectQueue.push([{ identifier: `EXP-1` }])
    // …but the team has no repositories row (and so no pins).
    h.selectQueue.push([])
    // No MERGED PR on the base either.
    h.selectQueue.push([], [])
    h.selectQueue.push([{ id: `issue-12` }])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true })
    ).resolves.toEqual({ merged: true })
    expect(h.resolveRepoDefaultBranchCached).toHaveBeenCalledWith(`owner/repo`)
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
  })

  it(`still refuses when GitHub's default is another branch`, async () => {
    h.resolveRepoDefaultBranchCached.mockResolvedValueOnce(`main`)
    h.selectQueue.push([onPr(`exp/EXP-11`)])
    h.selectQueue.push([{ identifier: `EXP-11` }])
    h.selectQueue.push([])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true })
    ).rejects.toMatchObject({
      message: `This pull request is stacked on EXP-11; merge EXP-11 first`,
    })
  })
})

// EXP-1248: an open-stack member never merges plainly; `mergeStack` lands it
// and everything beneath it in ONE merge-async, after making the line a
// GitHub stack.
describe(`mergeRepositoryPull on an open-stack member`, () => {
  const member = (n: number) => ({
    issueId: `issue-${n}`,
    identifier: `EXP-${n}`,
    boardId: `board-1`,
    prNumber: 230 + n,
    prUrl: `https://github.com/owner/repo/pull/${230 + n}`,
    branch: `exp/EXP-${n}`,
    prBaseBranch: n === 11 ? `master` : `exp/EXP-${n - 1}`,
  })

  it(`refuses a plain merge, naming what a merge through it lands`, async () => {
    h.selectQueue.push([onPr(`exp/EXP-11`)])
    h.openStackMember.mockResolvedValue({ kind: `stack`, landing: [member(11), member(12)] })

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `This pull request is part of an open stack. Merging through it lands EXP-11 (#241), EXP-12 (#242); merge with mergeStack to land them.`,
    })
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(h.mergeThrough).not.toHaveBeenCalled()
  })

  it(`merges through it with mergeStack: one stack ensure, one merge-async, every landed PR written`, async () => {
    h.selectQueue.push([onPr(`exp/EXP-11`)])
    h.openStackMember.mockResolvedValue({ kind: `stack`, landing: [member(11), member(12)] })
    // Stop branches (repo row, pins), then the linked issues of each landed PR.
    h.selectQueue.push(
      [{ id: `repo-1`, defaultBranch: `master`, defaultBranchOverride: null }],
      [],
      [{ id: `issue-11` }],
      [{ id: `issue-12` }]
    )

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true, mergeStack: true })
    ).resolves.toEqual({ merged: true })
    expect(h.ensureGithubStack).toHaveBeenCalledWith(
      expect.objectContaining({ lowerPrNumber: 241, newPrNumber: 242, stopBranches: [`master`] })
    )
    expect(h.mergeThrough).toHaveBeenCalledTimes(1)
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(h.applySessionPrState).toHaveBeenCalledTimes(2)
    expect(h.applyPrMergeState).toHaveBeenCalledTimes(2)
  })

  it(`surfaces a failed stack call instead of merging`, async () => {
    h.selectQueue.push([onPr(`exp/EXP-11`)])
    h.openStackMember.mockResolvedValue({ kind: `stack`, landing: [member(11), member(12)] })
    h.selectQueue.push([], [])
    h.ensureGithubStack.mockRejectedValueOnce(new Error(`GitHub could not stack PRs #241, #242 (422): nope`))

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true, mergeStack: true })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `GitHub could not stack PRs #241, #242 (422): nope`,
    })
    expect(h.mergeThrough).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 242)).toBeNull()
  })

  it(`refuses a tree child, naming its parent`, async () => {
    h.selectQueue.push([onPr(`exp/EXP-11`)])
    h.openStackMember.mockResolvedValue({ kind: `tree`, parent: `EXP-11`, landing: [] })

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true, mergeStack: true })
    ).rejects.toMatchObject({
      message: `This pull request is stacked on EXP-11; merge EXP-11 first`,
    })
  })

  it(`an issue-less run PR with a PR on its branch is a stack bottom`, async () => {
    h.selectQueue.push([], [{ prBaseBranch: null, branch: `exp/chat-1a2b3c4d` }])
    h.openChildPrUrls.mockResolvedValue(new Set([`https://github.com/owner/repo/pull/301`]))

    await expect(
      mergeRepositoryPull({ repo, prNumber: 300, userId: `actor`, viaAgent: true })
    ).rejects.toMatchObject({
      message: `This pull request is part of an open stack. Merging through it lands #300; merge with mergeStack to land them.`,
    })
  })
})
