package com.exponential.app.domain

import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale

/**
 * #NEW-37 (settings-nav.json `boards`): the Archived / Trash rows' captions ×4
 * (web `formatArchivedOn` / `formatPurgeCountdown`, desktop `all_boards.rs`,
 * iOS `PutAwayBoardCaption`).
 */
object PutAwayBoardCaption {
    private val DATE = DateTimeFormatter.ofPattern("MMM d, yyyy", Locale.US).withZone(ZoneOffset.UTC)

    /** "Archived Jul 1, 2026"; bare "Archived" without a timestamp. */
    fun archived(archivedAt: String?): String {
        val at = parse(archivedAt) ?: return "Archived"
        return "Archived ${DATE.format(at)}"
    }

    /** "Purges in ~Nh" (hours rounded up), "Purging soon" once due, "Purges soon" without a timestamp. */
    fun purge(purgeAt: String?, now: Instant = Instant.now()): String {
        val at = parse(purgeAt) ?: return "Purges soon"
        val ms = at.toEpochMilli() - now.toEpochMilli()
        if (ms <= 0) return "Purging soon"
        val hours = (ms + 3_600_000 - 1) / 3_600_000
        return "Purges in ~${hours}h"
    }

    private fun parse(raw: String?): Instant? =
        raw?.takeIf { it.isNotBlank() }?.let { runCatching { Instant.parse(it.trim()) }.getOrNull() }
}
