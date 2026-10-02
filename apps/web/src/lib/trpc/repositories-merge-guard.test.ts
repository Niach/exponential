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
  applySessionPrState: vi.fn(async () => ({ endedSessionIds: [] })),
  applyPrMergeState: vi.fn(async () => {}),
  retargetChildrenOfMergedPr: vi.fn(
    async (_opts: { prUrl: string; headBranch: string; teamId?: string }) => {}
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
    h.selectQueue.push([])
    h.selectQueue.push([])

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
    expect(h.updates).toEqual([{ prBaseBranch: `master` }])
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
      merged: true,
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
