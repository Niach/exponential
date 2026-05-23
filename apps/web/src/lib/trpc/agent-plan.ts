import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { and, desc, eq, sql } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { comments, issues } from "@/db/schema"
import {
  assertCanApprovePlan,
  assertCanMutateIssue,
  getIssueWorkspaceContext,
  getWorkspaceMember,
} from "@/lib/workspace-membership"

async function assertAgentForIssue(userId: string, issueId: string) {
  const issueContext = await getIssueWorkspaceContext(issueId)
  const member = await getWorkspaceMember(userId, issueContext.workspaceId)
  if (!member || member.role !== `agent`) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Only an agent member can mutate an agent plan`,
    })
  }
  return issueContext
}

export const agentPlanRouter = router({
  // Daemon writes the latest plan (or empty body + state='awaiting_answer'
  // when it just asked questions). Bumps the revision counter so the daemon
  // can detect "the server has the version I just submitted" without racing.
  submitPlan: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        plan: z.string().max(50_000),
        state: z.enum([`awaiting_approval`, `awaiting_answer`]),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const issueContext = await assertAgentForIssue(
        ctx.session.user.id,
        input.issueId
      )

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        const [latestComment] = await tx
          .select({ createdAt: comments.createdAt })
          .from(comments)
          .where(eq(comments.issueId, input.issueId))
          .orderBy(desc(comments.createdAt))
          .limit(1)

        // Each plan revision is its own comment so the timeline renders the
        // full history. The latest kind='plan' comment IS the current plan.
        // Only insert when there's actual body text — for state ==
        // 'awaiting_answer' the body is empty because the questions live in
        // a separate kind='question' comment posted alongside this call.
        if (input.plan.length > 0) {
          await tx.insert(comments).values({
            issueId: input.issueId,
            workspaceId: issueContext.workspaceId,
            authorId: ctx.session.user.id,
            body: { text: input.plan },
            kind: `plan`,
          })
        }

        const [issue] = await tx
          .update(issues)
          .set({
            agentPlanState: input.state,
            agentPlanRevision: sql`${issues.agentPlanRevision} + 1`,
            agentPlanApprovedAt: null,
            agentPlanApprovedBy: null,
            agentLastCommentSeenAt: latestComment?.createdAt ?? new Date(),
          })
          .where(eq(issues.id, input.issueId))
          .returning()
        return { txId, issue }
      })

      return result
    }),

  approvePlan: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertCanApprovePlan(ctx.session.user.id, input.issueId)

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [current] = await tx
          .select({ state: issues.agentPlanState })
          .from(issues)
          .where(eq(issues.id, input.issueId))
          .limit(1)
        if (!current) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
        }
        if (current.state !== `awaiting_approval`) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `No plan is awaiting approval`,
          })
        }
        const [issue] = await tx
          .update(issues)
          .set({
            agentPlanState: `approved`,
            agentPlanApprovedAt: new Date(),
            agentPlanApprovedBy: ctx.session.user.id,
          })
          .where(eq(issues.id, input.issueId))
          .returning()
        return { txId, issue }
      })

      return result
    }),

  // Convenience: explicitly reset to `drafting` so the daemon re-runs plan
  // mode on the next tick without the user having to leave a comment first.
  requestChanges: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertCanApprovePlan(ctx.session.user.id, input.issueId)

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [issue] = await tx
          .update(issues)
          .set({
            agentPlanState: `drafting`,
            agentPlanApprovedAt: null,
            agentPlanApprovedBy: null,
          })
          .where(
            and(
              eq(issues.id, input.issueId),
              // Only meaningful before coding starts.
              sql`${issues.agentPlanState} IN ('drafting', 'awaiting_approval', 'awaiting_answer')`
            )
          )
          .returning()
        return { txId, issue }
      })

      return result
    }),

  // Human-triggered retry after a failed pipeline run. If the plan was
  // already approved we DON'T reset plan state — the failure was in the
  // coding stage, so the daemon should resume coding against the existing
  // approved plan rather than throwing it away and re-planning. For any
  // other state, we hard-reset so the next run starts fresh.
  retry: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      // Anyone who can mutate the issue can retry it. Approval gating stays
      // with approvePlan — retrying isn't an approval, it's a re-roll.
      await assertCanMutateIssue(ctx.session.user.id, input.issueId)

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [current] = await tx
          .select({ state: issues.agentPlanState })
          .from(issues)
          .where(eq(issues.id, input.issueId))
          .limit(1)
        if (!current) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
        }
        const preserveApprovedPlan = current.state === `approved`
        const [issue] = await tx
          .update(issues)
          .set(
            preserveApprovedPlan
              ? {
                  // Touch updated_at so Electric / pollControl notice and the
                  // daemon re-enters with the existing approved plan intact.
                  updatedAt: new Date(),
                }
              : {
                  agentPlanState: null,
                  agentPlanRevision: 0,
                  agentPlanApprovedAt: null,
                  agentPlanApprovedBy: null,
                  agentLastCommentSeenAt: null,
                  updatedAt: new Date(),
                }
          )
          .where(eq(issues.id, input.issueId))
          .returning()
        return { txId, issue }
      })

      return result
    }),

  // Called by the daemon at the very start of the produce_plan stage so the
  // web client gets an "Agent has started" spinner the moment the pipeline
  // engages, without having to wait for the plan to finish generating.
  // Only flips a null state — never overrides awaiting_approval/approved/etc.
  markStarted: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertAgentForIssue(ctx.session.user.id, input.issueId)
      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [issue] = await tx
          .update(issues)
          .set({ agentPlanState: `drafting` })
          .where(
            and(
              eq(issues.id, input.issueId),
              sql`${issues.agentPlanState} IS NULL`
            )
          )
          .returning()
        return { txId, issue: issue ?? null }
      })
      return result
    }),

  // Called by the daemon when an issue is hard-reset (e.g., re-assigned after
  // a previous failure). Wipes plan state on the server so the UI doesn't
  // show a stale plan while the new run is producing a fresh one.
  resetPlan: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertAgentForIssue(ctx.session.user.id, input.issueId)

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [issue] = await tx
          .update(issues)
          .set({
            agentPlanState: null,
            agentPlanRevision: 0,
            agentPlanApprovedAt: null,
            agentPlanApprovedBy: null,
            agentLastCommentSeenAt: null,
          })
          .where(eq(issues.id, input.issueId))
          .returning()
        return { txId, issue }
      })

      return result
    }),
})
