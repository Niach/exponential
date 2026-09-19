import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, inArray } from "drizzle-orm"
import {
  WORKFLOW_MAX_ISSUES,
  workflowLaunchSchema,
  workflowNodeBudgetSchema,
  workflowTouchesSchema,
  wfGateSchema,
  wfNodeKindSchema,
  wfRiskSchema,
  wfStartOnSchema,
  type WorkflowLaunch,
} from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import {
  boards,
  issueRelations,
  issues,
  workflowNodes,
  workflows,
} from "@/db/schema"
import { assertTeamMember } from "@/lib/team-membership"
import { boardVisible } from "@/lib/board-visibility"
import { assertDeviceUsable } from "@/lib/trpc/automations"
import {
  nodeEdges,
  replanWorkflow,
  workflowIntegrationBranch,
} from "@/lib/workflows"

// EXP-981: workflows — a picked set of issues of ONE repository, planned as a
// DAG (`blocks` = edges, a parent with sub-issues = one compound node) and,
// from EXP-982 on, run by the deterministic engine on the runner device.
// Rows sync via the `workflows` + `workflow_nodes` shapes; this router is the
// write path (any member: a workflow is work, not a team setting) plus the
// `get`/`list` reads MCP answers from. P2 = DRAFT workflows only: start,
// pause and cancel land with the engine.

const codingAgentValues = contract.codingAgent.values as readonly string[]
const agentModelValues: Record<string, readonly string[]> = {
  claude: contract.codingModel.values,
  codex: contract.codexModel.values,
}
const agentEffortValues: Record<string, readonly string[]> = {
  claude: contract.codingEffort.values,
  codex: contract.codexEffort.values,
}

const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

/** The device's build cannot run workflows (it predates the engine). */
export const WORKFLOW_DEVICE_CAP = `workflows`
const WORKFLOW_DEVICE = {
  noun: `Workflow`,
  cap: WORKFLOW_DEVICE_CAP,
  capMessage: `Update Exponential on that machine to run workflows`,
}

function assertLaunch(launch: WorkflowLaunch): void {
  if (launch.agent && !codingAgentValues.includes(launch.agent)) {
    throw bad(`Unknown agent`)
  }
  const agent = launch.agent ?? `claude`
  if (launch.model && !agentModelValues[agent]!.includes(launch.model)) {
    throw bad(`Unknown ${agent} model`)
  }
  if (launch.effort && !agentEffortValues[agent]!.includes(launch.effort)) {
    throw bad(`Unknown ${agent} effort`)
  }
  if (launch.subagentModel) {
    if (agent !== `claude`) throw bad(`Only claude takes a subagent model`)
    if (!contract.codingModel.values.includes(launch.subagentModel)) {
      throw bad(`Unknown subagent model`)
    }
  }
}

const wireColumns = {
  id: workflows.id,
  teamId: workflows.teamId,
  repositoryId: workflows.repositoryId,
  name: workflows.name,
  status: workflows.status,
  deviceId: workflows.deviceId,
  launch: workflows.launch,
  gate: workflows.gate,
  startOn: workflows.startOn,
  integrationBranch: workflows.integrationBranch,
  finalPrUrl: workflows.finalPrUrl,
  finalPrNumber: workflows.finalPrNumber,
  finalPrState: workflows.finalPrState,
  decisions: workflows.decisions,
  metrics: workflows.metrics,
  startedAt: workflows.startedAt,
  endedAt: workflows.endedAt,
  createdAt: workflows.createdAt,
  updatedAt: workflows.updatedAt,
}

async function loadWorkflow(id: string) {
  const { db } = await import(`@/db/connection`)
  const [row] = await db
    .select(wireColumns)
    .from(workflows)
    .where(eq(workflows.id, id))
    .limit(1)
  if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Workflow not found` })
  return row
}

function assertDraft(status: string, what: string): void {
  if (status !== `draft`) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `${what} is only possible while the workflow is a draft`,
    })
  }
}

/**
 * The picked issues, validated: this team, visible boards, ONE repository,
 * and not started yet (anchor `backlog`: backlog + unstarted rows) — work in
 * flight already has a branch the workflow's bases know nothing about.
 * `repositoryId` = the workflow's when it has one, else the first pick's.
 */
async function loadPickableIssues(
  issueIds: readonly string[],
  teamId: string,
  repositoryId: string | null
) {
  const { db } = await import(`@/db/connection`)
  const ids = [...new Set(issueIds)]
  const rows = await db
    .select({
      id: issues.id,
      identifier: issues.identifier,
      number: issues.number,
      status: issues.status,
      teamId: issues.teamId,
      repositoryId: boards.repositoryId,
    })
    .from(issues)
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .where(and(inArray(issues.id, ids), boardVisible()))
  if (rows.length !== ids.length) throw bad(`Issue not found`)
  let repo = repositoryId
  for (const row of rows) {
    if (row.teamId !== teamId) throw bad(`${row.identifier} is in another team`)
    if (!row.repositoryId) {
      throw bad(`${row.identifier} is on a board without a repository`)
    }
    repo ??= row.repositoryId
    if (row.repositoryId !== repo) {
      throw bad(`A workflow covers ONE repository; ${row.identifier} is in another`)
    }
    if (row.status !== `backlog`) {
      throw bad(`${row.identifier} is already started; pick backlog issues`)
    }
  }
  return { rows, repositoryId: repo }
}

export const workflowsRouter = router({
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
          budget: workflowNodes.budget,
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
        launch: workflowLaunchSchema.optional(),
        gate: wfGateSchema.optional(),
        startOn: wfStartOnSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const existing = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, existing.teamId)
      const { id, name, ...config } = input
      // The name is a label; everything else is the run's configuration.
      if (Object.values(config).some((value) => value !== undefined)) {
        assertDraft(existing.status, `Changing how a workflow runs`)
      }
      const launch = input.launch ?? existing.launch
      if (input.launch) assertLaunch(input.launch)
      const deviceId =
        input.deviceId === undefined ? existing.deviceId : input.deviceId
      if (deviceId && (input.deviceId !== undefined || input.launch)) {
        await assertDeviceUsable(
          deviceId,
          existing.teamId,
          ctx.session.user.id,
          launch.agent,
          WORKFLOW_DEVICE
        )
      }
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [workflow] = await tx
          .update(workflows)
          .set({
            ...(name !== undefined && { name }),
            ...(input.deviceId !== undefined && { deviceId: input.deviceId }),
            ...(input.launch !== undefined && { launch: input.launch }),
            ...(input.gate !== undefined && { gate: input.gate }),
            ...(input.startOn !== undefined && { startOn: input.startOn }),
          })
          .where(eq(workflows.id, id))
          .returning(wireColumns)
        return { txId, workflow: workflow! }
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
      const picked = adding.length
        ? await loadPickableIssues(adding, existing.teamId, existing.repositoryId)
        : null
      if (current.length - input.removeIssueIds.length + adding.length > WORKFLOW_MAX_ISSUES) {
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
          await tx.insert(workflowNodes).values(
            picked.rows.map((row) => ({
              workflowId: input.id,
              teamId: existing.teamId,
              issueId: row.id,
            }))
          )
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
        budget: workflowNodeBudgetSchema.nullable().optional(),
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
      // Kind and touches shape the PLAN; risk and budget stay adjustable.
      if (input.kind !== undefined || input.touches !== undefined) {
        assertDraft(existing.status, `Re-planning a node`)
      }
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(workflowNodes)
          .set({
            ...(input.kind !== undefined && { kind: input.kind }),
            ...(input.risk !== undefined && { risk: input.risk }),
            ...(input.touches !== undefined && { touches: input.touches }),
            ...(input.budget !== undefined && { budget: input.budget }),
          })
          .where(eq(workflowNodes.id, node.id))
        return { txId, nodeId: node.id }
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
})
