import { and, eq, inArray, sql } from "drizzle-orm"
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
// import targets wholesale.
function repoFromPrUrl(prUrl: string): string | null {
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
  if (issueRows.length > 1) return null // ambiguous — never guess (REV-22)

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
  return movedRows.length === 1 ? movedRows[0].issueId : null
}

type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

// Coding-flow status transitions (EXP-120): an opened PR parks its issue in
// the team's PR-open target (default: the builtin In Review); a merged PR
// moves it to the PR-merge target (default: the builtin Done). Targets are
// per-team configurable, including "do nothing" (EXP-319). Explicit human
// resolutions (cancelled/duplicate) and already-reached targets are never
// overridden, so the webhook/cron/MCP writers stay idempotent.
const OPENED_FROM_STATUSES = new Set([`backlog`, `todo`, `in_progress`])
const MERGED_FROM_STATUSES = new Set([
  `backlog`,
  `todo`,
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
    // staleness window. Merge moves the session to `merged` — see
    // applyPrMergeState (EXP-358): only an explicit "Merge and close" /
    // kill ends it (`ended` stays the desktop kill-switch).
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
      from: opts.currentStatus,
      to: plan.status,
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
  // EXP-494: see applyPrOpenedState.
  actorViaAgent?: boolean
}): Promise<void> {
  const result = await db.transaction(
    async (
      tx
    ): Promise<{
      applied: boolean
      prUrl?: string | null
      headBranch?: string | null
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
        })
        .from(issues)
        .innerJoin(boards, eq(boards.id, issues.boardId))
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

      // EXP-358: the merge parks the issue's live coding session in `merged`
      // instead of killing it — the terminal stays open and steerable (the
      // desktop kill-switch fires only on the →ended edge, which now comes
      // solely from explicit ends: killSession, codingSessions.end, or
      // mergePr's closeSessions opt-in via endMergedPrSessions below).
      // Independent of the issue-status eligibility gate above. updatedAt
      // stamped explicitly (no $onUpdate on this table) so the badge starts
      // with a full staleness window. Batch (issue-less) session rows can't
      // be matched by issue_id and are left to their terminal exit.
      await tx
        .update(codingSessions)
        .set({ status: `merged`, updatedAt: new Date() })
        .where(
          and(
            eq(codingSessions.issueId, opts.issueId),
            inArray(codingSessions.status, [`running`, `in_review`])
          )
        )

      return {
        applied: true,
        prUrl: opts.prUrl ?? current.prUrl,
        headBranch: current.branch,
      }
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
      actorViaAgent: opts.actorViaAgent,
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

// The "Merge and close" opt-in (EXP-358): explicitly end every live session
// on the given issues after their PR merged. Matches `merged` too — the
// webhook usually parks the rows in `merged` before the user's mutation gets
// here, and a second click must still tear them down. Called by
// issues.mergePr({closeSessions:true}) only; the plain merge paths (webhook,
// poller, MCP tool) never end sessions anymore.
export async function endMergedPrSessions(issueIds: string[]): Promise<void> {
  if (issueIds.length === 0) return
  const endedSessionIds = await db.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    void txId
    const ended = await tx
      .update(codingSessions)
      .set({ status: `ended`, endedAt: new Date(), updatedAt: new Date() })
      .where(
        and(
          inArray(codingSessions.issueId, issueIds),
          inArray(codingSessions.status, [`running`, `in_review`, `merged`])
        )
      )
      .returning({ id: codingSessions.id })
    return ended.map((s) => s.id)
  })

  // Best-effort relay kills for the sessions just ended — the durable signal
  // is the synced row flip above; this only makes the live terminal/mirror
  // teardown immediate (relayPostKill never throws).
  if (endedSessionIds.length > 0) {
    const config = getSteerRelayConfig()
    if (config) {
      await Promise.all(endedSessionIds.map((id) => relayPostKill(config, id)))
    }
  }
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
