import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray } from "drizzle-orm"
import type { db as database } from "@/db/connection"
import {
  issues,
  repositories,
  workflowNodes,
  workflows,
} from "@/db/schema"
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
// few RANDOM nodes to audit by hand.

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

export function finalPrBody(args: {
  name: string
  nodes: Array<{ identifier: string; title: string; prUrl: string | null; kind: string }>
  audit: Array<{ identifier: string; prUrl: string | null }>
  decisions: string
}): string {
  const lines = [
    `The final pull request of the workflow **${args.name}**: every node below was reviewed and squash-merged into this branch by the merge train. This PR carries the whole diff.`,
    ``,
    `## Nodes`,
    ...args.nodes.map(
      (node) =>
        `- #${node.identifier}${node.kind === `leaf` ? `` : ` (${node.kind})`}${node.prUrl ? `: ${node.prUrl}` : ``}`
    ),
  ]
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
  return lines.join(`\n`)
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
      state: workflowNodes.state,
      kind: workflowNodes.kind,
      identifier: issues.identifier,
      title: issues.title,
      prUrl: issues.prUrl,
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
      await tx
        .update(workflows)
        .set({
          finalPrState: state,
          ...(state === `merged` && { status: `done`, endedAt: new Date() }),
        })
        .where(eq(workflows.id, workflow.id))
      if (state !== `merged`) return

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
