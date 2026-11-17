package com.exponential.app.domain

import java.time.Instant
import java.time.LocalDateTime
import java.time.OffsetDateTime
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.format.DateTimeFormatterBuilder
import java.time.temporal.ChronoField

/**
 * Tolerant wire-timestamp → epoch-ms parser. Room holds timestamps in TWO wire
 * formats (see [sortableTimestamp]): Electric's Postgres text encoding
 * `yyyy-MM-dd HH:mm:ss[.ffffff]+00` (space separator, hour-only offset) and
 * ISO-8601 `yyyy-MM-ddTHH:mm:ss[.SSS]Z` / `+HH:MM` from tRPC-era writes.
 * `Instant.parse` alone rejects the Electric form, which blanked every synced
 * row's relative time (EXP-169) and kept the EXP-153 staleness guard fail-open.
 */
object WireTimestamps {

    private val PG_TEXT: DateTimeFormatter = DateTimeFormatterBuilder()
        .appendPattern("yyyy-MM-dd HH:mm:ss")
        .optionalStart()
        .appendFraction(ChronoField.NANO_OF_SECOND, 1, 9, true)
        .optionalEnd()
        .optionalStart()
        .appendOffset("+HH:mm", "Z") // accepts "+00", "+05:30", and "Z"
        .optionalEnd()
        .toFormatter()

    fun parseEpochMs(value: String): Long? {
        // #ERR-28: the common shapes without java.time (a big chat's open
        // parsed ~95k stamps through it: ~56 MB of garbage); anything else
        // (whitespace, odd offsets, leap seconds) takes the tolerant path.
        val fast = parseFast(value)
        if (fast != NOT_FAST) return fast
        val v = value.trim()
        if (v.isEmpty()) return null
        // Sniff the separator (index 10 of both forms) instead of letting the
        // dominant Postgres form throw through two exception-driven ISO
        // attempts — this runs per visible row per state emission.
        return if (v.length > 10 && v[10] == ' ') {
            parsePgText(v) ?: parseIso(v)
        } else {
            parseIso(v) ?: parsePgText(v)
        }
    }

    /** Sentinel: [parseFast] can't read it ([Long.MIN_VALUE] is no wire time). */
    private const val NOT_FAST = Long.MIN_VALUE

    private fun digits(s: String, from: Int, count: Int): Int {
        var n = 0
        for (i in from until from + count) {
            val d = s[i] - '0'
            if (d !in 0..9) return -1
            n = n * 10 + d
        }
        return n
    }

    /**
     * Allocation-free `yyyy-MM-dd[ T]HH:mm:ss[.f{1,9}][offset]`: the space form
     * takes `+HH`, `+HH:mm`, `Z` or no offset (UTC), the `T` form `Z` or
     * `+HH:mm` (as [parseIso] does). [NOT_FAST] for anything else.
     */
    private fun parseFast(s: String): Long {
        val n = s.length
        if (n < 19 || s[4] != '-' || s[7] != '-' || s[13] != ':' || s[16] != ':') return NOT_FAST
        val sep = s[10]
        if (sep != ' ' && sep != 'T') return NOT_FAST
        val year = digits(s, 0, 4)
        val month = digits(s, 5, 2)
        val day = digits(s, 8, 2)
        val hour = digits(s, 11, 2)
        val minute = digits(s, 14, 2)
        val second = digits(s, 17, 2)
        if (year < 0 || month !in 1..12 || day < 1 || day > daysInMonth(year, month)) return NOT_FAST
        if (hour !in 0..23 || minute !in 0..59 || second !in 0..59) return NOT_FAST
        var i = 19
        var millis = 0
        if (i < n && s[i] == '.') {
            i++
            val start = i
            while (i < n && s[i] in '0'..'9') {
                if (i - start < 3) millis = millis * 10 + (s[i] - '0')
                i++
            }
            val count = i - start
            if (count == 0 || count > 9) return NOT_FAST
            if (count == 1) millis *= 100 else if (count == 2) millis *= 10
        }
        var offsetSeconds = 0
        if (i == n) {
            if (sep == 'T') return NOT_FAST
        } else {
            val c = s[i]
            if (c == 'Z' && i + 1 == n) {
                offsetSeconds = 0
            } else if (c == '+' || c == '-') {
                val rest = n - i - 1
                val oh: Int
                var om = 0
                if (rest == 2 && sep == ' ') {
                    oh = digits(s, i + 1, 2)
                } else if (rest == 5 && s[i + 3] == ':') {
                    oh = digits(s, i + 1, 2)
                    om = digits(s, i + 4, 2)
                } else {
                    return NOT_FAST
                }
                if (oh !in 0..18 || om !in 0..59) return NOT_FAST
                offsetSeconds = (oh * 3600 + om * 60) * (if (c == '-') -1 else 1)
            } else {
                return NOT_FAST
            }
        }
        val epochSeconds = epochDay(year, month, day) * 86_400L + hour * 3600L + minute * 60L + second - offsetSeconds
        return epochSeconds * 1000L + millis
    }

    private fun isLeap(year: Int) = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0

    private fun daysInMonth(year: Int, month: Int): Int = when (month) {
        2 -> if (isLeap(year)) 29 else 28
        4, 6, 9, 11 -> 30
        else -> 31
    }

    /** Days since 1970-01-01 of a proleptic Gregorian date (H. Hinnant's days_from_civil). */
    private fun epochDay(year: Int, month: Int, day: Int): Long {
        val y = (if (month <= 2) year - 1 else year).toLong()
        val era = Math.floorDiv(y, 400L)
        val yoe = y - era * 400
        val mp = (month + 9) % 12
        val doy = (153 * mp + 2) / 5 + day - 1
        val doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        return era * 146_097 + doe - 719_468
    }

    private fun parseIso(v: String): Long? =
        runCatching { Instant.parse(v).toEpochMilli() }
            .recoverCatching { OffsetDateTime.parse(v).toInstant().toEpochMilli() }
            .getOrNull()

    private fun parsePgText(v: String): Long? = runCatching {
        val parsed = PG_TEXT.parse(v)
        if (parsed.isSupported(ChronoField.OFFSET_SECONDS)) {
            OffsetDateTime.from(parsed).toInstant().toEpochMilli()
        } else {
            // Offset-less Postgres text (plain `timestamp` columns) is UTC.
            LocalDateTime.from(parsed).toInstant(ZoneOffset.UTC).toEpochMilli()
        }
    }.getOrNull()
}

/**
 * Large chats: sort by a parsed timestamp WITHOUT re-parsing it in the
 * comparator (decorate-sort-undecorate): [at] runs once per element, then
 * the order is ascending by it ([descending] = newest first), ties broken by
 * [tie]. The same order a `compareBy({ at(it) }, …)` comparator gives.
 */
internal inline fun <T> Iterable<T>.sortedByEpoch(
    descending: Boolean = false,
    tie: Comparator<in T>,
    at: (T) -> Long,
): List<T> {
    val decorated = map { at(it) to it }
    val order = Comparator<Pair<Long, T>> { a, b ->
        val c = if (descending) b.first.compareTo(a.first) else a.first.compareTo(b.first)
        if (c != 0) c else tie.compare(a.second, b.second)
    }
    return decorated.sortedWith(order).map { it.second }
}

