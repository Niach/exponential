import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1145 on the number-addressed merge path: `mergeRepositoryPull` backs
// `repositories.mergePull` (MCP `pr_merge({repositoryId, prNumber})`) and
// `codingSessions.mergePr` (MCP `pr_merge` with no subject, the run's own PR).
// A PR an issue links records its base (`pr_base_branch`); when that base is
// another OPEN PR's head it is refused before any claim or GitHub call. An
// issue-less PR records no base and merges as before.

const h = vi.hoisted(() => ({
  selectQueue: [] as unknown[][],
  mergePullRequestSmart: vi.fn(async () => ({
    merged: true,
    queued: false,
    sha: `abc`,
    mergedBy: null,
  })),
  applySessionPrState: vi.fn(async () => ({ endedSessionIds: [] })),
  applyPrMergeState: vi.fn(async () => {}),
}))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => {
      const rows = h.selectQueue.shift() ?? []
      const builder = {
        from: () => builder,
        where: () => builder,
        limit: async () => rows,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        then: (res: any, rej: any) => Promise.resolve(rows).then(res, rej),
      }
      return builder
    },
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
}))
vi.mock(`@/lib/integrations/pr-sync`, () => ({
  applySessionPrState: h.applySessionPrState,
  applyPrMergeState: h.applyPrMergeState,
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

beforeEach(() => {
  h.selectQueue.length = 0
  vi.clearAllMocks()
  _clearPrActorClaims()
})

describe(`mergeRepositoryPull on a PR stacked on an open PR (EXP-1145)`, () => {
  it(`refuses before any claim or GitHub call, naming the parent issue`, async () => {
    h.selectQueue.push([{ id: `issue-12`, prBaseBranch: `exp/EXP-11` }])
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
    h.selectQueue.push([{ id: `issue-12`, prBaseBranch: `exp/chat-1a2b3c4d` }])
    h.selectQueue.push([])
    h.selectQueue.push([{ prNumber: 240 }])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true })
    ).rejects.toMatchObject({
      message: `This pull request is stacked on #240; merge #240 first`,
    })
    expect(h.mergePullRequestSmart).not.toHaveBeenCalled()
  })

  it(`merges once the parent is no longer open`, async () => {
    h.selectQueue.push([{ id: `issue-12`, prBaseBranch: `exp/EXP-11` }])
    h.selectQueue.push([])
    h.selectQueue.push([])
    h.selectQueue.push([{ id: `issue-12` }])

    await expect(
      mergeRepositoryPull({ repo, prNumber: 242, userId: `actor`, viaAgent: true })
    ).resolves.toEqual({ merged: true })
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
  })
})
