// EXP-897: the stack PLAN — which issues an issue's run must be built on top
// of, in which order.
//
// The source of truth is the `blocks` relation (canonical direction: the row's
// `issue_id` blocks its `related_issue_id`), walked TRANSITIVELY: if A blocks
// B and B blocks C, starting C stacks on B which stacks on A. The chain is
// returned BOTTOM first and EXCLUDES the started issue, so `chain.at(-1)` —
// exposed as `lower` — is the foundation directly below it.
//
// This is the ONE place a blocking cycle is refused: every other stack surface
// (the launcher, `pr_open`, the merge walk) reads edges that already passed
// through here or through GitHub.
import { and, eq, inArray } from "drizzle-orm"
import { boards, issueRelations, issues, repositories } from "@/db/schema"
import { boardVisible } from "@/lib/board-visibility"
import { canonicalizeRelation, insertRelationInTx } from "@/lib/issue-relations"
import { effectiveBoardBranch } from "@/lib/trpc/repositories"
import type { Context } from "@/lib/trpc"

/** One member of the plan — what every client renders a chain row from. */
export interface StackPlanLink {
  issueId: string
  identifier: string
  title: string
  status: string
  branch: string | null
  prUrl: string | null
  prNumber: number | null
  prState: string | null
}

export interface StackPlan {
  /** Bottom → top, EXCLUDING the issue the plan was resolved for. */
  chain: StackPlanLink[]
  /** `chain.at(-1)` — the foundation directly below, or null (nothing to
   *  stack on; the caller then starts a plain run). */
  lower: StackPlanLink | null
  repositoryId: string
  repoFullName: string
  /** The ref the run's branch is cut from: the lower's OPEN PR branch when it
   *  has one, otherwise the board's effective default branch. */
  base: string
}

/** A `blocks` edge, in canonical direction. */
export interface BlockerEdge {
  /** The blocker. */
  issueId: string
  /** The blocked issue. */
  relatedIssueId: string
}

/** A cycle in the `blocks` graph — refused, never silently broken. */
export class StackCycleError extends Error {
  constructor(public path: string[]) {
    super(
      `Blocking cycle: ${path.join(` → `)}. Fix the relations before stacking.`
    )
  }
}

/** Statuses that take an issue out of a stack: it is already settled. */
const CLOSED_ANCHORS = new Set([`done`, `cancelled`, `duplicate`])

/**
 * Pure: order the transitive blockers of `targetId` BOTTOM first.
 *
 * `identifierOf` only feeds the cycle message. A level with several blockers
 * keeps them all, ordered by identifier — the chain is a stack, and every
 * blocker of the same issue must land below it either way. Issues missing from
 * `open` (done/cancelled/duplicate, or simply not loaded) are dropped WITH
 * their own blockers: a merged foundation is no longer part of the stack.
 */
export function orderBlockerChain(
  targetId: string,
  edges: BlockerEdge[],
  opts: {
    open: (issueId: string) => boolean
    identifierOf: (issueId: string) => string
  }
): string[] {
  const blockersOf = new Map<string, string[]>()
  for (const edge of edges) {
    const bucket = blockersOf.get(edge.relatedIssueId)
    if (bucket) bucket.push(edge.issueId)
    else blockersOf.set(edge.relatedIssueId, [edge.issueId])
  }
  const ordered: string[] = []
  const done = new Set<string>()
  const path: string[] = []
  const onPath = new Set<string>()

  const visit = (id: string) => {
    if (onPath.has(id)) {
      const from = path.indexOf(id)
      throw new StackCycleError(
        [...path.slice(from), id].map((each) => opts.identifierOf(each))
      )
    }
    if (done.has(id)) return
    onPath.add(id)
    path.push(id)
    const blockers = [...new Set(blockersOf.get(id) ?? [])]
      .filter((blocker) => opts.open(blocker))
      .sort((a, b) => opts.identifierOf(a).localeCompare(opts.identifierOf(b)))
    for (const blocker of blockers) visit(blocker)
    path.pop()
    onPath.delete(id)
    done.add(id)
    if (id !== targetId) ordered.push(id)
  }

  visit(targetId)
  return ordered
}

interface PlanIssueRow {
  id: string
  identifier: string
  title: string
  status: string
  boardId: string
  teamId: string
  branch: string | null
  prUrl: string | null
  prNumber: number | null
  prState: string | null
}

const PLAN_ISSUE_COLUMNS = {
  id: issues.id,
  identifier: issues.identifier,
  title: issues.title,
  status: issues.status,
  boardId: issues.boardId,
  teamId: issues.teamId,
  branch: issues.branch,
  prUrl: issues.prUrl,
  prNumber: issues.prNumber,
  prState: issues.prState,
}

function toLink(row: PlanIssueRow): StackPlanLink {
  return {
    issueId: row.id,
    identifier: row.identifier,
    title: row.title,
    status: row.status,
    branch: row.branch,
    prUrl: row.prUrl,
    prNumber: row.prNumber,
    prState: row.prState,
  }
}

/** Repo identity + effective base branch of an issue's board. */
async function boardRepo(
  db: Context[`db`],
  boardId: string
): Promise<{
  repositoryId: string
  fullName: string
  defaultBranch: string
} | null> {
  const [row] = await db
    .select({
      repositoryId: repositories.id,
      fullName: repositories.fullName,
      defaultBranch: repositories.defaultBranch,
      defaultBranchOverride: repositories.defaultBranchOverride,
      boardDefaultBranch: boards.defaultBranch,
    })
    .from(boards)
    .innerJoin(repositories, eq(repositories.id, boards.repositoryId))
    .where(and(eq(boards.id, boardId), boardVisible()))
    .limit(1)
  if (!row) return null
  return {
    repositoryId: row.repositoryId,
    fullName: row.fullName,
    defaultBranch: effectiveBoardBranch(
      { defaultBranch: row.boardDefaultBranch },
      row
    ),
  }
}

/**
 * Resolve the stack an issue's run should be built on.
 *
 * `stackOnIssueId` names the foundation explicitly (the composer's "Stacked
 * PR" pick, MCP's `stackOnIssueId`): the missing `blocks` row is WRITTEN here,
 * once, so the relation the clients nest on and the stack agree from the very
 * first start.
 *
 * Refusals are user-facing sentences (an agent reads them straight out of the
 * MCP result), and a blocker on ANOTHER repository is one: a stack lives in one
 * repository, so the only honest answer is "start it without stacking".
 */
export async function resolveStackChain(
  db: Context[`db`],
  issueId: string,
  opts: { stackOnIssueId?: string; actorUserId?: string | null } = {}
): Promise<StackPlan> {
  const [target] = await db
    .select(PLAN_ISSUE_COLUMNS)
    .from(issues)
    .where(eq(issues.id, issueId))
    .limit(1)
  if (!target) throw new Error(`Issue not found`)

  // The explicit pick is about to become a relation stamped with the target's
  // team and to have its branch and PR handed to the caller: it must exist and
  // belong to the same team, whatever layer resolved the id.
  if (opts.stackOnIssueId && opts.stackOnIssueId !== issueId) {
    const [lowerRow] = await db
      .select(PLAN_ISSUE_COLUMNS)
      .from(issues)
      .where(eq(issues.id, opts.stackOnIssueId))
      .limit(1)
    if (!lowerRow) throw new Error(`The issue to stack on was not found`)
    if (lowerRow.teamId !== target.teamId) {
      throw new Error(
        `${target.identifier} cannot stack on an issue in another team`
      )
    }
  }

  const repo = await boardRepo(db, target.boardId)
  if (!repo) {
    throw new Error(`No repository linked to this board. Link one in team settings.`)
  }

  // The explicit pick becomes a real relation — idempotently (a repeated
  // start, or a pick that only restates what the relations already say, writes
  // nothing).
  if (opts.stackOnIssueId && opts.stackOnIssueId !== issueId) {
    const canonical = canonicalizeRelation(
      opts.stackOnIssueId,
      issueId,
      `blocks`
    )
    await insertRelationInTx(db as never, {
      ...canonical,
      source: `user`,
      teamId: target.teamId,
      actorUserId: opts.actorUserId ?? null,
    })
  }

  // Walk the `blocks` graph upward from the target, level by level — a team's
  // full relation table is never loaded, only the edges reaching the frontier.
  const edges: BlockerEdge[] = []
  const rowsById = new Map<string, PlanIssueRow>([[target.id, target]])
  let frontier = [target.id]
  const expanded = new Set<string>()
  // Guard against a pathological graph: a stack of 20 is already absurd.
  for (let depth = 0; depth < 20 && frontier.length > 0; depth += 1) {
    const pending = frontier.filter((id) => !expanded.has(id))
    for (const id of pending) expanded.add(id)
    if (pending.length === 0) break
    const found = await db
      .select({
        issueId: issueRelations.issueId,
        relatedIssueId: issueRelations.relatedIssueId,
      })
      .from(issueRelations)
      .where(
        and(
          eq(issueRelations.type, `blocks`),
          inArray(issueRelations.relatedIssueId, pending)
        )
      )
    edges.push(...found)
    const nextIds = [
      ...new Set(
        found.map((edge) => edge.issueId).filter((id) => !rowsById.has(id))
      ),
    ]
    if (nextIds.length === 0) break
    const nextRows = await db
      .select(PLAN_ISSUE_COLUMNS)
      .from(issues)
      .where(inArray(issues.id, nextIds))
    for (const row of nextRows) rowsById.set(row.id, row)
    frontier = nextRows.map((row) => row.id)
  }

  const chainIds = orderBlockerChain(target.id, edges, {
    open: (id) => {
      const row = rowsById.get(id)
      return Boolean(row && !CLOSED_ANCHORS.has(row.status))
    },
    identifierOf: (id) => rowsById.get(id)?.identifier ?? id,
  })

  const chain: StackPlanLink[] = []
  for (const id of chainIds) {
    const row = rowsById.get(id)!
    const memberRepo = await boardRepo(db, row.boardId)
    if (!memberRepo || memberRepo.repositoryId !== repo.repositoryId) {
      throw new Error(
        `${target.identifier} is blocked by ${row.identifier}, which is on another repository — start it without stacking.`
      )
    }
    chain.push(toLink(row))
  }

  const lower = chain.length > 0 ? chain[chain.length - 1]! : null
  const base =
    lower && lower.prState === `open` && lower.branch
      ? lower.branch
      : repo.defaultBranch
  return {
    chain,
    lower,
    repositoryId: repo.repositoryId,
    repoFullName: repo.fullName,
    base,
  }
}
