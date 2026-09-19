import { TRPCError } from "@trpc/server"
import { asc, eq, inArray } from "drizzle-orm"
import type { db as database } from "@/db/connection"
import {
  issues,
  repositories,
  workflowNodes,
  workflows,
} from "@/db/schema"
import { createPullRequest, resolveRepoToken } from "@/lib/integrations/github-pr"
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
  const open = nodes.filter((node) => node.state !== `landed` && node.state !== `skipped`)
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

      const nodes = await tx
        .select({
          issueId: workflowNodes.issueId,
          members: workflowNodes.memberIssueIds,
        })
        .from(workflowNodes)
        .where(eq(workflowNodes.workflowId, workflow.id))
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
