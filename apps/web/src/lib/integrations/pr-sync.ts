import { and, eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { issues, projectRepositories, projects, repositories } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { recordIssueEvent } from "@/lib/integrations/activity"

// Parse a workspace issue identifier ("MET-12") out of a PR head-branch name.
// Matches the launcher's `exp/<IDENTIFIER>` convention and any custom prefix
// (e.g. `feature/MET-12`) by anchoring on the trailing `<IDENT>-<number>` tail.
// Pure — unit-tested in pr-sync.test.ts.
export function parseIssueIdentifierFromBranch(branch: string): string | null {
  const match = branch.match(/(?:^|\/)([A-Z0-9]+-\d+)$/)
  return match ? match[1] : null
}

// Resolve an issue by (repo full name + head branch), for the webhook's
// deterministic branch-based linking. The repo scopes the identifier lookup to
// the projects that repo backs, so identical identifiers in other workspaces
// never cross-match. Returns null when the branch doesn't parse, the repo isn't
// registered, or no linked project holds that identifier.
export async function findIssueIdByBranch(
  repoFullName: string,
  branch: string
): Promise<string | null> {
  const identifier = parseIssueIdentifierFromBranch(branch)
  if (!identifier) return null

  const repoRows = await db
    .select({ id: repositories.id })
    .from(repositories)
    .where(eq(repositories.fullName, repoFullName))
  if (repoRows.length === 0) return null

  const linkRows = await db
    .select({ projectId: projectRepositories.projectId })
    .from(projectRepositories)
    .where(
      inArray(
        projectRepositories.repositoryId,
        repoRows.map((r) => r.id)
      )
    )
  const projectIds = [...new Set(linkRows.map((l) => l.projectId))]
  if (projectIds.length === 0) return null

  const [issue] = await db
    .select({ id: issues.id })
    .from(issues)
    .where(
      and(inArray(issues.projectId, projectIds), eq(issues.identifier, identifier))
    )
    .limit(1)
  return issue?.id ?? null
}

// Link a freshly-opened PR onto an issue that has none yet (webhook `opened`
// fallback for out-of-band PRs). Idempotent: a no-op once the issue already
// carries a prUrl, so a PR opened by the MCP open_pr tool (which already wrote
// the linkage) is never double-linked.
export async function applyPrOpenedState(opts: {
  issueId: string
  prUrl: string
  prNumber: number
  branch: string
  actorUserId?: string | null
}): Promise<void> {
  await db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    void txId

    const [current] = await tx
      .select({ prUrl: issues.prUrl, workspaceId: projects.workspaceId })
      .from(issues)
      .innerJoin(projects, eq(projects.id, issues.projectId))
      .where(eq(issues.id, opts.issueId))
      .limit(1)

    if (!current) return
    if (current.prUrl) return // already linked (idempotent)

    await tx
      .update(issues)
      .set({
        prUrl: opts.prUrl,
        prNumber: opts.prNumber,
        prState: `open`,
        branch: opts.branch,
      })
      .where(eq(issues.id, opts.issueId))

    await recordIssueEvent(tx, {
      issueId: opts.issueId,
      workspaceId: current.workspaceId,
      actorUserId: opts.actorUserId ?? null,
      type: `pr_opened`,
      payload: {
        prUrl: opts.prUrl,
        prNumber: opts.prNumber,
        branch: opts.branch,
      },
    })
  })
}

// Shared PR-merge writer, callable outside tRPC (webhook + self-hosted cron).
// Applies the open→merged write semantics: flips prState to 'merged', stamps
// prMergedAt, and emits a single pr_merged activity event.
//
// Idempotent on the open→merged transition: if the issue is already 'merged'
// we return without touching anything, so the webhook and the outbound cron
// can never double-fire the pr_merged event for the same PR.
export async function applyPrMergeState(opts: {
  issueId: string
  prUrl?: string
  mergedAt?: Date | null
  actorUserId?: string | null
}): Promise<void> {
  await db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    void txId

    const [current] = await tx
      .select({
        prState: issues.prState,
        prUrl: issues.prUrl,
        workspaceId: projects.workspaceId,
      })
      .from(issues)
      .innerJoin(projects, eq(projects.id, issues.projectId))
      .where(eq(issues.id, opts.issueId))
      .limit(1)

    // Unknown issue, or already merged → nothing to do (idempotent).
    if (!current) return
    if (current.prState === `merged`) return

    await tx
      .update(issues)
      .set({
        prState: `merged`,
        prMergedAt: opts.mergedAt ?? new Date(),
      })
      .where(eq(issues.id, opts.issueId))

    await recordIssueEvent(tx, {
      issueId: opts.issueId,
      workspaceId: current.workspaceId,
      actorUserId: opts.actorUserId ?? null,
      type: `pr_merged`,
      payload: { prUrl: opts.prUrl ?? current.prUrl ?? null },
    })
  })
}
