import { beforeEach, describe, expect, it, vi } from "vitest"

// SLOP-3: `issues.mergePr({mergeStack: true})` lands the open chain below and
// including the issue, bottom-up, one plain squash at a time with the EXP-324
// heal awaited between; plus the chain walk itself (`openStackThrough`).

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
import { GitHubMergeError } from "@/lib/integrations/github-pr"
import type { StackMember } from "@/lib/pr-merge-guard"

const { openStackThrough } = await vi.importActual<
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

describe(`issues.mergePr({mergeStack: true}) (SLOP-3)`, () => {
  it(`merges a three-stack through its top bottom-up, the heal awaited between`, async () => {
    h.openStackThrough.mockResolvedValue([member(1), member(2), member(3)])
    queueMemberMerge(1)
    queueMemberMerge(2)
    queueMemberMerge(3)

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
    expect(h.log).toEqual([
      `merge #241`,
      `retarget exp/EXP-11`,
      `merge #242`,
      `retarget exp/EXP-12`,
      `merge #243`,
    ])
    expect(h.openStackThrough).toHaveBeenCalledWith(db, {
      issueId: ID(3),
      teamId: `ws-1`,
    })
    // Each member went through the full single-PR path.
    expect(h.applyPrMergeState).toHaveBeenCalledTimes(3)
    expect(h.endMergedPrSessions).toHaveBeenCalledTimes(3)
    // The recorded bases follow GitHub's move onto the default branch.
    expect(h.updates).toEqual([
      { prBaseBranch: `master` },
      { prBaseBranch: `master` },
    ])
  })

  it(`merges through the middle: the two below land, the top is untouched`, async () => {
    h.openStackThrough.mockResolvedValue([member(1), member(2)])
    queueMemberMerge(1)
    queueMemberMerge(2)

    const result = await caller.mergePr({ issueId: ID(2), mergeStack: true })

    expect(result.stack).toEqual([
      { identifier: `EXP-11`, prNumber: 241 },
      { identifier: `EXP-12`, prNumber: 242 },
    ])
    expect(h.log).toEqual([`merge #241`, `retarget exp/EXP-11`, `merge #242`])
    expect(h.mergePullRequestSmart).not.toHaveBeenCalledWith(
      expect.objectContaining({ prNumber: 243 })
    )
  })

  it(`stops at a failing member, naming what merged; a conflict stays CONFLICT`, async () => {
    h.openStackThrough.mockResolvedValue([member(1), member(2), member(3)])
    queueMemberMerge(1)
    queueMemberMerge(2)
    h.mergePullRequestSmart
      .mockImplementationOnce(async () => ({
        merged: true,
        queued: false,
        sha: `abc`,
        mergedBy: null,
      }))
      .mockRejectedValueOnce(
        new GitHubMergeError(405, `Pull Request is not mergeable`)
      )
    h.diagnoseUnmergeablePr.mockResolvedValueOnce({
      conflict: true,
      message: `Pull Request has merge conflicts with 'master': rebase onto origin/master, resolve the conflicts, push with --force-with-lease, then retry the merge.`,
    })

    await expect(
      caller.mergePr({ issueId: ID(3), mergeStack: true })
    ).rejects.toMatchObject({
      code: `CONFLICT`,
      message: `Merged EXP-11 (#241). EXP-12 (#242) did not merge: Pull Request has merge conflicts with 'master': rebase onto origin/master, resolve the conflicts, push with --force-with-lease, then retry the merge.`,
    })
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(2)
    expect(h.applyPrMergeState).toHaveBeenCalledTimes(1)
  })

  it(`never squashes into the merged branch when the heal left the base`, async () => {
    vi.useFakeTimers()
    try {
      h.openStackThrough.mockResolvedValue([member(1), member(2)])
      queueMemberMerge(1)
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

      const pending = caller.mergePr({ issueId: ID(2), mergeStack: true })
      const settled = expect(pending).rejects.toMatchObject({
        code: `PRECONDITION_FAILED`,
        message: `Merged EXP-11 (#241). EXP-12 (#242) did not merge: It is still based on exp/EXP-11; retarget it onto the default branch (exponential_pr_retarget) and merge again`,
      })
      await vi.runAllTimersAsync()
      await settled
      expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it(`is the plain merge for a PR in no stack`, async () => {
    h.openStackThrough.mockResolvedValue([member(1)])
    queueMemberMerge(1)

    await expect(
      caller.mergePr({ issueId: ID(1), mergeStack: true })
    ).resolves.toEqual({ merged: true })
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
    expect(h.retargetChildrenOfMergedPr).not.toHaveBeenCalled()
  })

  it(`keeps the plain merge of a stacked member refused`, async () => {
    h.selectQueue.push([
      {
        prNumber: 242,
        prUrl: member(2).prUrl,
        prState: `open`,
        identifier: `EXP-12`,
        title: `Upper`,
        prBaseBranch: `exp/EXP-11`,
      },
    ])
    h.selectQueue.push([{ identifier: `EXP-11` }])

    await expect(caller.mergePr({ issueId: ID(2) })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `This pull request is stacked on EXP-11; merge EXP-11 first`,
    })
    expect(h.openStackThrough).not.toHaveBeenCalled()
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
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
    const { db: fake, wheres } = fakeDb([[row(2)], [row(1)]])
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
      [row(2, { prBaseBranch: `exp/EXP-11` })],
    ])
    const chain = await openStackThrough(fake, {
      issueId: ID(2),
      teamId: `ws-1`,
    })
    expect(chain.map((m) => m.identifier)).toEqual([`EXP-11`, `EXP-12`])
    expect(select).toHaveBeenCalledTimes(3)
  })

  it(`caps the walk at ten members`, async () => {
    const answers = Array.from({ length: 20 }, (_, i) => [
      {
        ...row(1),
        issueId: `i-${i}`,
        prUrl: `https://github.com/owner/repo/pull/${i + 1}`,
        prNumber: i + 1,
        prBaseBranch: `b-${i + 1}`,
      },
    ])
    const { db: fake } = fakeDb(answers)
    const chain = await openStackThrough(fake, {
      issueId: `i-0`,
      teamId: `ws-1`,
    })
    expect(chain).toHaveLength(10)
  })
})
