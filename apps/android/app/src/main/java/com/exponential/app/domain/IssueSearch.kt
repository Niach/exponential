package com.exponential.app.domain

/**
 * EXP-892: the ONE issue-search engine every client runs. The Search tab, the
 * `#` autocomplete, the duplicate/relation pickers and the composer's issue
 * picker all rank the locally synced rows with [rank] and then splice the
 * server's full-text hits (`issues.search`: title + description + comment
 * bodies, stemmed) in behind them with [mergeServerHits]. Hand-mirrored on the
 * web (`lib/issue-search.ts` — the reference), iOS (`IssueSearch.swift`) and
 * desktop (`domain::issue_search`), byte-locked by
 * `packages/domain-contract/fixtures/issue-search.json` — same cases, same
 * test names, every platform.
 *
 * Ranking, per query token (every token must match SOMEWHERE — and semantics;
 * a row's score is the sum of its tokens' best field score):
 *
 *   identifier exact (whole identifier or its number)       100
 *   identifier prefix (whole identifier or its number)       80
 *   identifier substring                                     60
 *   title word prefix                                        50
 *   title substring                                          40
 *   description word prefix                                  20
 *   description substring                                    15
 *   creator name word prefix (the member or the agent)       10
 *
 * #NEW-21: a `by:<name>` token is a CREATOR filter, not a scored word: the
 * row stays only when its member's or agent's name has a word starting with
 * <name>, or the member's email starts with it (`by:lukas`, `by:lukas.m`,
 * `by:@triage`: one leading `@` dropped; an empty `by:` is ignored). A row
 * with no creator never matches one. A query of `by:` tokens alone lists the
 * matching rows in the empty-query order below.
 *
 * #NEW-132: four more FILTER tokens (never scored; every filter must hold; an
 * empty or unparseable value is ignored like an empty `by:`). They are the
 * structured syntax natural language translates into (#NEW-131):
 *
 *   is:open | is:closed   the row is undone / finished ([isOpen])
 *   status:<word>         a word of the status ANCHOR (`in_review` = in,
 *                         review) or of the team status NAME ([Row.statusName])
 *                         starts with <word>; a row with neither never matches
 *   created:<range>       the row's createdAt / updatedAt lies in <range>;
 *   updated:<range>       a row without that timestamp never matches
 *
 * A <range> is `<n>d|w|m|y` (the last n×1/7/30/365 days before `now`,
 * n = 1…9999), a UTC day `YYYY-MM-DD` or month `YYYY-MM`, a comparison
 * against one (`>D` after it ends, `>=D` from its start, `<D` before its
 * start, `<=D` until its end), or `D..D` (the first's start to the second's
 * end). Impossible dates (`2026-13`, `2026-02-30`) are ignored. A query of
 * filters alone lists the matching rows in the empty-query order.
 *
 * EXP-922: the ordering is THREE keys above the score, in this order:
 *
 *   1. an EXACT identifier hit (some token scored 100) — typing `EXP-42` finds
 *      EXP-42 first whatever state it is in;
 *   2. UNDONE before done ([CLOSED_STATUSES]: the `done`/`cancelled`/
 *      `duplicate` anchors every custom status dual-writes; an absent or
 *      unknown status counts as open);
 *   3. the score.
 *
 * Ties then order by `updatedAt` desc, then `createdAt` desc, then identifier
 * number desc, then the identifier string desc. An EMPTY query lists open work
 * first, newest CREATED first inside each half (the `#` menu's "recent work"
 * list). Queries and tokens drop one leading `#` so `#87` and `fix #87` both
 * find EXP-87.
 */
object IssueSearch {

    /** Anything the merge can dedupe: a local row or a server hit. */
    interface Hit {
        val id: String
    }

    /**
     * A rankable row. The timestamps are the wire strings Room holds (ISO or
     * Postgres text — [WireTimestamps] parses both); missing or unparseable
     * reads as the oldest possible instant, exactly like the web's
     * `Number.NEGATIVE_INFINITY`.
     */
    interface Row : Hit {
        val identifier: String
        val title: String
        val description: String?
        val createdAt: String?
        val updatedAt: String?

        /**
         * EXP-922: the builtin status ANCHOR (`issues.status`) the row
         * dual-writes. Absent/unknown = treated as open. Defaulted so a row
         * type that carries no status (a server-hit stand-in) still ranks.
         */
        val status: String? get() = null

        /**
         * #EXP-1382: [updatedAt] / [createdAt] already parsed to epoch millis
         * ([Long.MIN_VALUE] = missing or unparseable) by a row type that
         * prepares its rows ONCE (the Search tab caches them per issues
         * emission), so a keystroke's rank never re-parses a timestamp. Null =
         * parse the wire string.
         */
        val updatedAtMs: Long? get() = null
        val createdAtMs: Long? get() = null

        /** #NEW-21: the member who filed it (`issues.creator_id`'s name + email). */
        val creatorName: String? get() = null
        val creatorEmail: String? get() = null

        /** #NEW-21: the agent whose session filed it (`issues.creator_agent_id`). */
        val creatorAgentName: String? get() = null

        /** #NEW-132: the team status row's NAME (`issues.status_id`), for `status:`. */
        val statusName: String? get() = null
    }

    /** #EXP-1382: a wire timestamp as the rank's epoch millis (missing/unparseable = oldest). */
    fun epochMs(value: String?): Long = timeOf(value)

    /**
     * The `limit` every consumer gets without asking — and, since EXP-922, the
     * one every SEARCH SURFACE uses on all four clients, so the same query
     * returns the same rows whichever one you type it into.
     */
    const val DEFAULT_LIMIT = 30

    /**
     * EXP-922: the builtin status anchors that mean "this issue is finished" —
     * the `completed`/`cancelled`/`duplicate` categories' anchors, so a team's
     * CUSTOM statuses sort correctly too (they dual-write one of these into
     * `issues.status`). Web `ISSUE_SEARCH_CLOSED_STATUSES`.
     */
    val CLOSED_STATUSES = setOf("done", "cancelled", "duplicate")

    /**
     * Whether a row counts as UNDONE for ranking. An absent or unrecognized
     * status is open: a server hit that never synced carries none, and a client
     * that meets a status it does not know must not bury the row.
     */
    fun isOpen(status: String?): Boolean = status == null || status !in CLOSED_STATUSES

    private const val SCORE_IDENTIFIER_EXACT = 100
    private const val SCORE_IDENTIFIER_PREFIX = 80
    private const val SCORE_IDENTIFIER_CONTAINS = 60
    private const val SCORE_TITLE_WORD_PREFIX = 50
    private const val SCORE_TITLE_CONTAINS = 40
    private const val SCORE_DESCRIPTION_WORD_PREFIX = 20
    private const val SCORE_DESCRIPTION_CONTAINS = 15
    private const val SCORE_CREATOR_WORD_PREFIX = 10

    /** #NEW-21: the token prefix that turns a word into a creator filter. */
    const val CREATOR_PREFIX = "by:"

    /** #NEW-132: the filter-token prefixes. */
    const val STATE_PREFIX = "is:"
    const val STATUS_PREFIX = "status:"
    const val CREATED_PREFIX = "created:"
    const val UPDATED_PREFIX = "updated:"

    /**
     * #NEW-132: a half-open time window `[from, to)` in epoch ms
     * ([Long.MIN_VALUE] / [Long.MAX_VALUE] = unbounded). Web `IssueSearchRange`.
     */
    data class Range(val from: Long, val to: Long)

    /** #NEW-21 + #NEW-132: a query split into its scored words and its filters. */
    data class ParsedQuery(
        val words: List<String>,
        /** `by:` needles (prefix and one leading `@` dropped). */
        val creators: List<String>,
        /** `is:` values: true = open, false = closed. */
        val states: List<Boolean> = emptyList(),
        /** `status:` needles. */
        val statuses: List<String> = emptyList(),
        val created: List<Range> = emptyList(),
        val updated: List<Range> = emptyList(),
    ) {
        /** Whether the query filters at all (a filter-only query lists the
         *  matching rows in the empty-query order). Web `issueSearchHasFilters`. */
        val hasFilters: Boolean
            get() = creators.isNotEmpty() || states.isNotEmpty() || statuses.isNotEmpty() ||
                created.isNotEmpty() || updated.isNotEmpty()
    }

    private const val DAY_MS = 86_400_000L
    private val RELATIVE_DAYS = mapOf('d' to 1L, 'w' to 7L, 'm' to 30L, 'y' to 365L)
    private val RELATIVE_RANGE = Regex("^([0-9]{1,4})([dwmy])$")
    private val DATE = Regex("^([0-9]{4})-([0-9]{2})(?:-([0-9]{2}))?$")

    /**
     * Days since 1970-01-01 of a proleptic Gregorian UTC date (Howard
     * Hinnant's days_from_civil), the same integer arithmetic ×4.
     */
    private fun daysFromCivil(year: Long, month: Long, day: Long): Long {
        val y = if (month <= 2) year - 1 else year
        val era = Math.floorDiv(y, 400L)
        val yoe = y - era * 400
        val mp = (month + 9) % 12
        val doy = (153 * mp + 2) / 5 + day - 1
        val doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        return era * 146_097 + doe - 719_468
    }

    private fun daysInMonth(year: Long, month: Long): Long {
        if (month == 2L) return if ((year % 4 == 0L && year % 100 != 0L) || year % 400 == 0L) 29 else 28
        return if (month in setOf(4L, 6L, 9L, 11L)) 30 else 31
    }

    /** A UTC day `YYYY-MM-DD` or month `YYYY-MM` as its window, or null. */
    private fun parseDate(text: String): Range? {
        val match = DATE.matchEntire(text) ?: return null
        val year = match.groupValues[1].toLong()
        val month = match.groupValues[2].toLong()
        if (month < 1 || month > 12) return null
        val dayText = match.groups[3]?.value
        if (dayText == null) {
            val from = daysFromCivil(year, month, 1) * DAY_MS
            return Range(from, from + daysInMonth(year, month) * DAY_MS)
        }
        val day = dayText.toLong()
        if (day < 1 || day > daysInMonth(year, month)) return null
        val from = daysFromCivil(year, month, day) * DAY_MS
        return Range(from, from + DAY_MS)
    }

    /** #NEW-132: a `created:`/`updated:` value as its window, or null. Web `parseIssueSearchRange`. */
    fun parseRange(value: String, now: Long): Range? {
        RELATIVE_RANGE.matchEntire(value)?.let { relative ->
            val count = relative.groupValues[1].toLong()
            if (count < 1) return null
            val days = RELATIVE_DAYS.getValue(relative.groupValues[2][0])
            return Range(now - count * days * DAY_MS, Long.MAX_VALUE)
        }
        val comparators: List<Pair<String, (Range) -> Range>> = listOf(
            ">=" to { d: Range -> Range(d.from, Long.MAX_VALUE) },
            "<=" to { d: Range -> Range(Long.MIN_VALUE, d.to) },
            ">" to { d: Range -> Range(d.to, Long.MAX_VALUE) },
            "<" to { d: Range -> Range(Long.MIN_VALUE, d.from) },
        )
        for ((op, window) in comparators) {
            if (!value.startsWith(op)) continue
            return parseDate(value.substring(op.length))?.let(window)
        }
        val dots = value.indexOf("..")
        if (dots >= 0) {
            val start = parseDate(value.substring(0, dots))
            val end = parseDate(value.substring(dots + 2))
            return if (start != null && end != null) Range(start.from, end.to) else null
        }
        return parseDate(value)
    }

    /**
     * #NEW-21 + #NEW-132: the query split into its scored words and its
     * filter tokens. Every prefixed token leaves the words, valid or not. Web
     * `parseIssueSearchQuery`.
     */
    fun parseQuery(query: String, now: Long = System.currentTimeMillis()): ParsedQuery {
        val words = mutableListOf<String>()
        val creators = mutableListOf<String>()
        val states = mutableListOf<Boolean>()
        val statuses = mutableListOf<String>()
        val created = mutableListOf<Range>()
        val updated = mutableListOf<Range>()
        for (token in tokens(query)) {
            when {
                token.startsWith(CREATOR_PREFIX) -> {
                    var needle = token.substring(CREATOR_PREFIX.length)
                    if (needle.startsWith("@")) needle = needle.substring(1)
                    if (needle.isNotEmpty()) creators.add(needle)
                }
                token.startsWith(STATE_PREFIX) -> when (token.substring(STATE_PREFIX.length)) {
                    "open" -> states.add(true)
                    "closed" -> states.add(false)
                }
                token.startsWith(STATUS_PREFIX) -> {
                    val needle = token.substring(STATUS_PREFIX.length)
                    if (needle.isNotEmpty()) statuses.add(needle)
                }
                token.startsWith(CREATED_PREFIX) ->
                    parseRange(token.substring(CREATED_PREFIX.length), now)?.let { created.add(it) }
                token.startsWith(UPDATED_PREFIX) ->
                    parseRange(token.substring(UPDATED_PREFIX.length), now)?.let { updated.add(it) }
                else -> words.add(token)
            }
        }
        return ParsedQuery(words, creators, states, statuses, created, updated)
    }

    /** Trim, drop ONE leading `#`, lowercase. Empty = "recent work". */
    fun normalizeQuery(query: String): String {
        var q = query.trim()
        if (q.startsWith("#")) q = q.substring(1).trim()
        return q.lowercase()
    }

    /**
     * Whitespace-separated tokens of the normalized query, each shorn of a
     * leading `#`, empties dropped.
     *
     * The split reads [Char.isWhitespace], never `Regex("\\s+")`: Java's `\s`
     * is ASCII-only, so a NON-BREAKING space (U+00A0, what a paste out of a
     * rendered page carries) would glue two tokens into one here while the
     * other three engines — JS `\s`, Swift `isWhitespace`, Rust
     * `split_whitespace` — all split it.
     */
    fun tokens(query: String): List<String> {
        val text = normalizeQuery(query)
        val out = mutableListOf<String>()
        var start = 0
        for (i in 0..text.length) {
            if (i < text.length && !text[i].isWhitespace()) continue
            if (i > start) {
                val raw = text.substring(start, i)
                val token = if (raw.startsWith("#")) raw.substring(1) else raw
                if (token.isNotEmpty()) out.add(token)
            }
            start = i + 1
        }
        return out
    }

    /** The numeric tail as a number for ordering (`EXP-87` → 87), -1 when none. */
    fun identifierNumber(identifier: String): Int =
        identifierNumberText(identifier)?.toIntOrNull() ?: -1

    /** The row's total score for [tokens], or null when any token misses. */
    fun score(row: Row, tokens: List<String>): Int? = scoreRow(Prepared(row), tokens)?.total

    /**
     * A matched row's two ranking facts: the summed score, and whether any
     * token hit the identifier EXACTLY (the pin that keeps a typed `EXP-42` on
     * top).
     */
    private class ScoreDetail(val total: Int, val exact: Boolean)

    private fun scoreRow(prepared: Prepared, tokens: List<String>): ScoreDetail? {
        var total = 0
        var exact = false
        for (token in tokens) {
            val score = tokenScore(prepared, token) ?: return null
            if (score == SCORE_IDENTIFIER_EXACT) exact = true
            total += score
        }
        return ScoreDetail(total, exact)
    }

    /**
     * Rank the locally synced [rows] for [query]: the scored, ordered, capped
     * list described at the top of this file. Stable for equal keys.
     */
    fun <T : Row> rank(
        rows: List<T>,
        query: String,
        limit: Int = DEFAULT_LIMIT,
        exclude: Set<String> = emptySet(),
        now: Long = System.currentTimeMillis(),
    ): List<T> {
        val parsed = parseQuery(query, now)
        val tokens = parsed.words
        val filtered = parsed.hasFilters
        val pool = if (exclude.isEmpty() && !filtered) {
            rows
        } else {
            rows.filter { it.id !in exclude && (!filtered || matchesFilters(it, parsed)) }
        }
        if (tokens.isEmpty()) {
            return pool.sortedWith(OPEN_ORDER.then(CREATED_ORDER)).take(limit)
        }
        val scored = ArrayList<Scored<T>>()
        for (row in pool) {
            val score = scoreRow(Prepared(row), tokens) ?: continue
            scored.add(Scored(row, score.total, score.exact))
        }
        return scored
            .sortedWith(
                Comparator { a, b ->
                    var c = b.exact.compareTo(a.exact)
                    if (c == 0) c = OPEN_ORDER.compare(a.row, b.row)
                    if (c == 0) c = b.score.compareTo(a.score)
                    if (c == 0) c = RECENCY_ORDER.compare(a.row, b.row)
                    c
                },
            )
            .take(limit)
            .map { it.row }
    }

    /**
     * Splice the server's full-text hits in behind the locally ranked rows:
     * local order first, then every hit not already listed, in the server's
     * relevance order, deduped by id. [resolve] turns a hit into a renderable
     * row — the synced row when the id is local, a stand-in built from the
     * hit's own fields where the consumer can render one, or null to drop it
     * (a picker whose pool is a subset of the team's issues never widens).
     * The [limit] spans both halves.
     */
    fun <T : Hit, H : Hit> mergeServerHits(
        local: List<T>,
        hits: List<H>,
        limit: Int = DEFAULT_LIMIT,
        exclude: Set<String> = emptySet(),
        resolve: (H) -> T?,
    ): List<T> {
        val seen = HashSet<String>()
        val merged = ArrayList<T>()
        for (row in local) {
            if (row.id in exclude || !seen.add(row.id)) continue
            merged.add(row)
            if (merged.size >= limit) return merged
        }
        for (hit in hits) {
            if (hit.id in exclude || hit.id in seen) continue
            val row = resolve(hit) ?: continue
            seen.add(hit.id)
            merged.add(row)
            if (merged.size >= limit) break
        }
        return merged
    }

    // ── internals ───────────────────────────────────────────────────────────

    private class Scored<T : Row>(val row: T, val score: Int, val exact: Boolean)

    /**
     * One row's lowercased fields. The description is lowercased LAZILY: a
     * token that already matched the identifier or the title never touches it,
     * and descriptions are the only unbounded field here.
     */
    private class Prepared(row: Row) {
        val identifier: String = row.identifier.lowercase()
        val number: String? = identifierNumberText(row.identifier)
        val title: String = row.title.lowercase()
        private val rawDescription: String? = row.description
        private var lowered: String? = null
        val description: String
            get() = lowered ?: (rawDescription?.lowercase() ?: "").also { lowered = it }

        /** #NEW-21: the member's and the agent's names, and the email, lowercased. */
        val creatorName: String = row.creatorName?.lowercase() ?: ""
        val creatorAgentName: String = row.creatorAgentName?.lowercase() ?: ""
        val creatorEmail: String = row.creatorEmail?.lowercase() ?: ""

        /** #NEW-132: the status anchor's and the status name's lowercased text. */
        val statusAnchor: String = row.status?.lowercase() ?: ""
        val statusName: String = row.statusName?.lowercase() ?: ""

        fun statusWordPrefix(needle: String): Boolean =
            hasWordPrefix(statusAnchor, needle) || hasWordPrefix(statusName, needle)

        fun creatorWordPrefix(token: String): Boolean =
            hasWordPrefix(creatorName, token) || hasWordPrefix(creatorAgentName, token)
    }

    /** #NEW-21: whether a prepared row was filed by every `by:` needle. */
    private fun matchesCreators(row: Prepared, creators: List<String>): Boolean =
        creators.all { needle ->
            row.creatorWordPrefix(needle) ||
                (row.creatorEmail.isNotEmpty() && row.creatorEmail.startsWith(needle))
        }

    private fun inRanges(ms: Long, ranges: List<Range>): Boolean =
        ranges.all { ms != Long.MIN_VALUE && ms >= it.from && ms < it.to }

    /** #NEW-21 + #NEW-132: whether a row passes every filter token. */
    private fun matchesFilters(row: Row, parsed: ParsedQuery): Boolean {
        val prepared = Prepared(row)
        if (!matchesCreators(prepared, parsed.creators)) return false
        val open = isOpen(row.status)
        if (!parsed.states.all { it == open }) return false
        if (!parsed.statuses.all { prepared.statusWordPrefix(it) }) return false
        return inRanges(createdOf(row), parsed.created) && inRanges(updatedOf(row), parsed.updated)
    }

    /**
     * The best field score of [token] against a prepared row, or null when the
     * token matches nothing.
     */
    private fun tokenScore(row: Prepared, token: String): Int? {
        if (row.identifier == token || (row.number != null && row.number == token)) {
            return SCORE_IDENTIFIER_EXACT
        }
        if (
            row.identifier.startsWith(token) ||
            (isDigits(token) && row.number != null && row.number.startsWith(token))
        ) {
            return SCORE_IDENTIFIER_PREFIX
        }
        if (row.identifier.contains(token)) return SCORE_IDENTIFIER_CONTAINS
        if (hasWordPrefix(row.title, token)) return SCORE_TITLE_WORD_PREFIX
        if (row.title.contains(token)) return SCORE_TITLE_CONTAINS
        if (hasWordPrefix(row.description, token)) return SCORE_DESCRIPTION_WORD_PREFIX
        if (row.description.contains(token)) return SCORE_DESCRIPTION_CONTAINS
        if (row.creatorWordPrefix(token)) return SCORE_CREATOR_WORD_PREFIX
        return null
    }

    private fun isDigits(token: String): Boolean =
        token.isNotEmpty() && token.all { it in '0'..'9' }

    /** The digits after the identifier's last `-` (`EXP-87` → `87`), or null. */
    private fun identifierNumberText(identifier: String): String? {
        val dash = identifier.lastIndexOf('-')
        if (dash < 0) return null
        val tail = identifier.substring(dash + 1)
        return if (isDigits(tail)) tail else null
    }

    /**
     * Whether any WORD of [text] starts with [token] — the allocation-free
     * twin of the web's `words(text).some((w) => w.startsWith(token))`. Words
     * are maximal runs of `\p{L}`/`\p{N}` (the web splits on `[^\p{L}\p{N}]+`),
     * so a token carrying any non-word character can never be a word prefix.
     */
    private fun hasWordPrefix(text: String, token: String): Boolean {
        if (token.isEmpty() || text.isEmpty()) return false
        for (c in token) if (!isWordChar(c)) return false
        var i = 0
        val end = text.length - token.length
        while (i <= end) {
            if ((i == 0 || !isWordChar(text[i - 1])) && text.startsWith(token, i)) return true
            i++
        }
        return false
    }

    /**
     * `\p{L} | \p{N}`. `Char.isLetter()` IS `\p{L}`; `Char.isDigit()` covers
     * only Nd, so the other two numeric categories are named explicitly.
     * (Astral letters arrive as surrogate pairs and read as non-word — an
     * immaterial divergence from the web's `/u` regex.)
     */
    private fun isWordChar(c: Char): Boolean {
        if (c.isLetter()) return true
        return when (Character.getType(c)) {
            Character.DECIMAL_DIGIT_NUMBER.toInt(),
            Character.LETTER_NUMBER.toInt(),
            Character.OTHER_NUMBER.toInt(),
            -> true
            else -> false
        }
    }

    /**
     * Epoch millis, or [Long.MIN_VALUE] for a missing/unparseable stamp — the
     * web's `-Infinity`, which likewise sorts oldest AND ties with itself so
     * the comparison falls through to the next key.
     */
    private fun timeOf(value: String?): Long =
        value?.let { WireTimestamps.parseEpochMs(it) } ?: Long.MIN_VALUE

    /** updatedAt desc → createdAt desc → identifier number desc → identifier desc. */
    private val RECENCY_ORDER = Comparator<Row> { a, b ->
        var c = updatedOf(b).compareTo(updatedOf(a))
        if (c == 0) c = createdOf(b).compareTo(createdOf(a))
        if (c == 0) c = identifierNumber(b.identifier).compareTo(identifierNumber(a.identifier))
        if (c == 0) c = b.identifier.compareTo(a.identifier)
        c
    }

    private fun updatedOf(row: Row): Long = row.updatedAtMs ?: timeOf(row.updatedAt)
    private fun createdOf(row: Row): Long = row.createdAtMs ?: timeOf(row.createdAt)

    /** EXP-922: undone before done. */
    private val OPEN_ORDER = Comparator<Row> { a, b -> isOpen(b.status).compareTo(isOpen(a.status)) }

    /** The empty-query order: createdAt desc → identifier number desc → identifier desc. */
    private val CREATED_ORDER = Comparator<Row> { a, b ->
        var c = createdOf(b).compareTo(createdOf(a))
        if (c == 0) c = identifierNumber(b.identifier).compareTo(identifierNumber(a.identifier))
        if (c == 0) c = b.identifier.compareTo(a.identifier)
        c
    }
}
