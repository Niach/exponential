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
//   creator name word prefix (the member or the agent)       10
//
// #NEW-21: a `by:<name>` token is a CREATOR filter, not a scored word: the
// row stays only when its member's or agent's name has a word starting with
// <name>, or the member's email starts with it (`by:lukas`, `by:lukas.m`,
// `by:@triage`: one leading `@` dropped; an empty `by:` is ignored). A row
// with no creator never matches one. A query of `by:` tokens alone lists
// the matching rows in the empty-query order below.
//
// #NEW-132: four more FILTER tokens (never scored; every filter must hold;
// an empty or unparseable value is ignored like an empty `by:`). They are
// the structured syntax natural language translates into (#NEW-131):
//
//   is:open | is:closed   the row is undone / finished (`isIssueSearchOpen`)
//   status:<word>         a word of the status ANCHOR (`in_review` = in,
//                         review) or of the team status NAME (`statusName`)
//                         starts with <word>; a row with neither never matches
//   created:<range>       the row's createdAt / updatedAt lies in <range>;
//   updated:<range>       a row without that timestamp never matches
//
// A <range> is `<n>d|w|m|y` (the last n×1/7/30/365 days before `now`,
// n = 1…9999), a UTC day `YYYY-MM-DD` or month `YYYY-MM`, a comparison
// against one (`>D` after it ends, `>=D` from its start, `<D` before its
// start, `<=D` until its end), or `D..D` (the first's start to the second's
// end). Impossible dates (`2026-13`, `2026-02-30`) are ignored. A query of
// filters alone lists the matching rows in the empty-query order.
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
  /** #NEW-21: the member who filed it (`issues.creator_id`'s name + email). */
  creatorName?: string | null
  creatorEmail?: string | null
  /** #NEW-21: the agent whose session filed it (`issues.creator_agent_id`). */
  creatorAgentName?: string | null
  /** #NEW-132: the team status row's NAME (`issues.status_id`), for `status:`. */
  statusName?: string | null
}

export interface IssueSearchOptions {
  limit?: number
  exclude?: Iterable<string>
  /** #NEW-132: the clock relative `created:`/`updated:` ranges count back
   *  from (epoch ms or a Date); default = now. */
  now?: number | Date
}

const SCORE_IDENTIFIER_EXACT = 100
const SCORE_IDENTIFIER_PREFIX = 80
const SCORE_IDENTIFIER_CONTAINS = 60
const SCORE_TITLE_WORD_PREFIX = 50
const SCORE_TITLE_CONTAINS = 40
const SCORE_DESCRIPTION_WORD_PREFIX = 20
const SCORE_DESCRIPTION_CONTAINS = 15
const SCORE_CREATOR_WORD_PREFIX = 10

/** #NEW-21: the token prefix that turns a word into a creator filter. */
export const ISSUE_SEARCH_CREATOR_PREFIX = `by:`

/** #NEW-132: the filter-token prefixes. */
export const ISSUE_SEARCH_STATE_PREFIX = `is:`
export const ISSUE_SEARCH_STATUS_PREFIX = `status:`
export const ISSUE_SEARCH_CREATED_PREFIX = `created:`
export const ISSUE_SEARCH_UPDATED_PREFIX = `updated:`

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

/**
 * EXP-922: the ONE copy set every search surface renders — the web sheet, the
 * desktop ⌘K palette, the iOS Search tab and the Android Search screen. Same
 * words everywhere, so the four surfaces read as one feature; drift-gated by
 * `issue-search-surfaces.test.ts`, which greps the three native files for these
 * exact strings.
 */
export const ISSUE_SEARCH_PLACEHOLDER = `Search issues`
export const ISSUE_SEARCH_EMPTY_HINT = `Search issues across this team's boards.`
export const ISSUE_SEARCH_EMPTY_DETAIL = `Matches identifiers, titles, full text, and by:creator.`
export const ISSUE_SEARCH_NO_RESULTS = `No issues match`

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

/** #NEW-132: a half-open time window `[from, to)` in epoch ms (±Infinity
 *  = unbounded). */
export interface IssueSearchRange {
  from: number
  to: number
}

/** #NEW-21 + #NEW-132: a query split into its scored words and its filters. */
export interface IssueSearchQuery {
  words: string[]
  /** `by:` needles (prefix and one leading `@` dropped). */
  creators: string[]
  /** `is:` values: true = open, false = closed. */
  states: boolean[]
  /** `status:` needles. */
  statuses: string[]
  created: IssueSearchRange[]
  updated: IssueSearchRange[]
}

/** Whether a parsed query filters at all (a filter-only query lists the
 *  matching rows in the empty-query order). */
export function issueSearchHasFilters(parsed: IssueSearchQuery): boolean {
  return (
    parsed.creators.length > 0 ||
    parsed.states.length > 0 ||
    parsed.statuses.length > 0 ||
    parsed.created.length > 0 ||
    parsed.updated.length > 0
  )
}

const DAY_MS = 86_400_000
const RELATIVE_DAYS: Record<string, number> = { d: 1, w: 7, m: 30, y: 365 }

/** Days since 1970-01-01 of a proleptic Gregorian UTC date (Howard
 *  Hinnant's days_from_civil), the same integer arithmetic ×4. */
function daysFromCivil(year: number, month: number, day: number): number {
  const y = month <= 2 ? year - 1 : year
  const era = Math.floor(y / 400)
  const yoe = y - era * 400
  const mp = (month + 9) % 12
  const doy = Math.floor((153 * mp + 2) / 5) + day - 1
  const doe = yoe * 365 + Math.floor(yoe / 4) - Math.floor(yoe / 100) + doy
  return era * 146_097 + doe - 719_468
}

function daysInMonth(year: number, month: number): number {
  if (month === 2) return (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0 ? 29 : 28
  return [4, 6, 9, 11].includes(month) ? 30 : 31
}

/** A UTC day `YYYY-MM-DD` or month `YYYY-MM` as its window, or null. */
function parseIssueSearchDate(text: string): IssueSearchRange | null {
  const match = /^([0-9]{4})-([0-9]{2})(?:-([0-9]{2}))?$/.exec(text)
  if (!match) return null
  const year = Number(match[1])
  const month = Number(match[2])
  if (month < 1 || month > 12) return null
  if (match[3] === undefined) {
    const from = daysFromCivil(year, month, 1) * DAY_MS
    return { from, to: from + daysInMonth(year, month) * DAY_MS }
  }
  const day = Number(match[3])
  if (day < 1 || day > daysInMonth(year, month)) return null
  const from = daysFromCivil(year, month, day) * DAY_MS
  return { from, to: from + DAY_MS }
}

/** #NEW-132: a `created:`/`updated:` value as its window, or null. */
export function parseIssueSearchRange(value: string, now: number): IssueSearchRange | null {
  const relative = /^([0-9]{1,4})([dwmy])$/.exec(value)
  if (relative) {
    const count = Number(relative[1])
    if (count < 1) return null
    return { from: now - count * RELATIVE_DAYS[relative[2]!]! * DAY_MS, to: Number.POSITIVE_INFINITY }
  }
  const comparators: Array<[string, (d: IssueSearchRange) => IssueSearchRange]> = [
    [`>=`, (d) => ({ from: d.from, to: Number.POSITIVE_INFINITY })],
    [`<=`, (d) => ({ from: Number.NEGATIVE_INFINITY, to: d.to })],
    [`>`, (d) => ({ from: d.to, to: Number.POSITIVE_INFINITY })],
    [`<`, (d) => ({ from: Number.NEGATIVE_INFINITY, to: d.from })],
  ]
  for (const [op, window] of comparators) {
    if (!value.startsWith(op)) continue
    const date = parseIssueSearchDate(value.slice(op.length))
    return date ? window(date) : null
  }
  const dots = value.indexOf(`..`)
  if (dots >= 0) {
    const start = parseIssueSearchDate(value.slice(0, dots))
    const end = parseIssueSearchDate(value.slice(dots + 2))
    return start && end ? { from: start.from, to: end.to } : null
  }
  return parseIssueSearchDate(value)
}

/** #NEW-21 + #NEW-132: the query split into its scored words and its filter
 *  tokens. Every prefixed token leaves the words, valid or not. */
export function parseIssueSearchQuery(
  query: string,
  now: number | Date = Date.now()
): IssueSearchQuery {
  const nowMs = now instanceof Date ? now.getTime() : now
  const parsed: IssueSearchQuery = {
    words: [],
    creators: [],
    states: [],
    statuses: [],
    created: [],
    updated: [],
  }
  for (const token of issueSearchTokens(query)) {
    if (token.startsWith(ISSUE_SEARCH_CREATOR_PREFIX)) {
      let needle = token.slice(ISSUE_SEARCH_CREATOR_PREFIX.length)
      if (needle.startsWith(`@`)) needle = needle.slice(1)
      if (needle.length > 0) parsed.creators.push(needle)
    } else if (token.startsWith(ISSUE_SEARCH_STATE_PREFIX)) {
      const value = token.slice(ISSUE_SEARCH_STATE_PREFIX.length)
      if (value === `open`) parsed.states.push(true)
      else if (value === `closed`) parsed.states.push(false)
    } else if (token.startsWith(ISSUE_SEARCH_STATUS_PREFIX)) {
      const needle = token.slice(ISSUE_SEARCH_STATUS_PREFIX.length)
      if (needle.length > 0) parsed.statuses.push(needle)
    } else if (token.startsWith(ISSUE_SEARCH_CREATED_PREFIX)) {
      const range = parseIssueSearchRange(token.slice(ISSUE_SEARCH_CREATED_PREFIX.length), nowMs)
      if (range) parsed.created.push(range)
    } else if (token.startsWith(ISSUE_SEARCH_UPDATED_PREFIX)) {
      const range = parseIssueSearchRange(token.slice(ISSUE_SEARCH_UPDATED_PREFIX.length), nowMs)
      if (range) parsed.updated.push(range)
    } else {
      parsed.words.push(token)
    }
  }
  return parsed
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
  /** #NEW-21: the member's and the agent's name words, and the email. */
  creatorWords: string[]
  creatorEmail: string
  /** #NEW-132: the status anchor's and the status name's words. */
  statusWords: string[]
  /** #EXP-1382: the timestamps parsed ONCE per row (cached across keystrokes). */
  updatedMs: number
  createdMs: number
  identifierNumber: number
}

// #EXP-1382: a row's preparation is cached by row identity, so a keystroke
// re-scores the pool without re-lowercasing titles or re-parsing timestamps;
// a changed row is a new object (Electric replaces rows), so it re-prepares.
const PREPARED = new WeakMap<IssueSearchRow, Prepared>()

function preparedOf(row: IssueSearchRow): Prepared {
  let prepared = PREPARED.get(row)
  if (!prepared) {
    prepared = prepare(row)
    PREPARED.set(row, prepared)
  }
  return prepared
}

/**
 * One row's lowercased fields. The DESCRIPTION is lowercased and word-split
 * LAZILY (the Android mirror's rule): a token that already matched the
 * identifier or the title never touches it, and a description is the only
 * unbounded field a ranked pool carries. Cached per row (`preparedOf`).
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
    creatorWords: [
      ...words((row.creatorName ?? ``).toLowerCase()),
      ...words((row.creatorAgentName ?? ``).toLowerCase()),
    ],
    creatorEmail: (row.creatorEmail ?? ``).toLowerCase(),
    statusWords: [
      ...words((row.status ?? ``).toLowerCase()),
      ...words((row.statusName ?? ``).toLowerCase()),
    ],
    updatedMs: timeOf(row.updatedAt),
    createdMs: timeOf(row.createdAt),
    identifierNumber: issueIdentifierNumber(row.identifier),
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
  if (row.creatorWords.some((word) => word.startsWith(token))) {
    return SCORE_CREATOR_WORD_PREFIX
  }
  return null
}

/** #NEW-21: whether a prepared row was filed by every `by:` needle. */
function matchesCreators(row: Prepared, creators: readonly string[]): boolean {
  return creators.every(
    (needle) =>
      row.creatorWords.some((word) => word.startsWith(needle)) ||
      (row.creatorEmail !== `` && row.creatorEmail.startsWith(needle))
  )
}

function inRanges(ms: number, ranges: readonly IssueSearchRange[]): boolean {
  return ranges.every((range) => Number.isFinite(ms) && ms >= range.from && ms < range.to)
}

/** #NEW-21 + #NEW-132: whether a row passes every filter token. */
function matchesFilters(row: IssueSearchRow, parsed: IssueSearchQuery): boolean {
  const prepared = preparedOf(row)
  if (!matchesCreators(prepared, parsed.creators)) return false
  const open = isIssueSearchOpen(row.status)
  if (!parsed.states.every((state) => state === open)) return false
  if (
    !parsed.statuses.every((needle) =>
      prepared.statusWords.some((word) => word.startsWith(needle))
    )
  ) {
    return false
  }
  return inRanges(prepared.createdMs, parsed.created) && inRanges(prepared.updatedMs, parsed.updated)
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
  return scoreRow(preparedOf(row), tokens)?.total ?? null
}

function timeOf(value: string | Date | null | undefined): number {
  if (value == null) return Number.NEGATIVE_INFINITY
  const ms = value instanceof Date ? value.getTime() : Date.parse(value)
  return Number.isNaN(ms) ? Number.NEGATIVE_INFINITY : ms
}

function compareRecency(a: IssueSearchRow, b: IssueSearchRow): number {
  const pa = preparedOf(a)
  const pb = preparedOf(b)
  return (
    pb.updatedMs - pa.updatedMs ||
    pb.createdMs - pa.createdMs ||
    pb.identifierNumber - pa.identifierNumber ||
    (a.identifier < b.identifier ? 1 : a.identifier > b.identifier ? -1 : 0)
  )
}

function compareCreated(a: IssueSearchRow, b: IssueSearchRow): number {
  const pa = preparedOf(a)
  const pb = preparedOf(b)
  return (
    pb.createdMs - pa.createdMs ||
    pb.identifierNumber - pa.identifierNumber ||
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
  const parsed = parseIssueSearchQuery(query, options?.now ?? Date.now())
  const tokens = parsed.words
  const filtered = issueSearchHasFilters(parsed)
  const pool = rows.filter(
    (row) => !exclude.has(row.id) && (!filtered || matchesFilters(row, parsed))
  )
  if (tokens.length === 0) {
    return [...pool]
      .sort((a, b) => compareOpen(a, b) || compareCreated(a, b))
      .slice(0, limit)
  }
  const scored: Array<{ row: T; score: number; exact: boolean }> = []
  for (const row of pool) {
    const score = scoreRow(preparedOf(row), tokens)
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
