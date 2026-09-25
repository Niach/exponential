import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, sql } from "drizzle-orm"
import type { db as database } from "@/db/connection"
import {
  codingSessions,
  issues,
  repositories,
  workflowNodes,
  workflows,
} from "@/db/schema"
import {
  codingSessionResultSchema,
  type WorkflowNodeReview,
} from "@exp/db-schema/domain"
import { carriedReviewAtCap } from "@/lib/trpc/workflows/shared"
import { appBaseUrl } from "@/lib/notification-email-policy"
import {
  createPullRequest,
  getPullRequest,
  resolveRepoToken,
  retargetPullRequest,
} from "@/lib/integrations/github-pr"
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
    lines.push(``, `## Results`)
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
        ...group.map((result) => `[![${result.label}](${result.url})](${result.url})`)
      )
    }
  }
  return lines.join(`\n`)
}

/** The screenshots every run of the workflow published (`coding_sessions.
 *  results`, `exponential_sessions_results`), as the final PR shows them —
 *  one link per picture, the app serving the attachment to a member. */
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
