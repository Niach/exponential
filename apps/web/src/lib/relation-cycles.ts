import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"
import type { db as database } from "@/db/connection"
import { issueRelations, issues } from "@/db/schema"

// EXP-980: the transitive cycle check for the two DIRECTED relation types.
// `blocks` and `parent` are graphs people (and the workflow engine) schedule
// over: A blocks B blocks C blocks A is never a state anyone meant to reach,
// nothing on it can ever start, and the UNIQUE pair index cannot see it. The
// old guard only refused the direct reverse edge.
//
// A canonical row reads `issue_id` → `related_issue_id` (blocker → blocked,
// parent → child). Adding `from → to` closes a cycle exactly when `from` is
// already reachable from `to`.

type Tx = Parameters<Parameters<typeof database.transaction>[0]>[0]
type Executor = typeof database | Tx

export type DirectedRelationType = `blocks` | `parent`

export function isDirectedRelationType(type: string): type is DirectedRelationType {
  return type === `blocks` || type === `parent`
}

/** A walk this wide is not a graph anyone drew by hand; give up rather than
 *  scan the team. Refusing is the safe answer for a graph we cannot judge. */
const MAX_VISITED = 5000

export class RelationCycleTooLargeError extends Error {
  constructor() {
    super(`The relation graph is too large to check for cycles`)
  }
}

/**
 * The cycle adding `issueId → relatedIssueId` would close, as issue ids in
 * walking order starting AND ending at `issueId` (`[A, B, C, A]`), or null
 * when the edge is safe. Breadth-first, so the cycle named is a shortest one.
 */
export async function findRelationCycle(
  executor: Executor,
  edge: { issueId: string; relatedIssueId: string; type: DirectedRelationType }
): Promise<string[] | null> {
  if (edge.issueId === edge.relatedIssueId) return [edge.issueId, edge.issueId]

  const cameFrom = new Map<string, string>()
  const visited = new Set<string>([edge.relatedIssueId])
  let frontier = [edge.relatedIssueId]
  while (frontier.length > 0) {
    const rows = await executor
      .select({
        from: issueRelations.issueId,
        to: issueRelations.relatedIssueId,
      })
      .from(issueRelations)
      .where(
        and(
          eq(issueRelations.type, edge.type),
          inArray(issueRelations.issueId, frontier)
        )
      )
    const next: string[] = []
    // Sorted, so the path named is the same whatever order the rows came in.
    rows.sort((a, b) => (a.to < b.to ? -1 : a.to > b.to ? 1 : 0))
    for (const row of rows) {
      if (visited.has(row.to)) continue
      visited.add(row.to)
      cameFrom.set(row.to, row.from)
      if (row.to === edge.issueId) {
        const back = [row.to]
        let cursor: string | undefined = row.from
        while (cursor !== undefined) {
          back.push(cursor)
          cursor = cameFrom.get(cursor)
        }
        // `back` = [A, …, C, B]; the walk is A → B → C → … → A.
        return [edge.issueId, ...back.reverse()]
      }
      next.push(row.to)
    }
    if (visited.size > MAX_VISITED) throw new RelationCycleTooLargeError()
    frontier = next
  }
  return null
}

/** The refusal sentence: the cycle spelled out by identifier. */
export function relationCycleMessage(
  type: DirectedRelationType,
  identifiers: readonly string[]
): string {
  const verb = type === `blocks` ? `blocks` : `is the parent of`
  const path = identifiers.join(` → `)
  return identifiers.length <= 3
    ? `The opposite relation already exists (${path})`
    : `This would create a cycle: ${path} (each ${verb} the next)`
}

/** Throws `BAD_REQUEST` naming the cycle when `issueId → relatedIssueId`
 *  would close one. */
export async function assertNoRelationCycle(
  executor: Executor,
  edge: { issueId: string; relatedIssueId: string; type: DirectedRelationType }
): Promise<void> {
  let cycle: string[] | null
  try {
    cycle = await findRelationCycle(executor, edge)
  } catch (err) {
    if (err instanceof RelationCycleTooLargeError) {
      throw new TRPCError({ code: `BAD_REQUEST`, message: err.message })
    }
    throw err
  }
  if (!cycle) return

  const rows = await executor
    .select({ id: issues.id, identifier: issues.identifier })
    .from(issues)
    .where(inArray(issues.id, [...new Set(cycle)]))
  const identifierOf = new Map(rows.map((row) => [row.id, row.identifier]))
  throw new TRPCError({
    code: `BAD_REQUEST`,
    message: relationCycleMessage(
      edge.type,
      cycle.map((id) => identifierOf.get(id) ?? `?`)
    ),
  })
}
