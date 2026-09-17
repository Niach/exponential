// EXP-892: the ONE server-side issue-search predicate. tRPC `issues.search`
// (every client's "search everything" pass, WebMCP `search_issues`) and MCP
// `exponential_issues_list({search})` both match issues through
// `issueSearchMatchIds`, so an agent and a person searching for the same
// words find the same rows.
//
// Three index-friendly branches, unioned (REV-14): Postgres FTS over title +
// description and over comment bodies (the GIN expression indexes
// idx_issues_fts / idx_comments_body_fts — the tsvector expressions below
// must stay byte-identical to @exp/db-schema), plus a tokenized ILIKE
// fallback over the cheap title/identifier columns so identifiers (EXP-42)
// and partial words the stemmer misses still hit. The fallback mirrors the
// client engine (`lib/issue-search.ts`): every whitespace token must match
// the title OR the identifier, a leading `#` is dropped. Description and
// comment SUBSTRING fallbacks stay out on purpose — they would detoast and
// scan megabytes of markdown per keystroke; whole-word matches ride FTS.
import { sql, type SQL } from "drizzle-orm"
import { escapeLikePattern } from "@/lib/like-pattern"
import { issueSearchTokens } from "@/lib/issue-search"

/**
 * The ceiling a caller that searches ACROSS boards passes as `limit`
 * (EXP-892): MCP `exponential_issues_list({search})` may span every board an
 * agent was granted, and a `limit: 1000` list must not make Postgres walk
 * them all. Far above any page an agent asks for, low enough that the worst
 * case is bounded work rather than the whole tracker.
 */
export const ISSUE_SEARCH_SCAN_CAP = 5000

export type IssueSearchScope =
  /** One team, live boards only (the trigger-mirrored trash/archive columns,
   *  REV2-5 / EXP-500 — equivalent to joining boards, without the join). */
  | { teamId: string }
  /** An explicit board list the caller already access-checked (MCP). */
  | { boardIds: readonly string[] }

const ISSUE_TSVECTOR = sql`to_tsvector('english', coalesce(i.title, '') || ' ' || coalesce(i.description, ''))`

function scopeSql(alias: `i` | `c`, scope: IssueSearchScope): SQL {
  const col = (name: string) => sql.raw(`${alias}.${name}`)
  if (`teamId` in scope) {
    return sql`${col(`team_id`)} = ${scope.teamId}::uuid and ${col(`board_deleted_at`)} is null and ${col(`board_archived_at`)} is null`
  }
  if (scope.boardIds.length === 0) return sql`false`
  return sql`${col(`board_id`)} in (${sql.join(
    scope.boardIds.map((id) => sql`${id}::uuid`),
    sql`, `
  )})`
}

/** The FTS query text: the client's tokens re-joined, so `#87`, `fix #87`
 *  and `87` all ask Postgres for the same words. */
export function issueSearchFtsQuery(query: string): string {
  return issueSearchTokens(query).join(` `)
}

/** `ts_rank` of issue alias `i` for `query` — the relevance sort key. */
export function issueSearchRankSql(query: string): SQL {
  return sql`ts_rank(${ISSUE_TSVECTOR}, websearch_to_tsquery('english', ${issueSearchFtsQuery(query)}))`
}

/**
 * `1` when issue alias `i`'s identifier IS the query (case-insensitive),
 * else `0` — an ordering term that belongs AHEAD of `issueSearchRankSql`.
 * A row matched only by the identifier ILIKE branch has ts_rank 0, so
 * without this the exact `EXP-42` the person typed can be cut by `limit`
 * behind rows that merely mention the words. ILIKE with every metacharacter
 * escaped = an exact, case-insensitive compare, not a pattern.
 */
export function issueSearchIdentifierExactSql(query: string): SQL {
  return sql`case when i.identifier ilike ${escapeLikePattern(issueSearchFtsQuery(query))} then 1 else 0 end`
}

/**
 * A `select id` subquery of every issue in `scope` matching `query`, for
 * `... where issues.id in (<here>)`. Returns `select null where false` for a
 * query that normalizes to nothing, so callers need no special case.
 *
 * `limit` caps EACH branch (see `ISSUE_SEARCH_SCAN_CAP`), not just the union:
 * the tokenized ILIKE branch is a scan, and only a LIMIT inside it lets
 * Postgres stop once it has enough. Deliberately no ORDER BY — ordering would
 * force the complete match set to be computed first, which is exactly the
 * work the cap exists to avoid; a capped call trades an arbitrary slice of a
 * huge match set for bounded cost.
 */
export function issueSearchMatchIds(
  query: string,
  scope: IssueSearchScope,
  options: { readonly limit?: number } = {}
): SQL {
  const fts = issueSearchFtsQuery(query)
  const tokens = issueSearchTokens(query)
  if (fts === `` && tokens.length === 0) {
    return sql`select null::uuid as id where false`
  }
  const tokenMatch = sql.join(
    tokens.map((token) => {
      const like = `%${escapeLikePattern(token)}%`
      return sql`(i.title ilike ${like} or i.identifier ilike ${like})`
    }),
    sql` and `
  )
  const branches = [
    sql`
    select i.id from issues i
    where ${scopeSql(`i`, scope)}
      and ${ISSUE_TSVECTOR} @@ websearch_to_tsquery('english', ${fts})`,
    sql`
    select c.issue_id as id from comments c
    where ${scopeSql(`c`, scope)}
      and to_tsvector('english', c.body) @@ websearch_to_tsquery('english', ${fts})`,
    sql`
    select i.id from issues i
    where ${scopeSql(`i`, scope)}
      and ${tokenMatch}`,
  ]
  const { limit } = options
  if (limit === undefined) {
    return sql`${sql.join(branches, sql`
    union`)}
  `
  }
  // Each branch parenthesized so its own LIMIT binds to it and not to the
  // union, then wrapped so the whole thing stays ONE `select id` subquery.
  return sql`select m.id from (${sql.join(
    branches.map((branch) => sql`(${branch}
    limit ${limit})`),
    sql`
    union `
  )}) m`
}
