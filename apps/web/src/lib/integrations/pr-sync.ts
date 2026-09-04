import { and, eq, inArray, isNull, ne, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  codingSessions,
  issueEvents,
  issues,
  issueStatuses,
  boards,
  repositories,
  teams,
} from "@/db/schema"
import {
  CATEGORY_ANCHOR,
  type IssueStatus,
  type IssueStatusCategory,
} from "@/lib/domain"
import { applyStatusDerivations } from "@/lib/status-derivations"
import { generateTxId } from "@/lib/trpc"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { fireAndForgetPrNotify } from "@/lib/integrations/notifications"
import { getSteerRelayConfig, relayPostKill } from "@/lib/steer"
import { notifyParentOfChildEnd } from "@/lib/steer-child-messages"
import {
  listOpenPullsByBase,
  retargetPullRequest,
} from "@/lib/integrations/github-pr"
import {
  githubAppConfigured,
  resolveRepoDefaultBranchCached,
  resolveRepoInstallationTokenInfo,
} from "@/lib/integrations/github-app"

// Extract `owner/repo` from a GitHub PR URL. Deliberately a duplicate of the
// identical helper in lib/trpc/issues.ts — importing the router from here
// would be circular, and several router tests mock this module's other
// import targets wholesale. Exported for codingSessions.mergePr (EXP-734).
export function repoFromPrUrl(prUrl: string): string | null {
  const match = prUrl.match(/github\.com\/([^/]+\/[^/]+)\/pull\/\d+/)
  return match ? match[1] : null
}

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
// the boards that repo backs — but that scope can still be ambiguous: the SAME
// GitHub repo may be registered by several teams (unique(team_id, full_name)),
// and board prefixes are not unique, so "APP-12" can exist in more than one
// candidate board (REV-22). Linking the wrong team's issue writes its PR
// fields, flips its status, and notifies its subscribers — so on ambiguity we
// REFUSE to link (return null) instead of picking an arbitrary row; the
// webhook payload carries no signal that could break the tie (one GitHub repo
// has one App installation, shared by every registering team). Returns null
// when the branch doesn't parse, the repo isn't registered, no linked board
// holds that identifier, or the identifier is ambiguous across the candidates.
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

  // limit(2): one row more than the unambiguous case needs — enough to detect
  // a collision without loading every match.
  const issueRows = await db
    .select({ id: issues.id })
    .from(issues)
    .where(
      and(inArray(issues.boardId, boardIds), eq(issues.identifier, identifier))
    )
    .limit(2)
  if (issueRows.length === 1) return issueRows[0].id
  if (issueRows.length > 1) {
    // Ambiguous — never guess (REV-22), but say so: a silently unlinked PR
    // reads as "automation broken" with nothing to go on.
    console.warn(
      `pr-sync: identifier ${identifier} on ${repoFullName} matches issues in multiple teams — refusing to link`
    )
    return null
  }

  // Second chance (EXP-57): the branch may predate a cross-board move that
  // renumbered the issue — identifiers are monotonic and never reused, so a
  // retired identifier matches nothing above. Every move records its retired
  // identifier in a board_moved event; match it TEAM-scoped, because
  // the move re-pointed the issue's events onto the TARGET board, which may
  // not be backed by this repo at all. Within one team a retired identifier
  // maps to exactly one issue (never reused), but the candidate teams can
  // collide on it just like the direct lookup above — distinct issue, or
  // refuse (REV-22).
  const teamIds = [...new Set(boardRows.map((p) => p.teamId))]
  const movedRows = await db
    .selectDistinct({ issueId: issueEvents.issueId })
    .from(issueEvents)
    .where(
      and(
        inArray(issueEvents.teamId, teamIds),
        eq(issueEvents.type, `board_moved`),
        sql`${issueEvents.payload} ->> 'fromIdentifier' = ${identifier}`
      )
    )
    .limit(2)
  if (movedRows.length > 1) {
    console.warn(
      `pr-sync: retired identifier ${identifier} on ${repoFullName} matches moved issues in multiple teams — refusing to link`
    )
  }
  return movedRows.length === 1 ? movedRows[0].issueId : null
}

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

// Coding-flow status transitions (EXP-120): an opened PR parks its issue in
// the team's PR-open target (default: the builtin In Review); a merged PR
// moves it to the PR-merge target (default: the builtin Done). Targets are
// per-team configurable, including "do nothing" (EXP-319). Explicit human
// resolutions (cancelled/duplicate) and already-reached targets are never
// overridden, so the webhook/cron/MCP writers stay idempotent.
const OPENED_FROM_STATUSES = new Set([`backlog`, `in_progress`])
const MERGED_FROM_STATUSES = new Set([
  `backlog`,
  `in_progress`,
  `in_review`,
])

export type PrAutomationEvent = `opened` | `merged`

// The builtin fallback target per event — used when a team has no explicit
// target (pristine state, or the configured status row was deleted and the
// FK's SET NULL fell back).
export const PR_AUTOMATION_DEFAULT_KEY: Record<
  PrAutomationEvent,
  `in_review` | `done`
> = {
  opened: `in_review`,
  merged: `done`,
}

// Pure decision core of the PR lifecycle automation (unit-tested in
// pr-sync.test.ts). `target` is the already-resolved issue_statuses row the
// team points this event at (configured row, else the builtin default row),
// or null when even the builtin row is missing (unseeded team) — then the
// write degrades to the bare anchor enum and the populate_issue_status_id
// trigger resolves status_id. Returns the status write to apply, or null to
// leave the issue untouched.
export function planPrAutomationTransition(opts: {
  event: PrAutomationEvent
  automationEnabled: boolean
  current: { status: string; statusId: string | null }
  target: {
    id: string
    builtinKey: string | null
    category: IssueStatusCategory
  } | null
}): { status: IssueStatus; statusId?: string } | null {
  if (!opts.automationEnabled) return null

  // Eligibility gates on the ANCHOR enum (EXP-314): an issue parked in a
  // CUSTOM started/unstarted status anchors into these sets and IS moved.
  // Explicit human resolutions (cancelled/duplicate anchors) stay
  // never-overridden, and the default targets can't re-trigger themselves
  // (in_review is not in the opened set, done not in the merged set).
  const eligibleFrom =
    opts.event === `merged` ? MERGED_FROM_STATUSES : OPENED_FROM_STATUSES
  if (!eligibleFrom.has(opts.current.status)) return null

  // Already parked in the exact target row → nothing to do. Matters for
  // custom targets whose anchor sits inside the from-set (e.g. a started
  // "In review (custom)" open target): repeated webhook deliveries must not
  // re-fire status_changed events.
  if (opts.target && opts.current.statusId === opts.target.id) return null

  const anchor = opts.target
    ? (opts.target.builtinKey ?? CATEGORY_ANCHOR[opts.target.category])
    : PR_AUTOMATION_DEFAULT_KEY[opts.event]
  return {
    status: anchor as IssueStatus,
    ...(opts.target ? { statusId: opts.target.id } : {}),
  }
}

// Shared in-transaction status writer for the PR lifecycle: flips the issue's
// status to the team's configured target for the event (stamping/clearing
// completedAt via the shared derivation) and records the status_changed
// activity event. No notification fan-out here — the pr_opened/pr_merged
// notifications already cover the same moment.
export async function applyPrLifecycleStatusInTx(
  tx: Tx,
  opts: {
    issueId: string
    teamId: string
    actorUserId: string | null
    currentStatus: string
    event: PrAutomationEvent
  }
): Promise<void> {
  if (opts.event === `opened`) {
    // The agent's PR is up — its live session enters review so every client
    // can show "ready for review" instead of "coding now" (EXP-194). Placed
    // BEFORE the eligibility gate: the session must flip even when a human
    // already parked the issue in `in_review`. running-conditioned so an
    // `ended` row is never resurrected. updatedAt stamped explicitly (no
    // $onUpdate on this table) so the review badge starts with a full
    // staleness window. Merge ENDS the session — see applyPrMergeState
    // (EXP-498; `ended` stays the desktop kill-switch). needsInput resets
    // with the flip (EXP-531): the flag belongs to the coding phase, and the
    // desktop's post-turn idle nudge must not leave a reviewed session
    // reading "Needs input".
    await tx
      .update(codingSessions)
      .set({ status: `in_review`, needsInput: false, updatedAt: new Date() })
      .where(
        and(
          eq(codingSessions.issueId, opts.issueId),
          eq(codingSessions.status, `running`)
        )
      )
  }

  // EXP-319: the team's automation config for this event. "Do nothing"
  // (automation=false) leaves the issue's status entirely alone — completedAt
  // then only ever comes from a manual status change; prState/prMergedAt,
  // notifications, and the session lifecycle above/in the callers are
  // deliberately NOT gated on it.
  const [teamConfig] = await tx
    .select({
      prOpenedStatusId: teams.prOpenedStatusId,
      prOpenedAutomation: teams.prOpenedAutomation,
      prMergedStatusId: teams.prMergedStatusId,
      prMergedAutomation: teams.prMergedAutomation,
    })
    .from(teams)
    .where(eq(teams.id, opts.teamId))
    .limit(1)
  const automationEnabled =
    (opts.event === `opened`
      ? teamConfig?.prOpenedAutomation
      : teamConfig?.prMergedAutomation) ?? true
  if (!automationEnabled) return

  // The precise from-row (for the event's name snapshot + completedAt
  // derivation) and the target row: the team's configured status, else the
  // builtin default for the event. The configured lookup is team-scoped as
  // defense in depth (setPrAutomation already rejects cross-team rows). A
  // missing target (unseeded team) leaves statusId to the
  // populate_issue_status_id trigger, which resolves to NULL — never an
  // error; clients fall back to the anchor.
  const [fromRow] = await tx
    .select({
      statusId: issues.statusId,
      duplicateOfId: issues.duplicateOfId,
      fromName: issueStatuses.name,
    })
    .from(issues)
    .leftJoin(issueStatuses, eq(issueStatuses.id, issues.statusId))
    .where(eq(issues.id, opts.issueId))
    .limit(1)

  const configuredStatusId =
    opts.event === `opened`
      ? teamConfig?.prOpenedStatusId
      : teamConfig?.prMergedStatusId
  const targetColumns = {
    id: issueStatuses.id,
    name: issueStatuses.name,
    builtinKey: issueStatuses.builtinKey,
    category: issueStatuses.category,
  }
  let target = configuredStatusId
    ? ((
        await tx
          .select(targetColumns)
          .from(issueStatuses)
          .where(
            and(
              eq(issueStatuses.id, configuredStatusId),
              eq(issueStatuses.teamId, opts.teamId)
            )
          )
          .limit(1)
      )[0] ?? null)
    : null
  if (!target) {
    target =
      (
        await tx
          .select(targetColumns)
          .from(issueStatuses)
          .where(
            and(
              eq(issueStatuses.teamId, opts.teamId),
              eq(issueStatuses.builtinKey, PR_AUTOMATION_DEFAULT_KEY[opts.event])
            )
          )
          .limit(1)
      )[0] ?? null
  }

  const plan = planPrAutomationTransition({
    event: opts.event,
    automationEnabled,
    current: {
      status: opts.currentStatus,
      statusId: fromRow?.statusId ?? null,
    },
    target,
  })
  if (!plan) return

  // completedAt/duplicate derivations ride the same shared rules as every
  // other status writer (EXP-319): a merge target in a non-completed
  // category must NOT stamp completedAt, and a redundant terminal write must
  // not clobber the original completion time.
  const setValues: Record<string, unknown> = { ...plan }
  applyStatusDerivations(setValues, {
    status: opts.currentStatus,
    duplicateOfId: fromRow?.duplicateOfId ?? null,
  })
  await tx.update(issues).set(setValues).where(eq(issues.id, opts.issueId))

  await recordIssueEvent(tx, {
    issueId: opts.issueId,
    teamId: opts.teamId,
    actorUserId: opts.actorUserId,
    type: `status_changed`,
    payload: {
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
  // EXP-494: the actor came from an agent's MCP credential (claim/tool) —
  // forwarded to the notify so shared-server host attribution can swap to
  // the session's requester.
  actorViaAgent?: boolean
  // EXP-617: the webhook's `sender`/`pull_request.user` already mapped to an
  // app user. Notification-only — it never touches the PR linkage or the
  // status automation, whose actor stays the in-app one.
  githubActorUserId?: string | null
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

    // The link write is the atomic claim (REV-48): the read above is a plain
    // snapshot, so two concurrent deliveries (webhook redelivery racing the
    // original, or webhook + MCP open_pr) can both pass it — only the writer
    // that actually flips prUrl from NULL fires the event/notify below.
    const claimed = await tx
      .update(issues)
      .set({
        prUrl: opts.prUrl,
        prNumber: opts.prNumber,
        prState: `open`,
        branch: opts.branch,
      })
      .where(and(eq(issues.id, opts.issueId), isNull(issues.prUrl)))
      .returning({ id: issues.id })
    if (claimed.length === 0) return false

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

    // An open PR moves the issue to the team's PR-open target (EXP-120;
    // default In Review, per-team configurable since EXP-319).
    await applyPrLifecycleStatusInTx(tx, {
      issueId: opts.issueId,
      teamId: current.teamId,
      actorUserId: opts.actorUserId ?? null,
      currentStatus: current.status,
      event: `opened`,
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
      actorViaAgent: opts.actorViaAgent,
      githubActorUserId: opts.githubActorUserId ?? null,
    })
  }
}

// Shared PR-merge writer, callable outside tRPC (webhook + self-hosted cron).
// Applies the open→merged write semantics: flips prState to 'merged', stamps
// prMergedAt, and emits a single pr_merged activity event.
//
// Idempotent on the open→merged transition, enforced ATOMICALLY (REV-48): the
// transition UPDATE itself carries the `prState <> 'merged'` claim, so two
// concurrent invocations (webhook redelivery racing the original delivery, or
// webhook + the GITHUB_POLLING outbound cron) can never both fire the
// pr_merged event, the status flip, or the notify — only the writer whose
// UPDATE actually flipped the row proceeds.
export async function applyPrMergeState(opts: {
  issueId: string
  prUrl?: string
  // Backfill sources for an issue whose PR was never linked (REV-26): a merge
  // resolved via the branch-parse fallback lands on prUrl=null, and without
  // these the row would show prState='merged' with no PR link anywhere.
  prNumber?: number
  headBranch?: string
  mergedAt?: Date | null
  actorUserId?: string | null
  // EXP-494/EXP-617: see applyPrOpenedState. On this path the GitHub actor is
  // `merged_by` — whoever pressed Merge on github.com.
  actorViaAgent?: boolean
  githubActorUserId?: string | null
  // EXP-711: per-merge override of the team's `end_sessions_on_merge`
  // setting (MCP `pr_merge({ endSessions })`, carried to the webhook echo by
  // the merge claim). Unset = the team setting decides.
  endSessions?: boolean
}): Promise<void> {
  const result = await db.transaction(
    async (
      tx
    ): Promise<{
      applied: boolean
      prUrl?: string | null
      headBranch?: string | null
      endedSessionIds?: string[]
    }> => {
      const txId = await generateTxId(tx)
      void txId

      const [current] = await tx
        .select({
          prState: issues.prState,
          prUrl: issues.prUrl,
          branch: issues.branch,
          status: issues.status,
          teamId: boards.teamId,
          endSessionsOnMerge: teams.endSessionsOnMerge,
        })
        .from(issues)
        .innerJoin(boards, eq(boards.id, issues.boardId))
        .innerJoin(teams, eq(teams.id, boards.teamId))
        .where(eq(issues.id, opts.issueId))
        .limit(1)

      // Unknown issue, already merged, or a different (unlinked) PR → nothing
      // to do (idempotent; see prStateTransitionAllowed).
      if (!current) return { applied: false }
      if (
        !prStateTransitionAllowed(current, { to: `merged`, prUrl: opts.prUrl })
      ) {
        return { applied: false }
      }

      // Never-linked issue (branch-parse fallback whose `opened` webhook was
      // lost): backfill the PR linkage alongside the merge so the row doesn't
      // end up 'merged' with no PR link (REV-26). Guarded by the WHERE below,
      // so a concurrently-linked DIFFERENT PR can never be overwritten.
      const backfill =
        current.prUrl === null && opts.prUrl
          ? {
              prUrl: opts.prUrl,
              ...(opts.prNumber != null ? { prNumber: opts.prNumber } : {}),
              ...(opts.headBranch ? { branch: opts.headBranch } : {}),
            }
          : {}

      // Atomic restatement of prStateTransitionAllowed(to: 'merged'): the
      // WHERE is the claim — a concurrent writer that committed first leaves
      // zero rows here, and everything below is gated on that (REV-48).
      const claimed = await tx
        .update(issues)
        .set({
          prState: `merged`,
          prMergedAt: opts.mergedAt ?? new Date(),
          ...backfill,
        })
        .where(
          and(
            eq(issues.id, opts.issueId),
            or(isNull(issues.prState), ne(issues.prState, `merged`)),
            ...(opts.prUrl
              ? [or(isNull(issues.prUrl), eq(issues.prUrl, opts.prUrl))]
              : [])
          )
        )
        .returning({ id: issues.id })
      if (claimed.length === 0) return { applied: false }

      await recordIssueEvent(tx, {
        issueId: opts.issueId,
        teamId: current.teamId,
        actorUserId: opts.actorUserId ?? null,
        type: `pr_merged`,
        payload: { prUrl: opts.prUrl ?? current.prUrl ?? null },
      })

      // The merged PR moves the issue to the team's PR-merge target
      // (EXP-120: default in_review → done; per-team configurable since
      // EXP-319, including "do nothing").
      await applyPrLifecycleStatusInTx(tx, {
        issueId: opts.issueId,
        teamId: current.teamId,
        actorUserId: opts.actorUserId ?? null,
        currentStatus: current.status,
        event: `merged`,
      })

      // EXP-498 (reversing EXP-358): the merge ENDS the issue's live coding
      // sessions on every path — webhook, poller, and mergePr all funnel
      // through this claim, so the winner sweeping here means merge never
      // leaves a session running. The desktop reads the →ended edge as its
      // kill switch; the relay kill fires post-commit below. Independent of
      // the issue-status eligibility gate above.
      // EXP-711: unless the team switched that off (`end_sessions_on_merge`)
      // or this merge overrides the setting either way.
      const endSessions = opts.endSessions ?? current.endSessionsOnMerge
      const endedSessionIds = endSessions
        ? await endLiveIssueSessionsInTx(tx, opts.issueId)
        : []

      return {
        applied: true,
        prUrl: opts.prUrl ?? current.prUrl,
        headBranch: current.branch ?? opts.headBranch ?? null,
        endedSessionIds,
      }
    }
  )

  // Inside the open→merged idempotent guard: the webhook and the self-hosted
  // outbound cron can both call this, but only the transition that actually
  // flipped the state fans out — so an away phone gets exactly one
  // "it's merged" notification on in-app + push + email.
  if (result.applied) {
    // Post-commit: the durable teardown signal is the synced →ended flip
    // above; the relay kill only makes the live terminal/mirror teardown
    // immediate (a pre-commit kill would race the desktop re-reading a
    // still-running row).
    await tearDownEndedSessions(result.endedSessionIds ?? [])
    fireAndForgetPrNotify({
      issueId: opts.issueId,
      type: `pr_merged`,
      actorUserId: opts.actorUserId ?? null,
      actorViaAgent: opts.actorViaAgent,
      githubActorUserId: opts.githubActorUserId ?? null,
    })
    // EXP-324: heal the stack — retarget open child PRs that were based on
    // the just-merged head branch. GitHub only does this itself when the
    // base branch is DELETED; we squash-merge and leave it, so the children
    // would keep pointing at a dead branch (the EXP-320 incident).
    // Fire-and-forget: never blocks the webhook response or the caller.
    if (result.prUrl && result.headBranch) {
      void retargetChildrenOfMergedPr({
        prUrl: result.prUrl,
        headBranch: result.headBranch,
      }).catch((err) => {
        console.error(`retargetChildrenOfMergedPr failed:`, err)
      })
    }
  }
}

// The EXP-498 in-tx merge sweep: end every live session on the issue whose
// PR just merged. updatedAt stamped explicitly (no
// $onUpdate on this table). Batch (issue-less) session rows can't be matched
// by issue_id — the desktop self-closes those when its branch's issues sync
// a merged PR. Returns the ended row ids for the caller's POST-commit relay
// kill.
// EXP-637 decision 6: a session that merged its OWN PR (via the MCP
// `exponential_pr_merge` tool with its session header) is spared — it ends
// through `exponential_sessions_end` or its own exit instead, so an agent
// that merges and then keeps working isn't killed by its own success. The
// spare is a durable column, not an in-memory claim, so the webhook and the
// outbound poller honour it too.
export async function endLiveIssueSessionsInTx(
  tx: Tx,
  issueId: string
): Promise<string[]> {
  const ended = await tx
    .update(codingSessions)
    .set({
      status: `ended`,
      endedAt: new Date(),
      endedBy: `merge`,
      updatedAt: new Date(),
    })
    .where(
      and(
        eq(codingSessions.issueId, issueId),
        inArray(codingSessions.status, [`running`, `in_review`]),
        eq(codingSessions.mergedOwnPr, false)
      )
    )
    .returning({ id: codingSessions.id })
  return ended.map((s) => s.id)
}

// Best-effort teardown for just-ended sessions — the durable signal is the
// synced →ended row flip; this only makes the live terminal/mirror teardown
// immediate (relayPostKill never throws). Post-commit only.
//
// EXP-700: a merge-ended run may be an agent-started CHILD, and its parent is
// blocked waiting for a report it will now never send (`sessions_end` never
// ran). Tell the parent the child ended without one — `notifyParentOfChildEnd`
// no-ops for every row that is not agent-started with a live linked parent,
// and is internally caught + relay-timeout bounded, so this stays best-effort.
async function tearDownEndedSessions(sessionIds: string[]): Promise<void> {
  if (sessionIds.length === 0) return
  // Both halves ride the relay, so an instance without one skips the lookups
  // entirely rather than reading a parent it could never reach.
  const config = getSteerRelayConfig()
  if (!config) return
  await Promise.all(
    sessionIds.map((id) =>
      notifyParentOfChildEnd(db, id, { summary: null, endedBy: `merge` })
    )
  )
  await Promise.all(sessionIds.map((id) => relayPostKill(config, id)))
}

// EXP-711: the issues among `issueIds` whose team still ends sessions on
// merge — what the standalone sweeps fall back to when no per-merge override
// says otherwise.
async function issuesEndingSessionsOnMerge(
  issueIds: string[]
): Promise<string[]> {
  const rows = await db
    .select({ id: issues.id })
    .from(issues)
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .innerJoin(teams, eq(teams.id, boards.teamId))
    .where(
      and(inArray(issues.id, issueIds), eq(teams.endSessionsOnMerge, true))
    )
  return rows.map((row) => row.id)
}

// Idempotent belt-and-braces sweep (EXP-498): end every live session on the
// given issues whose PR already merged. The claim winner inside
// applyPrMergeState normally ends them in-tx; this catches the paths that
// lose the claim (issues.mergePr racing the webhook). Safe to call
// repeatedly — matched statuses exclude `ended`. `endSessions` is the
// EXP-711 per-merge override: false sweeps nothing, true sweeps regardless
// of the team setting, unset lets each issue's team decide.
export async function endMergedPrSessions(
  issueIds: string[],
  endSessions?: boolean
): Promise<void> {
  if (issueIds.length === 0 || endSessions === false) return
  const targetIds =
    endSessions === true
      ? issueIds
      : await issuesEndingSessionsOnMerge(issueIds)
  if (targetIds.length === 0) return
  const endedSessionIds = await db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    void txId
    const ended = await tx
      .update(codingSessions)
      .set({
        status: `ended`,
        endedAt: new Date(),
        endedBy: `merge`,
        updatedAt: new Date(),
      })
      .where(
        and(
          inArray(codingSessions.issueId, targetIds),
          inArray(codingSessions.status, [`running`, `in_review`]),
          eq(codingSessions.mergedOwnPr, false)
        )
      )
      .returning({ id: codingSessions.id })
    return ended.map((s) => s.id)
  })

  await tearDownEndedSessions(endedSessionIds)
}

// EXP-637/EXP-626: the issue-LESS counterpart. A chore PR opened with
// `exponential_pr_open({ repositoryId, head })` links no issue, so nothing
// above can find its session — the only handle is the branch recorded on the
// row. When a merge webhook resolves to no issues at all, end the live
// sessions of the teams that registered that repo and whose row sits on the
// merged head branch. Same `merged_own_pr` spare as every other merge path:
// the session that merged its own chore PR keeps running. EXP-711: teams
// that switched merge-ends-sessions off are skipped unless `endSessions`
// (the merge claim's per-call override) forces it either way.
export async function endSessionsOnMergedBranch(
  repoFullName: string,
  headBranch: string,
  endSessions?: boolean
): Promise<void> {
  if (!repoFullName || !headBranch || endSessions === false) return
  const teamRows = await db
    .select({ teamId: repositories.teamId })
    .from(repositories)
    .innerJoin(teams, eq(teams.id, repositories.teamId))
    .where(
      and(
        eq(repositories.fullName, repoFullName),
        ...(endSessions === true ? [] : [eq(teams.endSessionsOnMerge, true)])
      )
    )
  const teamIds = [...new Set(teamRows.map((r) => r.teamId))]
  if (teamIds.length === 0) return

  const endedSessionIds = await db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    void txId
    const ended = await tx
      .update(codingSessions)
      .set({
        status: `ended`,
        endedAt: new Date(),
        endedBy: `merge`,
        updatedAt: new Date(),
      })
      .where(
        and(
          isNull(codingSessions.issueId),
          eq(codingSessions.branch, headBranch),
          inArray(codingSessions.teamId, teamIds),
          inArray(codingSessions.status, [`running`, `in_review`]),
          eq(codingSessions.mergedOwnPr, false)
        )
      )
      .returning({ id: codingSessions.id })
    return ended.map((s) => s.id)
  })

  await tearDownEndedSessions(endedSessionIds)
}

// EXP-734: the session-PR state writer. An action/chat run's chore PR lives
// on its `coding_sessions` row (`pr_url/pr_number/pr_state`, stamped by the
// MCP pr_open chore path) — the issue-side writers above never see it. This
// is the ONE place that keeps the row in step with GitHub on every path (the
// shared merge helper, the webhook's closed/reopened legs, the self-hosted
// poller): flip `pr_state` along the same open⇄closed / →merged lifecycle
// as `applyPrStateFlip` (merged is terminal, so a racing duplicate degrades
// to a no-op), and on a merge END the live runs sitting on the PR like every
// other merge path does — the team's `endSessionsOnMerge` decides unless the
// merger's per-call `endSessions` override (EXP-711) says otherwise, and the
// `merged_own_pr` spare keeps the run that merged its own PR alive. Only
// issue-less rows are addressed: an issue run's PR state is the issue's.
export async function applySessionPrState(opts: {
  prUrl: string
  state: `open` | `closed` | `merged`
  endSessions?: boolean
}): Promise<{ endedSessionIds: string[] }> {
  if (!opts.prUrl) return { endedSessionIds: [] }
  const endedSessionIds = await db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    void txId

    const fromState =
      opts.state === `merged`
        ? or(isNull(codingSessions.prState), ne(codingSessions.prState, `merged`))
        : eq(codingSessions.prState, opts.state === `closed` ? `open` : `closed`)
    await tx
      .update(codingSessions)
      .set({ prState: opts.state, updatedAt: new Date() })
      .where(
        and(
          eq(codingSessions.prUrl, opts.prUrl),
          isNull(codingSessions.issueId),
          fromState
        )
      )
      .returning({ id: codingSessions.id })

    if (opts.state !== `merged` || opts.endSessions === false) return []
    const live = await tx
      .select({ id: codingSessions.id })
      .from(codingSessions)
      .innerJoin(teams, eq(teams.id, codingSessions.teamId))
      .where(
        and(
          eq(codingSessions.prUrl, opts.prUrl),
          isNull(codingSessions.issueId),
          inArray(codingSessions.status, [`running`, `in_review`]),
          eq(codingSessions.mergedOwnPr, false),
          ...(opts.endSessions === true
            ? []
            : [eq(teams.endSessionsOnMerge, true)])
        )
      )
    const targetIds = live.map((row) => row.id)
    if (targetIds.length === 0) return []
    const ended = await tx
      .update(codingSessions)
      .set({
        status: `ended`,
        endedAt: new Date(),
        endedBy: `merge`,
        updatedAt: new Date(),
      })
      .where(
        and(
          inArray(codingSessions.id, targetIds),
          inArray(codingSessions.status, [`running`, `in_review`])
        )
      )
      .returning({ id: codingSessions.id })
    return ended.map((row) => row.id)
  })

  await tearDownEndedSessions(endedSessionIds)
  return { endedSessionIds }
}

// Retarget every open PR based on a just-merged PR's head branch onto the
// repo's default branch (EXP-324). Exported for direct tests; called
// fire-and-forget from applyPrMergeState, so it must never throw for one
// child. Best-effort by design — every bail-out is silent (a self-hosted
// instance without the App, an unreachable repo, …) and the fix-conflicts /
// merge paths re-diagnose the base live anyway. Idempotent by construction:
// once retargeted, a child no longer matches the base filter.
export async function retargetChildrenOfMergedPr(opts: {
  prUrl: string
  headBranch: string
}): Promise<void> {
  const repo = repoFromPrUrl(opts.prUrl)
  if (!repo || !opts.headBranch) return
  if (!githubAppConfigured()) return
  // Override-first (EXP-462): children retarget onto the branch the team
  // actually develops on. The repo row is reached through the merged PR's
  // linked issues (prUrl → board → team) — same-team by construction; a PR
  // with no linked row (external PR, board since retargeted) falls back to
  // GitHub's live default. Archived rows still count: this read only wants
  // the team's pin + raw default, and an archived pin beats falling back to
  // GitHub's raw default (EXP-466). Duplicated inline rather than importing
  // the repositories router (see repoFromPrUrl above for why).
  const [linkedRepoRow] = await db
    .select({
      defaultBranch: repositories.defaultBranch,
      defaultBranchOverride: repositories.defaultBranchOverride,
      // EXP-712: the linked issue's board may develop on its own branch.
      boardDefaultBranch: boards.defaultBranch,
    })
    .from(issues)
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .innerJoin(
      repositories,
      and(
        eq(repositories.teamId, boards.teamId),
        eq(repositories.fullName, repo)
      )
    )
    .where(eq(issues.prUrl, opts.prUrl))
    .limit(1)
  const defaultBranch =
    linkedRepoRow?.boardDefaultBranch ??
    linkedRepoRow?.defaultBranchOverride ??
    (await resolveRepoDefaultBranchCached(repo))
  if (!defaultBranch) return
  // Load-bearing guard: a default-based PR's "children" would be every other
  // default-based PR in the repo.
  if (opts.headBranch === defaultBranch) return
  // EXP-466: with an override pinned, the effective compare above no longer
  // catches a head equal to the RAW GitHub default — and sweeping every
  // raw-default-based PR (prod/hotfix PRs) onto the pin would be the same
  // disaster. Guard on the stored raw default too.
  if (linkedRepoRow && opts.headBranch === linkedRepoRow.defaultBranch) return
  // EXP-712: a board pin shadows the team override in the effective compare,
  // so a head equal to the OVERRIDE needs its own guard for the same reason.
  if (
    linkedRepoRow?.defaultBranchOverride &&
    opts.headBranch === linkedRepoRow.defaultBranchOverride
  )
    return
  const resolved = await resolveRepoInstallationTokenInfo(repo)
  if (!resolved) return

  const children = await listOpenPullsByBase(
    repo,
    opts.headBranch,
    resolved.token
  )
  for (const child of children) {
    try {
      await retargetPullRequest({
        repo,
        prNumber: child.number,
        base: defaultBranch,
        token: resolved.token,
      })
    } catch (err) {
      // One unreachable child never blocks the rest.
      console.error(
        `retarget of ${repo}#${child.number} onto ${defaultBranch} failed:`,
        err
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

    // Only the linked PR may flip the linked issue, and only along the
    // open⇄closed lifecycle — the WHERE is prStateTransitionAllowed stated
    // atomically (REV-48), so a racing concurrent flip degrades to a no-op
    // instead of re-applying. (A never-linked issue has prState=null and can
    // never match the from-state, so no backfill question arises here.)
    const from = to === `closed` ? `open` : `closed`
    await tx
      .update(issues)
      .set({ prState: to })
      .where(
        and(
          eq(issues.id, issueId),
          eq(issues.prState, from),
          ...(prUrl
            ? [or(isNull(issues.prUrl), eq(issues.prUrl, prUrl))]
            : [])
        )
      )
  })
}
