import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, inArray, sql } from "drizzle-orm"
import type { db as database } from "@/db/connection"
import {
  codingSessions,
  issues,
  repositories,
  workflowEvents,
  workflowNodes,
  workflows,
} from "@/db/schema"
import {
  codingSessionResultSchema,
  type WfEventKind,
  type WorkflowNodeReview,
} from "@exp/db-schema/domain"
import { carriedReviewAtCap } from "@/lib/trpc/workflows/shared"
import { writeWorkflowEvent } from "@/lib/workflows/record-event"
import { appBaseUrl } from "@/lib/notification-email-policy"
import {
  createPullRequest,
  getPullRequest,
  reopenPullRequest,
  resolvePrBaseState,
  resolveRepoToken,
  retargetPullRequest,
} from "@/lib/integrations/github-pr"
import { assertTeamMember } from "@/lib/team-membership"
import { loadWorkflowEdges } from "@/lib/workflows"
import { applyPrLifecycleStatusInTx } from "@/lib/integrations/pr-sync"
import {
  effectiveDefaultBranch,
} from "@/lib/trpc/repositories"
import { resolveRepoDefaultBranchCached } from "@/lib/integrations/github-app"

// EXP-982: the ONE final pull request of a workflow — integration branch →
// the repository's default branch. It carries the whole diff for a person to
// review; the node PRs were each merged into the integration branch by the
// merge train. Summaries are claims, not evidence, so the body also names a
// few RANDOM nodes to audit by hand. EXP-1065: this PR is the ONE human
// sign-off of the whole run (no node ever waits for a person), so its body
// also carries what the run left for that person: the review findings the
// cap carried, the decisions log and every run's screenshot results.

type Db = typeof database

/** How many landed nodes the final PR asks a person to audit. */
export const FINAL_PR_AUDIT_COUNT = 3

/** Deterministic per workflow (seeded by its id), so a re-open names the same
 *  nodes. */
export function pickAuditNodes<T>(nodes: readonly T[], seed: string, count: number): T[] {
  let state = 0
  for (const char of seed) state = (state * 31 + char.charCodeAt(0)) >>> 0
  const pool = [...nodes]
  const out: T[] = []
  while (pool.length > 0 && out.length < count) {
    state = (state * 1664525 + 1013904223) >>> 0
    out.push(pool.splice(state % pool.length, 1)[0]!)
  }
  return out
}

/** EXP-1065: the findings a node landed WITH at the review cap. */
export interface FinalPrCarriedFindings {
  identifier: string
  round: number
  findings: string
  /** The reviewer's failed check, when its oracle failed. */
  oracleCommand: string | null
}

/** EXP-1065: one screenshot a run of the workflow published. */
export interface FinalPrResult {
  /** The node's issue, when the run belonged to one. */
  identifier: string | null
  topic: string
  label: string
  url: string
}

export function finalPrBody(args: {
  name: string
  nodes: Array<{ identifier: string; title: string; prUrl: string | null; kind: string }>
  audit: Array<{ identifier: string; prUrl: string | null }>
  decisions: string
  findings?: FinalPrCarriedFindings[]
  results?: FinalPrResult[]
}): string {
  const lines = [
    `The final pull request of the workflow **${args.name}**: every node below was reviewed and squash-merged into this branch by the merge train. This PR carries the whole diff and is the one place a person reviews it.`,
    ``,
    `## Nodes`,
    ...args.nodes.map(
      (node) =>
        `- #${node.identifier}${node.kind === `leaf` ? `` : ` (${node.kind})`}${node.prUrl ? `: ${node.prUrl}` : ``}`
    ),
  ]
  const findings = args.findings ?? []
  if (findings.length > 0) {
    lines.push(
      ``,
      `## Unresolved review findings`,
      `These nodes landed at the review cap with findings their author did not settle. Check each before merging:`
    )
    for (const entry of findings) {
      lines.push(`- [ ] #${entry.identifier} (review round ${entry.round})`)
      const text = entry.findings.trim() || `(the reviewer wrote no findings)`
      for (const line of text.split(/\r?\n/)) lines.push(`  ${line}`.trimEnd())
      if (entry.oracleCommand) lines.push(`  Checks failed: ${entry.oracleCommand}`)
    }
  }
  if (args.audit.length > 0) {
    lines.push(
      ``,
      `## Audit these`,
      `Summaries are claims, not evidence. Read these ${args.audit.length} randomly picked node PRs in full before merging:`,
      ...args.audit.map(
        (node) => `- [ ] #${node.identifier}${node.prUrl ? `: ${node.prUrl}` : ``}`
      )
    )
  }
  if (args.decisions.trim()) {
    lines.push(``, `## Decisions`, args.decisions.trim())
  }
  const results = args.results ?? []
  if (results.length > 0) {
    // Attachment reads need membership, so GitHub's image proxy would show
    // every picture broken: plain links, opened as a signed-in member.
    lines.push(
      ``,
      `## Results`,
      `Screenshots the runs published (they open in Exponential for a signed-in member):`
    )
    const topics = new Map<string, FinalPrResult[]>()
    for (const result of results) {
      const key = `${result.identifier ?? ``}\u0000${result.topic}`
      topics.set(key, [...(topics.get(key) ?? []), result])
    }
    for (const group of topics.values()) {
      const first = group[0]!
      lines.push(
        ``,
        `### ${first.identifier ? `${first.identifier} · ` : ``}${first.topic}`,
        ...group.map((result) => `- [${result.label}](${result.url})`)
      )
    }
  }
  return lines.join(`\n`)
}

/** The screenshots every run of the workflow published (`coding_sessions.
 *  results`, `exponential_sessions_results`), as the final PR shows them —
 *  one plain link per picture, the app serving the attachment to a member. */
export async function loadWorkflowResults(
  db: Db,
  workflowId: string,
  identifierOfNode: ReadonlyMap<string, string>
): Promise<FinalPrResult[]> {
  const rows = await db
    .select({
      nodeId: codingSessions.workflowNodeId,
      results: codingSessions.results,
      startedAt: codingSessions.startedAt,
    })
    .from(codingSessions)
    .where(eq(codingSessions.workflowId, workflowId))
    .orderBy(asc(codingSessions.startedAt))
  const base = appBaseUrl()
  const out: FinalPrResult[] = []
  for (const row of rows) {
    if (!Array.isArray(row.results)) continue
    for (const raw of row.results) {
      const parsed = codingSessionResultSchema.safeParse(raw)
      if (!parsed.success || !parsed.data.topic || !parsed.data.label || !parsed.data.attachmentId) {
        continue
      }
      out.push({
        identifier: (row.nodeId && identifierOfNode.get(row.nodeId)) || null,
        topic: parsed.data.topic,
        label: parsed.data.label,
        url: `${base}/api/attachments/${parsed.data.attachmentId}`,
      })
    }
  }
  return out
}

type Executor = Pick<Db, `select` | `insert` | `update` | `delete`>

/**
 * EXP-1082 §3 / EXP-1096: the SERVER-decided lines of a workflow's audit
 * trail (`completed`, `final_pr_reopened`, `cancelled`, the final-PR
 * `failed`), written inside the caller's transaction; the runner device
 * records its own decisions through `workflows.appendEvent`. One writer for
 * all of them: `writeWorkflowEvent` (lib/workflows/record-event.ts).
 */
export async function recordWorkflowEventInTx(
  tx: Executor,
  event: {
    workflowId: string
    teamId: string
    kind: WfEventKind
    message: string
    nodeId?: string | null
    sessionId?: string | null
  }
): Promise<void> {
  await writeWorkflowEvent(tx, event)
}

/** The decision line an all-skipped workflow ends on (EXP-1059). */
export const NOTHING_SHIPPED_DECISION = `nothing shipped: every node was skipped`

/** What the final PR's `closed` state means once the engine gave up on it. */
export const FINAL_PR_CLOSED_AGAIN_DECISION = (prNumber: number) =>
  `the final pull request #${prNumber} was closed again after one reopen; open it from the workflow page to continue`

export type ReopenFinalPrOutcome =
  | { reopened: true; url: string }
  /** Nothing to do: no final PR, or it is not closed (already open / merged). */
  | { reopened: false; reason: string; gaveUp: false }
  /** The ONE reopen was used up, or GitHub refused it: the workflow keeps
   *  running with the decision line and a `failed` event; a member opens
   *  the final PR again from the workflow page (`openFinalPr`). */
  | { reopened: false; reason: string; gaveUp: true }

async function finalPrToken(
  db: Db,
  workflow: { teamId: string; repositoryId: string | null },
  actorUserId: string
): Promise<{ fullName: string; token: string } | { error: string }> {
  if (!workflow.repositoryId) return { error: `The workflow's repository is gone` }
  const [repo] = await db
    .select({ fullName: repositories.fullName })
    .from(repositories)
    .where(eq(repositories.id, workflow.repositoryId))
    .limit(1)
  if (!repo) return { error: `The workflow's repository is gone` }
  const token = await resolveRepoToken({
    actorUserId,
    teamId: workflow.teamId,
    repo: repo.fullName,
  })
  if (!token) return { error: `The GitHub App no longer has access to ${repo.fullName}` }
  return { fullName: repo.fullName, token }
}

/**
 * EXP-1059: a final PR someone closed WITHOUT merging is reopened — ONCE by
 * the engine (`once: true`: a second close is a person's decision, the
 * engine records it and stops), any number of times by a member from the
 * workflow page (`once: false`). A refusal from GitHub (head branch gone,
 * PR locked) is reported the same way; the member path then opens a NEW
 * final PR instead (`workflows.openFinalPr`). Never throws on the once
 * path: the engine reads the outcome, a member gets the message.
 */
export async function reopenWorkflowFinalPr(
  db: Db,
  workflowId: string,
  actorUserId: string,
  opts: { once: boolean }
): Promise<ReopenFinalPrOutcome> {
  const [workflow] = await db
    .select({
      id: workflows.id,
      teamId: workflows.teamId,
      repositoryId: workflows.repositoryId,
      finalPrUrl: workflows.finalPrUrl,
      finalPrNumber: workflows.finalPrNumber,
      finalPrState: workflows.finalPrState,
      decisions: workflows.decisions,
    })
    .from(workflows)
    .where(eq(workflows.id, workflowId))
    .limit(1)
  if (!workflow) return { reopened: false, reason: `Workflow not found`, gaveUp: false }
  if (!workflow.finalPrUrl || workflow.finalPrNumber == null) {
    return { reopened: false, reason: `The workflow has no final pull request yet`, gaveUp: false }
  }
  if (workflow.finalPrState !== `closed`) {
    return {
      reopened: false,
      reason: `The final pull request is ${workflow.finalPrState ?? `open`}`,
      gaveUp: false,
    }
  }
  const prNumber = workflow.finalPrNumber

  const giveUp = async (reason: string): Promise<ReopenFinalPrOutcome> => {
    // Said once: the line and the event are the same on every later beat.
    const line = FINAL_PR_CLOSED_AGAIN_DECISION(prNumber)
    if (!workflow.decisions.includes(line)) {
      await db.transaction(async (tx) => {
        await tx
          .update(workflows)
          .set({ decisions: appendWorkflowDecision(workflow.decisions, line) })
          .where(eq(workflows.id, workflow.id))
        await recordWorkflowEventInTx(tx, {
          workflowId: workflow.id,
          teamId: workflow.teamId,
          kind: `failed`,
          message: reason,
        })
      })
    }
    return { reopened: false, reason, gaveUp: true }
  }

  if (opts.once) {
    const [already] = await db
      .select({ id: workflowEvents.id })
      .from(workflowEvents)
      .where(
        and(
          eq(workflowEvents.workflowId, workflow.id),
          eq(workflowEvents.kind, `final_pr_reopened`)
        )
      )
      .orderBy(desc(workflowEvents.at))
      .limit(1)
    if (already) {
      return giveUp(`The final pull request #${prNumber} was closed again after one reopen`)
    }
  }

  const access = await finalPrToken(db, workflow, actorUserId)
  if (`error` in access) {
    return opts.once
      ? giveUp(`Could not reopen the final pull request #${prNumber}: ${access.error}`)
      : { reopened: false, reason: access.error, gaveUp: true }
  }
  try {
    await reopenPullRequest({ repo: access.fullName, prNumber, token: access.token })
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    return opts.once
      ? giveUp(`Could not reopen the final pull request #${prNumber}: ${message}`)
      : { reopened: false, reason: message, gaveUp: true }
  }
  await db.transaction(async (tx) => {
    await tx
      .update(workflows)
      .set({
        finalPrState: `open`,
        decisions: appendWorkflowDecision(
          workflow.decisions,
          `reopened the final pull request #${prNumber} (it was closed without merging)`
        ),
      })
      .where(eq(workflows.id, workflow.id))
    await recordWorkflowEventInTx(tx, {
      workflowId: workflow.id,
      teamId: workflow.teamId,
      kind: `final_pr_reopened`,
      message: `Reopened the final pull request #${prNumber}`,
    })
  })
  return { reopened: true, url: workflow.finalPrUrl }
}

/**
 * EXP-1072: `issues.prepareConflictFix` for a WORKFLOW's final pull request
 * — the same answer the issue path gives (the live rebase target, a stale
 * base healed), so the fix-conflicts launcher treats the final PR like any
 * linked PR. The final PR targets the repository's default branch: a base
 * GitHub reports as anything else is retargeted there.
 */
export async function prepareWorkflowFinalPrConflictFix(
  db: Db,
  workflowId: string,
  actorUserId: string
): Promise<{
  repo: string
  prNumber: number
  headRef: string
  baseRef: string | null
  baseKind: string
  rebaseOnto: string
  retargeted: boolean
  defaultBranch: string
}> {
  const [workflow] = await db
    .select({
      teamId: workflows.teamId,
      repositoryId: workflows.repositoryId,
      integrationBranch: workflows.integrationBranch,
      finalPrUrl: workflows.finalPrUrl,
      finalPrNumber: workflows.finalPrNumber,
      finalPrState: workflows.finalPrState,
    })
    .from(workflows)
    .where(eq(workflows.id, workflowId))
    .limit(1)
  if (!workflow) throw new TRPCError({ code: `NOT_FOUND`, message: `Issue not found` })
  await assertTeamMember(actorUserId, workflow.teamId)
  if (!workflow.finalPrUrl || workflow.finalPrNumber == null) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `The workflow has no final pull request yet`,
    })
  }
  if (workflow.finalPrState !== `open`) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `The pull request is ${workflow.finalPrState}. Only open pull requests can be conflict-fixed.`,
    })
  }
  if (!workflow.repositoryId) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The workflow's repository is gone` })
  }
  const [repo] = await db
    .select()
    .from(repositories)
    .where(eq(repositories.id, workflow.repositoryId))
    .limit(1)
  if (!repo) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The workflow's repository is gone` })
  }
  const token = await resolveRepoToken({
    actorUserId,
    teamId: workflow.teamId,
    repo: repo.fullName,
  })
  if (!token) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `The GitHub App no longer has access to ${repo.fullName}`,
    })
  }
  const defaultBranch =
    effectiveDefaultBranch(repo) ?? (await resolveRepoDefaultBranchCached(repo.fullName))
  if (!defaultBranch) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Could not resolve the default branch of ${repo.fullName}`,
    })
  }
  let state
  try {
    state = await resolvePrBaseState({
      repo: repo.fullName,
      prNumber: workflow.finalPrNumber,
      token,
      defaultBranch,
    })
  } catch (err) {
    throw new TRPCError({
      code: `BAD_GATEWAY`,
      message:
        err instanceof Error ? err.message : `Failed to read the pull request from GitHub`,
    })
  }
  if (state.prState !== `open`) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `The pull request is already ${state.merged ? `merged` : `closed`} on GitHub`,
    })
  }
  let retargeted = false
  if (state.retargetTo != null) {
    await retargetPullRequest({
      repo: repo.fullName,
      prNumber: workflow.finalPrNumber,
      base: state.retargetTo,
      token,
    })
    retargeted = true
  }
  return {
    repo: repo.fullName,
    prNumber: workflow.finalPrNumber,
    headRef: state.headRef || workflow.integrationBranch,
    baseRef: state.baseRef,
    baseKind: state.kind,
    rebaseOnto: state.rebaseOnto,
    retargeted,
    defaultBranch,
  }
}

/** `2026-09-25: <text>` appended to the decisions log — the same shape
 *  `trpc/workflows/shared.ts` `appendDecisionLine` writes, kept local so this
 *  module never imports the router. */
export function appendWorkflowDecision(log: string, text: string, now = new Date()): string {
  const line = `${now.toISOString().slice(0, 10)}: ${text.replace(/\s+/g, ` `).trim()}`
  return log.trim() ? `${log.trimEnd()}\n${line}` : line
}

export async function openWorkflowFinalPr(
  db: Db,
  workflowId: string,
  actorUserId: string
): Promise<{ url: string }> {
  const [workflow] = await db
    .select({
      id: workflows.id,
      teamId: workflows.teamId,
      name: workflows.name,
      repositoryId: workflows.repositoryId,
      integrationBranch: workflows.integrationBranch,
      decisions: workflows.decisions,
    })
    .from(workflows)
    .where(eq(workflows.id, workflowId))
    .limit(1)
  if (!workflow?.repositoryId) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The workflow's repository is gone` })
  }
  const nodes = await db
    .select({
      id: workflowNodes.id,
      state: workflowNodes.state,
      kind: workflowNodes.kind,
      identifier: issues.identifier,
      title: issues.title,
      prUrl: issues.prUrl,
      // EXP-1065: what a node landed with at the review cap.
      reviewRound: workflowNodes.reviewRound,
      review: workflowNodes.review,
      approvedAt: workflowNodes.approvedAt,
    })
    .from(workflowNodes)
    .innerJoin(issues, eq(issues.id, workflowNodes.issueId))
    .where(eq(workflowNodes.workflowId, workflowId))
    .orderBy(asc(workflowNodes.wave), asc(workflowNodes.lane))
  // A `proposed` node was never admitted: it is not part of the workflow.
  const open = nodes.filter(
    (node) =>
      node.state !== `landed` && node.state !== `skipped` && node.state !== `proposed`
  )
  if (open.length > 0) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `${open.map((node) => node.identifier).join(`, `)} has not landed yet`,
    })
  }

  const [repo] = await db
    .select()
    .from(repositories)
    .where(eq(repositories.id, workflow.repositoryId))
    .limit(1)
  if (!repo) {
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `The workflow's repository is gone` })
  }
  const token = await resolveRepoToken({
    actorUserId,
    teamId: workflow.teamId,
    repo: repo.fullName,
  })
  if (!token) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `The GitHub App no longer has access to ${repo.fullName}`,
    })
  }
  const base =
    effectiveDefaultBranch(repo) ?? (await resolveRepoDefaultBranchCached(repo.fullName))
  if (!base) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `Could not resolve ${repo.fullName}'s default branch`,
    })
  }

  const landed = nodes.filter((node) => node.state === `landed`)
  const findings: FinalPrCarriedFindings[] = []
  for (const node of landed) {
    const carried = carriedReviewAtCap({
      reviewRound: node.reviewRound ?? 0,
      review: (node.review as WorkflowNodeReview | null) ?? null,
      approvedAt: node.approvedAt ?? null,
    })
    if (!carried) continue
    findings.push({
      identifier: node.identifier,
      round: carried.round,
      findings: carried.findings,
      oracleCommand:
        carried.oracle && carried.oracle.passed === false ? carried.oracle.command : null,
    })
  }
  const results = await loadWorkflowResults(
    db,
    workflow.id,
    new Map(nodes.map((node) => [node.id, node.identifier]))
  )
  const created = await createPullRequest({
    repo: repo.fullName,
    head: workflow.integrationBranch,
    base,
    title: `Workflow: ${workflow.name}`,
    body: finalPrBody({
      name: workflow.name,
      nodes: landed,
      audit: pickAuditNodes(landed, workflow.id, FINAL_PR_AUDIT_COUNT),
      decisions: workflow.decisions,
      findings,
      results,
    }),
    token,
  })
  await db
    .update(workflows)
    .set({
      finalPrUrl: created.url,
      finalPrNumber: created.number,
      finalPrState: `open`,
    })
    .where(eq(workflows.id, workflowId))
  return { url: created.url }
}

/**
 * A pull request changed state on GitHub (webhook / poller): if it is a
 * workflow's FINAL PR, record it. A merge completes the workflow and only NOW
 * moves every covered issue to the team's PR-merge status — their own PRs
 * merged into the integration branch long ago, which shipped nothing.
 * Returns true when the PR was a workflow's. Never throws.
 *
 * The in-app merge (`workflows.mergeFinalPr`) and the GitHub webhook both
 * call this for the same url within seconds, so the write is an atomic
 * CLAIM: the UPDATE matches only while the row does not read `merged` yet,
 * and the covered-issue fan-out runs for the one caller whose UPDATE took
 * the row. The loser writes nothing (no second `ended_at`, no second loop).
 */
export async function applyWorkflowFinalPrState(
  db: Db,
  prUrl: string,
  state: `merged` | `closed` | `open`
): Promise<boolean> {
  try {
    const [workflow] = await db
      .select({
        id: workflows.id,
        teamId: workflows.teamId,
        finalPrState: workflows.finalPrState,
      })
      .from(workflows)
      .where(eq(workflows.finalPrUrl, prUrl))
      .limit(1)
    if (!workflow) return false
    if (workflow.finalPrState === `merged`) return true

    await db.transaction(async (tx) => {
      const claimed = await tx
        .update(workflows)
        .set({
          finalPrState: state,
          ...(state === `merged` && { status: `done`, endedAt: new Date() }),
        })
        .where(
          and(
            eq(workflows.id, workflow.id),
            sql`${workflows.finalPrState} IS DISTINCT FROM 'merged'`
          )
        )
        .returning({ id: workflows.id })
      if (claimed.length === 0 || state !== `merged`) return

      // Only what LANDED shipped with this PR: a skipped node's work never
      // reached the branch, a proposed one was never part of the workflow.
      const nodes = await tx
        .select({
          issueId: workflowNodes.issueId,
          members: workflowNodes.memberIssueIds,
        })
        .from(workflowNodes)
        .where(
          and(eq(workflowNodes.workflowId, workflow.id), eq(workflowNodes.state, `landed`))
        )
      const covered = [
        ...new Set(nodes.flatMap((node) => [node.issueId, ...node.members])),
      ]
      if (covered.length === 0) return
      // EXP-1072: the SAME status writer the linked-PR merge path uses
      // (`applyPrMergeState` → `applyPrLifecycleStatusInTx`): the team's
      // PR-merge target, completedAt derivation, the status_changed event.
      // The member issues' own `pr_merged` events were recorded when their
      // node PRs landed on the integration branch.
      const rows = await tx
        .select({ id: issues.id, status: issues.status })
        .from(issues)
        .where(inArray(issues.id, covered))
      for (const row of rows) {
        await applyPrLifecycleStatusInTx(tx, {
          issueId: row.id,
          teamId: workflow.teamId,
          actorUserId: null,
          currentStatus: row.status,
          event: `merged`,
        })
      }
      await recordWorkflowEventInTx(tx, {
        workflowId: workflow.id,
        teamId: workflow.teamId,
        kind: `completed`,
        message: `Final pull request merged: ${covered.length} issue${covered.length === 1 ? `` : `s`} done`,
      })
    })
    return true
  } catch (err) {
    console.error(`[workflows] final PR state failed:`, err)
    return false
  }
}


/**
 * EXP-983: a node just landed. A dependent that based its pull request on
 * the landed node's branch (or on a synthetic merge of several blockers) and
 * now has NO unlanded blocker left is retargeted to the integration branch,
 * so its diff keeps showing only its own work; the engine then has it merge
 * the trunk in. Dependents that still wait on another blocker keep their
 * base. Best-effort: a refusal is logged, the engine's next land attempt
 * surfaces it. Returns the retargeted node ids.
 */
export async function retargetReleasedDependents(
  db: Db,
  workflowId: string,
  landedNodeId: string,
  actorUserId: string
): Promise<string[]> {
  try {
    const [workflow] = await db
      .select({
        teamId: workflows.teamId,
        repositoryId: workflows.repositoryId,
        integrationBranch: workflows.integrationBranch,
      })
      .from(workflows)
      .where(eq(workflows.id, workflowId))
      .limit(1)
    if (!workflow?.repositoryId) return []
    const { nodes, edges } = await loadWorkflowEdges(db, workflowId)
    const stateOf = new Map(nodes.map((node) => [node.id, node.state]))
    const released = nodes.filter((node) => {
      if (node.state === `landed` || node.state === `skipped`) return false
      if (!node.baseBranch || node.baseBranch === workflow.integrationBranch) return false
      const blockers = edges.filter(([, to]) => to === node.id).map(([from]) => from)
      if (!blockers.includes(landedNodeId)) return false
      return blockers.every((id) => {
        const state = stateOf.get(id)
        return id === landedNodeId || state === `landed` || state === `skipped`
      })
    })
    if (released.length === 0) return []

    const [repo] = await db
      .select({ fullName: repositories.fullName })
      .from(repositories)
      .where(eq(repositories.id, workflow.repositoryId))
      .limit(1)
    if (!repo) return []
    const token = await resolveRepoToken({
      actorUserId,
      teamId: workflow.teamId,
      repo: repo.fullName,
    })
    const prs = await db
      .select({
        id: issues.id,
        prNumber: issues.prNumber,
        prState: issues.prState,
        prBaseBranch: issues.prBaseBranch,
      })
      .from(issues)
      .where(
        inArray(
          issues.id,
          released.map((node) => node.issueId)
        )
      )
    const prOf = new Map(prs.map((row) => [row.id, row]))
    const done: string[] = []
    for (const node of released) {
      const pr = prOf.get(node.issueId)
      const prOpen = pr?.prNumber != null && pr.prState === `open`
      // The node's recorded base follows the PR, never the other way round:
      // an open PR still on the old base keeps the node there too, so the
      // next `landNode` sees the mismatch and refuses (the train stays
      // honest). No PR yet = nothing to move on GitHub; the node's base is
      // what `pr_open` will use.
      if (prOpen && pr.prBaseBranch !== workflow.integrationBranch) {
        if (!token) {
          console.error(
            `[workflows] retarget of node ${node.id} skipped: no GitHub App token for ${repo.fullName}`
          )
          continue
        }
        try {
          await retargetPullRequest({
            repo: repo.fullName,
            prNumber: pr.prNumber!,
            base: workflow.integrationBranch,
            token,
          })
          await db
            .update(issues)
            .set({ prBaseBranch: workflow.integrationBranch })
            .where(eq(issues.id, node.issueId))
        } catch (err) {
          console.error(`[workflows] retarget of node ${node.id} failed:`, err)
          continue
        }
      }
      await db
        .update(workflowNodes)
        .set({ baseBranch: workflow.integrationBranch })
        .where(eq(workflowNodes.id, node.id))
      done.push(node.id)
    }
    return done
  } catch (err) {
    console.error(`[workflows] retarget after landing failed:`, err)
    return []
  }
}

export type NodePrBaseCheck =
  | { ok: true; retargeted: boolean }
  | { ok: false; reason: string }

/** The one sentence `landNode` answers with while a node's PR is off-branch. */
export const NODE_PR_OFF_BRANCH_REASON = `Its pull request is not based on the workflow branch yet`

/**
 * The merge train's base assertion: a node's PR must be based on the
 * workflow's integration branch BEFORE `landNode` squash-merges it, or the
 * merge lands on whatever the PR points at (the default branch, when the
 * stack heal of `retargetChildrenOfMergedPr` or a person moved it) and the
 * work bypasses the workflow's ONE final PR. The recorded `issues.pr_base_branch`
 * is asked first; when it is unknown, GitHub is (with a token). A PR still
 * on another base is retargeted here when a token allows it; otherwise the
 * caller gets the waiting outcome. Never throws.
 */
export async function ensureNodePrOnIntegrationBranch(
  db: Db,
  args: {
    issueId: string
    repositoryId: string | null
    teamId: string
    integrationBranch: string
    actorUserId: string
  }
): Promise<NodePrBaseCheck> {
  try {
    const [pr] = await db
      .select({
        prNumber: issues.prNumber,
        prState: issues.prState,
        prBaseBranch: issues.prBaseBranch,
      })
      .from(issues)
      .where(eq(issues.id, args.issueId))
      .limit(1)
    if (!pr || pr.prNumber == null || pr.prState !== `open`) {
      return { ok: false, reason: `It has no open pull request` }
    }
    if (pr.prBaseBranch === args.integrationBranch) return { ok: true, retargeted: false }
    if (!args.repositoryId) return { ok: false, reason: NODE_PR_OFF_BRANCH_REASON }
    const [repo] = await db
      .select({ fullName: repositories.fullName })
      .from(repositories)
      .where(eq(repositories.id, args.repositoryId))
      .limit(1)
    if (!repo) return { ok: false, reason: NODE_PR_OFF_BRANCH_REASON }
    const token = await resolveRepoToken({
      actorUserId: args.actorUserId,
      teamId: args.teamId,
      repo: repo.fullName,
    })
    if (!token) return { ok: false, reason: NODE_PR_OFF_BRANCH_REASON }
    let base = pr.prBaseBranch
    if (base === null) {
      // Unknown locally (a PR opened outside `pr_open`, or before the column
      // existed): GitHub knows.
      base = (await getPullRequest(repo.fullName, pr.prNumber, token)).baseRef || null
      if (base === args.integrationBranch) {
        await db
          .update(issues)
          .set({ prBaseBranch: base })
          .where(eq(issues.id, args.issueId))
        return { ok: true, retargeted: false }
      }
    }
    await retargetPullRequest({
      repo: repo.fullName,
      prNumber: pr.prNumber,
      base: args.integrationBranch,
      token,
    })
    await db
      .update(issues)
      .set({ prBaseBranch: args.integrationBranch })
      .where(eq(issues.id, args.issueId))
    return { ok: true, retargeted: true }
  } catch (err) {
    console.error(`[workflows] node PR base check failed:`, err)
    return { ok: false, reason: NODE_PR_OFF_BRANCH_REASON }
  }
}
