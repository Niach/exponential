import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq } from "drizzle-orm"
import { workflowNodes, workflows } from "@/db/schema"
import { authedProcedure, generateTxId } from "@/lib/trpc"
import { assertTeamMember } from "@/lib/team-membership"
import {
  appendDecisionLine,
  assertEngine,
  loadWorkflow,
} from "./shared"

// EXP-1082 §7 / EXP-1072 / EXP-1059: the ONE final pull request's lifecycle.
// It is resolved as the workflow's own PR everywhere a PR is shown (Reviews
// ×4, the PR pickers, the workflow page), so it merges through
// `mergeFinalPr`, gets the fix-conflicts run like any linked PR, and its
// merge flips the member issues through the same status writer the
// linked-PR merge path uses (`applyWorkflowFinalPrState`).

export const workflowFinalPrProcedures = {

  /** Every node landed — open the ONE final PR, integration branch → the
   *  repository's default branch. Idempotent. The engine calls it once the
   *  last node lands; EXP-1032 lets any MEMBER call it too, so a runner
   *  device retired after the last landing cannot strand the workflow
   *  (`openWorkflowFinalPr` itself refuses while a node is still open).
   *  EXP-1059: on a final PR that was CLOSED without merging (the engine
   *  already spent its one reopen, or the runner is gone) a member's call
   *  reopens it, and when GitHub refuses that (head branch gone) opens a
   *  fresh one — the workflow page's way out of "closed". */
  openFinalPr: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const workflow = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, workflow.teamId)
      const { openWorkflowFinalPr, reopenWorkflowFinalPr } = await import(
        `@/lib/workflow-final-pr`
      )
      if (workflow.finalPrUrl && workflow.finalPrState !== `closed`) {
        return { url: workflow.finalPrUrl }
      }
      if (workflow.finalPrUrl) {
        const outcome = await reopenWorkflowFinalPr(ctx.db, input.id, ctx.session.user.id, {
          once: false,
        })
        if (outcome.reopened) return { url: outcome.url }
        if (!outcome.gaveUp) return { url: workflow.finalPrUrl }
        // GitHub will not reopen it: a NEW final PR from the same branch.
      }
      return openWorkflowFinalPr(ctx.db, input.id, ctx.session.user.id)
    }),

  /** ENGINE (EXP-1059): the final PR was closed without merging — reopen it
   *  ONCE. A second close is a person's decision: the workflow keeps
   *  running with a decision line and a `failed` event, and the engine's
   *  `finalPrReopened` host fact stops it asking again. `gaveUp` tells the
   *  host to remember that. */
  reopenFinalPr: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const workflow = await loadWorkflow(input.id)
      await assertEngine(workflow, ctx.session.user.id)
      const { reopenWorkflowFinalPr } = await import(`@/lib/workflow-final-pr`)
      const outcome = await reopenWorkflowFinalPr(ctx.db, input.id, ctx.session.user.id, {
        once: true,
      })
      return outcome.reopened
        ? { reopened: true as const, url: outcome.url }
        : { reopened: false as const, reason: outcome.reason, gaveUp: outcome.gaveUp }
    }),

  /** ENGINE (EXP-1059): every admitted node was SKIPPED — nothing reached
   *  the integration branch, so there is no final PR to open. The workflow
   *  ends `cancelled` with the decision line `nothing shipped: every node
   *  was skipped`; the engine's next pass then drops the branch as for any
   *  cancel. Refuses while a node is still open or landed. Idempotent. */
  cancelUnshipped: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const workflow = await loadWorkflow(input.id)
      await assertEngine(workflow, ctx.session.user.id)
      if (workflow.status === `cancelled`) return { cancelled: true as const }
      if (workflow.status !== `running` && workflow.status !== `paused`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `Only a started workflow can end unshipped`,
        })
      }
      const nodes = await ctx.db
        .select({ id: workflowNodes.id, state: workflowNodes.state })
        .from(workflowNodes)
        .where(eq(workflowNodes.workflowId, input.id))
      const admitted = nodes.filter((node) => node.state !== `proposed`)
      if (admitted.length === 0 || admitted.some((node) => node.state !== `skipped`)) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `Only a workflow whose every node was skipped ends unshipped`,
        })
      }
      const { NOTHING_SHIPPED_DECISION, recordWorkflowEventInTx } = await import(
        `@/lib/workflow-final-pr`
      )
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(workflows)
          .set({
            status: `cancelled`,
            endedAt: new Date(),
            decisions: appendDecisionLine(workflow.decisions, NOTHING_SHIPPED_DECISION, new Date()),
          })
          .where(and(eq(workflows.id, input.id), eq(workflows.status, workflow.status)))
        await recordWorkflowEventInTx(tx, {
          workflowId: input.id,
          teamId: workflow.teamId,
          kind: `cancelled`,
          message: `Nothing shipped: every node was skipped`,
        })
        return { cancelled: true as const, txId }
      })
    }),

  /**
   * EXP-1014: MEMBER: squash-merge the workflow's ONE final pull request
   * (integration branch → the default branch) from the workflow screen, the
   * one human review of the whole run. GitHub's acceptance completes the
   * workflow right here (`applyWorkflowFinalPrState`: status `done`,
   * `ended_at`, every covered issue to the team's PR-merge status through
   * the linked-PR merge path's status writer, a `completed` event), so a
   * self-hosted instance with no inbound webhook completes too; the
   * webhook's later echo is a no-op. Idempotent for an already merged PR.
   */
  mergeFinalPr: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const workflow = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, workflow.teamId)
      if (workflow.status === `cancelled`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The workflow was cancelled; its branch is not for merging`,
        })
      }
      if (!workflow.finalPrUrl || workflow.finalPrNumber == null) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The workflow has no final pull request yet`,
        })
      }
      if (workflow.finalPrState === `merged`) return { merged: true as const }
      if (!workflow.repositoryId) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `The workflow's repository is gone`,
        })
      }
      const { loadRepository, mergeRepositoryPull } = await import(`@/lib/trpc/repositories`)
      const repo = await loadRepository(workflow.repositoryId)
      // The repository row is looked up by id alone: it has to be THIS
      // team's, or a member would merge into another team's repository.
      if (repo.teamId !== workflow.teamId) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Repository not found` })
      }
      await mergeRepositoryPull({
        repo,
        prNumber: workflow.finalPrNumber,
        userId: ctx.session.user.id,
        viaAgent: ctx.viaMcp === true,
        prUrl: workflow.finalPrUrl,
      })
      // `mergeRepositoryPull` already applied this for the very same url
      // (EXP-1032); asking again costs one read and cannot apply twice — the
      // applier returns early on a row that reads `merged`.
      const { applyWorkflowFinalPrState } = await import(`@/lib/workflow-final-pr`)
      await applyWorkflowFinalPrState(ctx.db, workflow.finalPrUrl, `merged`)
      return { merged: true as const }
    }),
}
