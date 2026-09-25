import { z } from "zod"
import { TRPCError } from "@trpc/server"
import {
  and,
  asc,
  desc,
  eq,
  inArray,
} from "drizzle-orm"
import { WORKFLOW_MAX_ISSUES, type WorkflowLaunch } from "@exp/db-schema/domain"
import { normalizeWorkflowLaunch } from "@/lib/workflow-launch"
import { authedProcedure, generateTxId } from "@/lib/trpc"
import {
  issueRelations,
  issues,
  workflowNodes,
  workflows,
} from "@/db/schema"
import { assertTeamMember } from "@/lib/team-membership"
import { assertDeviceUsable } from "@/lib/trpc/automations"
import { nodeEdges, replanWorkflow, workflowIntegrationBranch } from "@/lib/workflows"
import {
  bad,
  WORKFLOW_DEVICE,
  normalizeLaunchLenient,
  storedLaunchFor,
  launchFromDeviceDefaults,
  seedLaunchFromBoundDevice,
  wireColumns,
  loadWorkflow,
  assertDraft,
  loadPickableIssues,
  appendDecisionLine,
  relayDecision,
} from "./shared"

export const workflowCreateProcedures = {
  /** Member-gated read for MCP; clients read the synced shapes. */
  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)
      return ctx.db
        .select(wireColumns)
        .from(workflows)
        .where(eq(workflows.teamId, input.teamId))
        .orderBy(desc(workflows.createdAt))
    }),

  /** The whole graph: the workflow, its nodes (layout included) and the
   *  `blocks` edges between them. `metrics.cycles` non-empty = cannot start. */
  get: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const workflow = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, workflow.teamId)
      const nodes = await ctx.db
        .select({
          id: workflowNodes.id,
          issueId: workflowNodes.issueId,
          identifier: issues.identifier,
          title: issues.title,
          issueStatus: issues.status,
          memberIssueIds: workflowNodes.memberIssueIds,
          kind: workflowNodes.kind,
          state: workflowNodes.state,
          risk: workflowNodes.risk,
          wave: workflowNodes.wave,
          lane: workflowNodes.lane,
          onCycle: workflowNodes.onCycle,
          sessionId: workflowNodes.sessionId,
          attempt: workflowNodes.attempt,
          baseBranch: workflowNodes.baseBranch,
          touches: workflowNodes.touches,
        })
        .from(workflowNodes)
        .innerJoin(issues, eq(issues.id, workflowNodes.issueId))
        .where(eq(workflowNodes.workflowId, input.id))
        .orderBy(asc(workflowNodes.wave), asc(workflowNodes.lane))
      const covered = nodes.flatMap((node) => [node.issueId, ...node.memberIssueIds])
      const blocks = covered.length
        ? await ctx.db
            .select({
              issueId: issueRelations.issueId,
              relatedIssueId: issueRelations.relatedIssueId,
            })
            .from(issueRelations)
            .where(
              and(
                eq(issueRelations.type, `blocks`),
                inArray(issueRelations.issueId, covered),
                inArray(issueRelations.relatedIssueId, covered)
              )
            )
        : []
      const edges = nodeEdges(nodes, blocks).map(([from, to]) => ({ from, to }))
      return { workflow, nodes, edges }
    }),

  create: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        name: z.string().trim().min(1).max(255).optional(),
        issueIds: z.array(z.string().uuid()).min(1).max(WORKFLOW_MAX_ISSUES),
        // EXP-1032: the runner MACHINE, bound at creation (the IDE sends its
        // own): the launch is seeded from it exactly as `update({deviceId})`
        // would. Omitted = unbound, on the contract defaults.
        deviceId: z.string().min(1).max(128).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)
      const picked = await loadPickableIssues(input.issueIds, input.teamId, null)
      // By NUMBER: "APP-10" sorts before "APP-6" as text.
      const first = [...picked.rows].sort((a, b) => a.number - b.number)[0]!
      const name =
        input.name ??
        (picked.rows.length > 1
          ? `${first.identifier} +${picked.rows.length - 1}`
          : first.identifier)
      // EXP-1029: a workflow with no runner is BORN on the contract defaults
      // (the screen has no settings panel); binding one, here or later
      // (`update({deviceId})`), re-seeds from that machine.
      let launch = launchFromDeviceDefaults(null)
      if (input.deviceId) {
        launch = await seedLaunchFromBoundDevice(
          input.deviceId,
          input.teamId,
          ctx.session.user.id
        )
        await assertDeviceUsable(
          input.deviceId,
          input.teamId,
          ctx.session.user.id,
          launch.agent,
          WORKFLOW_DEVICE
        )
      }

      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const id = crypto.randomUUID()
        const [workflow] = await tx
          .insert(workflows)
          .values({
            id,
            teamId: input.teamId,
            repositoryId: picked.repositoryId,
            creatorId: ctx.session.user.id,
            name,
            ...(input.deviceId && { deviceId: input.deviceId }),
            launch: storedLaunchFor(launch),
            integrationBranch: workflowIntegrationBranch(id),
          })
          .returning(wireColumns)
        await tx.insert(workflowNodes).values(
          picked.rows.map((row) => ({
            workflowId: id,
            teamId: input.teamId,
            issueId: row.id,
          }))
        )
        const metrics = await replanWorkflow(tx, id)
        return {
          txId,
          workflow: { ...workflow!, metrics: metrics ?? workflow!.metrics },
        }
      })
    }),

  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        name: z.string().trim().min(1).max(255).optional(),
        deviceId: z.string().min(1).max(128).nullable().optional(),
        // EXP-1066: `launch`, `startOn` and `gate` are no inputs any more —
        // the launch is seeded from the runner device, dependents start on
        // the blockers' contract, every node gets the agent review. An old
        // client still sending them is stripped by zod, nothing written.
        // EXP-982: an answer worth keeping. Appended as a dated line to the
        // log every node prompt carries, at ANY status: a decision is not
        // configuration.
        decision: z.string().trim().min(1).max(2000).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const existing = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, existing.teamId)
      const { id, name, decision } = input
      // The name is a label and a decision is not configuration; the runner
      // binding is, and only a draft takes one.
      if (input.deviceId !== undefined) {
        assertDraft(existing.status, `Changing how a workflow runs`)
      }
      // EXP-1032: binding a runner to a DRAFT re-seeds agent, account and both
      // models from THAT machine's agent defaults.
      let nextLaunch: WorkflowLaunch | undefined
      if (input.deviceId) {
        nextLaunch = await seedLaunchFromBoundDevice(
          input.deviceId,
          existing.teamId,
          ctx.session.user.id
        )
        await assertDeviceUsable(
          input.deviceId,
          existing.teamId,
          ctx.session.user.id,
          nextLaunch.agent,
          WORKFLOW_DEVICE
        )
      }
      // A patch of nothing (an old client sending only removed fields) leaves
      // nothing to set, which drizzle refuses with a 500. Answer with the row
      // as it is.
      if (name === undefined && input.deviceId === undefined && decision === undefined) {
        return ctx.db.transaction(async (tx) => {
          const txId = await generateTxId(tx)
          const [workflow] = await tx
            .select(wireColumns)
            .from(workflows)
            .where(eq(workflows.id, id))
          return { txId, workflow: workflow! }
        })
      }
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [workflow] = await tx
          .update(workflows)
          .set({
            ...(name !== undefined && { name }),
            ...(input.deviceId !== undefined && { deviceId: input.deviceId }),
            ...(nextLaunch !== undefined && { launch: storedLaunchFor(nextLaunch) }),
            ...(decision !== undefined && {
              decisions: appendDecisionLine(existing.decisions, decision, new Date()),
            }),
          })
          .where(eq(workflows.id, id))
          .returning(wireColumns)
        return { txId, workflow: workflow! }
      }).then(async (result) => {
        if (decision !== undefined) await relayDecision(id, decision)
        return result
      })
    }),

  /** Add or drop issues of a DRAFT. The planner uses it for the contract and
   *  integration issues it files. */
  setIssues: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        addIssueIds: z.array(z.string().uuid()).max(WORKFLOW_MAX_ISSUES).default([]),
        removeIssueIds: z.array(z.string().uuid()).max(WORKFLOW_MAX_ISSUES).default([]),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const existing = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, existing.teamId)
      assertDraft(existing.status, `Changing a workflow's issues`)
      const current = await ctx.db
        .select({ issueId: workflowNodes.issueId, members: workflowNodes.memberIssueIds })
        .from(workflowNodes)
        .where(eq(workflowNodes.workflowId, input.id))
      const covered = new Set(current.flatMap((row) => [row.issueId, ...row.members]))
      const adding = input.addIssueIds.filter((issueId) => !covered.has(issueId))
      // A MEMBER (a sub-issue folded into its parent's node) has no node row
      // to delete, and the replan would re-adopt it from the `parent`
      // relation anyway: the link is what makes it part of the workflow.
      const nodeOf = new Map(current.map((row) => [row.issueId, row]))
      const member = input.removeIssueIds.find(
        (issueId) => !nodeOf.has(issueId) && covered.has(issueId)
      )
      if (member) {
        throw bad(
          `That issue is a sub-issue folded into its parent's node; remove the parent relation (or the parent) instead`
        )
      }
      const picked = adding.length
        ? await loadPickableIssues(adding, existing.teamId, existing.repositoryId)
        : null
      // The cap counts ISSUES, members included, not nodes.
      const removing = input.removeIssueIds.reduce(
        (sum, issueId) => sum + (nodeOf.has(issueId) ? 1 + nodeOf.get(issueId)!.members.length : 0),
        0
      )
      if (covered.size - removing + adding.length > WORKFLOW_MAX_ISSUES) {
        throw bad(`A workflow holds at most ${WORKFLOW_MAX_ISSUES} issues`)
      }
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        if (input.removeIssueIds.length > 0) {
          await tx
            .delete(workflowNodes)
            .where(
              and(
                eq(workflowNodes.workflowId, input.id),
                inArray(workflowNodes.issueId, input.removeIssueIds)
              )
            )
        }
        if (picked) {
          await tx
            .insert(workflowNodes)
            .values(
              picked.rows.map((row) => ({
                workflowId: input.id,
                teamId: existing.teamId,
                issueId: row.id,
              }))
            )
            .onConflictDoNothing()
          if (!existing.repositoryId && picked.repositoryId) {
            await tx
              .update(workflows)
              .set({ repositoryId: picked.repositoryId })
              .where(eq(workflows.id, input.id))
          }
        }
        const metrics = await replanWorkflow(tx, input.id)
        return { txId, metrics }
      })
    }),

  /** Re-derive the compound nodes, the layout and the metrics now. */
  replan: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, existing.teamId)
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const metrics = await replanWorkflow(tx, input.id)
        return { txId, metrics }
      })
    }),


  // ── Running a workflow (EXP-982) ─────────────────────────────────────────
  // The server only flips intent; the deterministic ENGINE on the runner
  // device does the work off the synced rows (level-triggered, like the
  // automations host). There is no server scheduler.

  start: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, existing.teamId)
      assertDraft(existing.status, `Starting`)
      if (!existing.repositoryId) throw bad(`The workflow's repository is gone`)
      if (!existing.deviceId) throw bad(`Pick the device that runs this workflow first`)
      // compat: a row an old client saved with a claude review pin on a codex
      // workflow folds to a `strongModel` the engine cannot start on; it is
      // healed to the agent's default here, and written back so the engine
      // reads the same (see `normalizeLaunchLenient` for the trigger).
      const launch = normalizeLaunchLenient(existing.launch)
      const healed =
        launch.strongModel !== normalizeWorkflowLaunch(existing.launch).strongModel
      await assertDeviceUsable(
        existing.deviceId,
        existing.teamId,
        ctx.session.user.id,
        launch.agent,
        WORKFLOW_DEVICE
      )
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const metrics = await replanWorkflow(tx, input.id)
        if (!metrics || metrics.nodes === 0) throw bad(`The workflow has no issues`)
        if (metrics.cycles.length > 0) {
          throw bad(
            `These issues block each other in a cycle: ${metrics.cycles
              .map((keys) => keys.join(`, `))
              .join(`; `)}. Remove one relation to start.`
          )
        }
        await tx
          .update(workflowNodes)
          .set({
            state: `blocked`,
            attempt: 0,
            sessionId: null,
            approvedAt: null,
            checkpointAt: null,
            afterNodeIds: [],
            note: null,
          })
          .where(eq(workflowNodes.workflowId, input.id))
        const [workflow] = await tx
          .update(workflows)
          .set({
            status: `running`,
            startedAt: new Date(),
            endedAt: null,
            ...(healed && { launch: storedLaunchFor(launch) }),
          })
          .where(eq(workflows.id, input.id))
          .returning(wireColumns)
        return { txId, workflow: workflow! }
      })
    }),

  /** `paused`: the engine starts and lands nothing new; live runs finish. */
  pause: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, existing.teamId)
      if (existing.status !== `running`) throw bad(`Only a running workflow can be paused`)
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.update(workflows).set({ status: `paused` }).where(eq(workflows.id, input.id))
        return { txId }
      })
    }),

  resume: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, existing.teamId)
      if (existing.status !== `paused`) throw bad(`Only a paused workflow can be resumed`)
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.update(workflows).set({ status: `running` }).where(eq(workflows.id, input.id))
        return { txId }
      })
    }),

  /** Abandoning a workflow = the engine ends its live runs and deletes ONE
   *  branch. Landed work stays on that branch until then; nothing reached the
   *  default branch. */
  cancel: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, existing.teamId)
      if (existing.status !== `running` && existing.status !== `paused`) {
        throw bad(`Only a started workflow can be cancelled`)
      }
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(workflows)
          .set({ status: `cancelled`, endedAt: new Date() })
          .where(eq(workflows.id, input.id))
        return { txId }
      })
    }),

  delete: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const existing = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, existing.teamId)
      if (existing.status === `running` || existing.status === `paused`) {
        throw new TRPCError({
          code: `PRECONDITION_FAILED`,
          message: `Cancel the workflow before deleting it`,
        })
      }
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.delete(workflows).where(eq(workflows.id, input.id))
        return { txId }
      })
    }),
}
