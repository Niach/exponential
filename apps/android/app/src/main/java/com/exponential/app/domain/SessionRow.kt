package com.exponential.app.domain

// EXP-1248: the BIG session row's caption, x4 — fixture
// `packages/domain-contract/fixtures/list-item.json` (web
// `lib/session-row-caption.ts`, desktop `domain::session_row`, iOS
// `SessionRowCaption.swift`). The live states and their tones are
// `session-display.json`'s ([sessionStatusTone]); this only words them for a
// list: "<State> · <device> · <age>".

private const val MINUTE = 60_000L
private const val HOUR = 60 * MINUTE
private const val DAY = 24 * HOUR

/** The list's coarse age ladder: `now`, `5 min`, `21 h`, `2 d` (floored). */
fun listElapsed(ms: Long): String = when {
    ms < MINUTE -> "now"
    ms < HOUR -> "${ms / MINUTE} min"
    ms < DAY -> "${ms / HOUR} h"
    else -> "${ms / DAY} d"
}

/** The big row's second line and the tone it paints in. */
data class SessionRowCaption(val text: String, val tone: SessionStatusTone)

private fun liveWord(state: CodingSessionDisplayState): String = when (state) {
    CodingSessionDisplayState.Working -> "Building"
    CodingSessionDisplayState.NeedsInput -> "Needs input"
    CodingSessionDisplayState.Review -> "Ready for review"
    CodingSessionDisplayState.Done -> "Done"
}

/**
 * The big row's caption, first match wins: ended → `Done · <device> · <when>`
 * muted (since `endedAt`, else `updatedAt`); paused → `Paused · <device>`
 * muted; a usage wall ([blockedLabel]) → that label, amber; else the live
 * state's word, tone ([sessionStatusTone]) and age (since `startedAt` while
 * working / waiting, since `updatedAt` once in review or done). A missing
 * device or an unparsable/missing stamp drops that segment.
 */
fun sessionRowCaption(
    /** `runHasEnded(session)`. */
    ended: Boolean,
    /** An offline host ([SessionDevicePresentation.isPaused]). */
    paused: Boolean,
    /** Ignored once ended. */
    state: CodingSessionDisplayState,
    /** The resolved device label; null drops the segment. */
    device: String?,
    startedAt: String?,
    updatedAt: String?,
    endedAt: String?,
    /** The usage wall's badge label (`blockedBadgeLabel`), or null. */
    blockedLabel: String? = null,
    nowMs: Long,
): SessionRowCaption {
    fun since(stamp: String?): String? {
        val at = stamp?.let(WireTimestamps::parseEpochMs) ?: return null
        return listElapsed(nowMs - at)
    }
    fun join(vararg parts: String?): String = parts.filter { !it.isNullOrEmpty() }.joinToString(" · ")
    val deviceName = device?.trim()?.takeIf { it.isNotEmpty() }

    if (ended) {
        val time = since(endedAt) ?: since(updatedAt)
        return SessionRowCaption(join("Done", deviceName, time), SessionStatusTone.Muted)
    }
    if (paused) return SessionRowCaption(join("Paused", deviceName), SessionStatusTone.Muted)
    if (!blockedLabel.isNullOrEmpty()) return SessionRowCaption(blockedLabel, SessionStatusTone.Amber)
    val live = when (state) {
        CodingSessionDisplayState.Working, CodingSessionDisplayState.NeedsInput -> since(startedAt)
        else -> since(updatedAt)
    }
    return SessionRowCaption(join(liveWord(state), deviceName, live), sessionStatusTone(state))
}
