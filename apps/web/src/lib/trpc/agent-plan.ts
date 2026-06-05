import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { and, desc, eq, sql } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { comments, issueAgentState, issues, projects } from "@/db/schema"
import { createPullRequest } from "@/lib/integrations/github-pr"
import { resolveRepoInstallationToken } from "@/lib/integrations/github-app"
import {
  assertCanApprovePlan,
  assertCanMutateIssue,
  getIssueWorkspaceContext,
  getWorkspaceMember,
  resolveWorkspaceAccess,
} from "@/lib/workspace-membership"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { fireAndForgetAgentActionNotify } from "@/lib/integrations/notifications"
import { prStateSchema } from "@/lib/domain"

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
        // New agents pass their questions here (state='awaiting_answer')
        // instead of posting a separate kind='question' comment. Optional so
        // older agent builds keep working during the rollout window.
        question: z.string().max(50_000).optional(),
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

        // The structured store is the source of truth (web Plan Panel + daemon
        // read it). We ALSO mirror plan/question into comments as a server-side
        // dual-write so mobile clients — which still render plan/question from
        // comments and host their Approve UI there — keep working until they
        // ship a structured Plan Panel. Web hides these comments; no
        // notification fires (we never call fireAndForgetCommentNotify here).
        // Removing this dual-write is the P3c "drain" step, after mobile ships.
        const planText = input.plan.length > 0 ? { text: input.plan } : null
        const questionText =
          input.question && input.question.length > 0
            ? { text: input.question }
            : null
        // Captured so the watermark below includes the agent's own plan/question
        // comment — otherwise decide_stage would treat it as "new discussion"
        // and re-plan in a loop.
        let dualWriteAt: Date | null = null
        if (input.state === `awaiting_approval` && planText) {
          const [c] = await tx
            .insert(comments)
            .values({
              issueId: input.issueId,
              workspaceId: issueContext.workspaceId,
              authorId: ctx.session.user.id,
              body: planText,
              kind: `plan`,
            })
            .returning({ createdAt: comments.createdAt })
          dualWriteAt = c?.createdAt ?? null
        } else if (input.state === `awaiting_answer` && questionText) {
          const [c] = await tx
            .insert(comments)
            .values({
              issueId: input.issueId,
              workspaceId: issueContext.workspaceId,
              authorId: ctx.session.user.id,
              body: questionText,
              kind: `question`,
            })
            .returning({ createdAt: comments.createdAt })
          dualWriteAt = c?.createdAt ?? null
        }
        if (input.state === `awaiting_approval`) {
          await tx
            .insert(issueAgentState)
            .values({
              issueId: input.issueId,
              planText,
              question: null,
              questionAskedAt: null,
            })
            .onConflictDoUpdate({
              target: issueAgentState.issueId,
              set: {
                planText,
                question: null,
                questionAskedAt: null,
                updatedAt: new Date(),
              },
            })
        } else {
          await tx
            .insert(issueAgentState)
            .values({
              issueId: input.issueId,
              planText,
              question: questionText,
              questionAskedAt: new Date(),
            })
            .onConflictDoUpdate({
              target: issueAgentState.issueId,
              set: {
                // Preserve an existing plan if this call carried none.
                ...(planText ? { planText } : {}),
                question: questionText,
                questionAskedAt: new Date(),
                updatedAt: new Date(),
              },
            })
        }

        const [issue] = await tx
          .update(issues)
          .set({
            agentPlanState: input.state,
            agentPlanRevision: sql`${issues.agentPlanRevision} + 1`,
            agentPlanApprovedAt: null,
            agentPlanApprovedBy: null,
            agentLastCommentSeenAt:
              dualWriteAt ?? latestComment?.createdAt ?? new Date(),
          })
          .where(eq(issues.id, input.issueId))
          .returning()

        // Surface progress in the activity feed + (P3b) action-needed inbox.
        await recordIssueEvent(tx, {
          issueId: input.issueId,
          workspaceId: issueContext.workspaceId,
          actorUserId: ctx.session.user.id,
          type: input.state === `awaiting_approval` ? `plan_ready` : `agent_question`,
          payload: null,
        })

        return { txId, issue }
      })

      // Action-needed notification to the approver(s) only — the sole agent
      // event that pushes. Routine progress stays quiet.
      fireAndForgetAgentActionNotify({
        issueId: input.issueId,
        type:
          input.state === `awaiting_approval`
            ? `agent_plan_review`
            : `agent_question`,
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
      const issueContext = await assertAgentForIssue(
        ctx.session.user.id,
        input.issueId
      )
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
        // Only emit the "started" activity entry when we actually flipped a
        // null state (a real run kickoff), not on idempotent re-calls.
        if (issue) {
          await recordIssueEvent(tx, {
            issueId: input.issueId,
            workspaceId: issueContext.workspaceId,
            actorUserId: ctx.session.user.id,
            type: `agent_started`,
            payload: null,
          })
        }
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

  // The agent reports its PR onto the issue (one issue = one PR = one branch).
  // Writes the synced pr_* columns + emits pr_opened / pr_merged activity events
  // on transition. Replaces the old comment-body "PR opened:" convention.
  // Server-side PR creation: the agent pushes the branch, then calls this. The
  // server opens the PR with the owner's connected GitHub token (the agent no
  // longer holds a GitHub credential for the API) and records pr_* + pr_opened.
  openPr: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        branch: z.string().max(255),
        base: z.string().max(255),
        title: z.string().max(255).optional(),
        body: z.string().max(4000).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const issueContext = await assertAgentForIssue(
        ctx.session.user.id,
        input.issueId
      )

      const [row] = await ctx.db
        .select({
          repo: projects.githubRepo,
          identifier: issues.identifier,
          title: issues.title,
        })
        .from(issues)
        .innerJoin(projects, eq(projects.id, issues.projectId))
        .where(eq(issues.id, input.issueId))
        .limit(1)
      if (!row?.repo) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `No GitHub repo linked for this project`,
        })
      }

      const token = await resolveRepoInstallationToken(row.repo)
      if (!token) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The Exponential GitHub App isn't installed on ${row.repo}. Install it from Account → Integrations.`,
        })
      }

      const pr = await createPullRequest({
        repo: row.repo,
        head: input.branch,
        base: input.base,
        title: input.title ?? `[${row.identifier}] ${row.title}`,
        body: input.body ?? `Resolves ${row.identifier}.`,
        token,
      })

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [issue] = await tx
          .update(issues)
          .set({
            prUrl: pr.url,
            prNumber: pr.number,
            prState: `open`,
            branch: input.branch,
          })
          .where(eq(issues.id, input.issueId))
          .returning()
        await recordIssueEvent(tx, {
          issueId: input.issueId,
          workspaceId: issueContext.workspaceId,
          actorUserId: ctx.session.user.id,
          type: `pr_opened`,
          payload: { prUrl: pr.url, prNumber: pr.number },
        })
        return { txId, issue }
      })
    }),

  reportPr: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        prUrl: z.string().url(),
        prNumber: z.number().int().positive(),
        prState: prStateSchema,
        branch: z.string().max(255).optional(),
        mergedAt: z.string().datetime().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const issueContext = await assertAgentForIssue(
        ctx.session.user.id,
        input.issueId
      )

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [current] = await tx
          .select({ prState: issues.prState })
          .from(issues)
          .where(eq(issues.id, input.issueId))
          .limit(1)
        if (!current) {
          throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
        }

        const [issue] = await tx
          .update(issues)
          .set({
            prUrl: input.prUrl,
            prNumber: input.prNumber,
            prState: input.prState,
            branch: input.branch ?? undefined,
            prMergedAt:
              input.prState === `merged`
                ? input.mergedAt
                  ? new Date(input.mergedAt)
                  : new Date()
                : null,
          })
          .where(eq(issues.id, input.issueId))
          .returning()

        if (input.prState === `open` && current.prState !== `open`) {
          await recordIssueEvent(tx, {
            issueId: input.issueId,
            workspaceId: issueContext.workspaceId,
            actorUserId: ctx.session.user.id,
            type: `pr_opened`,
            payload: { prUrl: input.prUrl, prNumber: input.prNumber },
          })
        }
        if (input.prState === `merged` && current.prState !== `merged`) {
          await recordIssueEvent(tx, {
            issueId: input.issueId,
            workspaceId: issueContext.workspaceId,
            actorUserId: ctx.session.user.id,
            type: `pr_merged`,
            payload: { prUrl: input.prUrl },
          })
        }

        return { txId, issue }
      })
    }),

  // The agent reports a terminal error. Emits an agent_error activity event so
  // the timeline can surface a Retry affordance (the agent still posts the full
  // error as a comment separately, which notifies subscribers).
  reportError: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        message: z.string().max(2000),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const issueContext = await assertAgentForIssue(
        ctx.session.user.id,
        input.issueId
      )
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await recordIssueEvent(tx, {
          issueId: input.issueId,
          workspaceId: issueContext.workspaceId,
          actorUserId: ctx.session.user.id,
          type: `agent_error`,
          payload: { message: input.message },
        })
        return { txId }
      })
    }),

  // Human answers the agent's open question (replaces the old "reply with a
  // regular comment" convention). Clears the open question, flips the issue to
  // `drafting` (the daemon's re-plan signal), and records an agent_answer
  // event. Also dual-writes a regular comment during the rollout window so the
  // legacy comment-watermark daemon still detects the answer.
  answerQuestion: authedProcedure
    .input(
      z.object({
        issueId: z.string().uuid(),
        answer: z.string().min(1).max(10_000),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertCanMutateIssue(ctx.session.user.id, input.issueId)
      const issueContext = await getIssueWorkspaceContext(input.issueId)

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)

        await tx.insert(comments).values({
          issueId: input.issueId,
          workspaceId: issueContext.workspaceId,
          authorId: ctx.session.user.id,
          body: { text: input.answer },
          kind: `regular`,
        })

        await tx
          .update(issueAgentState)
          .set({ question: null, questionAskedAt: null, updatedAt: new Date() })
          .where(eq(issueAgentState.issueId, input.issueId))

        const [issue] = await tx
          .update(issues)
          .set({ agentPlanState: `drafting`, updatedAt: new Date() })
          .where(eq(issues.id, input.issueId))
          .returning()

        await recordIssueEvent(tx, {
          issueId: input.issueId,
          workspaceId: issueContext.workspaceId,
          actorUserId: ctx.session.user.id,
          type: `agent_answer`,
          payload: null,
        })

        return { txId, issue }
      })
    }),

  // Read the structured agent plan/question text for an issue (server-only —
  // not synced via Electric). Backs the web Plan Panel. Any workspace member or
  // public-workspace viewer may read it.
  getState: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const issueContext = await getIssueWorkspaceContext(input.issueId)
      await resolveWorkspaceAccess(ctx.session.user.id, issueContext.workspaceId)

      const [row] = await ctx.db
        .select({
          planText: issueAgentState.planText,
          question: issueAgentState.question,
          questionAskedAt: issueAgentState.questionAskedAt,
        })
        .from(issueAgentState)
        .where(eq(issueAgentState.issueId, input.issueId))
        .limit(1)

      const [issue] = await ctx.db
        .select({
          state: issues.agentPlanState,
          revision: issues.agentPlanRevision,
          approvedAt: issues.agentPlanApprovedAt,
        })
        .from(issues)
        .where(eq(issues.id, input.issueId))
        .limit(1)

      const planBody = row?.planText as { text: string } | null | undefined
      const questionBody = row?.question as { text: string } | null | undefined

      let planText = planBody?.text ?? null
      let question = questionBody?.text ?? null
      let questionAskedAt = row?.questionAskedAt ?? null

      // Rollout fallback: until the Rust daemon is updated to feed plan text +
      // questions through submitPlan's structured fields, they still arrive as
      // kind='plan'/'question' comments. Read the latest of those when the
      // structured store hasn't been populated for this issue yet.
      if (planText === null) {
        const [planComment] = await ctx.db
          .select({ body: comments.body })
          .from(comments)
          .where(
            and(eq(comments.issueId, input.issueId), eq(comments.kind, `plan`))
          )
          .orderBy(desc(comments.createdAt))
          .limit(1)
        planText =
          (planComment?.body as { text?: string } | undefined)?.text ?? null
      }
      if (question === null && issue?.state === `awaiting_answer`) {
        const [qComment] = await ctx.db
          .select({ body: comments.body, createdAt: comments.createdAt })
          .from(comments)
          .where(
            and(
              eq(comments.issueId, input.issueId),
              eq(comments.kind, `question`)
            )
          )
          .orderBy(desc(comments.createdAt))
          .limit(1)
        question =
          (qComment?.body as { text?: string } | undefined)?.text ?? null
        questionAskedAt = qComment?.createdAt ?? questionAskedAt
      }

      return {
        planText,
        question,
        questionAskedAt,
        state: issue?.state ?? null,
        revision: issue?.revision ?? 0,
        approvedAt: issue?.approvedAt ?? null,
      }
    }),
})
