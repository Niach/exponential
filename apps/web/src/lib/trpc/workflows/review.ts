import { z } from "zod"
import { TRPCError } from "@trpc/server"
import {
  and,
  eq,
  inArray,
  sql,
} from "drizzle-orm"
import {
  WORKFLOW_MAX_REVIEW_ROUNDS,
  wfReviewVerdictSchema,
  workflowReviewHeadSchema,
  workflowReviewOracleSchema,
  type WorkflowNodeReview,
} from "@exp/db-schema/domain"
import { authedProcedure, generateTxId } from "@/lib/trpc"
import { workflowNodes, workflows } from "@/db/schema"
import { assertTeamMember } from "@/lib/team-membership"
import {
  bad,
  loadWorkflow,
  assertEngine,
  loadNode,
  bumpMetrics,
  reviewOutcome,
  isReviewRunOfNode,
} from "./shared"

export const workflowReviewProcedures = {

  /** A member clears a node's open PR for the merge train by hand (the agent
   *  review normally does; this is the way out when it did not converge).
   *  `approved: false` takes it back while the node has not landed. */
  approveNode: authedProcedure
    .input(z.object({ nodeId: z.string().uuid(), approved: z.boolean().default(true) }))
    .mutation(async ({ ctx, input }) => {
      const node = await loadNode(input.nodeId)
      const workflow = await loadWorkflow(node.workflowId)
      await assertTeamMember(ctx.session.user.id, workflow.teamId)
      if (node.state === `landed`) throw bad(`That node already landed`)
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(workflowNodes)
          .set({
            approvedAt: input.approved ? new Date() : null,
            // A person approves the pull request as it IS. The engine reads
            // an approval as stale while the stored review names a head the
            // PR has moved past (`approval_is_stale`), which after a
            // request_changes round is always the case once the author
            // pushed its fixes: the reviewer's head goes, the verdict and
            // findings stay on record.
            ...(input.approved && {
              review: sql`CASE WHEN ${workflowNodes.review} IS NULL THEN NULL ELSE ${workflowNodes.review} - 'head' END`,
            }),
          })
          .where(eq(workflowNodes.id, input.nodeId))
        if (input.approved) {
          // EXP-984 metric: how long the node sat waiting for a person. The
          // row's `updated_at` is when it last moved (into review).
          const [row] = await tx
            .select({ since: workflowNodes.updatedAt })
            .from(workflowNodes)
            .where(eq(workflowNodes.id, input.nodeId))
            .limit(1)
          const minutes = row
            ? Math.max(0, Math.round((Date.now() - new Date(row.since).getTime()) / 60_000))
            : 0
          await tx
            .update(workflows)
            .set({ metrics: bumpMetrics({ operatorMinutes: minutes }) })
            .where(eq(workflows.id, workflow.id))
        }
        return { txId }
      })
    }),


  /** A reviewer RUN's verdict on one node (EXP-984, MCP
   *  `exponential_workflows_review_submit`). Only the runner's owner may
   *  submit, and only FROM the reviewer run the engine started for the node
   *  (`sessionId` = the calling run): the author's own run, or any other,
   *  cannot approve the node. A reviewer RESUMED by a person (an account
   *  switch, a Resume) is still that reviewer: the calling row may be a
   *  successor re-branded `agent` by the MCP resume path, so the check takes
   *  the node's review branch as evidence and otherwise walks
   *  `resumed_from_id` back to the run the workflow started (FEED-51). */
  submitReview: authedProcedure
    .input(
      z.object({
        nodeId: z.string().uuid(),
        sessionId: z.string().uuid(),
        verdict: wfReviewVerdictSchema,
        findings: z.string().trim().max(8000).default(``),
        oracle: workflowReviewOracleSchema.nullable().optional(),
        model: z.string().max(64).nullable().optional(),
        // The PR head the reviewer read; the engine lands only while the PR
        // still points at it.
        head: workflowReviewHeadSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const node = await loadNode(input.nodeId)
      const workflow = await loadWorkflow(node.workflowId)
      await assertEngine(workflow, ctx.session.user.id)
      if (node.state === `landed` || node.state === `skipped`) {
        throw bad(`That node is already settled`)
      }
      if (!(await isReviewRunOfNode(ctx.db, input.sessionId, node, workflow.id))) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the review run the workflow started for this node may submit its verdict; the node's own run cannot review itself. Check nodeId against the node named in your prompt. If you are the node's author, do not review. If you are the resumed reviewer and this persists, report it with exponential_report_bug and end.`,
        })
      }
      // Idempotent for the head already reviewed: a desktop/CLI 0.14.49/
      // 0.14.50 host spawns a SECOND reviewer after an account switch and both
      // pass the gate above (FEED-51), and a reviewer retries after a lost
      // response. While the stored review names this very head and no round
      // was claimed since, the verdict on record IS the answer: no round
      // bumped, nothing overwritten.
      const stored = node.review
      if (input.head && stored?.head === input.head && stored.round === node.reviewRound) {
        return {
          round: stored.round,
          ...reviewOutcome({
            verdict: stored.verdict,
            oraclePassed: stored.oracle ? stored.oracle.passed : null,
            round: stored.round,
          }),
        }
      }
      const oraclePassed = input.oracle ? input.oracle.passed : null
      return ctx.db.transaction(async (tx) => {
        // The round is claimed IN the update (concurrent verdicts cannot share
        // one), and only a node that is under review or being updated moves:
        // a paused or waiting node keeps what a person decided.
        const [claimed] = await tx
          .update(workflowNodes)
          .set({ reviewRound: sql`${workflowNodes.reviewRound} + 1` })
          .where(
            and(
              eq(workflowNodes.id, input.nodeId),
              inArray(workflowNodes.state, [`in_review`, `updating`])
            )
          )
          .returning({ round: workflowNodes.reviewRound })
        if (!claimed) throw bad(`That node is not under review`)
        const round = claimed.round
        if (round > WORKFLOW_MAX_REVIEW_ROUNDS) {
          throw bad(
            `Review rounds are used up after ${WORKFLOW_MAX_REVIEW_ROUNDS}; a person decides this node now`
          )
        }
        const outcome = reviewOutcome({
          verdict: input.verdict,
          oraclePassed,
          round,
        })
        const review: WorkflowNodeReview = {
          verdict: input.verdict,
          findings: input.findings,
          oracle: input.oracle ?? null,
          model: input.model ?? null,
          round,
          at: new Date().toISOString(),
          ...(input.head && { head: input.head }),
        }
        await tx
          .update(workflowNodes)
          .set({
            review,
            state: outcome.state,
            note: outcome.note,
            // A verdict at a newer head supersedes any earlier approval: an
            // approve stamps it, a request_changes withdraws it (a person can
            // approve again after reading the findings).
            approvedAt: outcome.approve ? new Date() : null,
          })
          .where(eq(workflowNodes.id, input.nodeId))
        await tx
          .update(workflows)
          .set({
            metrics: bumpMetrics({
              reviewRounds: 1,
              ...(input.verdict === `request_changes` &&
                (oraclePassed === false
                  ? { defectsByOracle: 1 }
                  : { defectsByAgentReview: 1 })),
            }),
          })
          .where(eq(workflows.id, workflow.id))
        return { round, ...outcome }
      })
    }),
}
