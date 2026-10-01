import { TRPCError } from "@trpc/server"
import {
  and,
  eq,
  inArray,
} from "drizzle-orm"
import {
  WORKFLOW_DECISIONS_MAX,
  WORKFLOW_MAX_REVIEW_ROUNDS,
  WORKFLOW_LAUNCH_DEFAULTS,
  workflowLaunchAgentValues,
  type WorkflowLaunch,
  type WorkflowLaunchAgent,
  type WorkflowNodeReview,
} from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { workflowDefaultsFor } from "@/lib/devices/workflow-defaults"
import {
  boards,
  codingSessions,
  devices,
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
import { isWorkflowReviewBranch } from "@/lib/workflows"

// EXP-981: workflows — a picked set of issues of ONE repository, planned as a
// DAG (`blocks` = edges, a parent with sub-issues = one compound node) and,
// from EXP-982 on, run by the deterministic engine on the runner device.
// Rows sync via the `workflows` + `workflow_nodes` shapes; this router is the
// write path (any member: a workflow is work, not a team setting) plus the
// `get`/`list` reads MCP answers from. P2 = DRAFT workflows only: start,
// pause and cancel land with the engine.

export const codingAgentValues = contract.codingAgent.values as readonly string[]
export const agentModelValues: Record<string, readonly string[]> = {
  claude: contract.codingModel.values,
  codex: contract.codexModel.values,
}
export const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

/** The device's build cannot run workflows (it predates the engine). */
export const WORKFLOW_DEVICE_CAP = `workflows`
export const WORKFLOW_DEVICE = {
  noun: `Workflow`,
  cap: WORKFLOW_DEVICE_CAP,
  capMessage: `Update Exponential on that machine to run workflows`,
}

/**
 * EXP-1032: the NORMALIZED launch is what gets stored and validated — two
 * models out of the agent's own closed vocabulary, and nothing else.
 */
export function assertLaunch(launch: WorkflowLaunch): void {
  if (!codingAgentValues.includes(launch.agent)) throw bad(`Unknown agent`)
  const models = agentModelValues[launch.agent]!
  for (const model of [launch.model, launch.strongModel]) {
    if (!models.includes(model)) throw bad(`Unknown ${launch.agent} model`)
  }
}

/**
 * EXP-1032: a workflow's launch, seeded from the machine BOUND to run it
 * (`update({deviceId})` on a draft — a workflow is created on the contract
 * defaults, with no runner) — the device's LAST USED agent is the agent it
 * runs on (no account named: the machine runs it on that agent's last used
 * login), its `launch_defaults.workflow` the two models. A device that
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
  return {
    agent,
    ...workflowDefaultsFor(agent, defaults?.workflow),
  }
}

/** The launch a workflow bound to `deviceId` is seeded with. A device row
 *  that is gone reads as one that advertises nothing. */
export async function launchForDevice(deviceId: string): Promise<WorkflowLaunch> {
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
export async function firstRunnableAgent(
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

/**
 * EXP-1032: the launch a workflow gets when a runner is bound to it (`create`
 * and `update` with a `deviceId`): the agent and both models from THAT
 * machine's agent defaults; the workflow screen has no settings panel, so the
 * device IS the choice. Ownership and the cap are asserted first, so those
 * refusals stay `assertDeviceUsable`'s. A machine that advertises an agent it
 * cannot actually run (or none at all) gets the first one it CAN run, on the
 * contract pair. No runnable agent at all leaves the seed
 * alone: the caller's final assert refuses it, as before.
 */
export async function seedLaunchFromBoundDevice(
  deviceId: string,
  teamId: string,
  userId: string
): Promise<WorkflowLaunch> {
  await assertDeviceUsable(deviceId, teamId, userId, null, WORKFLOW_DEVICE)
  const seeded = await launchForDevice(deviceId)
  const canRunSeeded = await assertDeviceUsable(
    deviceId,
    teamId,
    userId,
    seeded.agent,
    WORKFLOW_DEVICE
  ).then(
    () => true,
    () => false
  )
  const runnable = canRunSeeded
    ? seeded.agent
    : await firstRunnableAgent(deviceId, teamId, userId)
  const stand = workflowLaunchAgentValues.find(
    (value) => value === runnable && value !== seeded.agent
  )
  return stand ? { agent: stand, ...WORKFLOW_LAUNCH_DEFAULTS[stand] } : seeded
}

export const wireColumns = {
  id: workflows.id,
  teamId: workflows.teamId,
  repositoryId: workflows.repositoryId,
  name: workflows.name,
  status: workflows.status,
  deviceId: workflows.deviceId,
  launch: workflows.launch,
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

export async function loadWorkflow(id: string) {
  const { db } = await import(`@/db/connection`)
  const [row] = await db
    .select(wireColumns)
    .from(workflows)
    .where(eq(workflows.id, id))
    .limit(1)
  if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Workflow not found` })
  return row
}

export function assertDraft(status: string, what: string): void {
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
export async function loadPickableIssues(
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
export async function assertEngine(
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

export async function loadNode(nodeId: string) {
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
      review: workflowNodes.review,
      reviewRound: workflowNodes.reviewRound,
      attempt: workflowNodes.attempt,
      // EXP-1103: the review-wave gate reads the node's layer.
      wave: workflowNodes.wave,
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
export async function relayDecision(workflowId: string, text: string): Promise<void> {
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


/**
 * What one submitted review does to its node. Pure. EXP-1010: the agent
 * review is the ONLY gate, for every kind of node. EXP-1065: no person ever
 * takes over — `waiting` is never written.
 * - approve → the node is cleared for the train, unless the reviewer's own
 *   checks FAILED: an approval its evidence contradicts is a request for
 *   changes in all but name, and goes back to the author like one.
 * - request_changes → back to the author, up to the round cap. The cap is a
 *   bound on bouncing, not a hand-off: the last round's findings reach the
 *   author once more, then the node lands with the findings CARRIED into
 *   the final pull request ([`carriedReview`]), the one place a person
 *   reviews.
 */
export function reviewOutcome(args: {
  verdict: `approve` | `request_changes`
  oraclePassed: boolean | null
  round: number
}): { approve: boolean; state: `in_review` | `updating`; note: string } {
  const max = WORKFLOW_MAX_REVIEW_ROUNDS
  if (args.verdict === `approve` && args.oraclePassed !== false) {
    return {
      approve: true,
      state: `in_review`,
      note: args.oraclePassed
        ? `Agent review passed, backed by its checks`
        : `Agent review passed`,
    }
  }
  const checksFailed = args.verdict === `approve`
  if (args.round >= max) {
    return {
      approve: false,
      state: `updating`,
      note: checksFailed
        ? `Review round ${max} of ${max}: its checks still fail; lands after the author's last push, carried to the final review`
        : `Review round ${max} of ${max}: findings go to the author once more, then it lands with them carried`,
    }
  }
  return {
    approve: false,
    state: `updating`,
    note: checksFailed
      ? `Agent review passed but its checks failed: back to the author`
      : `Changes requested by the agent review (round ${args.round} of ${max})`,
  }
}

/**
 * EXP-1065/EXP-1103: the review a landed node carries UNRESOLVED into the
 * final pull request — a verdict that still asks for changes (a failed oracle
 * under an approve counts). A wave's fix run had ONE go at it; whatever it
 * left open is a person's to check at the final review, never another round.
 * `null` = nothing to carry.
 */
export function carriedReview(node: {
  review: WorkflowNodeReview | null
}): WorkflowNodeReview | null {
  const review = node.review
  if (!review) return null
  const changes = review.verdict === `request_changes` || review.oracle?.passed === false
  return changes ? review : null
}

export type Db = typeof import("@/db/connection").db

/**
 * FEED-51: is `sessionId` the review run the workflow started for the node,
 * or a RESUME of it? A resume (a person's account switch, the Resume button)
 * ends the recorded run and starts another under a new id; one performed
 * from a chat run re-stamps `started_reason=agent` with the chat as its
 * parent (EXP-906: the frame's own reason wins), so the calling row alone
 * does not decide (workflow 3b828f50, EXP-1030 r3). Every row on the chain
 * must be the review builtin and none may be the node's own run. A row is
 * THIS node's reviewer when it runs on the node's review branch
 * `exp/wf-<id8>-review-<IDENT>-r<n>` (the same evidence the engine's
 * `live_reviews_on_branches` uses). Every engine-started reviewer carries
 * its branch, so a row WITH a branch is judged by it alone: `started_reason =
 * workflow` on another node's branch is another node's reviewer, not a pass.
 * Only a branch-less row falls back to `started_reason = workflow`; failing
 * both, the walk follows `resumed_from_id` back, bounded and loop-safe (a
 * swept predecessor breaks the chain, the branch does not depend on it).
 */
export async function isReviewRunOfNode(
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
    } else if (row.startedReason === `workflow`) {
      return true
    }
    cursor = row.resumedFromId ?? null
  }
  return false
}
