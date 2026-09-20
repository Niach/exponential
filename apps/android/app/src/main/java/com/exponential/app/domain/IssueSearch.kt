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
    }

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
    ): List<T> {
        val tokens = tokens(query)
        val pool = if (exclude.isEmpty()) rows else rows.filter { it.id !in exclude }
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
        var c = timeOf(b.updatedAt).compareTo(timeOf(a.updatedAt))
        if (c == 0) c = timeOf(b.createdAt).compareTo(timeOf(a.createdAt))
        if (c == 0) c = identifierNumber(b.identifier).compareTo(identifierNumber(a.identifier))
        if (c == 0) c = b.identifier.compareTo(a.identifier)
        c
    }

    /** EXP-922: undone before done. */
    private val OPEN_ORDER = Comparator<Row> { a, b -> isOpen(b.status).compareTo(isOpen(a.status)) }

    /** The empty-query order: createdAt desc → identifier number desc → identifier desc. */
    private val CREATED_ORDER = Comparator<Row> { a, b ->
        var c = timeOf(b.createdAt).compareTo(timeOf(a.createdAt))
        if (c == 0) c = identifierNumber(b.identifier).compareTo(identifierNumber(a.identifier))
        if (c == 0) c = b.identifier.compareTo(a.identifier)
        c
    }
}
