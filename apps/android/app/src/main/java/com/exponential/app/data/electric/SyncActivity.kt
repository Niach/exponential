package com.exponential.app.data.electric

import android.os.SystemClock
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/**
 * The shapes every issue surface reads from. A refresh waits on exactly these
 * (the other ten — comments, attachments, notifications, events, … — are not
 * what "the list is stale" means), and the "Syncing…" chip watches the same
 * set. `issue_statuses` joined in EXP-314: the list's group headers ARE that
 * shape's rows, so a stale one shows the wrong statuses.
 */
val CORE_SHAPES = setOf("teams", "boards", "issues", "issue_labels", "labels", "issue_statuses")

/**
 * How long after a kick we keep admitting that we might be behind. An offline
 * device never catches up, and a spinner that never stops is worse than no
 * spinner — past this the chip goes quiet and the sync-diagnostics screen owns
 * the story.
 */
private const val CATCHING_UP_WINDOW_MS = 15_000L

/**
 * Whether the app is visibly behind the server: a core shape is still doing its
 * initial snapshot / catch-up, or a kick went out recently and some core shape
 * hasn't completed a poll since.
 *
 * [now] and [lastKickAt] are `SystemClock.elapsedRealtime` values; a shape that
 * succeeded within [KICK_FRESHNESS_MS] BEFORE the kick counts as caught up,
 * because the kick was suppressed as redundant for exactly that reason.
 */
fun isCatchingUp(
    shapes: Map<String, SyncStats.ShapeStatus>?,
    lastKickAt: Long,
    now: Long,
): Boolean {
    val core = CORE_SHAPES.map { shapes?.get(it) }
    if (core.any { it?.phase == "initial" || it?.phase == "catchup" }) return true
    if (lastKickAt == 0L || now - lastKickAt > CATCHING_UP_WINDOW_MS) return false
    return core.any { (it?.lastSuccessAtMs ?: 0L) < lastKickAt - KICK_FRESHNESS_MS }
}

/** Emits `elapsedRealtime` now and every [periodMs] — drives time-based UI state. */
fun elapsedTicker(periodMs: Long = 1_000L): Flow<Long> = flow {
    while (true) {
        emit(SystemClock.elapsedRealtime())
        delay(periodMs)
    }
}
