import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { authedProcedure } from "@/lib/trpc"
import { assertTeamMember } from "@/lib/team-membership"
import {
  loadWorkflow,
} from "./shared"

export const workflowFinalPrProcedures = {

  /** Every node landed — open the ONE final PR, integration branch → the
   *  repository's default branch. Idempotent. The engine calls it once the
   *  last node lands; EXP-1032 lets any MEMBER call it too, so a runner
   *  device retired after the last landing cannot strand the workflow
   *  (`openWorkflowFinalPr` itself refuses while a node is still open). */
  openFinalPr: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const workflow = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, workflow.teamId)
      if (workflow.finalPrUrl) return { url: workflow.finalPrUrl }
      const { openWorkflowFinalPr } = await import(`@/lib/workflow-final-pr`)
      return openWorkflowFinalPr(ctx.db, input.id, ctx.session.user.id)
    }),

  /**
   * EXP-1014: MEMBER: squash-merge the workflow's ONE final pull request
   * (integration branch → the default branch) from the workflow screen, the
   * one human review of the whole run. GitHub's acceptance completes the
   * workflow right here (`applyWorkflowFinalPrState`: status `done`,
   * `ended_at`, every covered issue to the team's PR-merge status), so a
   * self-hosted instance with no inbound webhook completes too; the webhook's
   * later echo is a no-op. Idempotent for an already merged PR.
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
