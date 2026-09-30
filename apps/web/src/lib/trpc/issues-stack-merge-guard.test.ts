import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1094 on `issues.mergePr({mergeStack: true})` (Reviews' "Merge stack",
// MCP `pr_merge({mergeStack})`, which routes through the same procedure): the
// entry issue alone was guarded, but a stack lands as a unit, so a plain PR
// stacked on a live workflow node's PR would have landed the node PR beneath
// it. Every member is asked; one covered member refuses the whole merge,
// named, before any claim or GitHub call.

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  assertIssueAccess: vi.fn(async () => ({
    issueId: `issue-1`,
    boardId: `board-1`,
    teamId: `ws-1`,
  })),
  liveWorkflowCoveringPr: vi.fn(
    async (
      _executor: unknown,
      _issueId: string,
      _prUrl: string
    ): Promise<{ workflowId: string; nodeId: string; issueId: string } | null> => null
  ),
  mergePullRequestSmart: vi.fn(async () => ({
    merged: true,
    queued: false,
    sha: `abc`,
    viaStack: false,
    stackNumber: null,
    stackMemberNumbers: [241],
  })),
  getPullRequest: vi.fn(async () => ({
    state: `open` as const,
    merged: false,
    draft: false,
    headRef: `exp/EXP-11`,
    baseRef: `master`,
    mergeable: true,
    mergeableState: `clean`,
  })),
  applyPrMergeState: vi.fn(async () => {}),
  endMergedPrSessions: vi.fn(async () => {}),
}))

vi.mock(`@/db/connection`, () => {
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
vi.mock(`@/lib/workflows`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/workflows")>()),
  liveWorkflowCoveringPr: h.liveWorkflowCoveringPr,
}))
vi.mock(`@/lib/team-membership`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/team-membership")>()),
  assertIssueAccess: h.assertIssueAccess,
}))
vi.mock(`@/lib/integrations/github-pr`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/integrations/github-pr")>()),
  fetchPullFiles: vi.fn(),
  mergePullRequestSmart: h.mergePullRequestSmart,
  findStackForPull: vi.fn(async () => null),
  getPullRequest: h.getPullRequest,
  closePullRequest: vi.fn(),
  resolvePrBaseState: vi.fn(),
  retargetPullRequest: vi.fn(async () => {}),
  diagnoseUnmergeablePr: vi.fn(async () => null),
}))
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => true,
  resolveRepoInstallationTokenInfo: async () => ({
    token: `tok`,
    installationId: 77,
    expiresAt: null,
  }),
  resolveRepoDefaultBranchCached: async () => `master`,
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  isInstallationLinkedToTeam: async () => true,
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

import {
  WORKFLOW_MERGE_REFUSAL,
  issuesRouter,
  stackedOnMessage,
  stackedOnOpenPr,
} from "@/lib/trpc/issues"
import {
  _clearPrActorClaims,
  takePrMergeClaim,
} from "@/lib/integrations/pr-actor-claims"

const LOWER_ISSUE = `22222222-2222-4222-8222-222222222222`
const UPPER_ISSUE = `44444444-4444-4444-8444-444444444444`
const LOWER_PR_URL = `https://github.com/owner/repo/pull/241`
const UPPER_PR_URL = `https://github.com/owner/repo/pull/242`

const db = {
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

// The entry is the UPPER, plain PR (EXP-12); the node PR (EXP-11) sits
// beneath it on a candidate stack (base-branch edges only).
const upperEntryRow = {
  prNumber: 242,
  prUrl: UPPER_PR_URL,
  prState: `open`,
  identifier: `EXP-12`,
  title: `Upper`,
  branch: `exp/EXP-12`,
  prBaseBranch: `exp/EXP-11`,
  prStackNumber: null,
}
const stackRows = [
  {
    id: LOWER_ISSUE,
    identifier: `EXP-11`,
    title: `Lower`,
    status: `in_review`,
    branch: `exp/EXP-11`,
    prUrl: LOWER_PR_URL,
    prNumber: 241,
    prState: `open`,
    prBaseBranch: `master`,
    prStackNumber: null,
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
    prStackNumber: null,
  },
]

beforeEach(() => {
  h.selectQueue.length = 0
  vi.clearAllMocks()
  h.liveWorkflowCoveringPr.mockResolvedValue(null)
  h.assertIssueAccess.mockResolvedValue({
    issueId: UPPER_ISSUE,
    boardId: `board-1`,
    teamId: `ws-1`,
  })
  _clearPrActorClaims()
})

describe(`issues.mergePr({mergeStack}) under a live workflow (EXP-1094)`, () => {
  it(`refuses the stack when a member BENEATH the entry is a live node's PR`, async () => {
    h.selectQueue.push([upperEntryRow])
    h.selectQueue.push(stackRows)
    h.liveWorkflowCoveringPr.mockImplementation(async (_executor, issueId) =>
      issueId === LOWER_ISSUE
        ? { workflowId: `wf-1`, nodeId: `n-1`, issueId: LOWER_ISSUE }
        : null
    )

    await expect(
      caller.mergePr({ issueId: UPPER_ISSUE, mergeStack: true })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `EXP-11's pull request ${WORKFLOW_MERGE_REFUSAL}`,
    })

    // Nothing touched GitHub, nothing was claimed.
    expect(h.getPullRequest).not.toHaveBeenCalled()
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 241)).toBeNull()
    expect(takePrMergeClaim(`owner/repo`, 242)).toBeNull()
  })

  it(`asks about every open member with its own PR url, then merges`, async () => {
    h.selectQueue.push([upperEntryRow])
    h.selectQueue.push(stackRows)

    await expect(
      caller.mergePr({ issueId: UPPER_ISSUE, mergeStack: true })
    ).resolves.toMatchObject({ merged: true })

    // The entry guard (issue path) plus one probe per member, bottom-up.
    const asked = h.liveWorkflowCoveringPr.mock.calls.map(([, issueId, prUrl]) => [issueId, prUrl])
    expect(asked).toEqual([
      [UPPER_ISSUE, UPPER_PR_URL],
      [LOWER_ISSUE, LOWER_PR_URL],
      [UPPER_ISSUE, UPPER_PR_URL],
    ])
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(2)
  })
})

// FEED-64: the merge call failed, the PR read merged — by a PERSON on
// github.com. Their merge, their attribution: the claim goes and the state
// write is left to the webhook (`merged_by`, EXP-617). Still `merged: true`.
describe(`issues.mergePr after a merge confirmed from the PR's state (FEED-64)`, () => {
  const plainRow = {
    prNumber: 242,
    prUrl: UPPER_PR_URL,
    prState: `open`,
    identifier: `EXP-12`,
    title: `Upper`,
    branch: `exp/EXP-12`,
    prBaseBranch: `master`,
    prStackNumber: null,
  }

  it(`hands a person's merge to the webhook: claim released, nothing written`, async () => {
    h.selectQueue.push([plainRow])
    h.mergePullRequestSmart.mockResolvedValueOnce({
      merged: true,
      queued: false,
      sha: `d6ef0be6e5`,
      mergedBy: { login: `danny`, id: 7, type: `User` },
      viaStack: false,
      stackNumber: null,
      stackMemberNumbers: [242],
    } as never)

    await expect(caller.mergePr({ issueId: UPPER_ISSUE })).resolves.toEqual({
      merged: true,
      note: `PR #242 was already merged on GitHub by danny; its issues complete when the merge webhook lands.`,
    })
    expect(h.applyPrMergeState).not.toHaveBeenCalled()
    expect(h.endMergedPrSessions).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 242)).toBeNull()
  })

  it(`treats our own App's confirmed merge exactly like a normal one`, async () => {
    h.selectQueue.push([plainRow])
    // EXP-1145: the candidate-stack read (nobody's PR branch is `master`).
    h.selectQueue.push([])
    h.selectQueue.push([{ id: UPPER_ISSUE }])
    h.mergePullRequestSmart.mockResolvedValueOnce({
      merged: true,
      queued: false,
      sha: `d6ef0be6e5`,
      mergedBy: { login: `exponential[bot]`, type: `Bot` },
      viaStack: false,
      stackNumber: null,
      stackMemberNumbers: [242],
    } as never)

    await expect(caller.mergePr({ issueId: UPPER_ISSUE })).resolves.toEqual({
      merged: true,
    })
    expect(h.applyPrMergeState).toHaveBeenCalledTimes(1)
    // The claim stays for the webhook echo (consumed there, attributed to us).
    expect(takePrMergeClaim(`owner/repo`, 242)).toMatchObject({ userId: `actor` })
  })
})

// EXP-1145: the dialog's "Merge this pull request" promises to land THIS PR
// and the ones below it — true only for a REAL GitHub stack (merge-async takes
// the members below along). On a candidate stack (the preview API 404'd, or
// the edge came from a raw `base`) the single-PR path would squash the member
// INTO its base = the lower PR's head branch, so the diff never reached the
// default branch while the issue flipped to Done. MCP `pr_merge` routes
// through the same procedure, so one guard covers both.
describe(`issues.mergePr on a candidate stack member (EXP-1145)`, () => {
  it(`refuses the plain merge, naming the PR it is stacked on, before any claim or GitHub call`, async () => {
    h.selectQueue.push([upperEntryRow])
    h.selectQueue.push([{ identifier: `EXP-11` }])

    await expect(caller.mergePr({ issueId: UPPER_ISSUE })).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `This pull request is stacked on EXP-11; merge the stack or retarget it first`,
    })
    expect(stackedOnMessage(`EXP-11`)).toBe(
      `This pull request is stacked on EXP-11; merge the stack or retarget it first`
    )
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(h.getPullRequest).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 242)).toBeNull()
  })

  it(`leaves a REAL stack member (prStackNumber set) to merge-async, which lands the PRs below`, async () => {
    h.selectQueue.push([{ ...upperEntryRow, prStackNumber: 7 }])
    // No candidate read happens; the next select is the linked-issues one.
    h.selectQueue.push([{ id: UPPER_ISSUE }])

    await expect(caller.mergePr({ issueId: UPPER_ISSUE })).resolves.toMatchObject({
      merged: true,
    })
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
    expect(h.mergePullRequestSmart).toHaveBeenCalledWith(
      expect.objectContaining({ prNumber: 242, knownStackNumber: 7 })
    )
    expect(db.select).toHaveBeenCalledTimes(2)
  })

  it(`merges a plain PR whose base is nobody's open PR branch`, async () => {
    h.selectQueue.push([upperEntryRow])
    // The lower PR is merged/closed (or the branch belongs to no team issue).
    h.selectQueue.push([])
    h.selectQueue.push([{ id: UPPER_ISSUE }])

    await expect(caller.mergePr({ issueId: UPPER_ISSUE })).resolves.toMatchObject({
      merged: true,
    })
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
  })

  it(`mergeStack on the same member is untouched (it walks the chain instead)`, async () => {
    h.selectQueue.push([upperEntryRow])
    h.selectQueue.push(stackRows)

    await expect(
      caller.mergePr({ issueId: UPPER_ISSUE, mergeStack: true })
    ).resolves.toMatchObject({ merged: true })
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(2)
  })
})

describe(`stackedOnOpenPr (EXP-1145)`, () => {
  it(`asks for the team issue whose OPEN PR head, in the same repo, is this PR's base`, async () => {
    const wheres: unknown[] = []
    const p = Promise.resolve([{ identifier: `EXP-11` }]) as Promise<unknown[]> &
      Record<string, (arg?: unknown) => unknown>
    for (const m of [`from`, `limit`]) p[m] = () => p
    p.where = (cond?: unknown) => {
      wheres.push(cond)
      return p
    }
    await expect(
      stackedOnOpenPr({ select: () => p } as never, {
        issueId: UPPER_ISSUE,
        teamId: `ws-1`,
        repoFullName: `owner/repo`,
        prBaseBranch: `exp/EXP-11`,
      })
    ).resolves.toBe(`EXP-11`)
    const { PgDialect } = await import(`drizzle-orm/pg-core`)
    const query = new PgDialect().sqlToQuery(wheres[0] as never)
    expect(query.sql).toContain(`"team_id" =`)
    expect(query.sql).toContain(`"id" <>`)
    expect(query.sql).toContain(`"branch" =`)
    expect(query.sql).toContain(`"pr_state" =`)
    expect(query.sql).toContain(`"pr_url" like`)
    expect(query.params).toEqual([
      `ws-1`,
      UPPER_ISSUE,
      `exp/EXP-11`,
      `open`,
      `https://github.com/owner/repo/pull/%`,
    ])
  })

  it(`never queries for a PR without a recorded base`, async () => {
    const select = vi.fn()
    await expect(
      stackedOnOpenPr({ select } as never, {
        issueId: UPPER_ISSUE,
        teamId: `ws-1`,
        repoFullName: `owner/repo`,
        prBaseBranch: null,
      })
    ).resolves.toBeNull()
    expect(select).not.toHaveBeenCalled()
  })
})
