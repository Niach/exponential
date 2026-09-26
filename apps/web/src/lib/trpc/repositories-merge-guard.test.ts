import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// EXP-1094 on the issue-LESS merge path: `repositories.mergePull` (MCP
// `pr_merge({repositoryId, prNumber})`, Reviews' external group) takes a bare
// PR number, so "no issue" is only the caller's claim. The number is resolved
// to the team's issue rows first, and a PR a running/paused workflow covers
// gets the same refusal issues.mergePr gives; every other PR merges as before.

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  wheres: [] as unknown[],
  liveWorkflowCoveringPr: vi.fn(
    async (): Promise<{ workflowId: string; nodeId: string; issueId: string } | null> => null
  ),
  mergePullRequestSmart: vi.fn(async () => ({
    merged: true,
    queued: false,
    sha: `abc`,
    viaStack: false,
    stackNumber: null,
    stackMemberNumbers: [241],
  })),
  applySessionPrState: vi.fn(async () => ({ endedSessionIds: [] })),
  applyWorkflowFinalPrState: vi.fn(async () => {}),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => {
      const rows = h.selectQueue.shift() ?? []
      const builder = {
        from: () => builder,
        where: (cond: unknown) => {
          h.wheres.push(cond)
          return builder
        },
        limit: async () => rows,
      }
      return builder
    },
  },
}))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/workflows`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/workflows")>()),
  liveWorkflowCoveringPr: h.liveWorkflowCoveringPr,
}))
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
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applySessionPrState: h.applySessionPrState,
}))
vi.mock(`@/lib/workflow-final-pr`, () => ({
  applyWorkflowFinalPrState: h.applyWorkflowFinalPrState,
}))
vi.mock(`@/lib/team-membership`, () => ({
  assertTeamMember: vi.fn(),
  getIssueTeamContext: vi.fn(),
}))

import {
  WORKFLOW_MERGE_REFUSAL,
  mergeRepositoryPull,
} from "@/lib/trpc/repositories"
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
const PR_URL = `https://github.com/owner/repo/pull/241`

beforeEach(() => {
  h.selectQueue.length = 0
  h.wheres.length = 0
  vi.clearAllMocks()
  h.liveWorkflowCoveringPr.mockResolvedValue(null)
  _clearPrActorClaims()
})

describe(`mergeRepositoryPull (EXP-1094: the chore path is no bypass)`, () => {
  it(`refuses a live workflow node's PR handed in by number, before any claim or GitHub call`, async () => {
    h.selectQueue.push([{ id: `issue-1`, identifier: `EXP-11` }])
    h.liveWorkflowCoveringPr.mockResolvedValueOnce({
      workflowId: `wf-1`,
      nodeId: `n-1`,
      issueId: `issue-1`,
    })

    await expect(
      mergeRepositoryPull({ repo, prNumber: 241, userId: `actor`, viaAgent: true })
    ).rejects.toMatchObject({
      code: `PRECONDITION_FAILED`,
      message: `EXP-11's pull request ${WORKFLOW_MERGE_REFUSAL}`,
    })

    expect(h.liveWorkflowCoveringPr).toHaveBeenCalledWith(
      expect.anything(),
      `issue-1`,
      PR_URL
    )
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
    expect(takePrMergeClaim(`owner/repo`, 241)).toBeNull()
  })

  it(`resolves the number to the TEAM's issue rows by the derived pr_url`, async () => {
    h.selectQueue.push([])

    await mergeRepositoryPull({ repo, prNumber: 241, userId: `actor`, viaAgent: false })

    const query = new PgDialect().sqlToQuery(h.wheres[0] as never)
    expect(query.sql).toContain(`"pr_url" =`)
    expect(query.sql).toContain(`"team_id" =`)
    expect(query.params).toEqual([PR_URL, `ws-1`])
    // No linked issue: nothing to ask the workflow about, the merge runs.
    expect(h.liveWorkflowCoveringPr).not.toHaveBeenCalled()
    expect(h.mergePullRequestSmart).toHaveBeenCalledWith({
      repo: `owner/repo`,
      prNumber: 241,
      token: `tok`,
    })
  })

  it(`prefers the caller's stored pr_url over the derived one`, async () => {
    h.selectQueue.push([])
    const stored = `https://github.com/old-owner/repo/pull/241`

    await mergeRepositoryPull({
      repo,
      prNumber: 241,
      prUrl: stored,
      userId: `actor`,
      viaAgent: false,
    })

    const query = new PgDialect().sqlToQuery(h.wheres[0] as never)
    expect(query.params).toEqual([stored, `ws-1`])
    expect(h.applySessionPrState).toHaveBeenCalledWith(
      expect.objectContaining({ prUrl: stored, state: `merged` })
    )
  })

  it(`merges an issue-linked PR no live workflow covers`, async () => {
    h.selectQueue.push([{ id: `issue-1`, identifier: `EXP-11` }])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 241, userId: `actor`, viaAgent: false })
    ).resolves.toEqual({ merged: true })

    expect(h.liveWorkflowCoveringPr).toHaveBeenCalledTimes(1)
    expect(h.mergePullRequestSmart).toHaveBeenCalledTimes(1)
    expect(h.applyWorkflowFinalPrState).toHaveBeenCalledWith(
      expect.anything(),
      PR_URL,
      `merged`
    )
  })
})
