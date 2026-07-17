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
