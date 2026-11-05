import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1706 (issue-lifecycle.json `reconcile`): the watchdog's live-PR reconciler.

const h = vi.hoisted(() => {
  const state = { rows: [] as Record<string, unknown>[] }
  const chain = () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const builder: any = {}
    for (const key of [`from`, `innerJoin`, `where`]) builder[key] = () => builder
    builder.orderBy = async () => state.rows
    return builder
  }
  return { state, db: { select: vi.fn(chain) } }
})

vi.mock(`@/db/connection`, () => ({ db: h.db }))
vi.mock(`@/lib/bootstrap-self-hosted`, () => ({ reconcilePullRequestState: vi.fn(async () => false) }))
vi.mock(`@/lib/integrations/github-app`, () => ({ resolveRepoInstallationToken: vi.fn(async () => `tok`) }))
vi.mock(`@/lib/integrations/github-client`, () => ({
  githubBudgetLowForToken: vi.fn(() => false),
  isGithubRateLimited: () => false,
}))

import { PR_RECONCILE } from "@exp/domain-contract/issue-lifecycle"
import { reconcilePullRequestState } from "@/lib/bootstrap-self-hosted"
import { githubBudgetLowForToken } from "@/lib/integrations/github-client"
import { reconcileStaleLivePullRequests, resetPrReconcileForTests } from "./pr-reconcile"

const row = (n: number) => ({
  id: `pr-${n}`,
  number: n,
  state: `open`,
  base: `master`,
  fullName: `o/r`,
  installationId: 1,
})

beforeEach(() => {
  vi.clearAllMocks()
  resetPrReconcileForTests()
  h.state.rows = []
})

describe(`reconcileStaleLivePullRequests`, () => {
  it(`re-reads each quiet live PR's state once per window`, async () => {
    h.state.rows = [row(1), row(2)]
    const now = new Date(`2026-11-06T12:00:00Z`)
    expect(await reconcileStaleLivePullRequests(now)).toBe(2)
    expect(reconcilePullRequestState).toHaveBeenCalledWith(row(1), `o/r`, `tok`)
    expect(await reconcileStaleLivePullRequests(new Date(now.getTime() + 60_000))).toBe(0)
    expect(await reconcileStaleLivePullRequests(new Date(now.getTime() + PR_RECONCILE.staleMs))).toBe(2)
  })

  it(`reads at most a batch per tick`, async () => {
    h.state.rows = Array.from({ length: PR_RECONCILE.batch + 5 }, (_, i) => row(i + 1))
    expect(await reconcileStaleLivePullRequests()).toBe(PR_RECONCILE.batch)
  })

  it(`skips while the installation budget is low`, async () => {
    h.state.rows = [row(1)]
    vi.mocked(githubBudgetLowForToken).mockReturnValue(true)
    expect(await reconcileStaleLivePullRequests()).toBe(0)
    expect(reconcilePullRequestState).not.toHaveBeenCalled()
  })
})
