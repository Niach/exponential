import { z } from "zod"
import { TRPCError } from "@trpc/server"
import {
  and,
  eq,
  inArray,
  isNull,
  sql,
} from "drizzle-orm"
import {
  WORKFLOW_MAX_ISSUES,
  WORKFLOW_MAX_REVIEW_ROUNDS,
  wfReviewVerdictSchema,
  workflowReviewHeadSchema,
  workflowReviewOracleSchema,
  type WorkflowNodeReview,
} from "@exp/db-schema/domain"
import { authedProcedure } from "@/lib/trpc"
import { codingSessions, issues, workflowNodes } from "@/db/schema"
import { isWorkflowReviewBranch } from "@/lib/workflows"
import { reviewBranchRound } from "@/lib/sessions/session-tree"
import { MAX_SESSION_CHAIN_DEPTH } from "@/lib/steer-child-messages"
import { recordWorkflowEvent } from "@/lib/workflows/record-event"
import {
  bad,
  loadWorkflow,
  assertEngine,
  loadNode,
  reviewOutcome,
  isReviewRunOfNode,
  type Db,
} from "./shared"

const roundsUsedUp = () =>
  `Review rounds are used up after ${WORKFLOW_MAX_REVIEW_ROUNDS}; the node lands once its author is done and its findings are carried to the final pull request`

/** What a submitted verdict does about the round (FEED-53). Pure.
 *  `branchRound` = the round of the calling run's review branch, null when
 *  the caller is on none (then the next round is claimed, up to the cap). */
export type ReviewClaimPlan =
  | { kind: `claim`; round: number | null }
  | { kind: `stored`; review: WorkflowNodeReview }
  | { kind: `refuse`; message: string }

export function planReviewClaim(args: {
  branchRound: number | null
  reviewRound: number
  stored: WorkflowNodeReview | null
  head?: string
  max: number
}): ReviewClaimPlan {
  const { branchRound, stored, max } = args
  if (branchRound === null) {
    if (args.reviewRound + 1 > max) return { kind: `refuse`, message: roundsUsedUp() }
    return { kind: `claim`, round: null }
  }
  if (branchRound > max) {
    return {
      kind: `refuse`,
      message: `Review round ${branchRound} is past the cap of ${max}; this review should not have started. End without a verdict.`,
    }
  }
  if (stored && stored.round === branchRound && sameLook(stored.head, args.head)) {
    // The same round's verdict is on record: a retry or a duplicate
    // reviewer of that round. Its answer stands, nothing is written.
    return { kind: `stored`, review: stored }
  }
  if (stored && stored.round > branchRound) {
    return {
      kind: `refuse`,
      message: `Review round ${stored.round} is already on record; round ${branchRound}'s verdict is superseded. End without a verdict.`,
    }
  }
  return { kind: `claim`, round: branchRound }
}

/** Two looks at one round read the same code unless both name a head and
 *  the heads differ. */
function sameLook(a: string | undefined, b: string | undefined): boolean {
  return !a || !b || a === b
}

/** The claim's guard for a branch round: the node does not already carry
 *  that round's verdict for the same look (see `sameLook`). */
function sameRoundOnRecord(round: number, head: string | undefined) {
  const sameRound = sql`${workflowNodes.review} IS NOT NULL AND (${workflowNodes.review}->>'round')::int = ${round}`
  if (!head) return sql`NOT (${sameRound})`
  return sql`NOT (${sameRound} AND (${workflowNodes.review}->>'head' IS NULL OR ${workflowNodes.review}->>'head' = ${head}))`
}

function storedAnswer(review: WorkflowNodeReview) {
  return {
    round: review.round,
    ...reviewOutcome({
      verdict: review.verdict,
      oraclePassed: review.oracle ? review.oracle.passed : null,
      round: review.round,
    }),
  }
}

function landedAnswer(review: WorkflowNodeReview) {
  const requested = review.verdict === `request_changes` || review.oracle?.passed === false
  return {
    round: review.round,
    approve: !requested,
    state: `landed` as const,
    note: requested
      ? `Changes requested by the review wave; the wave's fix run takes them`
      : `Review wave passed`,
  }
}

/** How many findings a verdict lists: its bullet or numbered lines, else
 *  one for any text at all. */
export function countFindings(findings: string): number {
  const items = findings.split(`\n`).filter((line) => /^\s*(?:[-*•]|\d+[.)])\s+\S/.test(line))
  if (items.length > 0) return items.length
  return findings.trim() ? 1 : 0
}

/** EXP-1096: the `review_verdict` event line. */
export function reviewVerdictLine(args: {
  identifier: string | null
  landed: boolean
  round: number
  verdict: `approve` | `request_changes`
  oraclePassed: boolean | null
  findings: string
}): string {
  const what = args.landed ? `review wave round ${args.round}` : `review round ${args.round}`
  const who = args.identifier ? `${args.identifier} ${what}` : what
  if (args.verdict === `approve` && args.oraclePassed !== false) return `${who}: approved`
  const count = countFindings(args.findings)
  const findings = count > 0 ? `, ${count} finding${count === 1 ? `` : `s`}` : ``
  if (args.verdict === `approve`) return `${who}: approved but its checks failed${findings}`
  return `${who}: changes requested${findings}`
}

/** The round of the calling run's review branch for this node, walking a
 *  resume succession back like `isReviewRunOfNode` (a branch-less resume
 *  inherits its predecessor's), and the node's identifier. */
async function callingReviewRound(
  db: Db,
  sessionId: string,
  issueId: string,
  workflowId: string
): Promise<{ round: number | null; identifier: string | null }> {
  const [issue] = await db
    .select({ identifier: issues.identifier })
    .from(issues)
    .where(eq(issues.id, issueId))
    .limit(1)
  const identifier = issue?.identifier ?? null
  if (!identifier) return { round: null, identifier }
  let cursor: string | null = sessionId
  const seen = new Set<string>()
  while (cursor && !seen.has(cursor) && seen.size < MAX_SESSION_CHAIN_DEPTH) {
    seen.add(cursor)
    const [row] = await db
      .select({ branch: codingSessions.branch, resumedFromId: codingSessions.resumedFromId })
      .from(codingSessions)
      .where(eq(codingSessions.id, cursor))
      .limit(1)
    if (!row) break
    if (row.branch) {
      if (!isWorkflowReviewBranch(workflowId, identifier, row.branch)) break
      return { round: reviewBranchRound(row.branch), identifier }
    }
    cursor = row.resumedFromId ?? null
  }
  return { round: null, identifier }
}

export const workflowReviewProcedures = {

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
      if (node.state === `skipped`) {
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
      const landed = node.state === `landed`
      // FEED-53: the verdict is bound to the CALLING run's review branch
      // (`-r<n>`): a started round always gets to record, whatever a
      // duplicate reviewer did to `review_round`; only a round past the cap
      // (which the engine should never have started) is refused.
      const calling = await callingReviewRound(ctx.db, input.sessionId, node.issueId, workflow.id)
      const plan = planReviewClaim({
        branchRound: calling.round,
        reviewRound: node.reviewRound,
        stored,
        head: input.head,
        max: WORKFLOW_MAX_REVIEW_ROUNDS,
      })
      if (plan.kind === `refuse`) throw bad(plan.message)
      if (plan.kind === `stored`) {
        return landed ? landedAnswer(plan.review) : storedAnswer(plan.review)
      }
      const states = landed ? [`landed`] : [`in_review`, `updating`]
      const branchRound = plan.round
      const result = await ctx.db.transaction(async (tx) => {
        // The round is claimed IN the update (concurrent verdicts cannot share
        // one), and only a node in the expected state moves: a landed,
        // skipped or failed node keeps what it is. A branch round moves
        // `review_round` UP to it, never past it; a second verdict of that
        // same round loses the claim here.
        const [claimed] = await tx
          .update(workflowNodes)
          .set({
            reviewRound:
              branchRound === null
                ? sql`${workflowNodes.reviewRound} + 1`
                : sql`GREATEST(${workflowNodes.reviewRound}, ${branchRound})`,
          })
          .where(
            and(
              eq(workflowNodes.id, input.nodeId),
              inArray(workflowNodes.state, states),
              branchRound === null
                ? sql`${workflowNodes.reviewRound} < ${WORKFLOW_MAX_REVIEW_ROUNDS}`
                : sameRoundOnRecord(branchRound, input.head)
            )
          )
          .returning({ round: workflowNodes.reviewRound })
        if (!claimed) return null
        const round = branchRound ?? claimed.round
        if (round > WORKFLOW_MAX_REVIEW_ROUNDS) throw bad(roundsUsedUp())
        const review: WorkflowNodeReview = {
          verdict: input.verdict,
          findings: input.findings,
          oracle: input.oracle ?? null,
          model: input.model ?? null,
          round,
          at: new Date().toISOString(),
          ...(input.head && { head: input.head }),
        }
        if (landed) {
          // EXP-1103: a review WAVE's verdict on a landed node's diff. The
          // node keeps its state; the verdict is recorded (once per wave —
          // the round is the claim) and the engine hands every request of
          // the wave to ONE fix run. `approved_at` stays with
          // `clearReviewWave`.
          await tx.update(workflowNodes).set({ review }).where(eq(workflowNodes.id, input.nodeId))
          return landedAnswer(review)
        }
        const outcome = reviewOutcome({ verdict: input.verdict, oraclePassed, round })
        await tx
          .update(workflowNodes)
          .set({
            review,
            state: outcome.state,
            note: outcome.note,
            // A verdict at a newer head supersedes any earlier approval: an
            // approve stamps it, a request_changes withdraws it (the next
            // round's approve, or the cap, clears the node again).
            approvedAt: outcome.approve ? new Date() : null,
          })
          .where(eq(workflowNodes.id, input.nodeId))
        return { round, ...outcome }
      })
      if (!result) {
        // Lost the claim: the same round's verdict landed meanwhile (the
        // answer is on record), the cap, or the node moved on.
        if (branchRound !== null) {
          const fresh = await loadNode(input.nodeId)
          if (fresh.review?.round === branchRound) {
            return landed ? landedAnswer(fresh.review) : storedAnswer(fresh.review)
          }
        } else if (states.includes(node.state)) {
          throw bad(roundsUsedUp())
        }
        throw bad(landed ? `That node is not landed any more` : `That node is not under review`)
      }
      // EXP-1096: one line per recorded verdict (a replayed one wrote none).
      await recordWorkflowEvent(ctx.db, {
        workflowId: workflow.id,
        teamId: workflow.teamId,
        nodeId: node.id,
        sessionId: input.sessionId,
        kind: `review_verdict`,
        message: reviewVerdictLine({
          identifier: calling.identifier,
          landed,
          round: result.round,
          verdict: input.verdict,
          oraclePassed,
          findings: input.findings,
        }),
      })
      return result
    }),

  /** ENGINE (EXP-1103): a review wave is done — its reviewers answered and
   *  its one fix run landed (or nothing was requested). Stamps `approved_at`
   *  on the wave's landed nodes, which releases the layers behind it (a deep
   *  graph) and, after the last wave, the final pull request. Idempotent:
   *  a node already stamped keeps its stamp; a node that is not landed is
   *  skipped. */
  clearReviewWave: authedProcedure
    .input(
      z.object({
        workflowId: z.string().uuid(),
        wave: z.number().int().min(0).max(999),
        nodeIds: z.array(z.string().uuid()).max(WORKFLOW_MAX_ISSUES),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const workflow = await loadWorkflow(input.workflowId)
      await assertEngine(workflow, ctx.session.user.id)
      if (input.nodeIds.length === 0) return { cleared: 0 }
      const rows = await ctx.db
        .update(workflowNodes)
        .set({ approvedAt: new Date() })
        .where(
          and(
            eq(workflowNodes.workflowId, workflow.id),
            inArray(workflowNodes.id, input.nodeIds),
            eq(workflowNodes.state, `landed`),
            isNull(workflowNodes.approvedAt)
          )
        )
        .returning({ id: workflowNodes.id })
      return { cleared: rows.length }
    }),
}
