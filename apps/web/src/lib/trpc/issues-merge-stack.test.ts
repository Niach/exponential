import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1248: `issues.mergePr({mergeStack: true})` on an open-stack member is
// ONE merge-async on it (GitHub lands it and everything beneath it), after
// making the line a GitHub stack; a plain merge on a member with PRs open
// beneath it is refused, the bottom (a landing of one) merges plainly.
// Plus the chain walk (`openStackThrough`) and the membership rule
// (`openStackMember`).

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
      mergedBy: null
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
  // Call order across merges and heals.
  log: [] as string[],
  retargetChildrenOfMergedPr: vi.fn(async (_opts: { headBranch: string }) => {}),
  openStackThrough: vi.fn(async (): Promise<unknown[]> => []),
  openStackMember: vi.fn(async (): Promise<unknown> => null),
  ensureGithubStack: vi.fn(async (_opts: Record<string, unknown>) => ({
    number: 7,
    baseRef: `master`,
    open: true,
    pulls: [],
  })),
  mergeThrough: vi.fn(
    async (
      _opts: Record<string, unknown>
    ): Promise<{
      merged: boolean
      queued: boolean
      sha: string | null
      mergedBy: null
    }> => ({ merged: true, queued: false, sha: `abc`, mergedBy: null })
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
vi.mock(`@/lib/pr-merge-guard`, async (importOriginal) => ({
  ...(await importOriginal<object>()),
  openStackThrough: h.openStackThrough,
  openStackMember: h.openStackMember,
}))
vi.mock(`@/lib/pr-stacks`, () => ({
  ensureGithubStack: h.ensureGithubStack,
  mergeThrough: h.mergeThrough,
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
  _clearPrActorClaims,
  takePrMergeClaim,
} from "@/lib/integrations/pr-actor-claims"
import { GitHubMergeError } from "@/lib/integrations/github-pr"
import type { StackMember } from "@/lib/pr-merge-guard"

const { openStackThrough, openStackMember } = await vi.importActual<
  typeof import("@/lib/pr-merge-guard")
>(`@/lib/pr-merge-guard`)

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

const ID = (n: number) => `${n}${n}${n}${n}${n}${n}${n}${n}-${n}${n}${n}${n}-4${n}${n}${n}-8${n}${n}${n}-${String(n).repeat(12)}`

function member(n: number): StackMember {
  return {
    issueId: ID(n),
    identifier: `EXP-1${n}`,
    boardId: `board-1`,
    prNumber: 240 + n,
    prUrl: `https://github.com/owner/repo/pull/${240 + n}`,
    branch: `exp/EXP-1${n}`,
    prBaseBranch: n === 1 ? `master` : `exp/EXP-1${n - 1}`,
  }
}

/** The two reads one member's merge makes: its row, then the linked issues
 *  (a null base: the guard has nothing to look up after the heal). */
function queueMemberMerge(n: number) {
  const m = member(n)
  h.selectQueue.push([
    {
      prNumber: m.prNumber,
      prUrl: m.prUrl,
      prState: `open`,
      identifier: m.identifier,
      title: `Member ${n}`,
      prBaseBranch: null,
    },
  ])
  h.selectQueue.push([{ id: m.issueId }])
}

beforeEach(() => {
  _clearPrActorClaims()
  h.selectQueue.length = 0
  h.updates.length = 0
  h.log.length = 0
  vi.clearAllMocks()
  h.assertIssueAccess.mockImplementation(async () => ({
    issueId: ID(1),
    boardId: `board-1`,
    teamId: `ws-1`,
  }))
  h.resolveRepoDefaultBranchCached.mockResolvedValue(`master`)
  h.resolveRepoInstallationTokenInfo.mockResolvedValue({
    token: `tok`,
    installationId: 77,
    expiresAt: null,
  })
  h.isInstallationLinkedToTeam.mockResolvedValue(true)
  h.openStackThrough.mockResolvedValue([])
  h.openStackMember.mockResolvedValue(null)
  h.mergeThrough.mockImplementation(async (opts) => {
    h.log.push(`merge through #${opts.prNumber}`)
    return { merged: true, queued: false, sha: `abc`, mergedBy: null }
  })
  h.mergePullRequestSmart.mockImplementation(async (opts) => {
    h.log.push(`merge #${opts.prNumber}`)
    return { merged: true, queued: false, sha: `abc`, mergedBy: null }
  })
  h.retargetChildrenOfMergedPr.mockImplementation(async (opts) => {
    // A real heal awaits GitHub: the next merge must wait for it.
    await new Promise((resolve) => setTimeout(resolve, 5))
    h.log.push(`retarget ${opts.headBranch}`)
  })
  // After the heal GitHub reports the next member on the default branch.
  h.getPullRequest.mockResolvedValue({
    state: `open` as const,
    merged: false,
    draft: false,
    headRef: `exp/EXP-12`,
    baseRef: `master`,
    mergeable: true,
    mergeableState: `clean`,
  })
})

/** The merging member's row (base = the member below, or the default). */
function queueRow(n: number) {
  const m = member(n)
  h.selectQueue.push([
    {
      prNumber: m.prNumber,
      prUrl: m.prUrl,
      prState: `open`,
      identifier: m.identifier,
      title: `Member ${n}`,
      prBaseBranch: m.prBaseBranch,
    },
  ])
}

const stackOf = (...ns: number[]) => ({
  kind: `stack` as const,
  landing: ns.map(member),
})

describe(`issues.mergePr({mergeStack: true}) (EXP-1248)`, () => {
  it(`merges through the top: one stack ensure, ONE merge-async, every landed PR written`, async () => {
    h.openStackMember.mockResolvedValue(stackOf(1, 2, 3))
    queueRow(3)
    // The repo row for the stop branches (none: GitHub's default), then the
    // linked issues of each landed PR.
    h.selectQueue.push([], [{ id: ID(1) }], [{ id: ID(2) }], [{ id: ID(3) }])

    await expect(
      caller.mergePr({ issueId: ID(3), mergeStack: true })
    ).resolves.toEqual({
      merged: true,
      stack: [
        { identifier: `EXP-11`, prNumber: 241 },
        { identifier: `EXP-12`, prNumber: 242 },
        { identifier: `EXP-13`, prNumber: 243 },
      ],
    })
    expect(h.ensureGithubStack).toHaveBeenCalledWith({
      repo: `owner/repo`,
      token: `tok`,
      lowerPrNumber: 242,
      newPrNumber: 243,
      stopBranches: [`master`],
    })
    expect(h.log).toEqual([`merge through #243`])
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(h.retargetChildrenOfMergedPr).not.toHaveBeenCalled()
    expect(h.applyPrMergeState).toHaveBeenCalledTimes(3)
    expect(h.endMergedPrSessions).toHaveBeenCalledTimes(3)
  })

  it(`merges through the middle: the two below land, the top is untouched`, async () => {
    h.openStackMember.mockResolvedValue(stackOf(1, 2))
    queueRow(2)
    h.selectQueue.push([], [{ id: ID(1) }], [{ id: ID(2) }])

    const result = await caller.mergePr({ issueId: ID(2), mergeStack: true })

    expect(result.stack).toEqual([
      { identifier: `EXP-11`, prNumber: 241 },
      { identifier: `EXP-12`, prNumber: 242 },
    ])
    expect(h.log).toEqual([`merge through #242`])
    expect(h.applyPrMergeState).toHaveBeenCalledTimes(2)
  })

  it.each([
    [`with mergeStack`, true],
    [`without mergeStack`, undefined],
  ])(`merges a stack bottom (a landing of one) plainly %s`, async (_label, mergeStack) => {
    h.openStackMember.mockResolvedValue(stackOf(1))
    queueMemberMerge(1)

    await expect(
      caller.mergePr({ issueId: ID(1), mergeStack })
    ).resolves.toEqual({ merged: true })
    expect(h.ensureGithubStack).not.toHaveBeenCalled()
    expect(h.mergeThrough).not.toHaveBeenCalled()
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
  })

  it.each([
    [`the top`, 3, [1, 2, 3], `EXP-11 (#241), EXP-12 (#242), EXP-13 (#243)`],
    [`the middle`, 2, [1, 2], `EXP-11 (#241), EXP-12 (#242)`],
  ])(`refuses a plain merge of %s, naming what a merge through it lands`, async (_label, n, landing, names) => {
    h.openStackMember.mockResolvedValue(stackOf(...landing))
    queueRow(n)

    await expect(caller.mergePr({ issueId: ID(n) })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `This pull request is part of an open stack. Merging through it lands ${names}; merge with mergeStack to land them.`,
    })
    expect(h.mergeThrough).not.toHaveBeenCalled()
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 240 + n)).toBeNull()
  })

  it(`a conflict stays CONFLICT and every claim goes`, async () => {
    h.openStackMember.mockResolvedValue(stackOf(1, 2))
    queueRow(2)
    h.selectQueue.push([])
    h.mergeThrough.mockRejectedValueOnce(
      new GitHubMergeError(405, `Pull Request is not mergeable`)
    )
    h.diagnoseUnmergeablePr.mockResolvedValueOnce({
      conflict: true,
      message: `Pull Request has merge conflicts with 'master'`,
    })

    await expect(
      caller.mergePr({ issueId: ID(2), mergeStack: true })
    ).rejects.toMatchObject({ code: `CONFLICT` })
    expect(h.applyPrMergeState).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 241)).toBeNull()
    expect(takePrMergeClaim(`owner/repo`, 242)).toBeNull()
  })

  it(`surfaces a failed stack call and never merges`, async () => {
    h.openStackMember.mockResolvedValue(stackOf(1, 2))
    queueRow(2)
    h.selectQueue.push([])
    h.ensureGithubStack.mockRejectedValueOnce(
      new Error(`GitHub could not stack PRs #241, #242 (422): Validation Failed`)
    )

    await expect(
      caller.mergePr({ issueId: ID(2), mergeStack: true })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `GitHub could not stack PRs #241, #242 (422): Validation Failed`,
    })
    expect(h.mergeThrough).not.toHaveBeenCalled()
  })

  it(`reports a queued merge-through without writing anything`, async () => {
    h.openStackMember.mockResolvedValue(stackOf(1, 2))
    queueRow(2)
    h.selectQueue.push([])
    h.mergeThrough.mockResolvedValueOnce({
      merged: true,
      queued: true,
      sha: null,
      mergedBy: null,
    })

    await expect(
      caller.mergePr({ issueId: ID(2), mergeStack: true })
    ).resolves.toMatchObject({ merged: false, queued: true })
    expect(h.applyPrMergeState).not.toHaveBeenCalled()
  })

  it(`refuses a tree child even with mergeStack, naming its parent`, async () => {
    h.openStackMember.mockResolvedValue({
      kind: `tree`,
      parent: `EXP-11`,
      landing: [member(1), member(2)],
    })
    queueRow(2)

    await expect(
      caller.mergePr({ issueId: ID(2), mergeStack: true })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `This pull request is stacked on EXP-11; merge EXP-11 first`,
    })
    expect(h.mergeThrough).not.toHaveBeenCalled()
  })

  it(`merges a tree root plainly (its children wait for it)`, async () => {
    h.openStackMember.mockResolvedValue({
      kind: `tree`,
      parent: null,
      landing: [member(1)],
    })
    queueMemberMerge(1)

    await expect(caller.mergePr({ issueId: ID(1) })).resolves.toEqual({
      merged: true,
    })
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
    expect(h.mergeThrough).not.toHaveBeenCalled()
  })

  it(`is the plain merge for a PR in no stack`, async () => {
    queueMemberMerge(1)

    await expect(
      caller.mergePr({ issueId: ID(1), mergeStack: true })
    ).resolves.toEqual({ merged: true })
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
    expect(h.mergeThrough).not.toHaveBeenCalled()
  })

  // A follow-up whose parent MERGED (a tree, or a stack merged by hand):
  // its recorded base is still the merged branch; without the merged-parent
  // check it squashed into it.
  it(`a PR left on a merged parent's branch does not merge into the dead branch`, async () => {
    vi.useFakeTimers()
    try {
      const upper = member(2)
      h.selectQueue.push([
        {
          prNumber: upper.prNumber,
          prUrl: upper.prUrl,
          prState: `open`,
          identifier: upper.identifier,
          title: `Member 2`,
          prBaseBranch: `exp/EXP-11`,
        },
      ])
      // No OPEN PR on the base (issue, run); EXP-11's MERGED one is; the
      // team has no repo row.
      h.selectQueue.push([], [], [{ prUrl: member(1).prUrl }], [])
      h.retargetChildrenOfMergedPr.mockResolvedValue(undefined)
      h.getPullRequest.mockResolvedValue({
        state: `open` as const,
        merged: false,
        draft: false,
        headRef: `exp/EXP-12`,
        baseRef: `exp/EXP-11`,
        mergeable: true,
        mergeableState: `clean`,
      })

      const pending = caller.mergePr({ issueId: ID(2) })
      const settled = expect(pending).rejects.toMatchObject({
        code: `PRECONDITION_FAILED`,
        message: `It is still based on exp/EXP-11; retarget it onto the default branch (exponential_pr_retarget) and merge again`,
      })
      await vi.runAllTimersAsync()
      await settled
      expect(h.retargetChildrenOfMergedPr).toHaveBeenCalledWith({
        prUrl: member(1).prUrl,
        headBranch: `exp/EXP-11`,
        teamId: `ws-1`,
      })
      expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
      expect(h.applyPrMergeState).not.toHaveBeenCalled()
    } finally {
      vi.useRealTimers()
    }
  })

  it(`merges once the heal moved the PR onto the default branch`, async () => {
    const upper = member(2)
    h.selectQueue.push([
      {
        prNumber: upper.prNumber,
        prUrl: upper.prUrl,
        prState: `open`,
        identifier: upper.identifier,
        title: `Member 2`,
        prBaseBranch: `exp/EXP-11`,
      },
    ])
    h.selectQueue.push([], [], [{ prUrl: member(1).prUrl }], [])
    h.selectQueue.push([{ id: upper.issueId }])

    await expect(caller.mergePr({ issueId: ID(2) })).resolves.toEqual({
      merged: true,
    })
    expect(h.log).toEqual([`retarget exp/EXP-11`, `merge #242`])
    expect(h.updates).toEqual([
      { prBaseBranch: `master` },
      { prBaseBranch: `master` },
    ])
  })
})

describe(`openStackMember (EXP-1248)`, () => {
  function fakeDb(answers: unknown[][]) {
    const select = vi.fn(() => {
      const rows = answers.shift() ?? []
      const p = Promise.resolve(rows) as Promise<unknown[]> &
        Record<string, (arg?: unknown) => unknown>
      for (const m of [`from`, `limit`, `where`]) p[m] = () => p
      return p
    })
    return { db: { select } as never, select }
  }
  const row = (n: number, over: Record<string, unknown> = {}) => {
    const m = member(n)
    return {
      issueId: m.issueId,
      identifier: m.identifier,
      boardId: m.boardId,
      prNumber: m.prNumber,
      prUrl: m.prUrl,
      prState: `open`,
      branch: m.branch,
      prBaseBranch: m.prBaseBranch,
      ...over,
    }
  }
  const url = (n: number) => ({ prUrl: member(n).prUrl })
  const link = (n: number) => ({ prUrl: member(n).prUrl, branch: member(n).branch })

  it(`a linear line below is a stack landing bottom-first`, async () => {
    const { db: fake } = fakeDb([
      [row(2)], // the start
      [row(1)], // the PR below
      [], // openStackThrough's repo row (GitHub's default: master)
      [], // nobody's open PR on master below EXP-11
      [], // openStackMember's repo row
      [url(2)], // EXP-11's open children: issue rows
      [], //                              run rows
      [], // EXP-12's open children: issue rows
      [], //                          run rows
    ])
    const membership = await openStackMember(fake, { issueId: ID(2), teamId: `ws-1` })
    expect(membership?.kind).toBe(`stack`)
    expect(membership?.landing.map((m) => m.identifier)).toEqual([`EXP-11`, `EXP-12`])
  })

  it(`the bottom with one PR on its branch lands alone: no member`, async () => {
    const { db: fake } = fakeDb([
      [row(1)], // the start, base master
      [], // nobody's open PR on master
      [], // the repo row
      [link(2)], // its children: issue rows
      [], //                     run rows
      [], // EXP-12's children: issue rows
      [], //                     run rows
    ])
    await expect(
      openStackMember(fake, { issueId: ID(1), teamId: `ws-1` })
    ).resolves.toBeNull()
  })

  // A ← B ← {C, D}: the clients' `prGraphShape` calls it a tree, so the
  // root's plain merge must pass and B waits for A.
  it(`a fork above the root's only child makes the root a tree root`, async () => {
    const { db: fake } = fakeDb([
      [row(1)], // A, base master
      [], // nobody's open PR on master
      [], // the repo row
      [link(2)], // A's children: B (issue rows)
      [], //                       (run rows)
      [link(3), link(4)], // B's children: C, D (issue rows)
      [], //                                    (run rows)
    ])
    await expect(
      openStackMember(fake, { issueId: ID(1), teamId: `ws-1` })
    ).resolves.toMatchObject({ kind: `tree`, parent: null })
  })

  it(`a fork above makes the middle a tree child naming its parent`, async () => {
    const { db: fake } = fakeDb([
      [row(2)], // B
      [row(1)], // A below it
      [], // openStackThrough's repo row
      [], // nobody's open PR on master below A
      [], // openStackMember's repo row
      [link(2)], // A's children: B only (issue rows)
      [], //                               (run rows)
      [link(3), link(4)], // B's children: C, D
      [],
    ])
    await expect(
      openStackMember(fake, { issueId: ID(2), teamId: `ws-1` })
    ).resolves.toMatchObject({ kind: `tree`, parent: `EXP-11` })
  })

  it(`a run PR forking above counts like an issue PR`, async () => {
    const { db: fake } = fakeDb([
      [row(1)],
      [],
      [],
      [link(2)],
      [],
      [link(3)], // B's children: C (issue rows)
      [{ prUrl: `https://github.com/owner/repo/pull/300`, branch: `exp/chat-1a2b3c4d` }], // a run PR
    ])
    await expect(
      openStackMember(fake, { issueId: ID(1), teamId: `ws-1` })
    ).resolves.toMatchObject({ kind: `tree`, parent: null })
  })

  it(`a batch PR's rows count once`, async () => {
    const { db: fake } = fakeDb([
      [row(1)],
      [],
      [],
      [link(2), link(2)], // one PR, two issue rows
      [],
    ])
    await expect(
      openStackMember(fake, { issueId: ID(1), teamId: `ws-1` })
    ).resolves.toBeNull()
  })

  it(`a lone PR is no member`, async () => {
    const { db: fake } = fakeDb([[row(1)], [], [], [], []])
    await expect(
      openStackMember(fake, { issueId: ID(1), teamId: `ws-1` })
    ).resolves.toBeNull()
  })

  it(`two PRs on the root's branch make it a tree root`, async () => {
    const { db: fake } = fakeDb([[row(1)], [], [], [link(2)], [link(3)]])
    await expect(
      openStackMember(fake, { issueId: ID(1), teamId: `ws-1` })
    ).resolves.toMatchObject({ kind: `tree`, parent: null })
  })

  it(`a fork below makes a child a tree child naming its parent`, async () => {
    const { db: fake } = fakeDb([
      [row(2)],
      [row(1)],
      [],
      [],
      [],
      [url(2), url(3)], // EXP-11 has two open children
      [],
    ])
    await expect(
      openStackMember(fake, { issueId: ID(2), teamId: `ws-1` })
    ).resolves.toMatchObject({ kind: `tree`, parent: `EXP-11` })
  })

  it(`is null for an issue without an open PR`, async () => {
    const { db: fake } = fakeDb([[row(1, { prState: `merged` })]])
    await expect(
      openStackMember(fake, { issueId: ID(1), teamId: `ws-1` })
    ).resolves.toBeNull()
  })
})

describe(`openStackThrough (SLOP-3)`, () => {
  function fakeDb(answers: unknown[][]) {
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
  const row = (n: number, over: Record<string, unknown> = {}) => {
    const m = member(n)
    return {
      issueId: m.issueId,
      identifier: m.identifier,
      boardId: m.boardId,
      prNumber: m.prNumber,
      prUrl: m.prUrl,
      prState: `open`,
      branch: m.branch,
      prBaseBranch: m.prBaseBranch,
      ...over,
    }
  }

  it(`walks DOWN from the issue to the bottom, bottom first`, async () => {
    // The start, the PR below it, then the team's repo row (none).
    const { db: fake, wheres } = fakeDb([[row(2)], [row(1)], []])
    const chain = await openStackThrough(fake, {
      issueId: ID(2),
      teamId: `ws-1`,
    })
    expect(chain.map((m) => m.identifier)).toEqual([`EXP-11`, `EXP-12`])
    const { PgDialect } = await import(`drizzle-orm/pg-core`)
    const query = new PgDialect().sqlToQuery(wheres[1] as never)
    expect(query.params).toEqual([
      `ws-1`,
      `exp/EXP-11`,
      `open`,
      `https://github.com/owner/repo/pull/%`,
    ])
  })

  it(`is empty for an issue without an open PR`, async () => {
    const { db: fake } = fakeDb([[row(1, { prState: `merged` })]])
    await expect(
      openStackThrough(fake, { issueId: ID(1), teamId: `ws-1` })
    ).resolves.toEqual([])
  })

  it(`survives a cycle`, async () => {
    const { db: fake, select } = fakeDb([
      [row(2, { prBaseBranch: `exp/EXP-11` })],
      [row(1, { prBaseBranch: `exp/EXP-12` })],
      // The team's repo row, read once (none).
      [],
      [row(2, { prBaseBranch: `exp/EXP-11` })],
    ])
    const chain = await openStackThrough(fake, {
      issueId: ID(2),
      teamId: `ws-1`,
    })
    expect(chain.map((m) => m.identifier)).toEqual([`EXP-11`, `EXP-12`])
    expect(select).toHaveBeenCalledTimes(4)
  })

  // A team on `develop` with its `develop → main` release PR open: the row
  // carrying branch `develop` is no stack member, or Merge stack on any plain
  // PR would land the release first.
  it.each([
    [`the team's pin`, `develop`, [] as unknown[]],
    [`GitHub's stored default`, `main`, [] as unknown[]],
    [`a board's pin`, `release/1.x`, [{ defaultBranch: `release/1.x` }]],
  ])(`stops at a base that is %s, though a PR is open from it`, async (_label, base, pins) => {
    const { db: fake, select } = fakeDb([
      [row(2, { prBaseBranch: base })],
      [row(1, { branch: base, prBaseBranch: `main` })],
      [{ id: `repo-1`, defaultBranch: `main`, defaultBranchOverride: `develop` }],
      pins,
    ])
    const chain = await openStackThrough(fake, {
      issueId: ID(2),
      teamId: `ws-1`,
    })
    expect(chain.map((m) => m.identifier)).toEqual([`EXP-12`])
    expect(select).toHaveBeenCalledTimes(4)
  })

  it(`keeps a real stack that bottoms out on the team's branch`, async () => {
    const { db: fake } = fakeDb([
      [row(2)],
      [row(1, { prBaseBranch: `develop` })],
      [{ id: `repo-1`, defaultBranch: `main`, defaultBranchOverride: `develop` }],
      [],
      // The release PR open from `develop`.
      [row(3, { branch: `develop`, prBaseBranch: `main` })],
    ])
    const chain = await openStackThrough(fake, {
      issueId: ID(2),
      teamId: `ws-1`,
    })
    expect(chain.map((m) => m.identifier)).toEqual([`EXP-11`, `EXP-12`])
  })

  it(`caps the walk at ten members`, async () => {
    const answers: unknown[][] = Array.from({ length: 20 }, (_, i) => [
      {
        ...row(1),
        issueId: `i-${i}`,
        prUrl: `https://github.com/owner/repo/pull/${i + 1}`,
        prNumber: i + 1,
        prBaseBranch: `b-${i + 1}`,
      },
    ])
    // The team's repo row, read once after the first PR below (none).
    answers.splice(2, 0, [])
    const { db: fake } = fakeDb(answers)
    const chain = await openStackThrough(fake, {
      issueId: `i-0`,
      teamId: `ws-1`,
    })
    expect(chain).toHaveLength(10)
  })
})
