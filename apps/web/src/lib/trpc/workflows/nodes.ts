import { z } from "zod"
import {
  and,
  eq,
  inArray,
  notInArray,
  sql,
} from "drizzle-orm"
import {
  WORKFLOW_MAX_ISSUES,
  wfNodeStateSchema,
  workflowTouchesSchema,
  wfNodeKindSchema,
  wfRiskSchema,
} from "@exp/db-schema/domain"
import { authedProcedure, generateTxId } from "@/lib/trpc"
import { codingSessions, devices, issues, workflowNodes, workflows } from "@/db/schema"
import { assertTeamMember } from "@/lib/team-membership"
import { parseVersionTuple } from "@/lib/client-version"
import { loadWorkflowEdges, replanWorkflow } from "@/lib/workflows"
import {
  reviewWaveGate,
  WAITING_FOR_REVIEW_WAVE,
  WAITING_FOR_RUN_TO_END,
} from "@/lib/workflow-waves"
import {
  bad,
  loadWorkflow,
  assertDraft,
  assertEngine,
  loadNode,
  mergeBelongsToAttempt,
  mergedNodeOutcome,
  appendDecisionLine,
  carriedReviewAtCap,
  carriedFindingsLine,
  type Db,
} from "./shared"
import { recordWorkflowEvent } from "@/lib/workflows/record-event"

/** Whether a node's run (if it has one) is no longer live. */
async function runEnded(db: Db, sessionId: string | null): Promise<boolean> {
  if (!sessionId) return true
  const [run] = await db
    .select({ status: codingSessions.status })
    .from(codingSessions)
    .where(eq(codingSessions.id, sessionId))
    .limit(1)
  return !run || (run.status !== `running` && run.status !== `in_review`)
}

/**
 * compat: the first engine that reads `landNode`'s run and review-wave
 * answers (`WAITING_FOR_RUN_TO_END` / `WAITING_FOR_REVIEW_WAVE`). An older
 * desktop/CLI engine (0.14.49..0.14.51) knows three refusal strings and
 * treats any other as a GitHub merge conflict, trapping the node in
 * `updating`, so it gets the blockers answer instead. Delete once
 * CLIENT_MIN_VERSION_DESKTOP/_CLI >= 0.14.52 and _ANDROID >= 0.14.45 and
 * _IOS >= 0.14.44.
 */
const WAVE_AWARE_ENGINE: [number, number, number] = [0, 14, 52]
const LEGACY_WAIT = `Waiting for its blockers to land`

/** Whether the workflow's runner (the caller's device row, `assertEngine`
 *  proved it) predates the wave-aware engine. No version counts as old. */
async function runnerPredatesWaves(
  db: Db,
  workflow: { deviceId: string | null },
  userId: string
): Promise<boolean> {
  if (!workflow.deviceId) return true
  const [device] = await db
    .select({ version: devices.version })
    .from(devices)
    .where(and(eq(devices.deviceId, workflow.deviceId), eq(devices.userId, userId)))
    .limit(1)
  const version = device?.version ? parseVersionTuple(device.version) : null
  if (!version) return true
  for (let i = 0; i < 3; i++) {
    if (version[i] !== WAVE_AWARE_ENGINE[i]) return version[i]! < WAVE_AWARE_ENGINE[i]!
  }
  return false
}

export const workflowNodeProcedures = {

  /**
   * compat: a MEMBER clears a node by hand. iOS <=0.14.43, Android <=0.14.44
   * and desktop/CLI <=0.14.51 show "Approve" on an `in_review` node without
   * `approved_at` and call this; an engine of those builds lands a node only
   * once `approved_at` is set, so this is the person's way past its review
   * cap. `approved_at` non-null is what the wave gate reads as cleared, so
   * the new engine reads a person's approval the same way. Delete once
   * CLIENT_MIN_VERSION_DESKTOP/_CLI >= 0.14.52 and _ANDROID >= 0.14.45 and
   * _IOS >= 0.14.44.
   */
  approveNode: authedProcedure
    .input(z.object({ nodeId: z.string().uuid(), approved: z.boolean().optional().default(true) }))
    .mutation(async ({ ctx, input }) => {
      const node = await loadNode(input.nodeId)
      const workflow = await loadWorkflow(node.workflowId)
      await assertTeamMember(ctx.session.user.id, workflow.teamId)
      if (node.state === `landed`) throw bad(`That node already landed`)
      if (node.state === `skipped`) throw bad(`That node was skipped`)
      if (input.approved && node.state !== `in_review` && node.state !== `updating`) {
        throw bad(`Only a node under review can be approved`)
      }
      // Idempotent: a second approval, or a withdrawal of none, writes nothing.
      const already = input.approved ? node.approvedAt !== null : node.approvedAt === null
      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        if (!already) {
          await tx
            .update(workflowNodes)
            .set({ approvedAt: input.approved ? new Date() : null })
            .where(eq(workflowNodes.id, input.nodeId))
        }
        return { txId, nodeId: node.id }
      })
      if (input.approved && !already) {
        await recordWorkflowEvent(ctx.db, {
          workflowId: workflow.id,
          teamId: workflow.teamId,
          nodeId: node.id,
          sessionId: node.sessionId,
          kind: `review_verdict`,
          message: `approved by a person`,
        })
      }
      return result
    }),

  /**
   * compat: the metrics counters left with migration 0146; a desktop/CLI
   * engine <=0.14.51 still reports them every beat and logs a warning on a
   * refusal. Accepted, nothing written. Delete once
   * CLIENT_MIN_VERSION_DESKTOP/_CLI >= 0.14.52 and _ANDROID >= 0.14.45 and
   * _IOS >= 0.14.44.
   */
  reportMetrics: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        deltas: z.record(z.string(), z.number()).optional(),
      })
    )
    .mutation(async () => ({ ok: true as const })),

  /** What the planner declares per node. Addressed by ISSUE (a member's id
   *  resolves to its compound node). */
  updateNode: authedProcedure
    .input(
      z.object({
        workflowId: z.string().uuid(),
        issueId: z.string().uuid(),
        kind: wfNodeKindSchema.optional(),
        risk: wfRiskSchema.optional(),
        touches: workflowTouchesSchema.optional(),
        // compat: migration 0134 dropped node budgets, but iOS ≤0.14.38,
        // Android ≤0.14.39 and desktop/CLI ≤0.14.46 still send a BUDGET-ONLY
        // patch (iOS on every blur). Accepted and ignored. Delete this key and
        // the no-op branch below once CLIENT_MIN_VERSION_IOS ≥ 0.14.39,
        // CLIENT_MIN_VERSION_ANDROID ≥ 0.14.40 and
        // CLIENT_MIN_VERSION_DESKTOP/CLI ≥ 0.14.47.
        budget: z.unknown().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const existing = await loadWorkflow(input.workflowId)
      await assertTeamMember(ctx.session.user.id, existing.teamId)
      const nodes = await ctx.db
        .select({
          id: workflowNodes.id,
          issueId: workflowNodes.issueId,
          members: workflowNodes.memberIssueIds,
        })
        .from(workflowNodes)
        .where(eq(workflowNodes.workflowId, input.workflowId))
      const node = nodes.find(
        (row) => row.issueId === input.issueId || row.members.includes(input.issueId)
      )
      if (!node) throw bad(`That issue is not part of the workflow`)
      // Kind and touches shape the PLAN; risk stays adjustable.
      if (input.kind !== undefined || input.touches !== undefined) {
        assertDraft(existing.status, `Re-planning a node`)
      }
      const patch = {
        ...(input.kind !== undefined && { kind: input.kind }),
        ...(input.risk !== undefined && { risk: input.risk }),
        ...(input.touches !== undefined && { touches: input.touches }),
      }
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        // compat: nothing left to write (the budget-only patch above) — an
        // empty `.set({})` throws drizzle's "No values to set". Same return.
        if (Object.keys(patch).length > 0) {
          await tx.update(workflowNodes).set(patch).where(eq(workflowNodes.id, node.id))
        }
        return { txId, nodeId: node.id }
      })
    }),

  /** A person unsticks a node: `retry` gives a failed (or stuck) node a fresh
   *  attempt, `skip` takes it out so its dependents can go on without it. */
  resolveNode: authedProcedure
    .input(z.object({ nodeId: z.string().uuid(), action: z.enum([`retry`, `skip`]) }))
    .mutation(async ({ ctx, input }) => {
      const node = await loadNode(input.nodeId)
      const workflow = await loadWorkflow(node.workflowId)
      await assertTeamMember(ctx.session.user.id, workflow.teamId)
      if (node.state === `landed`) throw bad(`That node already landed`)
      if (workflow.status !== `running` && workflow.status !== `paused`) {
        throw bad(`The workflow is not running`)
      }
      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(workflowNodes)
          .set(
            input.action === `skip`
              ? { state: `skipped`, note: null }
              : // A fresh attempt opens a fresh PR: the old approval and the
                // old review were about the old one.
                {
                  state: `blocked`,
                  attempt: 0,
                  sessionId: null,
                  note: null,
                  approvedAt: null,
                  review: null,
                  reviewRound: 0,
                  // EXP-1010: a PR that merged before now is the old
                  // attempt's; it lands nothing.
                  mergedInto: null,
                  // EXP-1066: the old attempt's announcement must not
                  // release dependents before the new attempt announces
                  // its own (the branch is reused, its head is not final).
                  checkpointAt: null,
                  retriedAt: new Date(),
                }
          )
          .where(eq(workflowNodes.id, input.nodeId))
        return { txId }
      })
      // A skip releases its dependents exactly like a landing does: the ones
      // with no unlanded blocker left move onto the integration branch.
      if (input.action === `skip`) {
        // EXP-1096: who took the node out, on the workflow's event log
        // (once: a repeated skip is no news).
        const [issue] = await ctx.db
          .select({ identifier: issues.identifier })
          .from(issues)
          .where(eq(issues.id, node.issueId))
          .limit(1)
        const who = ctx.session.user.name?.trim() || ctx.session.user.email || `a member`
        if (node.state !== `skipped`) await recordWorkflowEvent(ctx.db, {
          workflowId: workflow.id,
          teamId: workflow.teamId,
          nodeId: node.id,
          sessionId: node.sessionId,
          kind: `skipped`,
          message: `${issue?.identifier ?? `A node`} skipped by ${who}`,
        })
        const { retargetReleasedDependents } = await import(`@/lib/workflow-final-pr`)
        await retargetReleasedDependents(ctx.db, workflow.id, node.id, ctx.session.user.id)
      }
      return result
    }),

  /** A `proposed` node (a follow-up filed mid-run that was not plainly
   *  additive): a member admits it into the graph or dismisses it. */
  admitNode: authedProcedure
    .input(z.object({ nodeId: z.string().uuid(), admit: z.boolean() }))
    .mutation(async ({ ctx, input }) => {
      const node = await loadNode(input.nodeId)
      const workflow = await loadWorkflow(node.workflowId)
      await assertTeamMember(ctx.session.user.id, workflow.teamId)
      if (node.state !== `proposed`) throw bad(`That node is not a proposal`)
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        if (input.admit) {
          await tx
            .update(workflowNodes)
            .set({ state: `blocked`, note: null })
            .where(eq(workflowNodes.id, input.nodeId))
        } else {
          await tx.delete(workflowNodes).where(eq(workflowNodes.id, input.nodeId))
        }
        await replanWorkflow(tx, workflow.id)
        return { txId }
      })
    }),

  /** ENGINE: a node's state moved. FEED-56: with no `state` it is a PING,
   *  the pre-check before a Fresh wake: only the landed/skipped guard runs
   *  and nothing is written (a write off the device's one-beat-stale
   *  snapshot took a newer state back on every wake). */
  reportNode: authedProcedure
    .input(
      z.object({
        nodeId: z.string().uuid(),
        state: wfNodeStateSchema.optional(),
        sessionId: z.string().uuid().nullable().optional(),
        baseBranch: z.string().max(255).nullable().optional(),
        attempt: z.number().int().min(0).max(99).optional(),
        note: z.string().max(500).nullable().optional(),
        // EXP-983: the serialization edges a sibling conflict produced.
        afterNodeIds: z.array(z.string().uuid()).max(WORKFLOW_MAX_ISSUES).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const node = await loadNode(input.nodeId)
      const workflow = await loadWorkflow(node.workflowId)
      await assertEngine(workflow, ctx.session.user.id)
      // A landed node is final and only `landNode` makes one; a skipped one
      // is a person's call the engine never takes back.
      if (
        node.state === `landed` ||
        node.state === `skipped` ||
        input.state === `landed` ||
        input.state === `skipped`
      ) {
        return { updated: false }
      }
      if (input.state === undefined) {
        return { updated: true }
      }
      // EXP-1007: the read above and this write are not one transaction —
      // `landNode` can land the node in between. The WHERE repeats the
      // guard so a late report never takes a landing back.
      const rows = await ctx.db
        .update(workflowNodes)
        .set({
          state: input.state,
          ...(input.sessionId !== undefined && { sessionId: input.sessionId }),
          ...(input.baseBranch !== undefined && { baseBranch: input.baseBranch }),
          ...(input.attempt !== undefined && { attempt: input.attempt }),
          // EXP-1066: a fresh attempt (the engine's own retry) announces
          // its own contract; the old announcement releases nobody.
          ...(input.attempt !== undefined &&
            input.attempt !== node.attempt && { checkpointAt: null }),
          ...(input.note !== undefined && { note: input.note }),
          ...(input.afterNodeIds !== undefined && { afterNodeIds: input.afterNodeIds }),
        })
        .where(
          and(
            eq(workflowNodes.id, input.nodeId),
            notInArray(workflowNodes.state, [`landed`, `skipped`])
          )
        )
        .returning({ id: workflowNodes.id })
      // Whether the guard let the write through, not whether it was tried.
      return { updated: rows.length > 0 }
    }),

  /** ENGINE: the merge train's one step — squash-merge this node's PR into
   *  the integration branch. The approval is enforced HERE, not trusted from the
   *  device: the agent review's approval, or (EXP-1065) the review cap — the
   *  cap's worth of verdicts still asking for changes lands the node and
   *  CARRIES the findings into the decisions log and the final pull request,
   *  never to a person. A refusal by GitHub (a conflict with what landed
   *  before it) is an answer, not an error: the engine has the node merge the
   *  trunk in. */
  landNode: authedProcedure
    .input(z.object({ nodeId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const node = await loadNode(input.nodeId)
      const workflow = await loadWorkflow(node.workflowId)
      await assertEngine(workflow, ctx.session.user.id)
      if (node.state === `landed`) return { merged: true, reason: null, retargeted: [] as string[] }
      if (workflow.status !== `running`) {
        return { merged: false, reason: `The workflow is not running`, retargeted: [] as string[] }
      }
      // Merged already (a person pressed Merge on GitHub or in Reviews, the
      // node's own run did, or the webhook beat this call): the train's step
      // is done. EXP-1007: read BEFORE the approval and the landing order —
      // both guard a merge that has not happened yet. EXP-1010: nobody is
      // refused that merge (a workflow getting done beats who reviewed it);
      // it only has to be THIS attempt's, and to have gone somewhere the
      // final PR will carry.
      const [issue] = await ctx.db
        .select({ prState: issues.prState, prMergedAt: issues.prMergedAt })
        .from(issues)
        .where(eq(issues.id, node.issueId))
        .limit(1)
      const merged =
        issue?.prState === `merged` &&
        mergeBelongsToAttempt({
          mergedAt: issue.prMergedAt,
          startedAt: workflow.startedAt,
          retriedAt: node.retriedAt,
        })
      const waiting = (reason: string) => ({ merged: false, reason, retargeted: [] as string[] })
      // compat: an engine that predates the wave answers gets the one wait it
      // knows (`runnerPredatesWaves`).
      const waveWaiting = async (reason: string) =>
        waiting(
          (await runnerPredatesWaves(ctx.db, workflow, ctx.session.user.id))
            ? LEGACY_WAIT
            : reason
        )
      const graph = await loadWorkflowEdges(ctx.db, workflow.id)
      if (!merged) {
        // EXP-1103: no review gates a node any more — the waves review the
        // LANDED result. What gates a node is its RUN (it lands once the run
        // ended with its pull request up) and the review wave before its
        // layer, in a deep graph.
        if (!(await runEnded(ctx.db, node.sessionId))) {
          return waveWaiting(WAITING_FOR_RUN_TO_END)
        }
        if (reviewWaveGate(graph.nodes, node.wave) !== null) {
          // The literal is matched by shipped engines (`LandOutcome::is_waiting`).
          return waveWaiting(WAITING_FOR_REVIEW_WAVE)
        }
      }
      if (merged) {
        const others = graph.nodes.filter((row) => row.id !== node.id)
        const branches = others.length
          ? await ctx.db
              .select({ id: issues.id, branch: issues.branch })
              .from(issues)
              .where(inArray(issues.id, others.map((row) => row.issueId)))
          : []
        const branchOf = new Map(branches.map((row) => [row.id, row.branch]))
        const carriers = new Map<string, string>()
        for (const row of others) {
          const branch = branchOf.get(row.issueId)
          if (branch) carriers.set(branch, row.state)
        }
        const outcome = mergedNodeOutcome({
          mergedInto: node.mergedInto,
          integrationBranch: workflow.integrationBranch,
          carriers,
        })
        if (outcome.step === `fail`) {
          // Guarded: the engine asks again every beat while the PR reads
          // merged, and a person's skip must not be overwritten.
          await ctx.db
            .update(workflowNodes)
            .set({ state: `failed`, note: outcome.note.slice(0, 500), attempt: sql`GREATEST(${workflowNodes.attempt}, 2)` })
            .where(
              and(
                eq(workflowNodes.id, input.nodeId),
                notInArray(workflowNodes.state, [`landed`, `skipped`, `failed`])
              )
            )
          return waiting(`Waiting for its blockers to land`)
        }
        if (outcome.step === `wait`) return waiting(`Waiting for its blockers to land`)
      } else {
        // EXP-983: with speculative starts a dependent's PR can be up before
        // its blocker landed. The train lands in TOPOLOGICAL order, always.
        const stateOf = new Map(graph.nodes.map((row) => [row.id, row.state]))
        const waitsOn = graph.edges
          .filter(([, to]) => to === node.id)
          .some(([from]) => {
            const state = stateOf.get(from)
            return state !== `landed` && state !== `skipped`
          })
        if (waitsOn) return waiting(`Waiting for its blockers to land`)
      }
      const { issuesRouter, WORKFLOW_LANDING } = await import(`@/lib/trpc/issues`)
      const { ensureNodePrOnIntegrationBranch } = await import(`@/lib/workflow-final-pr`)
      try {
        if (merged) {
          // fall through to the landed write below
        } else {
          // The PR must be based on the integration branch, or the squash
          // lands wherever it points (the default branch, after the stack
          // heal or a person moved it) and bypasses the workflow's final PR.
          const base = await ensureNodePrOnIntegrationBranch(ctx.db, {
            issueId: node.issueId,
            repositoryId: workflow.repositoryId,
            teamId: workflow.teamId,
            integrationBranch: workflow.integrationBranch,
            actorUserId: ctx.session.user.id,
          })
          if (!base.ok) {
            return { merged: false, reason: base.reason, retargeted: [] as string[] }
          }
          // EXP-1094: the train is the one caller a live node's PR merges
          // through; the marker lets it past the hand-merge refusal.
          await issuesRouter
            .createCaller({ ...ctx, [WORKFLOW_LANDING]: true } as typeof ctx)
            .mergePr({ issueId: node.issueId, endSessions: true })
        }
      } catch (err) {
        const reason = err instanceof Error ? err.message : `GitHub refused the merge`
        return { merged: false, reason: reason.slice(0, 500), retargeted: [] as string[] }
      }
      // EXP-1010: the state guard IS the claim. A concurrent `skip` stays a
      // skip, and two landings of one node count once.
      // EXP-1103: a merge outside the train is only worth a decisions line
      // when it skipped the RUN gate (the run was still up); a review no
      // longer gates anything before the landing.
      const outside = merged && !node.approvedAt && !(await runEnded(ctx.db, node.sessionId))
      // EXP-1065 compat: a node an OLD engine bounced to the review cap
      // carries its last findings into the log as before.
      const carried = carriedReviewAtCap(node)
      let identifier: string | null = null
      const landed = await ctx.db.transaction(async (tx) => {
        const rows = await tx
          .update(workflowNodes)
          .set({ state: `landed`, note: null })
          .where(
            and(
              eq(workflowNodes.id, input.nodeId),
              notInArray(workflowNodes.state, [`landed`, `skipped`])
            )
          )
          .returning({ id: workflowNodes.id })
        if (rows.length === 0) return false
        let decisions: string | undefined
        if (outside || carried) {
          const [ident] = await tx
            .select({ identifier: issues.identifier })
            .from(issues)
            .where(eq(issues.id, node.issueId))
            .limit(1)
          identifier = ident?.identifier ?? null
          const [log] = await tx
            .select({ decisions: workflows.decisions })
            .from(workflows)
            .where(eq(workflows.id, workflow.id))
            .limit(1)
          decisions = log?.decisions ?? ``
          if (outside) {
            const where =
              node.mergedInto && node.mergedInto !== workflow.integrationBranch
                ? ` into ${node.mergedInto}`
                : ``
            decisions = appendDecisionLine(
              decisions,
              `${identifier ?? `A node`} was merged outside the train${where}, while its run was still up.`,
              new Date()
            )
          }
          if (carried) {
            // EXP-1065: what the last review still asked for, kept where
            // every node prompt and the final pull request read it.
            decisions = appendDecisionLine(
              decisions,
              carriedFindingsLine(identifier ?? `A node`, carried),
              new Date()
            )
          }
        }
        if (decisions !== undefined) {
          await tx
            .update(workflows)
            .set({ decisions })
            .where(eq(workflows.id, workflow.id))
        }
        return true
      })
      if (!landed) return { merged: true, reason: null, retargeted: [] as string[] }
      if (carried) {
        await recordWorkflowEvent(ctx.db, {
          workflowId: workflow.id,
          teamId: workflow.teamId,
          nodeId: node.id,
          sessionId: node.sessionId,
          kind: `cleared_at_cap`,
          message: `${identifier ?? `A node`} landed at the review cap with unresolved findings`,
        })
      }
      // EXP-983: dependents whose last unlanded blocker this was move their
      // PR onto the integration branch; the engine has them merge it in.
      const { retargetReleasedDependents } = await import(`@/lib/workflow-final-pr`)
      const retargeted = await retargetReleasedDependents(
        ctx.db,
        workflow.id,
        node.id,
        ctx.session.user.id
      )
      return { merged: true, reason: null, retargeted }
    }),
}
