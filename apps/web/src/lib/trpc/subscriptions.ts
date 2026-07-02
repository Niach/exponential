import { z } from "zod"
import { and, eq, sql } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { issueSubscribers } from "@/db/schema"
import {
  assertWorkspaceMember,
  getIssueWorkspaceContext,
} from "@/lib/workspace-membership"

// A manual subscribe/unsubscribe writes source='manual'. unsubscribed=true is
// what suppresses future auto-resubscribe (ensureSubscribed onConflictDoNothing
// leaves a manual row untouched).
async function setSubscription(
  ctx: { db: typeof import("@/db/connection").db; session: { user: { id: string } } },
  issueId: string,
  unsubscribed: boolean
) {
  const { workspaceId } = await getIssueWorkspaceContext(issueId)
  await assertWorkspaceMember(ctx.session.user.id, workspaceId)
  return await ctx.db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    // Raw SQL: uniq_issue_subscribers_user is a PARTIAL unique index, so the
    // conflict target must carry the index predicate — drizzle 0.39 silently
    // DROPS the `targetWhere` option (verified via .toSQL()).
    await tx.execute(sql`
      insert into issue_subscribers (issue_id, user_id, workspace_id, source, unsubscribed)
      values (${issueId}, ${ctx.session.user.id}, ${workspaceId}, 'manual', ${unsubscribed})
      on conflict (issue_id, user_id) where user_id is not null
      do update set unsubscribed = excluded.unsubscribed, source = 'manual', updated_at = now()
    `)
    return { txId }
  })
}

export const subscriptionsRouter = router({
  subscribe: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .mutation(({ ctx, input }) => setSubscription(ctx, input.issueId, false)),

  unsubscribe: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .mutation(({ ctx, input }) => setSubscription(ctx, input.issueId, true)),

  // Whether the caller is actively subscribed to an issue (clients also read the
  // synced issue_subscribers shape, but this is handy for one-off checks).
  isSubscribed: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const [row] = await ctx.db
        .select({ unsubscribed: issueSubscribers.unsubscribed })
        .from(issueSubscribers)
        .where(
          and(
            eq(issueSubscribers.issueId, input.issueId),
            eq(issueSubscribers.userId, ctx.session.user.id)
          )
        )
        .limit(1)
      return { subscribed: row ? !row.unsubscribed : false }
    }),
})
