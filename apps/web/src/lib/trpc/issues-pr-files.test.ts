import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1154: every md+ issue view with an open PR asks `issues.prFiles`, so a
// warm answer must skip the installation-token resolve (an uncached App-JWT
// call to `/repos/{repo}/installation`), like `branchDiff` does.

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  fetchPullFiles: vi.fn(async () => [
    { filename: `a.ts`, status: `modified`, additions: 1, deletions: 0 },
  ]),
  resolveRepoInstallationTokenInfo: vi.fn(async () => ({
    token: `tok`,
    installationId: 77,
    expiresAt: null,
  })),
}))

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/team-membership")>()
  return {
    ...actual,
    getIssueTeamContext: vi.fn(async () => ({
      teamId: `ws-1`,
      boardId: `board-1`,
    })),
    assertTeamMember: vi.fn(async () => undefined),
  }
})
vi.mock(`@/lib/integrations/github-pr`, async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("@/lib/integrations/github-pr")>()
  return { ...actual, fetchPullFiles: h.fetchPullFiles }
})
vi.mock(`@/lib/integrations/github-app`, () => ({
  githubAppConfigured: () => true,
  resolveRepoInstallationTokenInfo: h.resolveRepoInstallationTokenInfo,
  resolveRepoDefaultBranchCached: vi.fn(),
}))
vi.mock(`@/lib/trpc/integrations`, () => ({
  isInstallationLinkedToTeam: vi.fn(async () => true),
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applyPrClosedState: vi.fn(),
  applyPrMergeState: vi.fn(),
  applySessionPrState: vi.fn(async () => ({ endedSessionIds: [] })),
  endMergedPrSessions: vi.fn(),
  retargetChildrenOfMergedPr: vi.fn(),
}))

import { _clearPrFilesAnswerCache, issuesRouter } from "@/lib/trpc/issues"

const ISSUE_ID = `22222222-2222-4222-8222-222222222222`
const PR_URL = `https://github.com/owner/repo/pull/241`

const db = {
  select: vi.fn(() => {
    const rows = h.selectQueue.shift() ?? []
    const builder = {
      from: () => builder,
      where: () => builder,
      limit: async () => rows,
    }
    return builder
  }),
}

const caller = issuesRouter.createCaller({
  session: { user: { id: `actor` } },
  db,
  request: new Request(`http://localhost/`),
} as never)

beforeEach(() => {
  vi.clearAllMocks()
  h.selectQueue = []
  _clearPrFilesAnswerCache()
})

describe(`issues.prFiles`, () => {
  it(`resolves the token once, then answers warm reads from the cache`, async () => {
    h.selectQueue.push([{ prNumber: 241, prUrl: PR_URL }])
    const first = await caller.prFiles({ issueId: ISSUE_ID })
    expect(first.files).toHaveLength(1)
    expect(h.resolveRepoInstallationTokenInfo).toHaveBeenCalledTimes(1)
    expect(h.fetchPullFiles).toHaveBeenCalledWith(`owner/repo`, 241, `tok`)

    h.selectQueue.push([{ prNumber: 241, prUrl: PR_URL }])
    const second = await caller.prFiles({ issueId: ISSUE_ID })
    expect(second).toEqual(first)
    expect(h.resolveRepoInstallationTokenInfo).toHaveBeenCalledTimes(1)
    expect(h.fetchPullFiles).toHaveBeenCalledTimes(1)
  })

  it(`does not keep a failed read`, async () => {
    h.fetchPullFiles.mockRejectedValueOnce(new Error(`GitHub returned 502`))
    h.selectQueue.push([{ prNumber: 241, prUrl: PR_URL }])
    await expect(caller.prFiles({ issueId: ISSUE_ID })).rejects.toMatchObject({
      code: `BAD_GATEWAY`,
    })

    h.selectQueue.push([{ prNumber: 241, prUrl: PR_URL }])
    await caller.prFiles({ issueId: ISSUE_ID })
    expect(h.resolveRepoInstallationTokenInfo).toHaveBeenCalledTimes(2)
  })
})
