// EXP-1146: yolo mode (EXP-1105, `teams.yolo_mode`) merges a run's PR TREE
// once the tree is complete, root first.
//
// Follow-up runs are the default (SLOP-3): a run opens its PR, then starts a
// child run per follow-up it filed (`exponential_sessions_start`, the child
// carries `parent_session_id`), each child bases on its parent's branch and
// opens `pr_open({ base: <parent branch> })`. Merging at `pr_open` — what
// EXP-1105 did — is wrong twice: the root lands before its children exist
// (their base is squashed under them) and a child would land INSIDE its
// parent's branch. So a PR whose run may still spawn children is never merged
// at `pr_open`; instead every lifecycle edge (`pr_open`, the `agent_busy`
// false edge, `sessions_end`, the client end, the kill, the stale sweep)
// calls the ONE idempotent `maybeMergeYoloTree(sessionId)`, which walks to
// the root run, checks completeness and merges.
//
// The tree = the `parent_session_id` subtree under the root run, resume
// succession followed (`resumed_from_id`: a resume is a new row that inherits
// the parent and re-stamps the children; a superseded row is dropped, its
// successor speaks for it). Each run's PR is its own `pr_*` row when stamped
// (chore PRs today, every form once SLOP-9 lands), else its issue's PR, else
// — a batch row — the PR on the issue sharing its branch.
//
// Complete = every run is ended, or idle (`agent_busy` false) with its PR open
// (or already merged). A person-started root never ends; idle with its PR open
// counts. A live run without a PR blocks the tree; an ended one without a PR
// is skipped and named in the owner message. Completeness is stable at the
// moment the last run goes idle: a run spawns its children at the end of its
// turn and `sessions_start` polls for the child row before it returns.
//
// Merge order = by PR BASE, session-tree BFS as the tiebreak: a tree PR merges
// only once its base ref is no other tree PR's head. After every merged tree
// PR the EXP-324 heal (`retargetChildrenOfMergedPr`) is AWAITED, so the
// children move onto the default branch before their turn; rounds repeat
// until nothing progresses. For a follow-up tree that IS root first, one
// squash commit per PR; it also never lands a child into a merged-but-kept
// parent branch when the heal silently bailed. A child that is `dirty` after
// the retarget stays open (Fix merge conflicts is one click away in Reviews)
// and nothing else waits on it except its own descendants.
//
// A LIVE run survives the merge of its own PR exactly as it does today
// through the MCP `pr_merge` closure: `merged_own_pr` is stamped before its
// merge and reverted when that merge does not land
// (lib/sessions/merged-own-pr.ts). `endSessionsOnMerge` is untouched for
// everybody else. The server enforces no caps; those live in the playbook.
//
// Dependency-injected (`YoloDeps`) so the planner and the executor are unit
// tested against fakes; `defaultDeps()` binds the real DB, GitHub and tRPC.
// The tRPC router is loaded LAZILY: the session router is one of the hook
// sites and a static `appRouter` import from here would be a cycle.
import { and, eq, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { db } from "@/db/connection"
import { codingSessions, repositories, teams, users } from "@/db/schema"
import type { Context } from "@/lib/trpc"
import {
  loadSessionChain,
  MAX_SESSION_CHAIN_DEPTH,
} from "@/lib/steer-child-messages"
import {
  repoFromPrUrl,
  retargetChildrenOfMergedPr,
} from "@/lib/integrations/pr-sync"
import { getPullRequest } from "@/lib/integrations/github-pr"
import { resolveRepoInstallationTokenInfo } from "@/lib/integrations/github-app"
import { sendAgentMessage } from "@/lib/integrations/notifications"
import {
  priorOf,
  revertMergedOwnPr,
  stampMergedOwnPr,
  type MergedOwnPrPrior,
} from "@/lib/sessions/merged-own-pr"
import { buildServerActorContext } from "@/lib/trpc/synthetic-context"

// ---------------------------------------------------------------------------
// Rows → tree
// ---------------------------------------------------------------------------

/** One `coding_sessions` row of the subtree, with the PR candidates joined. */
export interface YoloRunRow {
  id: string
  parentSessionId: string | null
  resumedFromId: string | null
  userId: string
  status: string
  agentBusy: boolean
  needsInput: boolean
  issueId: string | null
  batchIssueIds: string[] | null
  actionName: string | null
  branch: string | null
  startedAt: string | Date
  prUrl: string | null
  prNumber: number | null
  prState: string | null
  workflowId: string | null
  /** The issue on `issue_id`. */
  issueIdentifier: string | null
  issuePrUrl: string | null
  issuePrNumber: number | null
  issuePrState: string | null
  issueBranch: string | null
  /** A batch row: the issue sharing the run's `branch` (SLOP-9 fallback). */
  batchIssueId: string | null
  batchIdentifier: string | null
  batchPrUrl: string | null
  batchPrNumber: number | null
  batchPrState: string | null
}

export type YoloRunKind = `issue` | `batch` | `chore`

export interface YoloRun {
  sessionId: string
  userId: string
  /** 0 = the root. */
  depth: number
  startedAt: number
  kind: YoloRunKind
  /** `EXP-12`, `EXP-12 +2`, the action's name, else `Chat`. */
  name: string
  status: string
  live: boolean
  agentBusy: boolean
  needsInput: boolean
  /** The issue `issues.mergePr` lands the PR through (issue and batch kinds). */
  mergeIssueId: string | null
  prUrl: string | null
  prNumber: number | null
  prState: string | null
  /** The PR's head branch as the DB knows it (GitHub refines it). */
  headBranch: string | null
  /** A chat/action run that owns no PR: done once idle, never named. */
  prless: boolean
}

export interface YoloTree {
  teamId: string
  rootSessionId: string
  ownerUserId: string
  rootName: string
  rootIssueId: string | null
  runs: YoloRun[]
}

function batchName(row: YoloRunRow): string {
  const total = Math.max(row.batchIssueIds?.length ?? 0, row.batchIdentifier ? 1 : 0)
  if (!row.batchIdentifier) return `Batch run`
  return total > 1 ? `${row.batchIdentifier} +${total - 1}` : row.batchIdentifier
}

function runOf(row: YoloRunRow, depth: number): YoloRun {
  const kind: YoloRunKind = row.issueId
    ? `issue`
    : row.batchIssueIds?.length
      ? `batch`
      : `chore`
  // The run's OWN stamp wins (SLOP-9: every form stamps the caller row), then
  // the issue on `issue_id`, then the batch issue sharing the branch.
  const pr = row.prUrl
    ? { url: row.prUrl, number: row.prNumber, state: row.prState }
    : row.issuePrUrl
      ? { url: row.issuePrUrl, number: row.issuePrNumber, state: row.issuePrState }
      : row.batchPrUrl
        ? { url: row.batchPrUrl, number: row.batchPrNumber, state: row.batchPrState }
        : null
  const name =
    kind === `issue`
      ? (row.issueIdentifier ?? `Issue run`)
      : kind === `batch`
        ? batchName(row)
        : (row.actionName?.trim() || `Chat`)
  const mergeIssueId =
    kind === `issue`
      ? row.issueId
      : kind === `batch`
        ? (row.batchIssueId ?? row.batchIssueIds?.[0] ?? null)
        : null
  return {
    sessionId: row.id,
    userId: row.userId,
    depth,
    startedAt: new Date(row.startedAt).getTime(),
    kind,
    name,
    status: row.status,
    live: row.status !== `ended`,
    agentBusy: Boolean(row.agentBusy),
    needsInput: Boolean(row.needsInput),
    mergeIssueId,
    prUrl: pr?.url ?? null,
    prNumber: pr?.number ?? null,
    prState: pr?.state ?? null,
    headBranch: row.issueBranch ?? row.branch ?? null,
    prless: kind === `chore` && !pr,
  }
}

/**
 * Pure: the subtree rows (the root's whole resume succession plus everything
 * reachable over `parent_session_id` and `resumed_from_id`) → the logical
 * runs in BFS order. A row another row resumed is SUPERSEDED and dropped; a
 * child still pointing at a superseded parent (the re-stamp is best-effort)
 * nests under that parent's newest successor.
 */
export function assembleYoloTree(
  rows: YoloRunRow[],
  rootSuccessionIds: string[],
  teamId: string
): YoloTree | null {
  const byId = new Map<string, YoloRunRow>()
  for (const row of rows) byId.set(row.id, row)
  const successorOf = new Map<string, string>()
  for (const row of rows) {
    if (row.resumedFromId && byId.has(row.resumedFromId)) {
      successorOf.set(row.resumedFromId, row.id)
    }
  }
  const canonical = (id: string): string => {
    let current = id
    let hops = 0
    while (successorOf.has(current) && hops < MAX_SESSION_CHAIN_DEPTH) {
      current = successorOf.get(current)!
      hops += 1
    }
    return current
  }
  const rootSet = new Set(rootSuccessionIds)
  const rootRow = rows.find((row) => rootSet.has(row.id) && !successorOf.has(row.id))
  if (!rootRow) return null
  const live = rows.filter((row) => !successorOf.has(row.id))
  const childrenOf = new Map<string, YoloRunRow[]>()
  for (const row of live) {
    if (row.id === rootRow.id) continue
    const parent = row.parentSessionId ? canonical(row.parentSessionId) : null
    if (!parent || !byId.has(parent)) continue
    const list = childrenOf.get(parent) ?? []
    list.push(row)
    childrenOf.set(parent, list)
  }
  const runs: YoloRun[] = []
  const seen = new Set<string>()
  const queue: { row: YoloRunRow; depth: number }[] = [{ row: rootRow, depth: 0 }]
  while (queue.length > 0) {
    const { row, depth } = queue.shift()!
    if (seen.has(row.id)) continue
    seen.add(row.id)
    runs.push(runOf(row, depth))
    const children = (childrenOf.get(row.id) ?? []).sort(
      (a, b) =>
        new Date(a.startedAt).getTime() - new Date(b.startedAt).getTime()
    )
    for (const child of children) queue.push({ row: child, depth: depth + 1 })
  }
  const root = runs[0]!
  return {
    teamId,
    rootSessionId: root.sessionId,
    ownerUserId: root.userId,
    rootName: root.name,
    rootIssueId: root.mergeIssueId,
    runs,
  }
}

// ---------------------------------------------------------------------------
// Planner
// ---------------------------------------------------------------------------

export interface YoloPlan {
  complete: boolean
  /** Live runs still working or without a PR — what the tree waits on. */
  blocking: YoloRun[]
  /** Ended runs that never got a PR (or whose PR closed unmerged). */
  skippedNoPr: YoloRun[]
  /** Runs with a PR (open or merged), BFS order. */
  order: YoloRun[]
}

function hasPr(run: YoloRun): boolean {
  return (
    run.prUrl !== null &&
    run.prNumber !== null &&
    (run.prState === `open` || run.prState === `merged` || run.prState === `draft`)
  )
}

/** Pure: is the tree complete, and in which order do its PRs land. */
export function planYoloTree(tree: YoloTree): YoloPlan {
  const blocking: YoloRun[] = []
  const skippedNoPr: YoloRun[] = []
  const order: YoloRun[] = []
  for (const run of tree.runs) {
    const withPr = hasPr(run)
    const done = !run.live || (!run.agentBusy && (withPr || run.prless))
    if (!done) blocking.push(run)
    if (!run.live && !withPr && !run.prless) skippedNoPr.push(run)
    if (withPr) order.push(run)
  }
  return { complete: blocking.length === 0, blocking, skippedNoPr, order }
}

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

export type YoloOutcomeKind =
  | `merged`
  | `merged_before`
  | `queued`
  | `dirty`
  | `failed`
  | `waiting`
  | `closed`

export interface YoloOutcome {
  kind: YoloOutcomeKind
  /** `failed`: GitHub's or the router's message. `waiting`: the base head. */
  detail?: string
}

export interface YoloPull {
  state: `open` | `closed`
  merged: boolean
  draft: boolean
  headRef: string
  baseRef: string
  mergeable: boolean | null
  mergeableState: string | null
}

export interface YoloMergeOutcome {
  merged: boolean
  queued?: boolean
}

export interface YoloDeps {
  loadTree: (sessionId: string) => Promise<YoloTree | null>
  getPull: (repo: string, prNumber: number) => Promise<YoloPull>
  /** The EXP-324 heal, awaited: children of `headBranch` onto the default. */
  retarget: (opts: { prUrl: string; headBranch: string }) => Promise<void>
  mergeIssuePr: (run: YoloRun, issueId: string) => Promise<YoloMergeOutcome>
  mergeChorePr: (
    run: YoloRun,
    repo: string,
    prNumber: number,
    teamId: string
  ) => Promise<YoloMergeOutcome>
  stampOwnPr: (sessionId: string) => Promise<void>
  revertOwnPr: (sessionId: string, prior: MergedOwnPrPrior) => Promise<void>
  notifyOwner: (args: {
    teamId: string
    ownerIds: string[]
    title: string
    body: string
    issueId: string | null
  }) => Promise<void>
  sleep: (ms: number) => Promise<void>
  log: (message: string, err?: unknown) => void
}

/** GitHub computes mergeability lazily after a base move or a squash onto the
 *  default branch: `mergeable` reads null for a moment. Re-read on these
 *  delays (≈10 s in all) before trusting the state. */
const MERGEABLE_POLL_MS = [1_000, 2_000, 3_000, 4_000]
/** After a retarget the base ref itself may lag one read. */
const RETARGET_SETTLE_MS = 2_000
/** EXP-1146: how long a child's base gets to leave a merged tree branch. Our
 *  own heal is synchronous, but a REAL GitHub stack member is retargeted by
 *  GitHub itself, seconds after the merge below it lands; one read after
 *  RETARGET_SETTLE_MS called such a child failed, its owner got a message and
 *  nothing retried (an unattended child never fires another edge). The same
 *  ladder as the mergeability poll, ≈12 s in all. */
const BASE_MOVE_POLL_MS = [RETARGET_SETTLE_MS, ...MERGEABLE_POLL_MS]

function isConflictRefusal(err: unknown): boolean {
  if (err instanceof TRPCError) return err.code === `CONFLICT`
  const message = err instanceof Error ? err.message : String(err)
  return /not mergeable|merge conflicts?/i.test(message)
}

export interface YoloExecution {
  outcomes: Map<string, YoloOutcome>
  mergedNow: number
}

export async function executeYoloTree(
  tree: YoloTree,
  plan: YoloPlan,
  deps: YoloDeps
): Promise<YoloExecution> {
  const outcomes = new Map<string, YoloOutcome>()
  let mergedNow = 0
  // head branch → the tree run on it. Seeded from the DB, refined by every
  // GitHub read (a batch row's branch may be unknown to the DB).
  const runByHead = new Map<string, YoloRun>()
  const mergedHeads = new Set<string>()
  for (const run of plan.order) {
    if (run.headBranch) runByHead.set(run.headBranch, run)
  }
  const heal = async (run: YoloRun, headBranch: string) => {
    try {
      await deps.retarget({ prUrl: run.prUrl!, headBranch })
    } catch (err) {
      deps.log(`[yolo-tree] retarget after ${run.name} failed:`, err)
    }
  }

  // A tree PR merged BEFORE this pass (a root the legacy path landed before
  // its children existed, a person merging by hand): its children still need
  // the heal, which is idempotent.
  const remaining: YoloRun[] = []
  for (const run of plan.order) {
    if (run.prState === `merged`) {
      outcomes.set(run.sessionId, { kind: `merged_before` })
      if (run.headBranch) {
        mergedHeads.add(run.headBranch)
        await heal(run, run.headBranch)
      }
    } else {
      remaining.push(run)
    }
  }

  let progress = true
  while (remaining.length > 0 && progress) {
    progress = false
    for (const run of [...remaining]) {
      const drop = () => {
        const index = remaining.indexOf(run)
        if (index >= 0) remaining.splice(index, 1)
      }
      const repo = repoFromPrUrl(run.prUrl!)
      if (!repo) {
        outcomes.set(run.sessionId, {
          kind: `failed`,
          detail: `${run.prUrl} is not a GitHub PR URL`,
        })
        drop()
        continue
      }
      let pull: YoloPull
      try {
        pull = await deps.getPull(repo, run.prNumber!)
      } catch (err) {
        outcomes.set(run.sessionId, {
          kind: `failed`,
          detail: err instanceof Error ? err.message : String(err),
        })
        drop()
        continue
      }
      if (pull.headRef) runByHead.set(pull.headRef, run)
      if (pull.merged) {
        outcomes.set(run.sessionId, { kind: `merged_before` })
        if (pull.headRef) {
          mergedHeads.add(pull.headRef)
          await heal(run, pull.headRef)
        }
        drop()
        progress = true
        continue
      }
      if (pull.state === `closed`) {
        outcomes.set(run.sessionId, { kind: `closed` })
        drop()
        continue
      }
      if (pull.draft) {
        outcomes.set(run.sessionId, { kind: `failed`, detail: `the PR is a draft` })
        drop()
        continue
      }
      // Based on another tree PR's head: wait for that PR to land and the
      // heal to move this one — never merge INTO a tree branch.
      const onAnotherTreeHead = (p: YoloPull) =>
        runByHead.has(p.baseRef) && runByHead.get(p.baseRef) !== run
      if (onAnotherTreeHead(pull)) {
        if (!mergedHeads.has(pull.baseRef)) continue
        let readError: unknown = null
        for (const delay of BASE_MOVE_POLL_MS) {
          await deps.sleep(delay)
          try {
            pull = await deps.getPull(repo, run.prNumber!)
          } catch (err) {
            readError = err
            break
          }
          if (!onAnotherTreeHead(pull)) break
        }
        if (readError) {
          outcomes.set(run.sessionId, {
            kind: `failed`,
            detail: readError instanceof Error ? readError.message : String(readError),
          })
          drop()
          continue
        }
        if (onAnotherTreeHead(pull)) {
          if (mergedHeads.has(pull.baseRef)) {
            outcomes.set(run.sessionId, {
              kind: `failed`,
              detail: `still based on the merged branch ${pull.baseRef}; retarget it onto the default branch (exponential_pr_retarget) and merge again`,
            })
            drop()
          }
          continue
        }
      }
      for (const delay of MERGEABLE_POLL_MS) {
        if (pull.mergeable !== null) break
        await deps.sleep(delay)
        try {
          pull = await deps.getPull(repo, run.prNumber!)
        } catch {
          break
        }
      }
      if (pull.mergeableState === `dirty`) {
        outcomes.set(run.sessionId, { kind: `dirty` })
        drop()
        continue
      }
      // The run's own merge: a LIVE run survives it (EXP-637 decision 6).
      const prior = run.live ? priorOf(run) : null
      if (prior) {
        try {
          await deps.stampOwnPr(run.sessionId)
        } catch (err) {
          deps.log(`[yolo-tree] merged_own_pr stamp on ${run.name} failed:`, err)
        }
      }
      try {
        const result =
          run.kind === `chore` || !run.mergeIssueId
            ? await deps.mergeChorePr(run, repo, run.prNumber!, tree.teamId)
            : await deps.mergeIssuePr(run, run.mergeIssueId)
        if (result.queued) {
          // GitHub's queue holds it; nothing below it can land this pass, and
          // the stamp stays — a queued merge is the run's own success in flight.
          outcomes.set(run.sessionId, { kind: `queued` })
          drop()
          continue
        }
        outcomes.set(run.sessionId, { kind: `merged` })
        mergedNow += 1
        drop()
        progress = true
        const head = pull.headRef || run.headBranch
        if (head) {
          mergedHeads.add(head)
          await heal(run, head)
        }
      } catch (err) {
        if (prior) {
          try {
            await deps.revertOwnPr(run.sessionId, prior)
          } catch (revertErr) {
            deps.log(`[yolo-tree] merged_own_pr revert on ${run.name} failed:`, revertErr)
          }
        }
        outcomes.set(
          run.sessionId,
          isConflictRefusal(err)
            ? { kind: `dirty` }
            : {
                kind: `failed`,
                detail: err instanceof Error ? err.message : String(err),
              }
        )
        drop()
      }
    }
  }
  // Whatever is left waits on a tree PR that did not land (dirty or failed).
  for (const run of remaining) {
    outcomes.set(run.sessionId, {
      kind: `waiting`,
      detail: run.headBranch ?? undefined,
    })
  }
  return { outcomes, mergedNow }
}

// ---------------------------------------------------------------------------
// The idempotent entry point
// ---------------------------------------------------------------------------

export type YoloMergeResult =
  | { status: `not_yolo` }
  | { status: `in_flight` }
  | { status: `incomplete`; blocking: string[] }
  | { status: `nothing` }
  | {
      status: `merged`
      mergedNow: number
      outcomes: Record<string, YoloOutcome>
    }
  | { status: `error`; error: string }

// One pass per root at a time (single-instance deploy, like the pr-open and
// pr-merge claims). A trigger landing mid-pass flags a rerun: the pass loops
// once more on a fresh snapshot, so a child that appeared meanwhile is not
// lost until its next edge.
const inFlight = new Map<string, { rerun: boolean }>()
// The leftovers already reported per root: an idle person-started root fires
// the busy edge on every turn, and the same open PRs must not message the
// owner every time.
const reported = new Map<string, string>()

/** Test hook: forget every in-flight and reported root. */
export function resetYoloTreeState(): void {
  inFlight.clear()
  reported.clear()
}

function describeLeftovers(
  tree: YoloTree,
  plan: YoloPlan,
  execution: YoloExecution
): { lines: string[]; signature: string; firstIssueId: string | null } {
  const lines: string[] = []
  const parts: string[] = []
  let firstIssueId: string | null = null
  const runByHead = new Map<string, YoloRun>()
  for (const run of tree.runs) if (run.headBranch) runByHead.set(run.headBranch, run)
  for (const run of plan.order) {
    const outcome = execution.outcomes.get(run.sessionId)
    if (!outcome) continue
    const pr = `#${run.prNumber} ${run.name}`
    switch (outcome.kind) {
      case `dirty`:
        lines.push(
          `${pr}: conflicts on the default branch after the retarget. It stays open; Fix merge conflicts is one click away in Reviews.`
        )
        break
      case `failed`:
        lines.push(`${pr}: not merged — ${outcome.detail ?? `unknown error`}`)
        break
      case `waiting`: {
        const base = outcome.detail ? runByHead.get(outcome.detail) : undefined
        lines.push(
          `${pr}: waits on ${base ? `#${base.prNumber} ${base.name}` : `a PR above it`}, which did not land.`
        )
        break
      }
      case `closed`:
        lines.push(`${pr}: its PR was closed unmerged; skipped.`)
        break
      default:
        continue
    }
    parts.push(`${run.sessionId}:${outcome.kind}`)
    if (!firstIssueId && run.mergeIssueId) firstIssueId = run.mergeIssueId
  }
  for (const run of plan.skippedNoPr) {
    lines.push(`${run.name}: ended without a PR; skipped.`)
    parts.push(`${run.sessionId}:no_pr`)
  }
  return { lines, signature: parts.sort().join(`,`), firstIssueId }
}

async function runPass(tree: YoloTree, deps: YoloDeps): Promise<YoloMergeResult> {
  const plan = planYoloTree(tree)
  if (!plan.complete) {
    return { status: `incomplete`, blocking: plan.blocking.map((run) => run.name) }
  }
  // Nothing to land (no PR in the tree, or every one merged already): no
  // pass, no message — a person who killed their own PR-less run is not told
  // about it.
  if (plan.order.length === 0 || plan.order.every((run) => run.prState === `merged`)) {
    return { status: `nothing` }
  }
  const execution = await executeYoloTree(tree, plan, deps)
  const leftovers = describeLeftovers(tree, plan, execution)
  if (leftovers.lines.length > 0) {
    if (reported.get(tree.rootSessionId) !== leftovers.signature) {
      reported.set(tree.rootSessionId, leftovers.signature)
      const ownerIds = [tree.ownerUserId]
      for (const run of plan.order) {
        const outcome = execution.outcomes.get(run.sessionId)
        if (outcome?.kind === `dirty` && !ownerIds.includes(run.userId)) {
          ownerIds.push(run.userId)
        }
      }
      try {
        await deps.notifyOwner({
          teamId: tree.teamId,
          ownerIds,
          title: `Yolo merge of ${tree.rootName}: ${execution.mergedNow} merged, ${leftovers.lines.length} need you`,
          body: leftovers.lines.join(`\n`),
          issueId: leftovers.firstIssueId ?? tree.rootIssueId,
        })
      } catch (err) {
        deps.log(`[yolo-tree] owner message for ${tree.rootName} failed:`, err)
      }
    }
  } else {
    reported.delete(tree.rootSessionId)
  }
  const outcomes: Record<string, YoloOutcome> = {}
  for (const [id, outcome] of execution.outcomes) outcomes[id] = outcome
  return { status: `merged`, mergedNow: execution.mergedNow, outcomes }
}

/**
 * The one trigger every lifecycle edge calls. Idempotent and never throws:
 * a merge must never fail the edge that fired it (a turn end, a close-out).
 * Cheap when it should be: the first read returns before any tree walk for a
 * team that is not in yolo mode.
 */
export async function maybeMergeYoloTree(
  sessionId: string,
  overrides: Partial<YoloDeps> = {}
): Promise<YoloMergeResult> {
  const deps: YoloDeps = { ...defaultDeps(), ...overrides }
  try {
    const tree = await deps.loadTree(sessionId)
    if (!tree) return { status: `not_yolo` }
    const running = inFlight.get(tree.rootSessionId)
    if (running) {
      running.rerun = true
      return { status: `in_flight` }
    }
    const mine = { rerun: false }
    inFlight.set(tree.rootSessionId, mine)
    try {
      let current = tree
      for (;;) {
        mine.rerun = false
        const result = await runPass(current, deps)
        if (!mine.rerun) return result
        const reloaded = await deps.loadTree(sessionId)
        if (!reloaded) return result
        current = reloaded
      }
    } finally {
      inFlight.delete(tree.rootSessionId)
    }
  } catch (err) {
    deps.log(`[yolo-tree] merge pass for ${sessionId} failed:`, err)
    return { status: `error`, error: err instanceof Error ? err.message : String(err) }
  }
}

// ---------------------------------------------------------------------------
// Real dependencies
// ---------------------------------------------------------------------------

/**
 * The tree the session belongs to, or null when there is nothing for yolo to
 * do: the team is not in yolo mode, the row is gone, or a run is a workflow
 * node (those land through the workflow's merge train, as EXP-1105 left it).
 */
export async function loadYoloTree(
  database: Context[`db`],
  sessionId: string
): Promise<YoloTree | null> {
  const [row] = await database
    .select({
      teamId: codingSessions.teamId,
      workflowId: codingSessions.workflowId,
      yoloMode: teams.yoloMode,
    })
    .from(codingSessions)
    .innerJoin(teams, eq(teams.id, codingSessions.teamId))
    .where(eq(codingSessions.id, sessionId))
    .limit(1)
  if (!row || !row.yoloMode || row.workflowId) return null

  const chain = await loadSessionChain(database, sessionId)
  if (!chain) return null

  // The root's resume succession, oldest first: the row `loadSessionChain`
  // stopped at may have been resumed since (its successor inherits the
  // parent, so the up-walk never passes through it).
  const succession = await database.execute(sql`
    with recursive succession as (
      select cs.id, 0 as hops
      from coding_sessions cs
      where cs.id = ${chain.rootSessionId}::uuid
      union all
      select s.id, succession.hops + 1
      from coding_sessions s
      join succession on s.resumed_from_id = succession.id
      where succession.hops < ${MAX_SESSION_CHAIN_DEPTH}
    )
    select id from succession order by hops asc
  `)
  const rootIds = (succession.rows ?? []).map((r) => r.id as string)
  if (rootIds.length === 0) return null

  const result = await database.execute(sql`
    with recursive down as (
      select cs.id, 0 as depth
      from coding_sessions cs
      where cs.id in (${sql.join(
        rootIds.map((id) => sql`${id}::uuid`),
        sql`, `
      )})
      union all
      select c.id, d.depth + 1
      from coding_sessions c
      join down d on c.parent_session_id = d.id or c.resumed_from_id = d.id
      where d.depth < ${MAX_SESSION_CHAIN_DEPTH}
    )
    select cs.id, cs.parent_session_id, cs.resumed_from_id, cs.user_id, cs.status,
           cs.agent_busy, cs.needs_input, cs.issue_id, cs.batch_issue_ids,
           cs.action_name, cs.branch, cs.started_at, cs.pr_url, cs.pr_number,
           cs.pr_state, cs.workflow_id,
           i.identifier as issue_identifier, i.pr_url as issue_pr_url,
           i.pr_number as issue_pr_number, i.pr_state as issue_pr_state,
           i.branch as issue_branch,
           b.id as batch_issue_id, b.identifier as batch_identifier,
           b.pr_url as batch_pr_url, b.pr_number as batch_pr_number,
           b.pr_state as batch_pr_state
    from (select distinct id from down) d
    join coding_sessions cs on cs.id = d.id
    left join issues i on i.id = cs.issue_id
    left join lateral (
      select bi.id, bi.identifier, bi.pr_url, bi.pr_number, bi.pr_state
      from issues bi
      where cs.issue_id is null and cs.branch is not null
        and bi.branch = cs.branch and bi.team_id = cs.team_id
      order by bi.created_at asc
      limit 1
    ) b on true
    where cs.team_id = ${row.teamId}::uuid
  `)
  const rows: YoloRunRow[] = (result.rows ?? []).map((r) => ({
    id: r.id as string,
    parentSessionId: (r.parent_session_id as string | null) ?? null,
    resumedFromId: (r.resumed_from_id as string | null) ?? null,
    userId: r.user_id as string,
    status: r.status as string,
    agentBusy: Boolean(r.agent_busy),
    needsInput: Boolean(r.needs_input),
    issueId: (r.issue_id as string | null) ?? null,
    batchIssueIds: (r.batch_issue_ids as string[] | null) ?? null,
    actionName: (r.action_name as string | null) ?? null,
    branch: (r.branch as string | null) ?? null,
    startedAt: r.started_at as string | Date,
    prUrl: (r.pr_url as string | null) ?? null,
    prNumber: (r.pr_number as number | null) ?? null,
    prState: (r.pr_state as string | null) ?? null,
    workflowId: (r.workflow_id as string | null) ?? null,
    issueIdentifier: (r.issue_identifier as string | null) ?? null,
    issuePrUrl: (r.issue_pr_url as string | null) ?? null,
    issuePrNumber: (r.issue_pr_number as number | null) ?? null,
    issuePrState: (r.issue_pr_state as string | null) ?? null,
    issueBranch: (r.issue_branch as string | null) ?? null,
    batchIssueId: (r.batch_issue_id as string | null) ?? null,
    batchIdentifier: (r.batch_identifier as string | null) ?? null,
    batchPrUrl: (r.batch_pr_url as string | null) ?? null,
    batchPrNumber: (r.batch_pr_number as number | null) ?? null,
    batchPrState: (r.batch_pr_state as string | null) ?? null,
  }))
  // A workflow node anywhere in the tree: the merge train owns it.
  if (rows.some((r) => r.workflowId)) return null
  return assembleYoloTree(rows, rootIds, row.teamId)
}

type Caller = ReturnType<
  Awaited<typeof import("@/routes/api/trpc/$")>[`appRouter`][`createCaller`]
>

function defaultDeps(): YoloDeps {
  const tokens = new Map<string, Promise<string | null>>()
  const tokenFor = (repo: string) => {
    let pending = tokens.get(repo)
    if (!pending) {
      pending = resolveRepoInstallationTokenInfo(repo).then(
        (resolved) => resolved?.token ?? null
      )
      tokens.set(repo, pending)
    }
    return pending
  }
  const callers = new Map<string, Promise<Caller>>()
  const callerFor = (userId: string) => {
    let pending = callers.get(userId)
    if (!pending) {
      pending = (async () => {
        const [user] = await db
          .select({
            id: users.id,
            email: users.email,
            name: users.name,
            image: users.image,
            emailVerified: users.emailVerified,
            createdAt: users.createdAt,
            updatedAt: users.updatedAt,
          })
          .from(users)
          .where(eq(users.id, userId))
          .limit(1)
        if (!user) throw new Error(`Run owner ${userId} not found`)
        // Lazy: the session router (a hook site) is part of appRouter.
        const { appRouter } = await import(`@/routes/api/trpc/$`)
        return appRouter.createCaller(
          buildServerActorContext(
            { ...user, image: user.image ?? null },
            `yolo`,
            { viaMcp: true }
          )
        )
      })()
      callers.set(userId, pending)
    }
    return pending
  }
  return {
    loadTree: (sessionId) => loadYoloTree(db, sessionId),
    getPull: async (repo, prNumber) =>
      getPullRequest(repo, prNumber, await tokenFor(repo)),
    retarget: (opts) => retargetChildrenOfMergedPr(opts),
    mergeIssuePr: async (run, issueId) => {
      const caller = await callerFor(run.userId)
      const result = await caller.issues.mergePr({ issueId })
      return { merged: result.merged, queued: result.queued === true }
    },
    mergeChorePr: async (run, repo, prNumber, teamId) => {
      const [repository] = await db
        .select({ id: repositories.id })
        .from(repositories)
        .where(
          and(eq(repositories.teamId, teamId), eq(repositories.fullName, repo))
        )
        .limit(1)
      if (!repository) {
        throw new Error(`${repo} is not a repository of this team`)
      }
      const caller = await callerFor(run.userId)
      await caller.repositories.mergePull({ repositoryId: repository.id, prNumber })
      return { merged: true }
    },
    stampOwnPr: (sessionId) => stampMergedOwnPr(db, sessionId),
    revertOwnPr: (sessionId, prior) => revertMergedOwnPr(db, sessionId, prior),
    notifyOwner: async ({ teamId, ownerIds, title, body, issueId }) => {
      await sendAgentMessage({
        teamId,
        senderUserId: ownerIds[0]!,
        recipientIds: ownerIds,
        title,
        body,
        issueId,
      })
    },
    sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
    log: (message, err) => console.error(message, err),
  }
}
