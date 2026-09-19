// EXP-892: the ONE issue-search engine every client runs. The global search
// page, the `#` autocomplete, the duplicate/relation pickers and the
// composer's issue picker all rank the locally synced rows with
// `rankIssueSearch` and then splice the server's full-text hits
// (`issues.search`: title + description + comment bodies, stemmed) in behind
// them with `mergeIssueSearchHits`. Hand-mirrored on iOS (`IssueSearch.swift`),
// Android (`IssueSearch.kt`) and desktop (`domain::issue_search`), byte-locked
// by `packages/domain-contract/fixtures/issue-search.json` — same cases, same
// test names, every platform.
//
// Ranking, per query token (every token must match SOMEWHERE — and semantics;
// a row's score is the sum of its tokens' best field score):
//
//   identifier exact (whole identifier or its number)       100
//   identifier prefix (whole identifier or its number)       80
//   identifier substring                                     60
//   title word prefix                                        50
//   title substring                                          40
//   description word prefix                                  20
//   description substring                                    15
//
// EXP-922: the ordering is THREE keys above the score, in this order:
//
//   1. an EXACT identifier hit (some token scored 100) — typing `EXP-42`
//      finds EXP-42 first whatever state it is in, which is the whole point
//      of typing an identifier;
//   2. UNDONE before done — a closed issue never sits above open work
//      (`ISSUE_SEARCH_CLOSED_STATUSES`: the `done`/`cancelled`/`duplicate`
//      anchors every custom status dual-writes, so customs sort right too;
//      a row with no status at all — an unsynced server hit — counts as
//      open, since a stand-in never outranks a row we actually know);
//   3. the score.
//
// Ties then order by `updatedAt` desc, then `createdAt` desc, then identifier
// number desc. An EMPTY query lists open work first, newest CREATED first
// inside each half (the `#` menu's "recent work" list). Queries and tokens
// drop one leading `#` so `#87` and `fix #87` both find EXP-87.

export interface IssueSearchRow {
  id: string
  identifier: string
  title: string
  description?: string | null
  createdAt?: string | Date | null
  updatedAt?: string | Date | null
  /** EXP-922: the builtin status ANCHOR (`issues.status`) the row dual-writes.
   *  Absent/unknown = treated as open. */
  status?: string | null
}

export interface IssueSearchOptions {
  limit?: number
  exclude?: Iterable<string>
}

const SCORE_IDENTIFIER_EXACT = 100
const SCORE_IDENTIFIER_PREFIX = 80
const SCORE_IDENTIFIER_CONTAINS = 60
const SCORE_TITLE_WORD_PREFIX = 50
const SCORE_TITLE_CONTAINS = 40
const SCORE_DESCRIPTION_WORD_PREFIX = 20
const SCORE_DESCRIPTION_CONTAINS = 15

/** The `limit` every consumer gets without asking — and, since EXP-922, the
 *  one every SEARCH SURFACE uses on all four clients, so the same query
 *  returns the same rows whichever one you type it into. */
export const ISSUE_SEARCH_DEFAULT_LIMIT = 30

/**
 * EXP-922: the builtin status anchors that mean "this issue is finished" —
 * the `completed`/`cancelled`/`duplicate` categories' anchors
 * (`CATEGORY_ANCHOR`), so a team's CUSTOM statuses sort correctly too: they
 * dual-write one of these into `issues.status`. Kept as plain strings rather
 * than importing the schema enum, because the same list is hand-mirrored in
 * three other languages and a search row may carry any wire value.
 */
export const ISSUE_SEARCH_CLOSED_STATUSES = [
  `done`,
  `cancelled`,
  `duplicate`,
] as const

/** Whether a row counts as UNDONE for ranking. An absent or unrecognized
 *  status is open: a server hit that never synced carries none, and a client
 *  that meets a status it does not know must not bury the row. */
export function isIssueSearchOpen(status?: string | null): boolean {
  if (status == null) return true
  return !(ISSUE_SEARCH_CLOSED_STATUSES as readonly string[]).includes(status)
}

/** Trim, drop ONE leading `#`, lowercase. Empty = "recent work". */
export function normalizeIssueSearchQuery(query: string): string {
  let q = query.trim()
  if (q.startsWith(`#`)) q = q.slice(1).trim()
  return q.toLowerCase()
}

/** Whitespace-separated tokens of the normalized query, each shorn of a
 *  leading `#`, empties dropped. */
export function issueSearchTokens(query: string): string[] {
  return normalizeIssueSearchQuery(query)
    .split(/\s+/)
    .map((token) => (token.startsWith(`#`) ? token.slice(1) : token))
    .filter((token) => token.length > 0)
}

const NON_WORD = /[^\p{L}\p{N}]+/u

function words(text: string): string[] {
  return text.split(NON_WORD).filter((word) => word.length > 0)
}

function isDigits(token: string): boolean {
  return token.length > 0 && /^[0-9]+$/.test(token)
}

/** The digits after the identifier's last `-` (`EXP-87` → `87`), or null. */
function identifierNumber(identifier: string): string | null {
  const dash = identifier.lastIndexOf(`-`)
  if (dash < 0) return null
  const tail = identifier.slice(dash + 1)
  return isDigits(tail) ? tail : null
}

/** The numeric tail as a number for ordering (`EXP-87` → 87), -1 when none. */
export function issueIdentifierNumber(identifier: string): number {
  const tail = identifierNumber(identifier)
  return tail === null ? -1 : Number.parseInt(tail, 10)
}

interface Prepared {
  identifier: string
  number: string | null
  title: string
  titleWords: string[]
  readonly description: string
  readonly descriptionWords: string[]
}

/**
 * One row's lowercased fields. The DESCRIPTION is lowercased and word-split
 * LAZILY (the Android mirror's rule): a token that already matched the
 * identifier or the title never touches it, and a description is the only
 * unbounded field a ranked pool carries — every keystroke prepares the whole
 * pool.
 */
function prepare(row: IssueSearchRow): Prepared {
  const title = row.title.toLowerCase()
  const raw = row.description ?? ``
  let description: string | null = null
  let descriptionWords: string[] | null = null
  return {
    identifier: row.identifier.toLowerCase(),
    number: identifierNumber(row.identifier),
    title,
    titleWords: words(title),
    get description(): string {
      description ??= raw.toLowerCase()
      return description
    },
    get descriptionWords(): string[] {
      descriptionWords ??= words(this.description)
      return descriptionWords
    },
  }
}

/** The best field score of `token` against a prepared row, or null when the
 *  token matches nothing. */
function tokenScore(row: Prepared, token: string): number | null {
  if (row.identifier === token || (row.number !== null && row.number === token)) {
    return SCORE_IDENTIFIER_EXACT
  }
  if (
    row.identifier.startsWith(token) ||
    (isDigits(token) && row.number !== null && row.number.startsWith(token))
  ) {
    return SCORE_IDENTIFIER_PREFIX
  }
  if (row.identifier.includes(token)) return SCORE_IDENTIFIER_CONTAINS
  if (row.titleWords.some((word) => word.startsWith(token))) {
    return SCORE_TITLE_WORD_PREFIX
  }
  if (row.title.includes(token)) return SCORE_TITLE_CONTAINS
  if (row.descriptionWords.some((word) => word.startsWith(token))) {
    return SCORE_DESCRIPTION_WORD_PREFIX
  }
  if (row.description.includes(token)) return SCORE_DESCRIPTION_CONTAINS
  return null
}

/** A matched row's two ranking facts: the summed score, and whether any token
 *  hit the identifier EXACTLY (the pin that keeps a typed `EXP-42` on top). */
interface Scored {
  total: number
  exact: boolean
}

function scoreRow(prepared: Prepared, tokens: string[]): Scored | null {
  let total = 0
  let exact = false
  for (const token of tokens) {
    const score = tokenScore(prepared, token)
    if (score === null) return null
    if (score === SCORE_IDENTIFIER_EXACT) exact = true
    total += score
  }
  return { total, exact }
}

/** The row's total score for `tokens`, or null when any token misses. */
export function issueSearchScore(row: IssueSearchRow, tokens: string[]): number | null {
  return scoreRow(prepare(row), tokens)?.total ?? null
}

function timeOf(value: string | Date | null | undefined): number {
  if (value == null) return Number.NEGATIVE_INFINITY
  const ms = value instanceof Date ? value.getTime() : Date.parse(value)
  return Number.isNaN(ms) ? Number.NEGATIVE_INFINITY : ms
}

function compareRecency(a: IssueSearchRow, b: IssueSearchRow): number {
  return (
    timeOf(b.updatedAt) - timeOf(a.updatedAt) ||
    timeOf(b.createdAt) - timeOf(a.createdAt) ||
    issueIdentifierNumber(b.identifier) - issueIdentifierNumber(a.identifier) ||
    (a.identifier < b.identifier ? 1 : a.identifier > b.identifier ? -1 : 0)
  )
}

function compareCreated(a: IssueSearchRow, b: IssueSearchRow): number {
  return (
    timeOf(b.createdAt) - timeOf(a.createdAt) ||
    issueIdentifierNumber(b.identifier) - issueIdentifierNumber(a.identifier) ||
    (a.identifier < b.identifier ? 1 : a.identifier > b.identifier ? -1 : 0)
  )
}

/** EXP-922: undone before done, as a comparator term (open = 1, closed = 0). */
function compareOpen(a: IssueSearchRow, b: IssueSearchRow): number {
  return Number(isIssueSearchOpen(b.status)) - Number(isIssueSearchOpen(a.status))
}

/**
 * Rank the locally synced `rows` for `query`: the scored, ordered, capped
 * list described at the top of this file. Stable for equal keys.
 */
export function rankIssueSearch<T extends IssueSearchRow>(
  rows: readonly T[],
  query: string,
  options?: IssueSearchOptions
): T[] {
  const limit = options?.limit ?? ISSUE_SEARCH_DEFAULT_LIMIT
  const exclude = new Set(options?.exclude ?? [])
  const tokens = issueSearchTokens(query)
  const pool = rows.filter((row) => !exclude.has(row.id))
  if (tokens.length === 0) {
    return [...pool]
      .sort((a, b) => compareOpen(a, b) || compareCreated(a, b))
      .slice(0, limit)
  }
  const scored: Array<{ row: T; score: number; exact: boolean }> = []
  for (const row of pool) {
    const score = scoreRow(prepare(row), tokens)
    if (score !== null) scored.push({ row, score: score.total, exact: score.exact })
  }
  scored.sort(
    (a, b) =>
      Number(b.exact) - Number(a.exact) ||
      compareOpen(a.row, b.row) ||
      b.score - a.score ||
      compareRecency(a.row, b.row)
  )
  return scored.slice(0, limit).map((entry) => entry.row)
}

export interface IssueSearchHit {
  id: string
}

/**
 * Splice the server's full-text hits in behind the locally ranked rows:
 * local order first, then every hit not already listed, in the server's
 * relevance order, deduped by id. `resolve` turns a hit into a renderable
 * row — the synced row when the id is local, a stand-in built from the hit's
 * own fields where the consumer can render one, or null to drop it (a
 * picker whose pool is a subset of the team's issues never widens). The
 * `limit` spans both halves.
 */
export function mergeIssueSearchHits<T extends { id: string }, H extends IssueSearchHit>(
  local: readonly T[],
  hits: readonly H[],
  options: IssueSearchOptions & { resolve: (hit: H) => T | null }
): T[] {
  const limit = options.limit ?? ISSUE_SEARCH_DEFAULT_LIMIT
  const exclude = new Set(options.exclude ?? [])
  const seen = new Set<string>()
  const merged: T[] = []
  for (const row of local) {
    if (exclude.has(row.id) || seen.has(row.id)) continue
    seen.add(row.id)
    merged.push(row)
    if (merged.length >= limit) return merged
  }
  for (const hit of hits) {
    if (exclude.has(hit.id) || seen.has(hit.id)) continue
    const row = options.resolve(hit)
    if (!row) continue
    seen.add(hit.id)
    merged.push(row)
    if (merged.length >= limit) break
  }
  return merged
}
