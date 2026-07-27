import { and, desc, eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  codingSessions,
  issueEvents,
  issues,
  issueStatuses,
  boards,
  repositories,
} from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { fireAndForgetPrNotify } from "@/lib/integrations/notifications"
import { getSteerRelayConfig, relayPostKill } from "@/lib/steer"

// Parse a team issue identifier ("MET-12") out of a PR head-branch name.
// Matches the launcher's `exp/<IDENTIFIER>` convention and any custom prefix
// (e.g. `feature/MET-12`) by anchoring on the trailing `<IDENT>-<number>` tail.
// Pure — unit-tested in pr-sync.test.ts.
export function parseIssueIdentifierFromBranch(branch: string): string | null {
  const match = branch.match(/(?:^|\/)([A-Z0-9]+-\d+)$/)
  return match ? match[1] : null
}

// Resolve an issue by (repo full name + head branch), for the webhook's
// deterministic branch-based linking. The repo scopes the identifier lookup to
// the boards that repo backs, so identical identifiers in other teams
// never cross-match. Returns null when the branch doesn't parse, the repo isn't
// registered, or no linked board holds that identifier.
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

  // Boards backed by any of these repos (v4: boards.repositoryId).
  const boardRows = await db
    .select({ boardId: boards.id, teamId: boards.teamId })
    .from(boards)
    .where(
      inArray(
        boards.repositoryId,
        repoRows.map((r) => r.id)
      )
    )
  const boardIds = [...new Set(boardRows.map((p) => p.boardId))]
  if (boardIds.length === 0) return null

  const [issue] = await db
    .select({ id: issues.id })
    .from(issues)
    .where(
      and(inArray(issues.boardId, boardIds), eq(issues.identifier, identifier))
    )
    .limit(1)
  if (issue) return issue.id

  // Second chance (EXP-57): the branch may predate a cross-board move that
  // renumbered the issue — identifiers are monotonic and never reused, so a
  // retired identifier matches nothing above. Every move records its retired
  // identifier in a board_moved event; match it TEAM-scoped, because
  // the move re-pointed the issue's events onto the TARGET board, which may
  // not be backed by this repo at all. Latest event wins (a chain of moves
  // still resolves each retired identifier to the same issue exactly once).
  const teamIds = [...new Set(boardRows.map((p) => p.teamId))]
  const [movedEvent] = await db
    .select({ issueId: issueEvents.issueId })
    .from(issueEvents)
    .where(
      and(
        inArray(issueEvents.teamId, teamIds),
        eq(issueEvents.type, `board_moved`),
        sql`${issueEvents.payload} ->> 'fromIdentifier' = ${identifier}`
      )
    )
    .orderBy(desc(issueEvents.createdAt))
    .limit(1)
  return movedEvent?.issueId ?? null
}

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

// Coding-flow status transitions (EXP-120): an opened PR parks its issue in
// `in_review`; a merged PR completes it (`done`). Explicit human resolutions
// (cancelled/duplicate) and already-reached targets are never overridden, so
// the webhook/cron/MCP writers stay idempotent.
const IN_REVIEW_FROM_STATUSES = new Set([`backlog`, `todo`, `in_progress`])
const DONE_FROM_STATUSES = new Set([
  `backlog`,
  `todo`,
  `in_progress`,
  `in_review`,
])

// Shared in-transaction status writer for the PR lifecycle: flips the issue's
// status (stamping/clearing completedAt) and records the status_changed
// activity event. No notification fan-out here — the pr_opened/pr_merged
// notifications already cover the same moment.
export async function applyPrLifecycleStatusInTx(
  tx: Tx,
  opts: {
    issueId: string
    teamId: string
    actorUserId: string | null
    currentStatus: string
    to: `in_review` | `done`
  }
): Promise<void> {
  if (opts.to === `in_review`) {
    // The agent's PR is up — its live session enters review so every client
    // can show "ready for review" instead of "coding now" (EXP-194). Placed
    // BEFORE the eligibility gate: the session must flip even when a human
    // already parked the issue in `in_review`. running-conditioned so an
    // `ended` row is never resurrected. updatedAt stamped explicitly (no
    // $onUpdate on this table) so the review badge starts with a full
    // staleness window. Merge (`done`) ends the session — see
    // applyPrMergeState (EXP-268): the ended flip is the desktop
    // kill-switch, which on merge is exactly the wanted teardown.
    await tx
      .update(codingSessions)
      .set({ status: `in_review`, updatedAt: new Date() })
      .where(
        and(
          eq(codingSessions.issueId, opts.issueId),
          eq(codingSessions.status, `running`)
        )
      )
  }

  // Eligibility gates on the ANCHOR enum (EXP-314): an issue parked in a
  // CUSTOM started/unstarted status anchors into these sets and IS flipped —
  // PR automation deliberately exits custom statuses into the team's builtin
  // In Review/Done (per-status automation targets are v2). Explicit human
  // resolutions (cancelled/duplicate anchors) stay never-overridden.
  const eligible =
    opts.to === `done` ? DONE_FROM_STATUSES : IN_REVIEW_FROM_STATUSES
  if (!eligible.has(opts.currentStatus)) return

  // The precise from-row (for the event's name snapshot) and the builtin
  // target row. A missing target (unseeded team) leaves statusId to the
  // populate_issue_status_id trigger, which resolves to NULL — never an
  // error; clients fall back to the anchor.
  const [fromRow] = await tx
    .select({ statusId: issues.statusId, fromName: issueStatuses.name })
    .from(issues)
    .leftJoin(issueStatuses, eq(issueStatuses.id, issues.statusId))
    .where(eq(issues.id, opts.issueId))
    .limit(1)
  const [target] = await tx
    .select({ id: issueStatuses.id, name: issueStatuses.name })
    .from(issueStatuses)
    .where(
      and(
        eq(issueStatuses.teamId, opts.teamId),
        eq(issueStatuses.builtinKey, opts.to)
      )
    )
    .limit(1)

  await tx
    .update(issues)
    .set({
      status: opts.to,
      ...(target ? { statusId: target.id } : {}),
      completedAt: opts.to === `done` ? new Date() : null,
    })
    .where(eq(issues.id, opts.issueId))

  await recordIssueEvent(tx, {
    issueId: opts.issueId,
    teamId: opts.teamId,
    actorUserId: opts.actorUserId,
    type: `status_changed`,
    payload: {
      from: opts.currentStatus,
      to: opts.to,
      fromStatusId: fromRow?.statusId ?? null,
      toStatusId: target?.id ?? null,
      fromName: fromRow?.fromName ?? null,
      toName: target?.name ?? null,
    },
  })
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
      .select({
        prUrl: issues.prUrl,
        status: issues.status,
        teamId: boards.teamId,
      })
      .from(issues)
      .innerJoin(boards, eq(boards.id, issues.boardId))
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
      teamId: current.teamId,
      actorUserId: opts.actorUserId ?? null,
      type: `pr_opened`,
      payload: {
        prUrl: opts.prUrl,
        prNumber: opts.prNumber,
        branch: opts.branch,
      },
    })

    // An open PR moves the issue into review (EXP-120).
    await applyPrLifecycleStatusInTx(tx, {
      issueId: opts.issueId,
      teamId: current.teamId,
      actorUserId: opts.actorUserId ?? null,
      currentStatus: current.status,
      to: `in_review`,
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
  const result = await db.transaction(
    async (tx): Promise<{ applied: boolean; endedSessionIds: string[] }> => {
      const txId = await generateTxId(tx)
      void txId

      const [current] = await tx
        .select({
          prState: issues.prState,
          prUrl: issues.prUrl,
          status: issues.status,
          teamId: boards.teamId,
        })
        .from(issues)
        .innerJoin(boards, eq(boards.id, issues.boardId))
        .where(eq(issues.id, opts.issueId))
        .limit(1)

      // Unknown issue, already merged, or a different (unlinked) PR → nothing
      // to do (idempotent; see prStateTransitionAllowed).
      if (!current) return { applied: false, endedSessionIds: [] }
      if (
        !prStateTransitionAllowed(current, { to: `merged`, prUrl: opts.prUrl })
      ) {
        return { applied: false, endedSessionIds: [] }
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
        teamId: current.teamId,
        actorUserId: opts.actorUserId ?? null,
        type: `pr_merged`,
        payload: { prUrl: opts.prUrl ?? current.prUrl ?? null },
      })

      // The merged PR completes the issue (EXP-120: in_review → done).
      await applyPrLifecycleStatusInTx(tx, {
        issueId: opts.issueId,
        teamId: current.teamId,
        actorUserId: opts.actorUserId ?? null,
        currentStatus: current.status,
        to: `done`,
      })

      // EXP-268: the merge ends the issue's live coding session. The ended
      // flip IS the desktop kill-switch (the desktop kills the agent and
      // closes the terminal tab on its own row's →ended edge — on merge
      // that teardown is exactly what we want), and the post-commit relay
      // kill below tears live mirrors down immediately. Independent of the
      // issue-status eligibility gate above. Batch (issue-less) session rows
      // can't be matched by issue_id and are left to their terminal exit.
      const endedSessions = await tx
        .update(codingSessions)
        .set({ status: `ended`, endedAt: new Date(), updatedAt: new Date() })
        .where(
          and(
            eq(codingSessions.issueId, opts.issueId),
            inArray(codingSessions.status, [`running`, `in_review`])
          )
        )
        .returning({ id: codingSessions.id })

      return { applied: true, endedSessionIds: endedSessions.map((s) => s.id) }
    }
  )

  // Inside the open→merged idempotent guard: the webhook and the self-hosted
  // outbound cron can both call this, but only the transition that actually
  // flipped the state fans out — so an away phone gets exactly one
  // "it's merged" notification on in-app + push + email.
  if (result.applied) {
    fireAndForgetPrNotify({
      issueId: opts.issueId,
      type: `pr_merged`,
      actorUserId: opts.actorUserId ?? null,
    })
  }

  // Best-effort relay kills for the sessions the merge just ended — the
  // durable signal is the synced row flip above; this only makes the live
  // terminal/mirror teardown immediate (relayPostKill never throws).
  if (result.endedSessionIds.length > 0) {
    const config = getSteerRelayConfig()
    if (config) {
      await Promise.all(
        result.endedSessionIds.map((id) => relayPostKill(config, id))
      )
    }
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
