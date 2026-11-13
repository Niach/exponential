// #EXP-1617: our own CI's checks of a PR at a commit, as check inputs (the
// newest run's job per check name). The GitHub repair snapshot merges them
// in, so a repair read never drops the rows our runner wrote.

import { and, desc, eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { ciJobs, ciRuns, teams } from "@/db/schema"
import type { CheckInput } from "@/lib/integrations/pr-checks"
import { ciRunUrl } from "@/lib/ci/runs"

export async function ownPullRequestChecks(pullRequestId: string, sha: string): Promise<CheckInput[]> {
  const rows = await db
    .select({ job: ciJobs, runId: ciRuns.id, slug: teams.slug })
    .from(ciJobs)
    .innerJoin(ciRuns, eq(ciRuns.id, ciJobs.runId))
    .innerJoin(teams, eq(teams.id, ciRuns.teamId))
    .where(and(eq(ciRuns.pullRequestId, pullRequestId), eq(ciRuns.sha, sha)))
    .orderBy(desc(ciRuns.number))
  const seen = new Map<string, CheckInput>()
  for (const { job, runId, slug } of rows) {
    if (seen.has(job.check)) continue
    seen.set(job.check, {
      name: job.check,
      status: job.status,
      conclusion: job.conclusion,
      startedAt: job.startedAt ?? (job.status === `queued` ? job.queuedAt : null),
      completedAt: job.completedAt,
      detailsUrl: ciRunUrl(slug, runId, job.id),
    })
  }
  return [...seen.values()]
}
