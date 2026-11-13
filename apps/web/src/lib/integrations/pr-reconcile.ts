import { repoInstallationId } from "@/lib/ecode/provider-config"
import { and, asc, eq, inArray, lt } from "drizzle-orm"
import { PR_RECONCILE } from "@exp/domain-contract/issue-lifecycle"
import { db } from "@/db/connection"
import { pullRequests, repositories } from "@/db/schema"
import { reconcilePullRequestState } from "@/lib/bootstrap-self-hosted"
import { resolveRepoInstallationToken } from "@/lib/integrations/github-app"
import { githubBudgetLowForToken, isGithubRateLimited } from "@/lib/integrations/github-client"
import { LIVE_PR_STATES } from "@/lib/pr-merge-guard"

// EXP-1706 (issue-lifecycle.json `reconcile`): the watchdog's live-PR
// reconciler. A lost `closed` delivery used to strand a merged PR as open
// forever on cloud (the checks sweep re-reads checks only, the poller runs
// only with GITHUB_POLLING). Every tick re-reads the state of live PRs whose
// row went quiet, each at most once per window (in-memory, per process),
// oldest first, inside the installation budget.

const lastRead = new Map<string, number>()

export async function reconcileStaleLivePullRequests(now: Date = new Date()): Promise<number> {
  const cutoff = new Date(now.getTime() - PR_RECONCILE.staleMs)
  const rows = await db
    .select({
      id: pullRequests.id,
      number: pullRequests.number,
      state: pullRequests.state,
      base: pullRequests.base,
      fullName: repositories.fullName,
      installationId: repoInstallationId(),
    })
    .from(pullRequests)
    .innerJoin(repositories, eq(repositories.id, pullRequests.repositoryId))
    .where(and(inArray(pullRequests.state, [...LIVE_PR_STATES]), lt(pullRequests.updatedAt, cutoff)))
    .orderBy(asc(pullRequests.updatedAt))
  const live = new Set(rows.map((row) => row.id))
  for (const id of lastRead.keys()) if (!live.has(id)) lastRead.delete(id)

  let read = 0
  for (const row of rows) {
    if (read >= PR_RECONCILE.batch) break
    const last = lastRead.get(row.id)
    if (last !== undefined && now.getTime() - last < PR_RECONCILE.staleMs) continue
    try {
      const token = await resolveRepoInstallationToken(row.fullName, { fallbackInstallationId: row.installationId })
      // Interactive reads (an agent's op, a person's merge) keep the rest.
      if (!token || githubBudgetLowForToken(token, now.getTime())) continue
      lastRead.set(row.id, now.getTime())
      read++
      await reconcilePullRequestState(row, row.fullName, token)
    } catch (err) {
      if (isGithubRateLimited(err)) continue
      console.error(`[pr-reconcile] ${row.fullName}#${row.number}:`, err)
    }
  }
  return read
}

export function resetPrReconcileForTests(): void {
  lastRead.clear()
}
