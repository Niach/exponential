import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, inArray, sql } from "drizzle-orm"
import {
  WORKFLOW_DECISIONS_MAX,
  WORKFLOW_MAX_ISSUES,
  wfNodeStateSchema,
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
  codingSessions,
  devices,
  issueRelations,
  issues,
  workflowNodes,
  workflows,
} from "@/db/schema"
import { getSteerRelayConfig, relayPostInput } from "@/lib/steer"
import { oneLine } from "@/lib/steer-child-messages"
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


/**
 * EXP-982: the ENGINE's write path. The engine runs on ONE device
 * (`workflows.device_id`, the single writer); a device row's id is only
 * unique per user, so "the caller is the engine" = the caller owns a device
 * row with that id.
 */
async function assertEngine(
  workflow: { deviceId: string | null; status: string },
  userId: string
): Promise<void> {
  const { db } = await import(`@/db/connection`)
  if (!workflow.deviceId) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `This workflow has no runner` })
  }
  const [device] = await db
    .select({ id: devices.id })
    .from(devices)
    .where(and(eq(devices.deviceId, workflow.deviceId), eq(devices.userId, userId)))
    .limit(1)
  if (!device) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Only the workflow's runner device may report for it`,
    })
  }
}

async function loadNode(nodeId: string) {
  const { db } = await import(`@/db/connection`)
  const [node] = await db
    .select({
      id: workflowNodes.id,
      workflowId: workflowNodes.workflowId,
      issueId: workflowNodes.issueId,
      kind: workflowNodes.kind,
      state: workflowNodes.state,
      approvedAt: workflowNodes.approvedAt,
    })
    .from(workflowNodes)
    .where(eq(workflowNodes.id, nodeId))
    .limit(1)
  if (!node) throw new TRPCError({ code: `NOT_FOUND`, message: `Node not found` })
  return node
}

/** A node lands without a person only when the workflow has no gate AND it
 *  is not the contract: the contract carries the most risk, so it is ALWAYS
 *  human-gated. `agent` review is advisory until EXP-984 and gates like
 *  `human` until then. */
export function nodeNeedsApproval(gate: string, kind: string): boolean {
  return kind === `contract` || gate !== `none`
}

/** `2026-09-19: <text>` appended to the log every node prompt carries. */
export function appendDecisionLine(log: string, text: string, now: Date): string {
  const line = `${now.toISOString().slice(0, 10)}: ${text.replace(/\s+/g, ` `).trim()}`
  const next = log.trim() ? `${log.trimEnd()}\n${line}` : line
  // Newest decisions win the budget: drop whole lines from the top.
  let out = next
  while (out.length > WORKFLOW_DECISIONS_MAX && out.includes(`\n`)) {
    out = out.slice(out.indexOf(`\n`) + 1)
  }
  return out.slice(-WORKFLOW_DECISIONS_MAX)
}

/** A recorded decision reaches every run of the workflow that is parked on a
 *  question (the sibling that asked the same thing included). Best-effort. */
async function relayDecision(workflowId: string, text: string): Promise<void> {
  try {
    const config = getSteerRelayConfig()
    if (!config) return
    const { db } = await import(`@/db/connection`)
    const waiting = await db
      .select({ id: codingSessions.id })
      .from(workflowNodes)
      .innerJoin(codingSessions, eq(codingSessions.id, workflowNodes.sessionId))
      .where(
        and(
          eq(workflowNodes.workflowId, workflowId),
          eq(codingSessions.needsInput, true),
          inArray(codingSessions.status, [`running`, `in_review`])
        )
      )
    for (const session of waiting) {
      await relayPostInput(
        config,
        session.id,
        oneLine(`[Exponential workflow decision] ${text}`)
      )
    }
  } catch (err) {
    console.error(`[workflows] decision relay failed:`, err)
  }
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
        // EXP-982: an answer worth keeping. Appended as a dated line to the
        // log every node prompt carries, at ANY status: a decision is not
        // configuration.
        decision: z.string().trim().min(1).max(2000).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const existing = await loadWorkflow(input.id)
      await assertTeamMember(ctx.session.user.id, existing.teamId)
      const { id, name, decision, ...config } = input
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
      if (existing.startOn !== `landed`) {
        // `contract` / `pr_open` starts arrive with EXP-983.
        throw bad(`Only "When landed" starts are available yet`)
      }
      await assertDeviceUsable(
        existing.deviceId,
        existing.teamId,
        ctx.session.user.id,
        existing.launch.agent,
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
          .set({ state: `blocked`, attempt: 0, sessionId: null, approvedAt: null, note: null })
          .where(eq(workflowNodes.workflowId, input.id))
        const [workflow] = await tx
          .update(workflows)
          .set({ status: `running`, startedAt: new Date(), endedAt: null })
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

  /** The human gate: a member approves a node's open PR for the merge train.
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
          .set({ approvedAt: input.approved ? new Date() : null })
          .where(eq(workflowNodes.id, input.nodeId))
        return { txId }
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
      return ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(workflowNodes)
          .set(
            input.action === `skip`
              ? { state: `skipped`, note: null }
              : { state: `blocked`, attempt: 0, sessionId: null, note: null }
          )
          .where(eq(workflowNodes.id, input.nodeId))
        return { txId }
      })
    }),

  /** ENGINE: a node's state moved. */
  reportNode: authedProcedure
    .input(
      z.object({
        nodeId: z.string().uuid(),
        state: wfNodeStateSchema,
        sessionId: z.string().uuid().nullable().optional(),
        baseBranch: z.string().max(255).nullable().optional(),
        attempt: z.number().int().min(0).max(99).optional(),
        note: z.string().max(500).nullable().optional(),
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
      await ctx.db
        .update(workflowNodes)
        .set({
          state: input.state,
          ...(input.sessionId !== undefined && { sessionId: input.sessionId }),
          ...(input.baseBranch !== undefined && { baseBranch: input.baseBranch }),
          ...(input.attempt !== undefined && { attempt: input.attempt }),
          ...(input.note !== undefined && { note: input.note }),
        })
        .where(eq(workflowNodes.id, input.nodeId))
      return { updated: true }
    }),

  /** ENGINE: the merge train's one step — squash-merge this node's PR into
   *  the integration branch. The gate is enforced HERE, not trusted from the
   *  device. A refusal by GitHub (a conflict with what landed before it) is
   *  an answer, not an error: the engine has the node merge the trunk in. */
  landNode: authedProcedure
    .input(z.object({ nodeId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const node = await loadNode(input.nodeId)
      const workflow = await loadWorkflow(node.workflowId)
      await assertEngine(workflow, ctx.session.user.id)
      if (node.state === `landed`) return { merged: true, reason: null }
      if (workflow.status !== `running`) {
        return { merged: false, reason: `The workflow is not running` }
      }
      if (nodeNeedsApproval(workflow.gate, node.kind) && !node.approvedAt) {
        return { merged: false, reason: `Waiting for a person to approve` }
      }
      // Merged already (a person pressed Merge on GitHub, or the webhook
      // beat this call): the train's step is done.
      const [issue] = await ctx.db
        .select({ prState: issues.prState })
        .from(issues)
        .where(eq(issues.id, node.issueId))
        .limit(1)
      const { issuesRouter } = await import(`@/lib/trpc/issues`)
      try {
        if (issue?.prState === `merged`) {
          // fall through to the landed write below
        } else
          await issuesRouter
            .createCaller(ctx)
            .mergePr({ issueId: node.issueId, endSessions: true })
      } catch (err) {
        const reason = err instanceof Error ? err.message : `GitHub refused the merge`
        return { merged: false, reason: reason.slice(0, 500) }
      }
      await ctx.db
        .update(workflowNodes)
        .set({ state: `landed`, note: null })
        .where(eq(workflowNodes.id, input.nodeId))
      await ctx.db
        .update(workflows)
        .set({
          metrics: sql`jsonb_set(${workflows.metrics}, '{landed}', (coalesce((${workflows.metrics}->>'landed')::int, 0) + 1)::text::jsonb)`,
        })
        .where(eq(workflows.id, workflow.id))
      return { merged: true, reason: null }
    }),

  /** ENGINE: every node landed — open the ONE final PR, integration branch →
   *  the repository's default branch. Idempotent. */
  openFinalPr: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const workflow = await loadWorkflow(input.id)
      await assertEngine(workflow, ctx.session.user.id)
      if (workflow.finalPrUrl) return { url: workflow.finalPrUrl }
      const { openWorkflowFinalPr } = await import(`@/lib/workflow-final-pr`)
      return openWorkflowFinalPr(ctx.db, input.id, ctx.session.user.id)
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
