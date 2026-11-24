// #NEW-135: the ONE server-side issue search, lexical + semantic. tRPC
// `issues.search` (every client's server pass) lists `searchIssues`; MCP
// `exponential_issues_list({search})` filters by `issueSearchIdsSql`. Both
// run the same two rules on top of the lexical predicate
// (lib/issue-search-sql.ts), so a person and an agent find the same issues:
//
//   * a worded query: the lexical hits exactly as before (same order, same
//     limit), THEN the issues that MEAN the same and matched no word: the
//     nearest neighbours of the query's embedding, nearest first, the query's
//     filter tokens applied. Lexical always wins: a semantic hit never
//     reorders or displaces a lexical one, it only fills the room the limit
//     leaves, and at most `ISSUE_SEARCH_SEMANTIC_MAX_HITS` of them;
//   * `similar:<ID>`: the issues nearest to THAT issue's stored embedding
//     (its duplicates), nearest first, itself excluded, narrowed by the
//     query's filter tokens and, when it has words, by the lexical predicate.
//     The identifier resolves inside the caller's scope only: unknown, out of
//     scope, trashed or not embedded yet all answer "no hits", never an
//     error, so the answer never says whether such an issue exists.
//
// Semantic search is OPTIONAL everywhere: a Postgres without pgvector, a
// missing model, `ISSUE_EMBEDDINGS=off`, an embedder that is loading, busy or
// broken, or any failure in this branch leaves exactly the lexical result
// (and `similar:` empty). The embedding of a query is awaited for at most
// `ISSUE_SEARCH_EMBED_BUDGET_MS`, counted from the start of the search, so
// the model can never hold lexical hits back; a vector that arrives late is
// kept for the next keystroke. Only a search with NO lexical hit (nothing to
// hold back) and an agent's listing wait `ISSUE_SEARCH_EMBED_PATIENT_BUDGET_MS`.
//
// PRIVACY: the query text and its vector stay in this process (the embedder
// is a local worker, lib/issue-embeddings/embedder.ts); nothing here opens a
// connection but the one to our own Postgres.
import { sql, type SQL } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  ISSUE_SEARCH_CREATED_PREFIX,
  ISSUE_SEARCH_CREATOR_PREFIX,
  ISSUE_SEARCH_SIMILAR_PREFIX,
  ISSUE_SEARCH_STATE_PREFIX,
  ISSUE_SEARCH_STATUS_PREFIX,
  ISSUE_SEARCH_UPDATED_PREFIX,
  issueSearchHasFilters,
  parseIssueSearchQuery,
} from "@/lib/issue-search"
import {
  issueSearchFilterSql,
  issueSearchIdentifierExactSql,
  issueSearchMatchIds,
  issueSearchOpenSql,
  issueSearchRankSql,
  issueSearchRecencySql,
  scopeSql,
  type IssueSearchScope,
} from "@/lib/issue-search-sql"
import { escapeLikePattern } from "@/lib/like-pattern"
import { embedQuery, embedderConfigured } from "./embedder"
import { ISSUE_EMBEDDING_MODEL } from "./input"
import {
  issueEmbeddingsAvailable,
  issueEmbeddingsTableExists,
  nearestIssueIds,
  type EmbeddingExecutor,
} from "./store"

// ── The ranking knobs (one line each) ───────────────────────────────────────

/** Cosine distance above which a neighbour of a QUERY is not appended.
 *  Measured on a 32-issue German/English tracker (#NEW-135's report): the
 *  issue a query means sits at 0.09 to 0.17, the nearest unrelated one at
 *  0.16 to 0.20 (a one-word or nonsense query: everything at 0.175 to 0.19).
 *  The bands touch, so no value is clean; 0.16 keeps nearly every real match
 *  and nearly no noise, where the store's 0.2 default appended unrelated
 *  issues to almost every query. */
export const ISSUE_SEARCH_SEMANTIC_MAX_DISTANCE = 0.16
/** How many semantic hits at most follow the lexical ones. Small on purpose:
 *  the first neighbour is the issue meant in nearly every measured query,
 *  the ones after it are often unrelated yet inside the cutoff, so the cap
 *  is what keeps noise out of the list. */
export const ISSUE_SEARCH_SEMANTIC_MAX_HITS = 3
/** Cosine distance above which an issue is not `similar:` to another one.
 *  Issue to issue the bands overlap completely (a German/English twin at
 *  0.11 to 0.12, two unrelated short issues as near as 0.10), so this is
 *  only the far tail; the ORDER is what `similar:` offers, and the cap
 *  below keeps the list short. */
export const ISSUE_SEARCH_SIMILAR_MAX_DISTANCE = 0.2
/** How many neighbours `similar:` lists at most (the search and MCP's list). */
export const ISSUE_SEARCH_SIMILAR_MAX_HITS = 10
/** How long a search waits for its query's embedding, from its start. */
export const ISSUE_SEARCH_EMBED_BUDGET_MS = 150
/** The same wait when it can delay nothing: the lexical branches found no
 *  issue (the person is looking at "no match" either way), or the caller is
 *  an agent's listing. Long enough for a cold model to load (about a second
 *  after five idle minutes), so a pasted sentence still gets its answer. */
export const ISSUE_SEARCH_EMBED_PATIENT_BUDGET_MS = 2000
/** Query words shorter than this in total never reach the model. */
export const ISSUE_SEARCH_SEMANTIC_MIN_CHARS = 3

/** The slim row `issues.search` returns. */
export interface IssueSearchHitRow {
  id: string
  identifier: string
  title: string
  boardId: string
  status: string
  statusId: string | null
  priority: string
}

const TOKEN_PREFIXES = [
  ISSUE_SEARCH_CREATOR_PREFIX,
  ISSUE_SEARCH_STATE_PREFIX,
  ISSUE_SEARCH_STATUS_PREFIX,
  ISSUE_SEARCH_CREATED_PREFIX,
  ISSUE_SEARCH_UPDATED_PREFIX,
  ISSUE_SEARCH_SIMILAR_PREFIX,
]

/**
 * What of a query the model reads: its plain words in the person's own
 * spelling (the engine's words are lowercased; German nouns are not), every
 * prefixed token dropped. Null = no semantic branch: no words, fewer than
 * `ISSUE_SEARCH_SEMANTIC_MIN_CHARS` characters, or one bare identifier or
 * number (`EXP-42`, `87`), which names an issue instead of describing one.
 */
export function issueSearchSemanticText(query: string): string | null {
  const words = query
    .trim()
    .split(/\s+/)
    .map((token) => (token.startsWith(`#`) ? token.slice(1) : token))
    .filter((token) => {
      const lower = token.toLowerCase()
      return token.length > 0 && !TOKEN_PREFIXES.some((prefix) => lower.startsWith(prefix))
    })
  const text = words.join(` `)
  if (text.length < ISSUE_SEARCH_SEMANTIC_MIN_CHARS) return null
  if (words.length === 1 && /^(?:\p{L}[\p{L}\p{N}]*-)?[0-9]+$/u.test(text)) return null
  return text
}

/**
 * Lexical first, then the semantic hits not already listed, in their own
 * (nearest-first) order, at most `ISSUE_SEARCH_SEMANTIC_MAX_HITS` of them and
 * never past `limit`.
 */
export function appendSemanticHits<T extends { id: string }>(
  lexical: readonly T[],
  semantic: readonly T[],
  limit: number
): T[] {
  const merged = lexical.slice(0, Math.max(0, limit))
  const seen = new Set(merged.map((row) => row.id))
  let appended = 0
  for (const row of semantic) {
    if (merged.length >= limit || appended >= ISSUE_SEARCH_SEMANTIC_MAX_HITS) break
    if (seen.has(row.id)) continue
    seen.add(row.id)
    merged.push(row)
    appended += 1
  }
  return merged
}

/** Everything the search reads from outside: the database and the model.
 *  `issueSearchBackend` is the real one; the unit tests pass a fake. */
export interface IssueSearchBackend {
  /** pgvector table present and the model on disk. */
  available(): Promise<boolean>
  /** The lexical hits, in `issues.search`'s order. */
  lexical(query: string, scope: IssueSearchScope, limit: number): Promise<IssueSearchHitRow[]>
  /** `embedQuery`: null = cannot answer. */
  embed(text: string): Promise<Float32Array | null>
  /** The stored embedding of the issue `identifier` names inside `scope`. */
  source(identifier: string, scope: IssueSearchScope): Promise<{ issueId: string; vector: number[] } | null>
  /** `nearestIssueIds`. */
  nearest(
    vector: ArrayLike<number>,
    scope: IssueSearchScope,
    options: { k: number; maxDistance: number; filter?: SQL }
  ): Promise<Array<{ issueId: string; distance: number }>>
  /** The slim rows of `ids` (any order). */
  rows(ids: readonly string[]): Promise<IssueSearchHitRow[]>
}

function toHitRow(row: Record<string, unknown>): IssueSearchHitRow {
  return {
    id: row.id as string,
    identifier: row.identifier as string,
    title: row.title as string,
    boardId: row.boardId as string,
    status: row.status as string,
    statusId: (row.statusId as string | null) ?? null,
    priority: row.priority as string,
  }
}

const HIT_COLUMNS = sql`
          i.id,
          i.identifier,
          i.title,
          i.board_id as "boardId",
          i.status,
          i.status_id as "statusId",
          i.priority`

/** The real backend over `executor` (a transaction in the real-DB tests). */
export function issueSearchBackend(
  executor: EmbeddingExecutor = db,
  embed: (text: string) => Promise<Float32Array | null> = embedQuery
): IssueSearchBackend {
  return {
    available: async () =>
      executor === db
        ? issueEmbeddingsAvailable()
        : embedderConfigured() && (await issueEmbeddingsTableExists(executor)),
    lexical: async (query, scope, limit) => {
      // Full-text issue search (EXP-3): Postgres FTS over issue title +
      // description AND comment bodies, relevance-ordered, with a tokenized
      // title/identifier ILIKE fallback (EXP-42); the predicate's notes live
      // in lib/issue-search-sql.ts.
      const result = await executor.execute(sql`
        with matches as (${issueSearchMatchIds(query, scope)})
        select${HIT_COLUMNS}
        from issues i
        join matches m on m.id = i.id
        -- An exact identifier hit sorts FIRST: those rows come from the
        -- ILIKE branch, so their ts_rank is 0 and the limit below would
        -- otherwise drop the very issue the person typed (EXP-892).
        -- EXP-922: then UNDONE before done, above the relevance rank — the
        -- same three keys the client engine sorts by, so the hits spliced in
        -- behind the local ranking carry on in the same order.
        order by
          ${issueSearchIdentifierExactSql(query)} desc,
          ${issueSearchOpenSql()} desc,
          ${issueSearchRankSql(query)} desc,
          ${issueSearchRecencySql(query)}
        limit ${limit}
      `)
      return result.rows.map(toHitRow)
    },
    embed,
    source: async (identifier, scope) => {
      // ILIKE with every metacharacter escaped = an exact, case-insensitive
      // compare. The scope is the search's own: a foreign team's identifier
      // finds no row here, exactly like one that does not exist.
      const result = await executor.execute<{ issue_id: string; embedding: string }>(sql`
        select e.issue_id, e.embedding::text as embedding
        from issues i
        join issue_embeddings e on e.issue_id = i.id
        where ${scopeSql(`i`, scope)}
          and i.identifier ilike ${escapeLikePattern(identifier)}
          and e.model = ${ISSUE_EMBEDDING_MODEL}
        limit 1`)
      const row = result.rows[0]
      if (!row) return null
      const vector = JSON.parse(row.embedding) as number[]
      return { issueId: row.issue_id, vector }
    },
    nearest: (vector, scope, options) => nearestIssueIds(vector, scope, options, executor),
    rows: async (ids) => {
      if (ids.length === 0) return []
      const result = await executor.execute(sql`
        select${HIT_COLUMNS}
        from issues i
        where i.id in (${sql.join(
          ids.map((id) => sql`${id}::uuid`),
          sql`, `
        )})`)
      return result.rows.map(toHitRow)
    },
  }
}

// A query's vector, kept a while: the same words come again (the next
// keystroke's request after a backspace, the same search over another scope),
// and a vector that missed its budget is ready for the request after it.
const VECTOR_CACHE_MAX = 128
const vectorCache = new Map<string, Promise<Float32Array | null>>()

function cachedVector(text: string, embed: IssueSearchBackend[`embed`]): Promise<Float32Array | null> {
  const known = vectorCache.get(text)
  if (known) {
    vectorCache.delete(text)
    vectorCache.set(text, known)
    return known
  }
  const pending = embed(text).catch(() => null)
  vectorCache.set(text, pending)
  void pending.then((vector) => {
    if (!vector && vectorCache.get(text) === pending) vectorCache.delete(text)
  })
  if (vectorCache.size > VECTOR_CACHE_MAX) {
    const oldest = vectorCache.keys().next().value
    if (oldest !== undefined) vectorCache.delete(oldest)
  }
  return pending
}

/** Forgets every kept query vector (tests). */
export function resetIssueSearchVectors(): void {
  vectorCache.clear()
}

/** `promise`, or null once `deadline` (epoch ms) has passed. */
function before<T>(promise: Promise<T>, deadline: number): Promise<T | null> {
  const left = deadline - Date.now()
  if (left <= 0) {
    // Already late: only a settled promise still counts.
    return Promise.race([promise, Promise.resolve().then(() => null)])
  }
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(null), left)
    timer.unref?.()
    void promise.then(
      (value) => {
        clearTimeout(timer)
        resolve(value)
      },
      () => {
        clearTimeout(timer)
        resolve(null)
      }
    )
  })
}

export interface IssueSearchOptions {
  readonly backend?: IssueSearchBackend
  /** Overrides both embed budgets (tests). */
  readonly budgetMs?: number
}

function logFailure(what: string, error: unknown): void {
  console.error(`[issue-search] ${what} failed, lexical only: ${error instanceof Error ? error.message : String(error)}`)
}

/**
 * `similar:<ID>`: the ids nearest to that issue, nearest first, at most
 * `limit` and `ISSUE_SEARCH_SIMILAR_MAX_HITS`. `narrow` = the query's other tokens as a predicate over `i`.
 */
async function similarIds(
  backend: IssueSearchBackend,
  identifier: string,
  scope: IssueSearchScope,
  narrow: SQL | null,
  limit: number
): Promise<string[]> {
  try {
    if (!(await backend.available())) return []
    const source = await backend.source(identifier, scope)
    if (!source) return []
    const self = sql`i.id <> ${source.issueId}::uuid`
    const hits = await backend.nearest(source.vector, scope, {
      k: Math.min(limit, ISSUE_SEARCH_SIMILAR_MAX_HITS),
      maxDistance: ISSUE_SEARCH_SIMILAR_MAX_DISTANCE,
      filter: narrow ? sql`${self} and ${narrow}` : self,
    })
    return hits.map((hit) => hit.issueId)
  } catch (error) {
    logFailure(`similar:`, error)
    return []
  }
}

/** The words and filters a `similar:` query carries besides the token, as a
 *  predicate over issue alias `i`; null when it carries none. */
function similarNarrowSql(query: string, scope: IssueSearchScope): SQL | null {
  const parsed = parseIssueSearchQuery(query)
  if (parsed.words.length === 0 && !issueSearchHasFilters(parsed)) return null
  return sql`i.id in (${issueSearchMatchIds(query, scope)})`
}

/**
 * The search every client's server pass lists: `limit` rows at most, in the
 * order described at the top of this file.
 */
export async function searchIssues(
  query: string,
  scope: IssueSearchScope,
  limit: number,
  options: IssueSearchOptions = {}
): Promise<IssueSearchHitRow[]> {
  const backend = options.backend ?? issueSearchBackend()
  const parsed = parseIssueSearchQuery(query)

  if (parsed.similar.length > 0) {
    const ids = await similarIds(backend, parsed.similar[0]!, scope, similarNarrowSql(query, scope), limit)
    try {
      const byId = new Map((await backend.rows(ids)).map((row) => [row.id, row]))
      return ids.flatMap((id) => byId.get(id) ?? [])
    } catch (error) {
      logFailure(`similar: rows`, error)
      return []
    }
  }

  // The embedding starts NOW, beside the lexical query, and is awaited only
  // until the budget that started with it runs out.
  const started = Date.now()
  const text = issueSearchSemanticText(query)
  const vector: Promise<Float32Array | null> =
    text === null
      ? Promise.resolve(null)
      : backend
          .available()
          .then((ok) => (ok ? cachedVector(text, backend.embed) : null))
          .catch(() => null)

  const lexical = await backend.lexical(query, scope, limit)
  if (text === null || lexical.length >= limit) return lexical
  try {
    const budget =
      options.budgetMs ?? (lexical.length === 0 ? ISSUE_SEARCH_EMBED_PATIENT_BUDGET_MS : ISSUE_SEARCH_EMBED_BUDGET_MS)
    const found = await before(vector, started + budget)
    if (!found) return lexical
    const listed =
      lexical.length === 0
        ? null
        : sql`i.id not in (${sql.join(
            lexical.map((row) => sql`${row.id}::uuid`),
            sql`, `
          )})`
    const filters = issueSearchHasFilters(parsed) ? issueSearchFilterSql(parsed) : null
    const filter = listed && filters ? sql`${listed} and ${filters}` : (listed ?? filters ?? undefined)
    const hits = await backend.nearest(found, scope, {
      k: Math.min(ISSUE_SEARCH_SEMANTIC_MAX_HITS, limit - lexical.length),
      maxDistance: ISSUE_SEARCH_SEMANTIC_MAX_DISTANCE,
      filter,
    })
    if (hits.length === 0) return lexical
    const byId = new Map((await backend.rows(hits.map((hit) => hit.issueId))).map((row) => [row.id, row]))
    return appendSemanticHits(
      lexical,
      hits.flatMap((hit) => byId.get(hit.issueId) ?? []),
      limit
    )
  } catch (error) {
    logFailure(`the semantic branch`, error)
    return lexical
  }
}

function idListSql(ids: readonly string[]): SQL {
  if (ids.length === 0) return sql`select null::uuid as id where false`
  return sql`select s.id from unnest(array[${sql.join(
    ids.map((id) => sql`${id}::uuid`),
    sql`, `
  )}]) as s(id)`
}

/**
 * `issueSearchMatchIds` plus the semantic rules, for a caller that filters a
 * listing of its own (MCP `exponential_issues_list({search})`): a
 * `select id` subquery of the lexical matches and the semantic hits
 * `searchIssues` would append behind them (the same neighbours: nearest
 * first among the issues no lexical branch matched, the same cap and
 * cutoff), or, for a `similar:` query, of that issue's neighbours alone.
 * `limit` caps each lexical branch like `issueSearchMatchIds`' own.
 */
export async function issueSearchIdsSql(
  query: string,
  scope: IssueSearchScope,
  options: IssueSearchOptions & { readonly limit?: number } = {}
): Promise<SQL> {
  const backend = options.backend ?? issueSearchBackend()
  const parsed = parseIssueSearchQuery(query)
  if (parsed.similar.length > 0) {
    return idListSql(
      await similarIds(backend, parsed.similar[0]!, scope, similarNarrowSql(query, scope), ISSUE_SEARCH_SIMILAR_MAX_HITS)
    )
  }
  const lexical = issueSearchMatchIds(query, scope, { limit: options.limit })
  const text = issueSearchSemanticText(query)
  if (text === null) return lexical
  try {
    const deadline = Date.now() + (options.budgetMs ?? ISSUE_SEARCH_EMBED_PATIENT_BUDGET_MS)
    if (!(await backend.available())) return lexical
    const found = await before(cachedVector(text, backend.embed), deadline)
    if (!found) return lexical
    const unlisted = sql`i.id not in (${lexical})`
    const hits = await backend.nearest(found, scope, {
      k: ISSUE_SEARCH_SEMANTIC_MAX_HITS,
      maxDistance: ISSUE_SEARCH_SEMANTIC_MAX_DISTANCE,
      filter: issueSearchHasFilters(parsed) ? sql`${unlisted} and ${issueSearchFilterSql(parsed)}` : unlisted,
    })
    if (hits.length === 0) return lexical
    return sql`select u.id from (${lexical}
    union
    ${idListSql(hits.map((hit) => hit.issueId))}) u`
  } catch (error) {
    logFailure(`the semantic branch`, error)
    return lexical
  }
}
