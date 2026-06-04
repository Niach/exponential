import { eq } from "drizzle-orm"
import { db } from "@/db/connection"
import { issues, projects } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { recordIssueEvent } from "@/lib/integrations/activity"

// Shared PR-merge writer, callable outside tRPC (webhook + self-hosted cron).
// Mirrors agentPlan.reportPr's open→merged write semantics: flips prState to
// 'merged', stamps prMergedAt, and emits a single pr_merged activity event.
//
// Idempotent on the open→merged transition: if the issue is already 'merged'
// we return without touching anything, so the webhook and the outbound cron
// can never double-fire the pr_merged event for the same PR.
export async function applyPrMergeState(opts: {
  issueId: string
  prUrl?: string
  mergedAt?: Date | null
  actorUserId?: string | null
}): Promise<void> {
  await db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    void txId

    const [current] = await tx
      .select({
        prState: issues.prState,
        prUrl: issues.prUrl,
        workspaceId: projects.workspaceId,
      })
      .from(issues)
      .innerJoin(projects, eq(projects.id, issues.projectId))
      .where(eq(issues.id, opts.issueId))
      .limit(1)

    // Unknown issue, or already merged → nothing to do (idempotent).
    if (!current) return
    if (current.prState === `merged`) return

    await tx
      .update(issues)
      .set({
        prState: `merged`,
        prMergedAt: opts.mergedAt ?? new Date(),
      })
      .where(eq(issues.id, opts.issueId))

    await recordIssueEvent(tx, {
      issueId: opts.issueId,
      workspaceId: current.workspaceId,
      actorUserId: opts.actorUserId ?? null,
      type: `pr_merged`,
      payload: { prUrl: opts.prUrl ?? current.prUrl ?? null },
    })
  })
}
