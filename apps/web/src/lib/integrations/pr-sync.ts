import { and, desc, eq, inArray, isNull, lte, ne, or, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  boards,
  issueEvents,
  issueRelations,
  issues,
  issueStatuses,
  pullRequestIssues,
  pullRequests,
  repositories,
  sessionIssues,
  sessions,
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
import { emitEventInTx } from "@/lib/events"
import type { IssueEventOrigin } from "@/lib/integrations/event-origin"
import { syncDuplicateMirror } from "@/lib/issue-relations"
import { fireAndForgetPrNotify } from "@/lib/integrations/notifications"
import { BUS_COPY, busLine } from "@/lib/sessions/bus"
import { postSystemCard } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import {
  deleteBranch,
  fetchPullStack,
  listOpenPullsByBase,
  retargetPullRequest,
} from "@/lib/integrations/github-pr"
import {
  githubAppConfigured,
  resolveRepoDefaultBranchCached,
  resolveRepoInstallationTokenInfo,
} from "@/lib/integrations/github-app"
import {
  isRepoDefaultBranch,
  LIVE_PR_STATES,
  loadRepoDefaultBranches,
} from "@/lib/pr-merge-guard"

// EXP-1291: the PR model's state writer. ALL PR and branch state lives on
// `pull_requests` (+ `pull_request_issues`, the issues a PR closes); `issues`
// carries none. Every path that learns a PR's state (pr_open, the merge and
// close paths, the GitHub webhook, the GITHUB_POLLING poller) funnels
// through here, so the linked issues get the team's PR automation, the
// pr_opened/pr_merged issue events, the notifications and the thread's
// event card exactly once (each transition is an atomic claim, REV-48).
// Merging NEVER ends a session (EXP-1291 dropped EXP-498's merge sweep).

export type PullRequestRow = typeof pullRequests.$inferSelect

/** `owner/repo` out of a GitHub PR URL. */
export function repoFromPrUrl(prUrl: string): string | null {
  const match = prUrl.match(/github\.com\/([^/]+\/[^/]+)\/pull\/\d+/)
  return match ? match[1]! : null
}

// Parse a team issue identifier ("MET-12") out of a PR head-branch name.
// Matches the launcher's `exp/<IDENTIFIER>` convention and any custom prefix
// (e.g. `feature/MET-12`) by anchoring on the trailing `<IDENT>-<number>` tail.
export function parseIssueIdentifierFromBranch(branch: string): string | null {
  const match = branch.match(/(?:^|\/)([A-Z0-9]+-\d+)$/)
  return match ? match[1]! : null
}

// Resolve an issue by (repo full name + head branch), for the webhook's
// out-of-band PR linking. The SAME GitHub repo may be registered by several
// teams and board prefixes are not unique, so on ambiguity we REFUSE to link
// (REV-22). EXP-57: a retired identifier (cross-board move) matches through
// its board_moved event, team-scoped.
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
  const boardRows = await db
    .select({ boardId: boards.id, teamId: boards.teamId })
    .from(boards)
    .where(inArray(boards.repositoryId, repoRows.map((r) => r.id)))
  const boardIds = [...new Set(boardRows.map((p) => p.boardId))]
  if (boardIds.length === 0) return null
  const issueRows = await db
    .select({ id: issues.id })
    .from(issues)
    .where(and(inArray(issues.boardId, boardIds), eq(issues.identifier, identifier)))
    .limit(2)
  if (issueRows.length === 1) return issueRows[0]!.id
  if (issueRows.length > 1) {
    console.warn(
      `pr-sync: identifier ${identifier} on ${repoFullName} matches issues in multiple teams — refusing to link`
    )
    return null
  }
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
  return movedRows.length === 1 ? movedRows[0]!.issueId : null
}

// Coding-flow status transitions (EXP-120): an opened PR parks its issue in
// the team's PR-open target (default: the builtin In Review); a merged PR
// moves it to the PR-merge target (default: the builtin Done). Targets are
// per-team configurable, including "do nothing" (EXP-319). NEW-1: the
// session side is fixtures/issue-lifecycle.json, applied by
// lib/sessions/issue-lifecycle.ts through these writers: a WORKER taking
// an issue = `started` (backlog) or `resumed` (in review), its last worker
// dropping it = `unlinked` (back to backlog); those three always target
// the builtins (no team toggle). Explicit human resolutions
// (cancelled/duplicate) and already-reached targets are never overridden,
// so every writer stays idempotent.
const STARTED_FROM_STATUSES = new Set([`backlog`])
const RESUMED_FROM_STATUSES = new Set([`in_review`])
const UNLINKED_FROM_STATUSES = new Set([`in_progress`])
const OPENED_FROM_STATUSES = new Set([`backlog`, `in_progress`])
const MERGED_FROM_STATUSES = new Set([`backlog`, `in_progress`, `in_review`])

export type PrAutomationEvent = `opened` | `merged`
/** The PR events plus the session lifecycle's `started`, `resumed` and
 *  `unlinked` (NEW-1). */
export type IssueAutomationEvent = PrAutomationEvent | `started` | `resumed` | `unlinked`

const ELIGIBLE_FROM: Record<IssueAutomationEvent, Set<string>> = {
  started: STARTED_FROM_STATUSES,
  resumed: RESUMED_FROM_STATUSES,
  unlinked: UNLINKED_FROM_STATUSES,
  opened: OPENED_FROM_STATUSES,
  merged: MERGED_FROM_STATUSES,
}

/** The session lifecycle's events: always on, builtin targets. */
const BUILTIN_ONLY_EVENTS = new Set<IssueAutomationEvent>([`started`, `resumed`, `unlinked`])

// The builtin fallback target per event.
export const PR_AUTOMATION_DEFAULT_KEY: Record<
  IssueAutomationEvent,
  `backlog` | `in_progress` | `in_review` | `done`
> = {
  started: `in_progress`,
  resumed: `in_progress`,
  unlinked: `backlog`,
  opened: `in_review`,
  merged: `done`,
}

// Pure decision core of the PR lifecycle automation. `target` = the
// already-resolved issue_statuses row the team points this event at, or null
// when even the builtin row is missing (the write then degrades to the bare
// anchor and the populate_issue_status_id trigger resolves status_id).
export function planPrAutomationTransition(opts: {
  event: IssueAutomationEvent
  automationEnabled: boolean
  current: { status: string; statusId: string | null }
  target: { id: string; builtinKey: string | null; category: IssueStatusCategory } | null
}): { status: IssueStatus; statusId?: string } | null {
  if (!opts.automationEnabled) return null
  if (!ELIGIBLE_FROM[opts.event].has(opts.current.status)) return null
  // Already parked in the exact target row (a custom target whose anchor
  // sits inside the from-set): repeated deliveries must not re-fire events.
  if (opts.target && opts.current.statusId === opts.target.id) return null
  const anchor = opts.target
    ? (opts.target.builtinKey ?? CATEGORY_ANCHOR[opts.target.category])
    : PR_AUTOMATION_DEFAULT_KEY[opts.event]
  return {
    status: anchor as IssueStatus,
    ...(opts.target ? { statusId: opts.target.id } : {}),
  }
}

type AutomationTarget = {
  id: string
  name: string
  builtinKey: string | null
  category: IssueStatusCategory
}

/** A team's resolved automation for one event: the switch and the target
 *  row. Resolved once per event and handed down to every issue it moves. */
export type ResolvedAutomation = { enabled: boolean; target: AutomationTarget | null }

/** The team's switch + target row for an event (EXP-319). The session
 *  lifecycle's events (`started`, `resumed`, `unlinked`) have no team
 *  setting: always on, the builtin row. */
async function resolveAutomationTarget(
  tx: Tx,
  teamId: string,
  event: IssueAutomationEvent
): Promise<ResolvedAutomation> {
  let enabled = true
  let configuredStatusId: string | null = null
  if (!BUILTIN_ONLY_EVENTS.has(event)) {
    const opened = event !== `merged`
    const [teamConfig] = await tx
      .select({
        prOpenedStatusId: teams.prOpenedStatusId,
        prOpenedAutomation: teams.prOpenedAutomation,
        prMergedStatusId: teams.prMergedStatusId,
        prMergedAutomation: teams.prMergedAutomation,
      })
      .from(teams)
      .where(eq(teams.id, teamId))
      .limit(1)
    enabled = (opened ? teamConfig?.prOpenedAutomation : teamConfig?.prMergedAutomation) ?? true
    configuredStatusId =
      (opened ? teamConfig?.prOpenedStatusId : teamConfig?.prMergedStatusId) ?? null
  }
  if (!enabled) return { enabled, target: null }
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
          .where(and(eq(issueStatuses.id, configuredStatusId), eq(issueStatuses.teamId, teamId)))
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
              eq(issueStatuses.teamId, teamId),
              eq(issueStatuses.builtinKey, PR_AUTOMATION_DEFAULT_KEY[event])
            )
          )
          .limit(1)
      )[0] ?? null
  }
  return { enabled, target }
}

// In-transaction status writer for the issue lifecycle: the team's configured
// target for the event (EXP-319, "do nothing" = automation false), the
// shared completedAt/duplicate derivations, and the status_changed event
// marked `automation` (FEED-87). Returns whether the issue moved.
export async function applyPrLifecycleStatusInTx(
  tx: Tx,
  opts: {
    issueId: string
    teamId: string
    actorUserId: string | null
    currentStatus: string
    event: IssueAutomationEvent
    origin?: IssueEventOrigin
    /** The event's automation when the caller already resolved it (a batch
     *  moving many issues resolves the team's target once). */
    automation?: ResolvedAutomation
    /** Extra keys on the status_changed payload (EXP-1350: `resumed`'s
     *  event + thread, read back by the re-apply). */
    payloadExtra?: Record<string, unknown>
  }
): Promise<boolean> {
  const { enabled: automationEnabled, target } =
    opts.automation ?? (await resolveAutomationTarget(tx, opts.teamId, opts.event))
  if (!automationEnabled) return false

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

  const plan = planPrAutomationTransition({
    event: opts.event,
    automationEnabled,
    current: { status: opts.currentStatus, statusId: fromRow?.statusId ?? null },
    target,
  })
  if (!plan) return false

  const setValues: Record<string, unknown> = { ...plan }
  const previousDuplicateOfId = fromRow?.duplicateOfId ?? null
  applyStatusDerivations(setValues, {
    status: opts.currentStatus,
    duplicateOfId: previousDuplicateOfId,
  })
  await tx.update(issues).set(setValues).where(eq(issues.id, opts.issueId))
  await syncDuplicateMirror(tx, {
    issueId: opts.issueId,
    teamId: opts.teamId,
    actorUserId: opts.actorUserId,
    previousDuplicateOfId,
    nextDuplicateOfId:
      setValues.duplicateOfId === undefined
        ? previousDuplicateOfId
        : (setValues.duplicateOfId as string | null),
  })
  await recordIssueEvent(tx, {
    issueId: opts.issueId,
    teamId: opts.teamId,
    actorUserId: opts.actorUserId,
    ...(opts.origin ? { origin: opts.origin } : {}),
    type: `status_changed`,
    payload: {
      fromStatusId: fromRow?.statusId ?? null,
      toStatusId: target?.id ?? null,
      fromName: fromRow?.fromName ?? null,
      toName: target?.name ?? null,
      ...opts.payloadExtra,
      automation: true,
    },
  })
  return true
}

/** NEW-1: the issues among `issueIds` linked to a live (open/draft) PR. */
export async function withLivePrInTx(tx: Tx, issueIds: string[]): Promise<Set<string>> {
  if (issueIds.length === 0) return new Set()
  const rows = await tx
    .selectDistinct({ issueId: pullRequestIssues.issueId })
    .from(pullRequestIssues)
    .innerJoin(pullRequests, eq(pullRequests.id, pullRequestIssues.pullRequestId))
    .where(
      and(
        inArray(pullRequestIssues.issueId, issueIds),
        inArray(pullRequests.state, [...LIVE_PR_STATES])
      )
    )
  return new Set(rows.map((row) => row.issueId))
}

// ── the issue tree + the PR tree (EXP-1334) ─────────────────────────────────

// How far an anchor is along the PR lifecycle; a resolution
// (cancelled/duplicate) never holds a parent back.
const STAGE_RANK: Record<string, number> = {
  backlog: 0,
  in_progress: 1,
  in_review: 2,
  done: 3,
  cancelled: 3,
  duplicate: 3,
}
const EVENT_RANK: Record<PrAutomationEvent, number> = { opened: 2, merged: 3 }
const MAX_TREE_WALK = 10

/** Pure: a sub-issue is at least at the event's stage (its anchor, or it
 *  sits in the team's exact target row: a custom target anchors lower). */
export function subIssueReached(
  event: PrAutomationEvent,
  child: { status: string; statusId: string | null },
  targetId: string | null
): boolean {
  if (targetId && child.statusId === targetId) return true
  return (STAGE_RANK[child.status] ?? 0) >= EVENT_RANK[event]
}

type PrChainRow = Pick<PullRequestRow, `id` | `state` | `basePullRequestId`>

/** A merged PR is IN the default branch when its base is no PR (the
 *  default or a branch nobody stacks on), or the PR it was stacked on is
 *  merged and in it (a stack landing bottom-up). */
async function prLandedInTx(tx: Tx, pr: PrChainRow): Promise<boolean> {
  let current: PrChainRow | undefined = pr
  for (let depth = 0; current && depth <= MAX_TREE_WALK; depth++) {
    if (current.state !== `merged`) return false
    if (!current.basePullRequestId) return true
    ;[current] = await tx
      .select({
        id: pullRequests.id,
        state: pullRequests.state,
        basePullRequestId: pullRequests.basePullRequestId,
      })
      .from(pullRequests)
      .where(eq(pullRequests.id, current.basePullRequestId))
      .limit(1)
    // The PR below is gone: nothing holds this one back.
    if (!current) return true
  }
  return false
}

/** The merged PRs stacked (transitively) on `pullRequestId` that were held
 *  back until it landed. A live PR in between ends the walk. */
async function heldMergedAboveInTx(tx: Tx, pullRequestId: string): Promise<PullRequestRow[]> {
  const found: PullRequestRow[] = []
  let frontier = [pullRequestId]
  for (let depth = 0; frontier.length > 0 && depth < MAX_TREE_WALK; depth++) {
    const rows = await tx
      .select()
      .from(pullRequests)
      .where(and(inArray(pullRequests.basePullRequestId, frontier), eq(pullRequests.state, `merged`)))
    found.push(...rows)
    frontier = rows.map((row) => row.id)
  }
  return found
}

/** A merged PR is in the default branch: its issues and those of every
 *  merged PR held above it take the team's merge automation (idempotent:
 *  an issue already at the target never moves or re-fires). */
async function applyLandedInTx(
  tx: Tx,
  pr: Pick<PullRequestRow, `id` | `teamId`>,
  actorUserId: string | null
): Promise<string[]> {
  const held = await heldMergedAboveInTx(tx, pr.id)
  const rows = await tx
    .select({ id: pullRequestIssues.issueId })
    .from(pullRequestIssues)
    .where(inArray(pullRequestIssues.pullRequestId, [pr.id, ...held.map((row) => row.id)]))
  return applyPrEventToIssuesInTx(tx, {
    teamId: pr.teamId,
    issueIds: rows.map((row) => row.id),
    event: `merged`,
    actorUserId,
  })
}

/** The issues a parent's sub-issues must share to hold it back: linked to
 *  the same PRs, or to those PRs' threads. `prIds` = the parent's PRs that
 *  carry the event (live/merged for `opened`, landed for `merged`). */
async function parentScopeInTx(tx: Tx, parentId: string, event: PrAutomationEvent) {
  const prs = await tx
    .select({
      id: pullRequests.id,
      state: pullRequests.state,
      basePullRequestId: pullRequests.basePullRequestId,
      rootSessionId: pullRequests.rootSessionId,
    })
    .from(pullRequestIssues)
    .innerJoin(pullRequests, eq(pullRequests.id, pullRequestIssues.pullRequestId))
    .where(eq(pullRequestIssues.issueId, parentId))
  const carrying: typeof prs = []
  for (const pr of prs) {
    if (event === `opened` ? pr.state !== `closed` : await prLandedInTx(tx, pr)) carrying.push(pr)
  }
  if (carrying.length === 0) return null
  const prIds = carrying.map((pr) => pr.id)
  const roots = [...new Set(carrying.map((pr) => pr.rootSessionId).filter((id): id is string => !!id))]
  const viaPr = await tx
    .select({ issueId: pullRequestIssues.issueId })
    .from(pullRequestIssues)
    .where(inArray(pullRequestIssues.pullRequestId, prIds))
  const viaThread = roots.length
    ? await tx
        .select({ issueId: sessionIssues.issueId })
        .from(sessionIssues)
        .where(inArray(sessionIssues.sessionId, roots))
    : []
  return new Set([...viaPr, ...viaThread].map((row) => row.issueId))
}

/** Sub-issues (canonical `parent` rows read parent → child) per parent. */
async function subIssuesOfInTx(tx: Tx, parentIds: string[]): Promise<Map<string, string[]>> {
  const map = new Map<string, string[]>()
  if (parentIds.length === 0) return map
  const rows = await tx
    .select({ parent: issueRelations.issueId, child: issueRelations.relatedIssueId })
    .from(issueRelations)
    .where(and(eq(issueRelations.type, `parent`), inArray(issueRelations.issueId, parentIds)))
  for (const row of rows) map.set(row.parent, [...(map.get(row.parent) ?? []), row.child])
  return map
}

/**
 * EXP-1334: move a PR event's issues, respecting the issue tree. A plain
 * issue moves at once; a PARENT moves only once every sub-issue linked to the
 * same PR or thread has reached the stage (its sub-issues are never moved for
 * it). Each move then re-checks the mover's own parents (linked to a PR that
 * carries the event), so a parent held back follows its last sub-issue.
 */
export async function applyPrEventToIssuesInTx(
  tx: Tx,
  opts: {
    teamId: string
    issueIds: string[]
    event: PrAutomationEvent
    actorUserId: string | null
    origin?: IssueEventOrigin
  }
): Promise<string[]> {
  if (opts.issueIds.length === 0) return []
  // One resolve per event: a team that switched the automation off moves
  // nothing, so no issue or sub-issue is ever read for it.
  const automation = await resolveAutomationTarget(tx, opts.teamId, opts.event)
  if (!automation.enabled) return []
  const { target } = automation
  const moved: string[] = []
  const tried = new Set<string>()

  const statusOf = async (ids: string[]) =>
    ids.length === 0
      ? []
      : tx
          .select({ id: issues.id, status: issues.status, statusId: issues.statusId })
          .from(issues)
          .where(and(inArray(issues.id, ids), eq(issues.teamId, opts.teamId)))

  const move = async (issue: { id: string; status: string }) => {
    tried.add(issue.id)
    const applied = await applyPrLifecycleStatusInTx(tx, {
      issueId: issue.id,
      teamId: opts.teamId,
      actorUserId: opts.actorUserId,
      currentStatus: issue.status,
      event: opts.event,
      automation,
      ...(opts.origin ? { origin: opts.origin } : {}),
    })
    if (applied) moved.push(issue.id)
    return applied
  }

  /** A parent is ready when no in-scope sub-issue lags behind. */
  const ready = async (parentId: string, children: string[]): Promise<boolean | null> => {
    const scope = await parentScopeInTx(tx, parentId, opts.event)
    if (!scope) return null
    const gating = children.filter((child) => scope.has(child))
    const rows = await statusOf(gating)
    return rows.every((row) => subIssueReached(opts.event, row, target?.id ?? null))
  }

  // 1. The event's own issues: plain ones first, then parents bottom-up.
  const own = await statusOf([...new Set(opts.issueIds)])
  const childrenOf = await subIssuesOfInTx(
    tx,
    own.map((row) => row.id)
  )
  const pending = own.filter((row) => childrenOf.has(row.id))
  for (const row of own) if (!childrenOf.has(row.id)) await move(row)
  for (let pass = 0; pending.length > 0 && pass <= MAX_TREE_WALK; pass++) {
    let progressed = false
    for (const row of [...pending]) {
      if (!(await ready(row.id, childrenOf.get(row.id)!))) continue
      const [fresh] = await statusOf([row.id])
      if (fresh) await move(fresh)
      pending.splice(pending.indexOf(row), 1)
      progressed = true
    }
    if (!progressed) break
  }

  // 2. Up the tree: the parents of what moved follow when they are ready.
  let frontier = [...moved]
  for (let depth = 0; frontier.length > 0 && depth < MAX_TREE_WALK; depth++) {
    const parentRows = await tx
      .select({ parent: issueRelations.issueId })
      .from(issueRelations)
      .where(and(eq(issueRelations.type, `parent`), inArray(issueRelations.relatedIssueId, frontier)))
    const parents = [...new Set(parentRows.map((row) => row.parent))].filter((id) => !tried.has(id))
    const kids = await subIssuesOfInTx(tx, parents)
    const next: string[] = []
    for (const parentId of parents) {
      if (!(await ready(parentId, kids.get(parentId) ?? []))) continue
      const [fresh] = await statusOf([parentId])
      if (fresh && (await move(fresh))) next.push(parentId)
    }
    frontier = next
  }
  return moved
}

// ── reads ───────────────────────────────────────────────────────────────────

export async function loadPullRequest(id: string): Promise<PullRequestRow | null> {
  const [row] = await db.select().from(pullRequests).where(eq(pullRequests.id, id)).limit(1)
  return row ?? null
}

export async function loadPullRequestByUrl(url: string): Promise<PullRequestRow | null> {
  const [row] = await db.select().from(pullRequests).where(eq(pullRequests.url, url)).limit(1)
  return row ?? null
}

export async function loadPullRequestByNumber(
  repositoryId: string,
  number: number
): Promise<PullRequestRow | null> {
  const [row] = await db
    .select()
    .from(pullRequests)
    .where(and(eq(pullRequests.repositoryId, repositoryId), eq(pullRequests.number, number)))
    .limit(1)
  return row ?? null
}

/** The live PR on `head` in this repository (the idempotent `pr_open`). */
export async function loadLivePullRequestByHead(
  repositoryId: string,
  head: string
): Promise<PullRequestRow | null> {
  const [row] = await db
    .select()
    .from(pullRequests)
    .where(
      and(
        eq(pullRequests.repositoryId, repositoryId),
        eq(pullRequests.head, head),
        inArray(pullRequests.state, [...LIVE_PR_STATES])
      )
    )
    .orderBy(desc(pullRequests.createdAt))
    .limit(1)
  return row ?? null
}

/** Every live PR linked to the issue, newest first, with how many issues
 *  each links and whether the issue's assigned tree session opened it
 *  (FEED-111: a thread-level PR links the root's issues too, so newest-first
 *  alone picks the wrong one; `pickIssuePullRequest` ranks these). */
export async function livePullRequestsForIssue(
  issueId: string
): Promise<{ pr: PullRequestRow; linkedIssues: number; assigned: boolean }[]> {
  const rows = await db
    .select({ pr: pullRequests })
    .from(pullRequestIssues)
    .innerJoin(pullRequests, eq(pullRequests.id, pullRequestIssues.pullRequestId))
    .where(
      and(eq(pullRequestIssues.issueId, issueId), inArray(pullRequests.state, [...LIVE_PR_STATES]))
    )
    .orderBy(desc(pullRequests.createdAt))
  if (rows.length === 0) return []
  const counts = await db
    .select({ pullRequestId: pullRequestIssues.pullRequestId, n: sql<number>`count(*)::int` })
    .from(pullRequestIssues)
    .where(inArray(pullRequestIssues.pullRequestId, rows.map((row) => row.pr.id)))
    .groupBy(pullRequestIssues.pullRequestId)
  // NEW-48: a session_issues row sits on the session that HOLDS the issue
  // (NEW-1), so the PR that session opened is the issue's own.
  const assignees = await db
    .select({ sessionId: sessionIssues.sessionId })
    .from(sessionIssues)
    .where(eq(sessionIssues.issueId, issueId))
  const linked = new Map(counts.map((row) => [row.pullRequestId, Number(row.n)]))
  const assigned = new Set(assignees.map((row) => row.sessionId))
  return rows.map(({ pr }) => ({
    pr,
    linkedIssues: linked.get(pr.id) ?? 1,
    assigned: pr.sessionId != null && assigned.has(pr.sessionId),
  }))
}

/** A session's newest live PR: one it opened, else its thread's newest. */
export async function newestLivePullRequestForSession(session: {
  id: string
  rootId: string
}): Promise<PullRequestRow | null> {
  const newest = async (where: ReturnType<typeof eq>) => {
    const [row] = await db
      .select()
      .from(pullRequests)
      .where(and(where, inArray(pullRequests.state, [...LIVE_PR_STATES])))
      .orderBy(desc(pullRequests.createdAt))
      .limit(1)
    return row ?? null
  }
  return (
    (await newest(eq(pullRequests.sessionId, session.id))) ??
    (await newest(eq(pullRequests.rootSessionId, session.rootId)))
  )
}

/** The live PRs of one repository (the base inference's candidates). */
export async function livePullRequestsOfRepo(repositoryId: string): Promise<PullRequestRow[]> {
  return db
    .select()
    .from(pullRequests)
    .where(
      and(eq(pullRequests.repositoryId, repositoryId), inArray(pullRequests.state, [...LIVE_PR_STATES]))
    )
}

/** The live PRs of a thread (root), for the PR body sync. */
export async function livePullRequestsOfRoot(rootSessionId: string): Promise<PullRequestRow[]> {
  return db
    .select()
    .from(pullRequests)
    .where(
      and(
        eq(pullRequests.rootSessionId, rootSessionId),
        inArray(pullRequests.state, [...LIVE_PR_STATES])
      )
    )
}

export async function linkedIssueIds(pullRequestId: string): Promise<string[]> {
  const rows = await db
    .select({ issueId: pullRequestIssues.issueId })
    .from(pullRequestIssues)
    .where(eq(pullRequestIssues.pullRequestId, pullRequestId))
  return rows.map((row) => row.issueId)
}

/**
 * The live PR (same repository) whose head is `base`: what a PR on that base
 * is stacked on. A branch the repo is developed on is never a stack parent,
 * even while a PR FROM it is open (a `develop → main` release PR).
 */
export async function resolveBasePullRequestId(
  database: Pick<typeof db, `select`>,
  opts: {
    teamId: string
    repositoryId: string
    repoFullName: string
    base: string
    selfId?: string | null
  }
): Promise<string | null> {
  const [parent] = await database
    .select({ id: pullRequests.id })
    .from(pullRequests)
    .where(
      and(
        eq(pullRequests.repositoryId, opts.repositoryId),
        eq(pullRequests.head, opts.base),
        inArray(pullRequests.state, [...LIVE_PR_STATES]),
        ...(opts.selfId ? [ne(pullRequests.id, opts.selfId)] : [])
      )
    )
    .orderBy(desc(pullRequests.createdAt))
    .limit(1)
  if (!parent) return null
  const defaults = await loadRepoDefaultBranches(database, {
    teamId: opts.teamId,
    repoFullName: opts.repoFullName,
  })
  return isRepoDefaultBranch(defaults, opts.base) ? null : parent.id
}

// ── writes ──────────────────────────────────────────────────────────────────

type IssueLinkRow = { id: string; teamId: string; status: string }

/**
 * Link issues to a PR (`pull_request_issues`, insert-only): every NEWLY
 * linked one records `pr_opened` and takes the team's PR-open automation (a
 * merged PR's newly linked issue takes the merge automation instead, once
 * that PR is in the default branch). EXP-1334: parents wait for their
 * sub-issues (`applyPrEventToIssuesInTx`). Issues of another team are
 * skipped. Returns the newly linked ids.
 */
export async function linkPullRequestIssuesInTx(
  tx: Tx,
  pr: Pick<PullRequestRow, `id` | `teamId` | `url` | `number` | `head` | `state` | `basePullRequestId`>,
  issueIds: string[],
  actor: { userId: string | null; origin?: IssueEventOrigin }
): Promise<string[]> {
  if (issueIds.length === 0) return []
  const rows: IssueLinkRow[] = await tx
    .select({ id: issues.id, teamId: issues.teamId, status: issues.status })
    .from(issues)
    .where(and(inArray(issues.id, issueIds), eq(issues.teamId, pr.teamId)))
  if (rows.length === 0) return []
  const inserted = await tx
    .insert(pullRequestIssues)
    .values(rows.map((row) => ({ pullRequestId: pr.id, issueId: row.id, teamId: pr.teamId })))
    .onConflictDoNothing()
    .returning({ issueId: pullRequestIssues.issueId })
  const fresh = new Set(inserted.map((row) => row.issueId))
  const linked: string[] = []
  for (const row of rows) {
    if (!fresh.has(row.id)) continue
    linked.push(row.id)
    await recordIssueEvent(tx, {
      issueId: row.id,
      teamId: pr.teamId,
      actorUserId: actor.userId,
      ...(actor.origin ? { origin: actor.origin } : {}),
      type: `pr_opened`,
      payload: { prUrl: pr.url, prNumber: pr.number, branch: pr.head },
    })
  }
  const merged = pr.state === `merged`
  // A merged PR still stacked on an unmerged one is not in the default
  // branch yet: its issues wait for the stack's bottom to land.
  if (!merged || (await prLandedInTx(tx, pr))) {
    await applyPrEventToIssuesInTx(tx, {
      teamId: pr.teamId,
      issueIds: linked,
      event: merged ? `merged` : `opened`,
      actorUserId: actor.userId,
      ...(actor.origin ? { origin: actor.origin } : {}),
    })
  }
  return linked
}

export interface RecordOpenedArgs {
  teamId: string
  rootSessionId: string | null
  sessionId: string | null
  repositoryId: string
  repoFullName: string
  number: number
  url: string
  title: string | null
  head: string
  base: string
  draft?: boolean
  issueIds: string[]
  actorUserId: string | null
  actorViaAgent?: boolean
  githubActorUserId?: string | null
  origin?: IssueEventOrigin
}

/**
 * A PR is open on GitHub: upsert its row (by url; a known row keeps its
 * thread and opener, a closed one reopens), recompute its stack parent, link
 * the issues and, the first time the row exists, post the thread's
 * `pr_opened` card. Notifications fire post-commit for the newly linked
 * issues only, so the pr_open + webhook pair stays single-fire.
 */
/** The payload of the builtin `pr.opened` / `pr.merged` events. */
function prEventPayload(
  pr: Pick<PullRequestRow, `id` | `number` | `url` | `title` | `head` | `base` | `repositoryId` | `sessionId` | `rootSessionId`>,
  issueIds: readonly string[]
) {
  return {
    pullRequestId: pr.id,
    number: pr.number,
    url: pr.url,
    title: pr.title,
    head: pr.head,
    base: pr.base,
    repositoryId: pr.repositoryId,
    sessionId: pr.sessionId,
    rootSessionId: pr.rootSessionId,
    issueIds: [...issueIds],
  }
}

export async function recordPullRequestOpened(
  args: RecordOpenedArgs
): Promise<{ pullRequest: PullRequestRow; created: boolean; linkedIssueIds: string[]; txid: number }> {
  const result = await db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const basePullRequestId = await resolveBasePullRequestId(tx, {
      teamId: args.teamId,
      repositoryId: args.repositoryId,
      repoFullName: args.repoFullName,
      base: args.base,
    })
    const state = args.draft ? (`draft` as const) : (`open` as const)
    const [inserted] = await tx
      .insert(pullRequests)
      .values({
        teamId: args.teamId,
        rootSessionId: args.rootSessionId,
        sessionId: args.sessionId,
        repositoryId: args.repositoryId,
        number: args.number,
        url: args.url,
        title: args.title,
        head: args.head,
        base: args.base,
        basePullRequestId,
        state,
      })
      .onConflictDoNothing()
      .returning()
    let pullRequest = inserted
    // The thread whose card announces it: a new row's, or the one this call
    // gives a row the webhook recorded first (it beat pr_open).
    let announceRoot = inserted?.rootSessionId ?? null
    if (!pullRequest) {
      const [existing] = await tx
        .select()
        .from(pullRequests)
        .where(eq(pullRequests.url, args.url))
        .limit(1)
      if (!existing) throw new Error(`pull request ${args.url} vanished mid-write`)
      const [updated] = await tx
        .update(pullRequests)
        .set({
          ...(args.title ? { title: args.title } : {}),
          base: args.base,
          basePullRequestId: basePullRequestId === existing.id ? null : basePullRequestId,
          // A known row keeps its thread; a row the webhook created first
          // (it beat pr_open) learns it here.
          rootSessionId: existing.rootSessionId ?? args.rootSessionId,
          sessionId: existing.sessionId ?? args.sessionId,
          ...(existing.state === `closed` ? { state, githubUpdatedAt: new Date() } : {}),
          updatedAt: new Date(),
        })
        .where(eq(pullRequests.id, existing.id))
        .returning()
      pullRequest = updated ?? existing
      if (!existing.rootSessionId) announceRoot = pullRequest.rootSessionId
    }
    const linked = await linkPullRequestIssuesInTx(tx, pullRequest, args.issueIds, {
      userId: args.actorUserId,
      ...(args.origin ? { origin: args.origin } : {}),
    })
    if (inserted) {
      // NEW-2: the builtin `pr.opened` event, once per PR row.
      await emitEventInTx(tx, {
        teamId: pullRequest.teamId,
        name: `pr.opened`,
        payload: prEventPayload(pullRequest, args.issueIds),
        emittedBy: pullRequest.sessionId
          ? { sessionId: pullRequest.sessionId }
          : args.actorUserId
            ? { userId: args.actorUserId }
            : { server: true },
      })
    }
    if (announceRoot) {
      await postSystemCard(tx, announceRoot, {
        kind: `event`,
        event: `pr_opened`,
        pullRequestId: pullRequest.id,
      })
      // NEW-1: the opener's parent and watchers wake on it (the bus).
      const opener = pullRequest.sessionId
        ? (
            await tx
              .select({ id: sessions.id, parentId: sessions.parentId })
              .from(sessions)
              .where(eq(sessions.id, pullRequest.sessionId))
              .limit(1)
          )[0]
        : undefined
      if (opener) {
        await busLine(
          tx,
          opener,
          `pr_opened`,
          BUS_COPY.pr_opened(pullRequest.number, pullRequest.url),
          { waking: true, pullRequestId: pullRequest.id }
        )
      }
    }
    return { pullRequest, created: Boolean(inserted), linkedIssueIds: linked, txid }
  })
  for (const issueId of result.linkedIssueIds) {
    fireAndForgetPrNotify({
      issueId,
      type: `pr_opened`,
      actorUserId: args.actorUserId,
      actorViaAgent: args.actorViaAgent,
      githubActorUserId: args.githubActorUserId ?? null,
    })
  }
  if (result.created || result.linkedIssueIds.length > 0) invalidateOpenPullsLazy(args.teamId)
  return result
}

/** EXP-1244: a linked PR leaves the team's "unlinked" Reviews list. Lazy:
 *  the router module is never this module's load-time dependency. */
function invalidateOpenPullsLazy(teamId: string): void {
  void import(`@/lib/trpc/repositories`)
    .then((mod) => mod.invalidateOpenPulls(teamId))
    .catch(() => {})
}

/**
 * The PR merged (any path: the merge call, the webhook, the poller). The
 * `state <> 'merged'` UPDATE is the claim (REV-48): only its winner records
 * `pr_merged` on every linked issue, applies the team's merge automation,
 * posts the thread's card, notifies and retargets the PRs stacked on it.
 * EXP-1334: a PR merged into ANOTHER PR's branch (stacked) leaves its issues
 * alone; once the stack's bottom lands in the default branch, every merged
 * PR stacked on it takes the merge automation too. `base` = the base the
 * caller saw at merge time (the webhook's `pr.base.ref`, the poller's read):
 * written BEFORE the landed check, so a PR retargeted onto the default
 * branch whose `edited` delivery is late or lost is not held by the stale
 * stack parent it still points at. Never ends a session.
 */
export async function applyPullRequestMerged(opts: {
  pullRequestId: string
  mergedAt?: Date | null
  base?: string | null
  actorUserId?: string | null
  actorViaAgent?: boolean
  githubActorUserId?: string | null
  /** NEW-5: our own merge — delete the head whatever its name, with the
   *  merge's token. Omitted = only an `exp/` head goes (a merge made
   *  outside Exponential, seen by the webhook or the poller). */
  deleteHead?: { token: string; anyBranch: boolean }
}): Promise<{ applied: boolean; txid: number | null }> {
  const result = await db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const [claimed] = await tx
      .update(pullRequests)
      .set({ state: `merged`, mergedAt: opts.mergedAt ?? new Date(), updatedAt: new Date() })
      .where(and(eq(pullRequests.id, opts.pullRequestId), ne(pullRequests.state, `merged`)))
      .returning()
    if (!claimed) return { applied: false as const, issueIds: [] as string[], txid: null, pr: null }
    if (opts.base && opts.base !== claimed.base) {
      claimed.basePullRequestId = await writeBaseInTx(tx, claimed, opts.base)
      claimed.base = opts.base
    }
    const linked = await tx
      .select({ id: issues.id })
      .from(pullRequestIssues)
      .innerJoin(issues, eq(issues.id, pullRequestIssues.issueId))
      .where(eq(pullRequestIssues.pullRequestId, claimed.id))
    for (const issue of linked) {
      await recordIssueEvent(tx, {
        issueId: issue.id,
        teamId: claimed.teamId,
        actorUserId: opts.actorUserId ?? null,
        type: `pr_merged`,
        payload: { prUrl: claimed.url },
      })
    }
    if (await prLandedInTx(tx, claimed)) {
      await applyLandedInTx(tx, claimed, opts.actorUserId ?? null)
    }
    if (claimed.rootSessionId) {
      await postSystemCard(tx, claimed.rootSessionId, {
        kind: `event`,
        event: `pr_merged`,
        pullRequestId: claimed.id,
      })
    }
    // NEW-2: the builtin `pr.merged` event.
    await emitEventInTx(tx, {
      teamId: claimed.teamId,
      name: `pr.merged`,
      payload: prEventPayload(claimed, linked.map((issue) => issue.id)),
      emittedBy: opts.actorUserId ? { userId: opts.actorUserId } : { server: true },
    })
    return { applied: true as const, issueIds: linked.map((issue) => issue.id), txid, pr: claimed }
  })
  if (!result.applied || !result.pr) return { applied: false, txid: null }
  for (const issueId of result.issueIds) {
    fireAndForgetPrNotify({
      issueId,
      type: `pr_merged`,
      actorUserId: opts.actorUserId ?? null,
      actorViaAgent: opts.actorViaAgent,
      githubActorUserId: opts.githubActorUserId ?? null,
    })
  }
  invalidateOpenPullsLazy(result.pr.teamId)
  // EXP-1397: the merging session's untagged topics went out with this PR:
  // stamped with its URL, they leave the thread's still-open bodies.
  const rootId = result.pr.rootSessionId
  if (rootId) {
    try {
      const { tagGuideForMergedPullRequest } = await import(`@/lib/sessions/guide`)
      if (await tagGuideForMergedPullRequest(result.pr)) {
        const { syncPullRequestBodies } = await import(`@/lib/pull-requests`)
        void syncPullRequestBodies(rootId).catch((error: unknown) =>
          console.warn(`[pr-sync] body sync after a merge failed`, error)
        )
      }
    } catch (error) {
      console.warn(`[pr-sync] tagging the Guide after a merge failed`, error)
    }
  }
  // EXP-324: the PRs based on the merged head move onto the default branch,
  // THEN (NEW-5) the head branch itself goes: any branch when the merge was
  // ours (`deleteHead`), only `exp/` ones when it happened outside (webhook,
  // poller). Fire-and-forget: never blocks the webhook or the merge caller.
  void deleteMergedHeadBranch({
    pullRequestId: result.pr.id,
    token: opts.deleteHead?.token ?? null,
    anyBranch: opts.deleteHead?.anyBranch ?? false,
  })
  return { applied: true, txid: result.txid }
}

/** NEW-5: whether a merged PR's head branch may be deleted on GitHub. Never
 *  a default branch; only `exp/` branches unless `anyBranch` (our own
 *  merge); never while a live PR still sits on it or reuses it as its head
 *  (a native-stack child GitHub restacks itself, a reopened PR). */
export function mergedHeadDeletable(opts: {
  head: string
  anyBranch: boolean
  isDefaultBranch: boolean
  openChildren: number
  liveOnSameHead: boolean
}): boolean {
  const head = opts.head.trim()
  if (!head || opts.isDefaultBranch) return false
  if (!opts.anyBranch && !head.startsWith(`exp/`)) return false
  return opts.openChildren === 0 && !opts.liveOnSameHead
}

/**
 * NEW-5: delete a merged PR's head branch on GitHub, after its children were
 * retargeted (EXP-324) so nothing still rests on it. Best effort: every
 * failure logs and nothing throws — a merge never fails over its branch.
 * `token` = the installation token the merge used; resolved when omitted.
 */
export async function deleteMergedHeadBranch(opts: {
  pullRequestId: string
  token?: string | null
  anyBranch?: boolean
}): Promise<`deleted` | `gone` | `kept` | `failed`> {
  try {
    await retargetChildrenOfMergedPr(opts.pullRequestId)
  } catch (err) {
    console.error(`retargetChildrenOfMergedPr failed:`, err)
  }
  try {
    const merged = await loadPullRequest(opts.pullRequestId)
    if (!merged || merged.state !== `merged` || !merged.head) return `kept`
    const repo = repoFromPrUrl(merged.url)
    if (!repo) return `kept`
    if (!opts.anyBranch && !merged.head.startsWith(`exp/`)) return `kept`
    const defaults = await loadRepoDefaultBranches(db, { teamId: merged.teamId, repoFullName: repo })
    const githubDefault = await resolveRepoDefaultBranchCached(repo).catch(() => null)
    const token = opts.token ?? (await resolveRepoInstallationTokenInfo(repo))?.token ?? null
    if (!token) return `kept`
    const [children, sameHead] = await Promise.all([
      listOpenPullsByBase(repo, merged.head, token),
      loadLivePullRequestByHead(merged.repositoryId, merged.head),
    ])
    const deletable = mergedHeadDeletable({
      head: merged.head,
      anyBranch: opts.anyBranch ?? false,
      isDefaultBranch: isRepoDefaultBranch(defaults, merged.head) || merged.head === githubDefault,
      openChildren: children.length,
      liveOnSameHead: sameHead != null && sameHead.id !== merged.id,
    })
    if (!deletable) return `kept`
    return await deleteBranch({ repo, branch: merged.head, token })
  } catch (err) {
    console.error(`deleting the merged head of ${opts.pullRequestId} failed:`, err)
    return `failed`
  }
}

/** Closed without merging (live → closed). Posts the thread's card. */
export async function applyPullRequestClosed(opts: {
  pullRequestId: string
}): Promise<{ applied: boolean; txid: number | null }> {
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const [claimed] = await tx
      .update(pullRequests)
      .set({ state: `closed`, updatedAt: new Date() })
      .where(
        and(
          eq(pullRequests.id, opts.pullRequestId),
          inArray(pullRequests.state, [...LIVE_PR_STATES])
        )
      )
      .returning()
    if (!claimed) return { applied: false, txid: null }
    if (claimed.rootSessionId) {
      await postSystemCard(tx, claimed.rootSessionId, {
        kind: `event`,
        event: `pr_closed`,
        pullRequestId: claimed.id,
      })
    }
    return { applied: true, txid }
  })
}

/** Reopened on GitHub (closed → open/draft), with its live base. */
export async function applyPullRequestReopened(opts: {
  pullRequestId: string
  base?: string | null
  draft?: boolean
  /** The delivery's `updated_at` (GitHub's clock); omitted = now. */
  githubUpdatedAt?: Date
}): Promise<void> {
  await db.transaction(async (tx) => {
    await generateTxId(tx)
    const [row] = await tx
      .update(pullRequests)
      .set({
        state: opts.draft ? `draft` : `open`,
        githubUpdatedAt: opts.githubUpdatedAt ?? new Date(),
        updatedAt: new Date(),
      })
      .where(and(eq(pullRequests.id, opts.pullRequestId), eq(pullRequests.state, `closed`)))
      .returning()
    if (row && opts.base) await writeBaseInTx(tx, row, opts.base)
  })
}

/** GitHub's draft flag (live rows only). EXP-1418: written only when it
 *  differs (every pull_request delivery carries it); returns the flipped
 *  row's root, or null when nothing changed. */
export async function applyPullRequestDraft(opts: {
  pullRequestId: string
  draft: boolean
  /** GitHub's `updated_at` of this state, stored as `github_updated_at`
   *  (omitted = now: our own flip). */
  githubUpdatedAt?: Date
  /** A generic delivery's `updated_at`: the row flips only when its last
   *  applied PR state (`github_updated_at`, never the row's `updated_at`,
   *  which any write moves) is no newer (an older delivery never wins). */
  notBefore?: Date
}): Promise<{ rootSessionId: string | null } | null> {
  const [row] = await db
    .update(pullRequests)
    .set({
      state: opts.draft ? `draft` : `open`,
      githubUpdatedAt: opts.githubUpdatedAt ?? opts.notBefore ?? new Date(),
      updatedAt: new Date(),
    })
    .where(
      and(
        eq(pullRequests.id, opts.pullRequestId),
        eq(pullRequests.state, opts.draft ? `open` : `draft`),
        opts.notBefore
          ? or(isNull(pullRequests.githubUpdatedAt), lte(pullRequests.githubUpdatedAt, opts.notBefore))
          : undefined
      )
    )
    .returning({ rootSessionId: pullRequests.rootSessionId })
  return row ?? null
}

/** Stores `base` and the live PR it sits on; returns that stack parent. */
async function writeBaseInTx(
  tx: Tx,
  row: Pick<PullRequestRow, `id` | `teamId` | `repositoryId` | `url`>,
  base: string
): Promise<string | null> {
  const repoFullName = repoFromPrUrl(row.url) ?? ``
  const basePullRequestId = await resolveBasePullRequestId(tx, {
    teamId: row.teamId,
    repositoryId: row.repositoryId,
    repoFullName,
    base,
    selfId: row.id,
  })
  await tx
    .update(pullRequests)
    .set({ base, basePullRequestId, updatedAt: new Date() })
    .where(eq(pullRequests.id, row.id))
  return basePullRequestId
}

/** A PR's base changed (our retarget, github.com, GitHub's own restack):
 *  store it and the open PR it now sits on. A MERGED PR whose corrected base
 *  lands it in the default branch (the `edited` delivery arrived after
 *  `closed`, or the merge was recorded off a stale base) releases the
 *  issues its merge held back, its own and those held above it. */
export async function applyPullRequestBase(opts: {
  pullRequestId: string
  base: string
}): Promise<{ txid: number }> {
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const [row] = await tx
      .select()
      .from(pullRequests)
      .where(eq(pullRequests.id, opts.pullRequestId))
      .limit(1)
    if (!row || row.base === opts.base) return { txid }
    const basePullRequestId = await writeBaseInTx(tx, row, opts.base)
    if (row.state === `merged` && (await prLandedInTx(tx, { ...row, basePullRequestId }))) {
      await applyLandedInTx(tx, row, null)
    }
    return { txid }
  })
}

/**
 * Retarget every live PR based on a just-merged PR's head onto the branch
 * the team develops on (EXP-324). Best-effort and idempotent: every bail-out
 * is silent, one failed child never blocks the rest. EXP-1248: a child
 * GitHub holds in a native stack is skipped (GitHub restacks it itself, and
 * our PATCH could unstack it); its new base arrives as an `edited` webhook.
 */
export async function retargetChildrenOfMergedPr(pullRequestId: string): Promise<void> {
  const merged = await loadPullRequest(pullRequestId)
  if (!merged) return
  const repo = repoFromPrUrl(merged.url)
  if (!repo || !merged.head || !githubAppConfigured()) return
  const [repoRow] = await db
    .select({
      defaultBranch: repositories.defaultBranch,
      defaultBranchOverride: repositories.defaultBranchOverride,
    })
    .from(repositories)
    .where(eq(repositories.id, merged.repositoryId))
    .limit(1)
  // EXP-712: the merged PR's (first) linked issue's board may pin its own.
  const [boardPin] = await db
    .select({ defaultBranch: boards.defaultBranch })
    .from(pullRequestIssues)
    .innerJoin(issues, eq(issues.id, pullRequestIssues.issueId))
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .where(eq(pullRequestIssues.pullRequestId, merged.id))
    .limit(1)
  const defaults = await loadRepoDefaultBranches(db, { teamId: merged.teamId, repoFullName: repo })
  // Load-bearing guard: a default-based PR's "children" would be every other
  // default-based PR in the repo (a merged `develop → main` promotion too).
  if (isRepoDefaultBranch(defaults, merged.head)) return
  if (repoRow && merged.head === repoRow.defaultBranch) return
  const target =
    boardPin?.defaultBranch ??
    repoRow?.defaultBranchOverride ??
    repoRow?.defaultBranch ??
    (await resolveRepoDefaultBranchCached(repo))
  if (!target || target === merged.head) return
  const resolved = await resolveRepoInstallationTokenInfo(repo)
  if (!resolved) return
  const children = await listOpenPullsByBase(repo, merged.head, resolved.token)
  for (const child of children) {
    try {
      if (await inNativeStack(repo, child.number, resolved.token)) continue
      await retargetPullRequest({ repo, prNumber: child.number, base: target, token: resolved.token })
      const row = await loadPullRequestByUrl(child.url)
      if (row) await applyPullRequestBase({ pullRequestId: row.id, base: target })
    } catch (err) {
      console.error(`retarget of ${repo}#${child.number} onto ${target} failed:`, err)
    }
  }
}

/** Read live (the webhook's `stack` is not stored). An unreadable PR counts
 *  as plain: the retarget is the pre-stacks behaviour. */
async function inNativeStack(repo: string, prNumber: number, token: string): Promise<boolean> {
  try {
    return (await fetchPullStack({ repo, prNumber, token })) !== null
  } catch (err) {
    console.error(`stack read of ${repo}#${prNumber} failed:`, err)
    return false
  }
}

/**
 * The webhook's `opened` for a PR nobody recorded (an agent's `gh`, the
 * compare view): link it when its head names exactly one issue
 * (`exp/<IDENTIFIER>`), in that issue's team, on the team's repo row. A
 * known url only learns its base.
 */
export async function adoptOutOfBandPullRequest(opts: {
  repoFullName: string
  url: string
  number: number
  title: string | null
  head: string
  base: string
  draft?: boolean
  actorUserId: string | null
  actorViaAgent?: boolean
  githubActorUserId?: string | null
}): Promise<void> {
  const known = await loadPullRequestByUrl(opts.url)
  if (known) {
    if (known.base !== opts.base) await applyPullRequestBase({ pullRequestId: known.id, base: opts.base })
    return
  }
  const issueId = await findIssueIdByBranch(opts.repoFullName, opts.head)
  if (!issueId) return
  const [issue] = await db
    .select({ teamId: issues.teamId })
    .from(issues)
    .where(eq(issues.id, issueId))
    .limit(1)
  if (!issue) return
  const [repo] = await db
    .select({ id: repositories.id })
    .from(repositories)
    .where(and(eq(repositories.teamId, issue.teamId), eq(repositories.fullName, opts.repoFullName)))
    .limit(1)
  if (!repo) return
  await recordPullRequestOpened({
    teamId: issue.teamId,
    rootSessionId: null,
    sessionId: null,
    repositoryId: repo.id,
    repoFullName: opts.repoFullName,
    number: opts.number,
    url: opts.url,
    title: opts.title,
    head: opts.head,
    base: opts.base,
    ...(opts.draft ? { draft: true } : {}),
    issueIds: [issueId],
    actorUserId: opts.actorUserId,
    ...(opts.actorViaAgent !== undefined ? { actorViaAgent: opts.actorViaAgent } : {}),
    githubActorUserId: opts.githubActorUserId ?? null,
  })
}
