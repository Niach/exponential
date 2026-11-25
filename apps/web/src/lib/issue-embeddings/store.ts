// #NEW-134: the `issue_embeddings` table (db/out/custom/0002): whether it
// exists, and the k-nearest-issues query #NEW-135 puts behind `issues.search`.
//
// The table is created at boot ONLY on a Postgres that ships pgvector, and is
// in no Drizzle schema, so everything here is raw SQL behind
// `issueEmbeddingsAvailable()`: on a stock Postgres every caller is a no-op.
import { sql, type SQL } from "drizzle-orm"
import { db } from "@/db/connection"
import { scopeSql, type IssueSearchScope } from "@/lib/issue-search-sql"
import { embedderConfigured } from "./embedder"
import { ISSUE_EMBEDDING_MODEL, vectorLiteral } from "./input"

// eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
type Tx = Parameters<Parameters<typeof import("@/db/connection").db.transaction>[0]>[0]
export type EmbeddingExecutor = typeof db | Tx

/** How long a "the table is there" answer is trusted, and a "not there" one:
 *  the table appears at a boot (never mid-run), so the short one only bounds
 *  how long a server that booted before its Postgres image was switched (or
 *  raced the boot SQL) stays lexical-only. */
const AVAILABLE_TTL_MS = 60 * 60 * 1000
const UNAVAILABLE_TTL_MS = 5 * 60 * 1000

/** Uncached: does this database have the embeddings table? */
export async function issueEmbeddingsTableExists(executor: EmbeddingExecutor = db): Promise<boolean> {
  const result = await executor.execute<{ ok: boolean }>(
    sql`select to_regclass('public.issue_embeddings') is not null as ok`
  )
  return result.rows[0]?.ok === true
}

let tableCheck: { at: number; ok: boolean } | null = null

/**
 * True when this server can embed AND store: the feature is not switched off,
 * the model is on disk and the database has the table. Cached (an hour once
 * true, five minutes while false); never throws: a database error is "no".
 */
export async function issueEmbeddingsAvailable(): Promise<boolean> {
  if (!embedderConfigured()) return false
  const now = Date.now()
  if (tableCheck && now - tableCheck.at < (tableCheck.ok ? AVAILABLE_TTL_MS : UNAVAILABLE_TTL_MS)) {
    return tableCheck.ok
  }
  let ok = false
  try {
    ok = await issueEmbeddingsTableExists()
  } catch {
    ok = false
  }
  tableCheck = { at: now, ok }
  return ok
}

/** Forgets the cached answer (tests). */
export function resetIssueEmbeddingsAvailability(): void {
  tableCheck = null
}

/**
 * The default cosine-DISTANCE cutoff (1 - cosine similarity) of
 * `nearestIssueIds`. A SOFT bound, not a relevance test: e5 vectors sit close
 * together and the bands overlap. Measured with this model (int8) in
 * issue-embeddings.real-db.test.ts: a query and the issue it means land at
 * 0.09 to 0.19, the other issues at 0.16 to 0.28; two short unrelated issues
 * in the SAME language can be as near as 0.11 to each other. What is
 * reliable is the ORDER (the right issues come first, across German and
 * English); #NEW-135 should rank and blend by distance and treat this value
 * only as "drop the clearly unrelated tail".
 */
export const ISSUE_EMBEDDING_MAX_DISTANCE = 0.2
export const ISSUE_EMBEDDING_DEFAULT_K = 20

export interface NearestIssueOptions {
  /** How many issues at most (default 20). */
  readonly k?: number
  /** Cosine distance above which a neighbour is not a match (default 0.2). */
  readonly maxDistance?: number
  /** #NEW-135: one more predicate over issue alias `i` (the query's filter
   *  tokens, ids already listed): a neighbour failing it is skipped, not
   *  counted against `k`. */
  readonly filter?: SQL
}

/**
 * `select issue_id, distance` of the issues in `scope` nearest to `vector`,
 * nearest first. The scope is `issueSearchMatchIds`' own (`scopeSql`):
 * member-checked teams or boards, live boards only, no trashed issue. Rows
 * embedded by another model are never compared.
 *
 * Run it through `nearestIssueIds`, which turns on pgvector's iterative index
 * scan: without it an HNSW scan stops after its first candidates, and a team
 * that owns a small share of the table would get fewer than `k` back.
 */
export function nearestIssuesSql(
  vector: ArrayLike<number>,
  scope: IssueSearchScope,
  options: NearestIssueOptions = {}
): SQL {
  const k = Math.max(1, Math.min(200, Math.floor(options.k ?? ISSUE_EMBEDDING_DEFAULT_K)))
  const maxDistance = options.maxDistance ?? ISSUE_EMBEDDING_MAX_DISTANCE
  const query = sql`${vectorLiteral(vector)}::vector`
  // The team narrowing repeated on the embeddings table itself: for a team
  // with few rows the planner can take idx_issue_embeddings_team and sort
  // exactly instead of walking the whole graph.
  const team =
    `teamId` in scope
      ? sql`e.team_id = ${scope.teamId}::uuid and `
      : `teamIds` in scope && scope.teamIds.length > 0
        ? sql`e.team_id in (${sql.join(
            scope.teamIds.map((id) => sql`${id}::uuid`),
            sql`, `
          )}) and `
        : sql``
  return sql`
    select e.issue_id, (e.embedding <=> ${query})::float8 as distance
    from issue_embeddings e
    join issues i on i.id = e.issue_id
    where ${team}e.model = ${ISSUE_EMBEDDING_MODEL}
      and ${scopeSql(`i`, scope)}
      and (e.embedding <=> ${query}) <= ${maxDistance}${options.filter ? sql`
      and (${options.filter})` : sql``}
    order by e.embedding <=> ${query}
    limit ${k}`
}

/**
 * The `k` issues in `scope` nearest to `vector` (an `embedQuery` result)
 * within the distance cutoff, nearest first. Empty when the table is absent.
 */
export async function nearestIssueIds(
  vector: ArrayLike<number>,
  scope: IssueSearchScope,
  options: NearestIssueOptions = {},
  executor: EmbeddingExecutor = db
): Promise<Array<{ issueId: string; distance: number }>> {
  if (executor === db ? !(await issueEmbeddingsAvailable()) : !(await issueEmbeddingsTableExists(executor))) return []
  const rows = await executor.transaction(async (tx) => {
    await tx.execute(sql`set local hnsw.iterative_scan = relaxed_order`)
    const result = await tx.execute<{ issue_id: string; distance: number }>(nearestIssuesSql(vector, scope, options))
    return result.rows
  })
  return rows
    .map((row) => ({ issueId: row.issue_id, distance: Number(row.distance) }))
    .sort((a, b) => a.distance - b.distance)
}
