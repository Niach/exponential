import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, inArray, notInArray, sql } from "drizzle-orm"
import {
  WORKFLOW_DECISIONS_MAX,
  WORKFLOW_MAX_REVIEW_ROUNDS,
  wfReviewVerdictSchema,
  workflowReviewHeadSchema,
  workflowReviewOracleSchema,
  type WorkflowNodeReview,
  WORKFLOW_LAUNCH_DEFAULTS,
  WORKFLOW_MAX_ISSUES,
  wfNodeStateSchema,
  workflowLaunchAgentValues,
  workflowLaunchSchema,
  workflowTouchesSchema,
  wfNodeKindSchema,
  wfRiskSchema,
  wfStartOnSchema,
  type WorkflowLaunch,
  type WorkflowLaunchAgent,
} from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { normalizeWorkflowLaunch } from "@/lib/workflow-launch"
import { workflowDefaultsFor } from "@/lib/devices/workflow-defaults"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import {
  boards,
  codingSessions,
  devices,
  issueRelations,
  issues,
  workflowNodes,
  workflows,
  type DeviceLaunchDefaults,
} from "@/db/schema"
import { getSteerRelayConfig, relayPostInput } from "@/lib/steer"
import { MAX_SESSION_CHAIN_DEPTH, oneLine } from "@/lib/steer-child-messages"
import { assertTeamMember } from "@/lib/team-membership"
import { boardVisible } from "@/lib/board-visibility"
import { BUILTIN_REVIEW_NODE_NAME } from "@/lib/builtin-actions"
import { assertDeviceUsable } from "@/lib/trpc/automations"
import {
  isWorkflowReviewBranch,
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
const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

/** The device's build cannot run workflows (it predates the engine). */
export const WORKFLOW_DEVICE_CAP = `workflows`
const WORKFLOW_DEVICE = {
  noun: `Workflow`,
  cap: WORKFLOW_DEVICE_CAP,
  capMessage: `Update Exponential on that machine to run workflows`,
}

/**
 * EXP-1032: the NORMALIZED launch is what gets stored and validated — two
 * models out of the agent's own closed vocabulary, and nothing else. The
 * deprecated pins an old client still sends were folded into `strongModel`
 * by `normalizeWorkflowLaunch` long before this.
 */
function assertLaunch(launch: WorkflowLaunch): void {
  if (!codingAgentValues.includes(launch.agent)) throw bad(`Unknown agent`)
  const models = agentModelValues[launch.agent]!
  for (const model of [launch.model, launch.strongModel]) {
    if (!models.includes(model)) throw bad(`Unknown ${launch.agent} model`)
  }
}

/**
 * EXP-1032: a workflow's launch, seeded from the machine BOUND to run it
 * (`update({deviceId})` on a draft — a workflow is created on the contract
 * defaults, with no runner) — the device's default ACCOUNT names the agent it
 * runs on, its `launch_defaults.workflow` the two models. A device that
 * advertises none (or no device at all, `null`) falls back to contract
 * `workflowLaunch` defaults. A model outside that agent's vocabulary is a
 * stale advertisement: the fallback stands in rather than a launch
 * `assertLaunch` would refuse.
 */
export function launchFromDeviceDefaults(
  defaults: DeviceLaunchDefaults | null | undefined
): WorkflowLaunch {
  const agent: WorkflowLaunchAgent =
    workflowLaunchAgentValues.find((value) => value === defaults?.defaultAgent) ?? `claude`
  // EXP-1020's clamp: a stored name counts only for the agent it belongs to,
  // else that agent's contract pair stands in.
  const launch: WorkflowLaunch = {
    agent,
    ...workflowDefaultsFor(agent, defaults?.workflow),
  }
  // The default account is one of `defaultAgent`'s profile ids, so it only
  // ever rides beside a valid agent (`clampLaunchDefaults`).
  if (defaults?.defaultAccount) launch.account = defaults.defaultAccount
  return launch
}

/** The launch a workflow bound to `deviceId` is seeded with. A device row
 *  that is gone reads as one that advertises nothing. */
async function launchForDevice(deviceId: string): Promise<WorkflowLaunch> {
  const { db } = await import(`@/db/connection`)
  const [row] = await db
    .select({ launchDefaults: devices.launchDefaults })
    .from(devices)
    .where(eq(devices.deviceId, deviceId))
    .limit(1)
  return launchFromDeviceDefaults(row?.launchDefaults ?? null)
}

/** The first agent that device can run for this caller, or null. Asks
 *  `assertDeviceUsable` itself, so ownership/sharing stay ITS call. */
async function firstRunnableAgent(
  deviceId: string,
  teamId: string,
  userId: string
): Promise<string | null> {
  for (const agent of codingAgentValues) {
    try {
      await assertDeviceUsable(deviceId, teamId, userId, agent, WORKFLOW_DEVICE)
      return agent
    } catch {
      // Not that one.
    }
  }
  return null
}

const wireColumns = {
  id: workflows.id,
  teamId: workflows.teamId,
  repositoryId: workflows.repositoryId,
  name: workflows.name,
  status: workflows.status,
  deviceId: workflows.deviceId,
  launch: workflows.launch,
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
      mergedInto: workflowNodes.mergedInto,
      retriedAt: workflowNodes.retriedAt,
    })
    .from(workflowNodes)
    .where(eq(workflowNodes.id, nodeId))
    .limit(1)
  if (!node) throw new TRPCError({ code: `NOT_FOUND`, message: `Node not found` })
  return node
}

/**
 * EXP-1010: does the merged PR count as THIS attempt's? A merge older than
 * the workflow's start, or than the node's last retry, belongs to an earlier
 * life of the issue and would land the node empty. Pure.
 */
export function mergeBelongsToAttempt(args: {
  mergedAt: Date | string | null
  startedAt: Date | string | null
  retriedAt: Date | string | null
}): boolean {
  if (!args.mergedAt) return true // nobody stamped it: trust the state
  const merged = new Date(args.mergedAt).getTime()
  const floor = Math.max(
    args.startedAt ? new Date(args.startedAt).getTime() : 0,
    args.retriedAt ? new Date(args.retriedAt).getTime() : 0
  )
  return merged >= floor
}

/**
 * EXP-1010: what a node whose PR merged OUTSIDE the train does next. Pure.
 * - the integration branch, a base nobody recorded, or a branch that is none
 *   of the workflow's (the default branch): it lands.
 * - a BLOCKER's branch: its code reaches the integration branch only with
 *   that blocker, so it waits for it, and fails once the blocker is skipped
 *   (the final PR would lack its code while its issue is marked done).
 * - a synthetic `<integration>-base-…` merge base: nothing ever lands that
 *   branch, so the code is lost and the node fails.
 */
export function mergedNodeOutcome(args: {
  mergedInto: string | null
  integrationBranch: string
  /** branch → state of the workflow's OTHER nodes. */
  carriers: ReadonlyMap<string, string>
}): { step: `land` } | { step: `wait` } | { step: `fail`; note: string } {
  const base = args.mergedInto
  if (!base || base === args.integrationBranch) return { step: `land` }
  const lost = `its code is not in the workflow's branch. Retry runs it again`
  if (base.startsWith(`${args.integrationBranch}-base-`)) {
    return { step: `fail`, note: `Merged into ${base}, a throwaway base: ${lost}` }
  }
  const carrier = args.carriers.get(base)
  if (carrier === undefined || carrier === `landed`) return { step: `land` }
  if (carrier === `skipped`) {
    return { step: `fail`, note: `Merged into ${base}, which was skipped: ${lost}` }
  }
  return { step: `wait` }
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
] as const

/**
 * What one submitted review does to its node. Pure. EXP-1010: the agent
 * review is the ONLY gate, for every kind of node.
 * - approve → the node is cleared for the train, unless the reviewer's own
 *   checks FAILED (an approval its evidence contradicts stays advisory and
 *   waits for a person).
 * - request_changes → back to the author, up to the round cap; after that the
 *   node stops bouncing and waits for a person.
 */
export function reviewOutcome(args: {
  verdict: `approve` | `request_changes`
  oraclePassed: boolean | null
  round: number
}): { approve: boolean; state: `in_review` | `updating` | `waiting`; note: string } {
  if (args.verdict === `approve`) {
    const stands = args.oraclePassed !== false
    return {
      approve: stands,
      state: `in_review`,
      note: stands
        ? args.oraclePassed
          ? `Agent review passed, backed by its checks`
          : `Agent review passed`
        : `Agent review passed but its checks failed: needs a person`,
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

type Db = typeof import("@/db/connection").db

/**
 * FEED-51: is `sessionId` the review run the workflow started for the node,
 * or a RESUME of it? A resume (a person's account switch, the Resume button)
 * ends the recorded run and starts another under a new id; one performed
 * from a chat run re-stamps `started_reason=agent` with the chat as its
 * parent (EXP-906: the frame's own reason wins), so the calling row alone
 * does not decide (workflow 3b828f50, EXP-1030 r3). Every row on the chain
 * must be the review builtin and none may be the node's own run. A row is
 * the engine's reviewer when the workflow started it (`started_reason =
 * workflow`) or when it runs on the node's review branch
 * `exp/wf-<id8>-review-<IDENT>-r<n>` (the same evidence the engine's
 * `live_reviews_on_branches` uses); failing both, the walk follows
 * `resumed_from_id` back, bounded and loop-safe (a swept predecessor breaks
 * the chain, the branch does not depend on it).
 */
async function isReviewRunOfNode(
  db: Db,
  sessionId: string,
  node: { sessionId: string | null; issueId: string },
  workflowId: string
): Promise<boolean> {
  let cursor: string | null = sessionId
  const seen = new Set<string>()
  let identifier: string | null | undefined
  while (cursor && !seen.has(cursor) && seen.size < MAX_SESSION_CHAIN_DEPTH) {
    seen.add(cursor)
    const [row] = await db
      .select({
        id: codingSessions.id,
        actionName: codingSessions.actionName,
        startedReason: codingSessions.startedReason,
        branch: codingSessions.branch,
        resumedFromId: codingSessions.resumedFromId,
      })
      .from(codingSessions)
      .where(eq(codingSessions.id, cursor))
      .limit(1)
    if (
      !row ||
      row.actionName !== BUILTIN_REVIEW_NODE_NAME ||
      row.id === node.sessionId
    ) {
      return false
    }
    if (row.startedReason === `workflow`) return true
    if (row.branch) {
      if (identifier === undefined) {
        const [issue] = await db
          .select({ identifier: issues.identifier })
          .from(issues)
          .where(eq(issues.id, node.issueId))
          .limit(1)
        identifier = issue?.identifier ?? null
      }
      if (identifier && isWorkflowReviewBranch(workflowId, identifier, row.branch)) {
        return true
      }
    }
    cursor = row.resumedFromId ?? null
  }
  return false
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
            // EXP-1029: a workflow is BORN on the contract defaults — no
            // runner is bound yet, and the screen has no settings panel.
            // Binding one (`update({deviceId})`) re-seeds from that machine.
            launch: launchFromDeviceDefaults(null),
            // EXP-1029: every new workflow starts its dependents on the
            // blockers' CONTRACT. There is no choice any more.
            startOn: `contract`,
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
      // EXP-1029: `startOn` is fixed to `contract` — an old client still sends
      // it, and it changes nothing at all (not even the draft gate).
      const { id, name, decision, startOn: _startOn, ...config } = input
      void _startOn
      // The name is a label; everything else is the run's configuration.
      if (Object.values(config).some((value) => value !== undefined)) {
        assertDraft(existing.status, `Changing how a workflow runs`)
      }
      // EXP-1029: a launch is REPLACED whole by its normalized self — no
      // phase-pin merge dance. An old client's pins fold into `strongModel`.
      let nextLaunch = input.launch
        ? normalizeWorkflowLaunch(input.launch)
        : undefined
      if (nextLaunch) assertLaunch(nextLaunch)
      const deviceId =
        input.deviceId === undefined ? existing.deviceId : input.deviceId
      // EXP-1032: binding a runner to a DRAFT re-seeds agent, account and both
      // models from THAT machine's agent defaults — the workflow screen has no
      // settings panel, so the device IS the choice. (Setting `deviceId`
      // already asserted the draft above.)
      if (input.deviceId && !input.launch) {
        // Ownership and the cap first: those refusals must stay theirs.
        await assertDeviceUsable(
          input.deviceId,
          existing.teamId,
          ctx.session.user.id,
          null,
          WORKFLOW_DEVICE
        )
        const seeded = await launchForDevice(input.deviceId)
        const canRunSeeded = await assertDeviceUsable(
          input.deviceId,
          existing.teamId,
          ctx.session.user.id,
          seeded.agent,
          WORKFLOW_DEVICE
        ).then(
          () => true,
          () => false
        )
        // The machine advertises an agent it cannot actually run (or none at
        // all): the first one it CAN run stands in. No runnable agent at all
        // leaves the seed alone and the assert below refuses, as before.
        const runnable = canRunSeeded
          ? seeded.agent
          : await firstRunnableAgent(
              input.deviceId,
              existing.teamId,
              ctx.session.user.id
            )
        const stand = workflowLaunchAgentValues.find(
          (value) => value === runnable && value !== seeded.agent
        )
        // The stand-in agent drops the account too: a profile id belongs to
        // the agent it was advertised for.
        nextLaunch = stand
          ? { agent: stand, ...WORKFLOW_LAUNCH_DEFAULTS[stand] }
          : seeded
      }
      const launch = nextLaunch ?? normalizeWorkflowLaunch(existing.launch)
      if (deviceId && (input.deviceId !== undefined || input.launch)) {
        await assertDeviceUsable(
          deviceId,
          existing.teamId,
          ctx.session.user.id,
          launch.agent,
          WORKFLOW_DEVICE
        )
      }
      // compat: older clients still send the removed `gate` (EXP-1010) and
      // `startOn` (EXP-1029); both are ignored, and a patch of nothing but
      // those leaves nothing to set, which drizzle refuses with a 500. Answer
      // with the row as it is.
      if (
        name === undefined &&
        input.deviceId === undefined &&
        nextLaunch === undefined &&
        decision === undefined
      ) {
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
            ...(nextLaunch !== undefined && { launch: nextLaunch }),
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
        normalizeWorkflowLaunch(existing.launch).agent,
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
                  retriedAt: new Date(),
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
        // compat: a desktop/CLI ≤0.14.46 engine can still report the dropped
        // `paused` (a budget trip between migration 0134 and its shape
        // refetch); it is written as `failed` below. Delete the `.or` and the
        // mapping once CLIENT_MIN_VERSION_DESKTOP/CLI ≥ 0.14.47.
        state: wfNodeStateSchema.or(z.literal(`paused`)),
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
      // compat: see the input — `paused` no longer exists; `failed` is the
      // state that offers what it offered (Retry / Skip).
      const state = input.state === `paused` ? `failed` : input.state
      // A landed node is final and only `landNode` makes one; a skipped one
      // is a person's call the engine never takes back.
      if (
        node.state === `landed` ||
        node.state === `skipped` ||
        state === `landed` ||
        state === `skipped`
      ) {
        return { updated: false }
      }
      // EXP-1007: the read above and this write are not one transaction —
      // `landNode` can land the node in between. The WHERE repeats the
      // guard so a late report never takes a landing back.
      const rows = await ctx.db
        .update(workflowNodes)
        .set({
          state,
          ...(input.sessionId !== undefined && { sessionId: input.sessionId }),
          ...(input.baseBranch !== undefined && { baseBranch: input.baseBranch }),
          ...(input.attempt !== undefined && { attempt: input.attempt }),
          // compat: a `paused` node was HELD for a person, and the engine
          // restarts a `failed` one whose attempt is ≤ 1 — past the free retry.
          ...(input.state === `paused` && {
            attempt: sql`GREATEST(${input.attempt ?? workflowNodes.attempt}, 2)`,
          }),
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
      if (!merged && !node.approvedAt) {
        // The literal is matched by shipped engines (`LandOutcome::is_waiting`).
        return waiting(`Waiting for a person to approve`)
      }
      const graph = await loadWorkflowEdges(ctx.db, workflow.id)
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
      const { issuesRouter } = await import(`@/lib/trpc/issues`)
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
          await issuesRouter
            .createCaller(ctx)
            .mergePr({ issueId: node.issueId, endSessions: true })
        }
      } catch (err) {
        const reason = err instanceof Error ? err.message : `GitHub refused the merge`
        return { merged: false, reason: reason.slice(0, 500), retargeted: [] as string[] }
      }
      // EXP-1010: the state guard IS the claim. A concurrent `skip` stays a
      // skip, and two landings of one node count once.
      const outside = merged && !node.approvedAt
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
        if (outside) {
          const [ident] = await tx
            .select({ identifier: issues.identifier })
            .from(issues)
            .where(eq(issues.id, node.issueId))
            .limit(1)
          const [log] = await tx
            .select({ decisions: workflows.decisions })
            .from(workflows)
            .where(eq(workflows.id, workflow.id))
            .limit(1)
          const where =
            node.mergedInto && node.mergedInto !== workflow.integrationBranch
              ? ` into ${node.mergedInto}`
              : ``
          decisions = appendDecisionLine(
            log?.decisions ?? ``,
            `${ident?.identifier ?? `A node`} was merged outside the train${where}, before a review approved it.`,
            new Date()
          )
        }
        await tx
          .update(workflows)
          .set({
            metrics: bumpMetrics({ landed: 1 }),
            ...(decisions !== undefined && { decisions }),
          })
          .where(eq(workflows.id, workflow.id))
        return true
      })
      if (!landed) return { merged: true, reason: null, retargeted: [] as string[] }
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
