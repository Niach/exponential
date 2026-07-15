import { and, desc, eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { issueEvents, issues, projects, repositories } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { fireAndForgetPrNotify } from "@/lib/integrations/notifications"

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

  // Projects backed by any of these repos (v4: projects.repositoryId).
  const projectRows = await db
    .select({ projectId: projects.id, workspaceId: projects.workspaceId })
    .from(projects)
    .where(
      inArray(
        projects.repositoryId,
        repoRows.map((r) => r.id)
      )
    )
  const projectIds = [...new Set(projectRows.map((p) => p.projectId))]
  if (projectIds.length === 0) return null

  const [issue] = await db
    .select({ id: issues.id })
    .from(issues)
    .where(
      and(inArray(issues.projectId, projectIds), eq(issues.identifier, identifier))
    )
    .limit(1)
  if (issue) return issue.id

  // Second chance (EXP-57): the branch may predate a cross-project move that
  // renumbered the issue — identifiers are monotonic and never reused, so a
  // retired identifier matches nothing above. Every move records its retired
  // identifier in a project_moved event; match it WORKSPACE-scoped, because
  // the move re-pointed the issue's events onto the TARGET project, which may
  // not be backed by this repo at all. Latest event wins (a chain of moves
  // still resolves each retired identifier to the same issue exactly once).
  const workspaceIds = [...new Set(projectRows.map((p) => p.workspaceId))]
  const [movedEvent] = await db
    .select({ issueId: issueEvents.issueId })
    .from(issueEvents)
    .where(
      and(
        inArray(issueEvents.workspaceId, workspaceIds),
        eq(issueEvents.type, `project_moved`),
        sql`${issueEvents.payload} ->> 'fromIdentifier' = ${identifier}`
      )
    )
    .orderBy(desc(issueEvents.createdAt))
    .limit(1)
  return movedEvent?.issueId ?? null
}

// Pure transition guard for the merge/close writers below. One PR per issue
// (a batch PR may be linked to several issues, but each issue has exactly one
// linked PR): only the LINKED PR may flip the issue's prState — the webhook's
// branch-identifier fallback would otherwise let any second PR whose head
// branch ends in the identifier (e.g. `backport/EXP-42`) falsely flip the
// issue while its real PR is still open. Unit-tested in pr-sync.test.ts.
export function prStateTransitionAllowed(
  current: { prState: string | null; prUrl: string | null },
  transition: { to: `merged` | `closed` | `open`; prUrl?: string }
): boolean {
  if (current.prUrl && transition.prUrl && current.prUrl !== transition.prUrl) {
    return false
  }
  if (transition.to === `merged`) {
    // Idempotent open→merged (a closed PR can be reopened+merged on GitHub,
    // so merge is allowed from any state except merged itself).
    return current.prState !== `merged`
  }
  if (transition.to === `open`) {
    // Reopen only heals a closed PR (webhook `reopened`).
    return current.prState === `closed`
  }
  // Close only applies to an open PR.
  return current.prState === `open`
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
  const applied = await db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    void txId

    const [current] = await tx
      .select({ prUrl: issues.prUrl, workspaceId: projects.workspaceId })
      .from(issues)
      .innerJoin(projects, eq(projects.id, issues.projectId))
      .where(eq(issues.id, opts.issueId))
      .limit(1)

    if (!current) return false
    if (current.prUrl) return false // already linked (idempotent)

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
    return true
  })

  // Notify only when this call actually linked the PR (the idempotent guard
  // above makes the MCP-open + webhook pair single-fire). The MCP open_pr
  // path writes the linkage itself and notifies separately; deliver()'s
  // dedupe window absorbs the overlap.
  if (applied) {
    fireAndForgetPrNotify({
      issueId: opts.issueId,
      type: `pr_opened`,
      actorUserId: opts.actorUserId ?? null,
    })
  }
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
  const applied = await db.transaction(async (tx) => {
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

    // Unknown issue, already merged, or a different (unlinked) PR → nothing
    // to do (idempotent; see prStateTransitionAllowed).
    if (!current) return false
    if (!prStateTransitionAllowed(current, { to: `merged`, prUrl: opts.prUrl })) {
      return false
    }

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
    return true
  })

  // Inside the open→merged idempotent guard: the webhook and the self-hosted
  // outbound cron can both call this, but only the transition that actually
  // flipped the state fans out — so an away phone gets exactly one
  // "it's merged" notification on in-app + push + email.
  if (applied) {
    fireAndForgetPrNotify({
      issueId: opts.issueId,
      type: `pr_merged`,
      actorUserId: opts.actorUserId ?? null,
    })
  }
}

// PR closed WITHOUT merging (webhook `closed` with merged=false + the
// self-hosted poller). Flips open→closed so the issue drops out of the
// Reviews open-PR surfaces and the poller's re-fetch set. State-only: no
// pr_closed event/notification type exists in the domain contract yet —
// adding one is a four-client codegen change, deliberately out of scope.
export async function applyPrClosedState(opts: {
  issueId: string
  prUrl?: string
}): Promise<void> {
  await applyPrStateFlip(opts.issueId, opts.prUrl, `closed`)
}

// PR reopened on GitHub after a close-without-merge (webhook `reopened`):
// heal closed→open so the badge, the Reviews surfaces, and the tRPC mergePr
// open-state precondition all track the PR again.
export async function applyPrReopenedState(opts: {
  issueId: string
  prUrl?: string
}): Promise<void> {
  await applyPrStateFlip(opts.issueId, opts.prUrl, `open`)
}

async function applyPrStateFlip(
  issueId: string,
  prUrl: string | undefined,
  to: `closed` | `open`
): Promise<void> {
  await db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    void txId

    const [current] = await tx
      .select({ prState: issues.prState, prUrl: issues.prUrl })
      .from(issues)
      .where(eq(issues.id, issueId))
      .limit(1)

    // Only the linked PR may flip the linked issue, and only along the
    // open⇄closed lifecycle — see prStateTransitionAllowed.
    if (!current) return
    if (!prStateTransitionAllowed(current, { to, prUrl })) return

    await tx
      .update(issues)
      .set({ prState: to })
      .where(eq(issues.id, issueId))
  })
}
