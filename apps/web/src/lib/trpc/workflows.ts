import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, inArray, sql } from "drizzle-orm"
import {
  WORKFLOW_DECISIONS_MAX,
  WORKFLOW_MAX_REVIEW_ROUNDS,
  wfReviewVerdictSchema,
  workflowReviewHeadSchema,
  workflowReviewOracleSchema,
  type WorkflowNodeReview,
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
import { BUILTIN_REVIEW_NODE_NAME } from "@/lib/builtin-actions"
import { assertDeviceUsable } from "@/lib/trpc/automations"
import {
  loadWorkflowEdges,
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
 * unique per user, so "the caller is the engine" = a MEMBER of the
 * workflow's team who owns a device row with that id. Device ids are client
 * strings, so ownership alone would let a stranger's device of the same id
 * report for the workflow.
 */
async function assertEngine(
  workflow: { deviceId: string | null; status: string; teamId: string },
  userId: string
): Promise<void> {
  const { db } = await import(`@/db/connection`)
  await assertTeamMember(userId, workflow.teamId)
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
      sessionId: workflowNodes.sessionId,
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

/** A budget pause reaches the person who started the workflow (inbox row +
 *  push). Best-effort. */
async function notifyNodePaused(
  workflow: { id: string; teamId: string; name: string },
  issueId: string,
  note: string | null
): Promise<void> {
  try {
    const { db } = await import(`@/db/connection`)
    const [row] = await db
      .select({ creatorId: workflows.creatorId, identifier: issues.identifier })
      .from(workflows)
      .innerJoin(issues, eq(issues.id, issueId))
      .where(eq(workflows.id, workflow.id))
      .limit(1)
    if (!row?.creatorId) return
    const { sendAgentMessage } = await import(`@/lib/integrations/notifications`)
    await sendAgentMessage({
      teamId: workflow.teamId,
      senderUserId: row.creatorId,
      recipientIds: [row.creatorId],
      title: `${row.identifier} paused in ${workflow.name}`,
      body: note ?? `The node went over its budget.`,
    })
  } catch (err) {
    console.error(`[workflows] pause notice failed:`, err)
  }
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


/** EXP-984: bump counters inside `workflows.metrics` (the shape keys the
 *  layout owns are never touched). */
export function bumpMetrics(deltas: Record<string, number>) {
  let expr = sql`${workflows.metrics}`
  for (const [key, delta] of Object.entries(deltas)) {
    if (!/^[a-zA-Z]+$/.test(key) || !Number.isFinite(delta)) continue
    expr = sql`jsonb_set(${expr}, ${`{${key}}`}::text[], (coalesce((${workflows.metrics}->>${key})::numeric, 0) + ${delta})::text::jsonb)`
  }
  return expr
}

/** The counters the engine (or the server) may bump. The layout's shape keys
 *  (`nodes`, `edges`, `depth`, `width`, `cycles`, `cycleEdges`) are not among
 *  them. */
export const WORKFLOW_COUNTERS = [
  `landed`,
  `mergeIns`,
  `contractChanges`,
  `escalations`,
  `duplicateEscalations`,
  `operatorMinutes`,
  `reviewRounds`,
  `defectsByOracle`,
  `defectsByAgentReview`,
  `admitted`,
  `budgetPauses`,
] as const

/**
 * What one submitted review does to its node. Pure.
 * - approve + an oracle that PASSED, on a non-contract node → the approval
 *   stands in for the person (evidence, not opinion).
 * - approve without a passing oracle → advisory: the node still waits for a
 *   person, with the reviewer's word on it.
 * - request_changes → back to the author, up to the round cap; after that the
 *   node stops bouncing and waits for a person.
 */
export function reviewOutcome(args: {
  verdict: `approve` | `request_changes`
  oraclePassed: boolean | null
  kind: string
  round: number
}): { approve: boolean; state: `in_review` | `updating` | `waiting`; note: string } {
  if (args.verdict === `approve`) {
    const backed = args.oraclePassed === true && args.kind !== `contract`
    return {
      approve: backed,
      state: `in_review`,
      note: backed
        ? `Agent review passed, backed by its checks`
        : `Agent review passed (advisory): needs a person`,
    }
  }
  if (args.round >= WORKFLOW_MAX_REVIEW_ROUNDS) {
    return {
      approve: false,
      state: `waiting`,
      note: `Review did not converge after ${WORKFLOW_MAX_REVIEW_ROUNDS} rounds`,
    }
  }
  return { approve: false, state: `updating`, note: `Changes requested by the agent review` }
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
                }
          )
          .where(eq(workflowNodes.id, input.nodeId))
        return { txId }
      })
      // A skip releases its dependents exactly like a landing does: the ones
      // with no unlanded blocker left move onto the integration branch.
      if (input.action === `skip`) {
        const { retargetReleasedDependents } = await import(`@/lib/workflow-final-pr`)
        await retargetReleasedDependents(ctx.db, workflow.id, node.id, ctx.session.user.id)
      }
      return result
    }),


  /** A reviewer RUN's verdict on one node (EXP-984, MCP
   *  `exponential_workflows_review_submit`). Only the runner's owner may
   *  submit, and only FROM the reviewer run the engine started for the node
   *  (`sessionId` = the calling run): the author's own run, or any other,
   *  cannot approve the node. */
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
      const [reviewer] = await ctx.db
        .select({
          id: codingSessions.id,
          actionName: codingSessions.actionName,
          startedReason: codingSessions.startedReason,
        })
        .from(codingSessions)
        .where(eq(codingSessions.id, input.sessionId))
        .limit(1)
      if (
        !reviewer ||
        reviewer.actionName !== BUILTIN_REVIEW_NODE_NAME ||
        reviewer.startedReason !== `workflow` ||
        reviewer.id === node.sessionId
      ) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only the review run the workflow started for this node may submit its verdict; the node's own run cannot review itself`,
        })
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
          kind: node.kind,
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
          await tx
            .update(workflows)
            .set({ metrics: bumpMetrics({ admitted: 1 }) })
            .where(eq(workflows.id, workflow.id))
        } else {
          await tx.delete(workflowNodes).where(eq(workflowNodes.id, input.nodeId))
        }
        await replanWorkflow(tx, workflow.id)
        return { txId }
      })
    }),

  /** ENGINE: counters only the device can see (merge-ins, contract changes). */
  reportMetrics: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        // `partialRecord`: zod 4's `record` over an enum key is EXHAUSTIVE,
        // and the engine sends one or two counters at a time.
        deltas: z.partialRecord(
          z.enum(WORKFLOW_COUNTERS),
          z.number().int().min(0).max(10_000)
        ),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const workflow = await loadWorkflow(input.id)
      await assertEngine(workflow, ctx.session.user.id)
      await ctx.db
        .update(workflows)
        .set({ metrics: bumpMetrics(input.deltas as Record<string, number>) })
        .where(eq(workflows.id, input.id))
      return { ok: true }
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
      await ctx.db
        .update(workflowNodes)
        .set({
          state: input.state,
          ...(input.sessionId !== undefined && { sessionId: input.sessionId }),
          ...(input.baseBranch !== undefined && { baseBranch: input.baseBranch }),
          ...(input.attempt !== undefined && { attempt: input.attempt }),
          ...(input.note !== undefined && { note: input.note }),
          ...(input.afterNodeIds !== undefined && { afterNodeIds: input.afterNodeIds }),
        })
        .where(eq(workflowNodes.id, input.nodeId))
      // EXP-984: the engine paused the node over its budget. A paused node
      // does nothing until a person looks, so it says so ONCE, on the edge.
      if (input.state === `paused` && node.state !== `paused`) {
        await ctx.db
          .update(workflows)
          .set({ metrics: bumpMetrics({ budgetPauses: 1 }) })
          .where(eq(workflows.id, workflow.id))
        await notifyNodePaused(workflow, node.issueId, input.note ?? null)
      }
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
      if (node.state === `landed`) return { merged: true, reason: null, retargeted: [] as string[] }
      if (workflow.status !== `running`) {
        return { merged: false, reason: `The workflow is not running`, retargeted: [] as string[] }
      }
      if (nodeNeedsApproval(workflow.gate, node.kind) && !node.approvedAt) {
        return { merged: false, reason: `Waiting for a person to approve`, retargeted: [] as string[] }
      }
      // EXP-983: with speculative starts a dependent's PR can be up before
      // its blocker landed. The train lands in TOPOLOGICAL order, always.
      const graph = await loadWorkflowEdges(ctx.db, workflow.id)
      const stateOf = new Map(graph.nodes.map((row) => [row.id, row.state]))
      const waitsOn = graph.edges
        .filter(([, to]) => to === node.id)
        .some(([from]) => {
          const state = stateOf.get(from)
          return state !== `landed` && state !== `skipped`
        })
      if (waitsOn) {
        return {
          merged: false,
          reason: `Waiting for its blockers to land`,
          retargeted: [] as string[],
        }
      }
      // Merged already (a person pressed Merge on GitHub, or the webhook
      // beat this call): the train's step is done.
      const [issue] = await ctx.db
        .select({ prState: issues.prState })
        .from(issues)
        .where(eq(issues.id, node.issueId))
        .limit(1)
      const { issuesRouter } = await import(`@/lib/trpc/issues`)
      const { ensureNodePrOnIntegrationBranch } = await import(`@/lib/workflow-final-pr`)
      try {
        if (issue?.prState === `merged`) {
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
          await issuesRouter
            .createCaller(ctx)
            .mergePr({ issueId: node.issueId, endSessions: true })
        }
      } catch (err) {
        const reason = err instanceof Error ? err.message : `GitHub refused the merge`
        return { merged: false, reason: reason.slice(0, 500), retargeted: [] as string[] }
      }
      await ctx.db
        .update(workflowNodes)
        .set({ state: `landed`, note: null })
        .where(eq(workflowNodes.id, input.nodeId))
      await ctx.db
        .update(workflows)
        .set({ metrics: bumpMetrics({ landed: 1 }) })
        .where(eq(workflows.id, workflow.id))
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
