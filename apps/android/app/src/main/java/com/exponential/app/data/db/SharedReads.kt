package com.exponential.app.data.db

import androidx.room.RoomDatabase
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.shareIn

/**
 * #ERR-23: a Room observe query over a WHOLE table runs once per collector
 * and each collector keeps its own copy of every row. On a large account
 * (thousands of sessions with their digests, thousands of issues with their
 * descriptions) the app-wide readers (the inbox dot, the system search
 * index, the home widgets, the Chats list, each open chat, every session
 * card in a transcript) held ten copies of the same tables, re-read all of
 * them on every synced write and ran the 256 MB heap out.
 *
 * [sharedRead] turns such a query into ONE upstream per DAO instance (= per
 * account database): every collector gets the same list instance, the
 * query re-runs once per invalidation, and the copy is dropped
 * [STOP_TIMEOUT_MS] after the last collector leaves (no replay after that,
 * so a returning reader never sees a stale table). A failure (a database
 * closed under an account switch) reaches the collectors as before, so
 * [scopedQuery]'s closed-database guard still applies.
 */
internal fun <T> Flow<T>.sharedRead(
    scope: CoroutineScope = SharedReads.scope,
    stopTimeoutMs: Long = SharedReads.STOP_TIMEOUT_MS,
): Flow<T> =
    map { Result.success(it) }
        .catch { emit(Result.failure(it)) }
        // #ERR-28: a newer table REPLACES the one a slow reader hasn't taken
        // yet: the upstream never waits holding a finished copy, so a write
        // burst keeps two copies at most (the one being read + the newest),
        // never a third parked in the share's buffer.
        .conflate()
        .shareIn(scope, SharingStarted.WhileSubscribed(stopTimeoutMs, 0), replay = 1)
        .map { it.getOrThrow() }

/**
 * #ERR-23: a whole-table read per synced write is a whole new copy of the
 * table per write; the sync writes several rows a second while agents run,
 * so even ONE reader kept a second and third copy in flight on the query
 * threads. [throttledRead] re-reads on the first change at once, then at most
 * once per [minIntervalMs]: changes landing meanwhile fold into one re-read
 * (the invalidations are conflated), so the newest rows always arrive.
 */
internal fun <T> Flow<*>.throttledRead(
    minIntervalMs: Long = SharedReads.MIN_INTERVAL_MS,
    read: suspend () -> T,
): Flow<T> = flow {
    this@throttledRead.conflate().collect {
        emit(read())
        delay(minIntervalMs)
    }
}

/** One account database's whole [table]: shared ([sharedRead]) and throttled ([throttledRead]). */
internal fun <T> RoomDatabase.wholeTableRead(table: String, read: suspend () -> T): Flow<T> =
    invalidationTracker.createFlow(table).throttledRead(read = read).sharedRead()

internal object SharedReads {
    /** A screen change (old screen leaves, new one arrives) keeps the copy. */
    const val STOP_TIMEOUT_MS = 5_000L

    /** The fastest a whole table is re-read while the sync keeps writing it. */
    const val MIN_INTERVAL_MS = 1_000L
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
}

/**
 * #ERR-28: a whole table re-read hands back NEW row objects for every row,
 * though a synced write changed one or two; [read] maps only the rows that
 * differ from the last read ([equals]) and hands the unchanged rows' mapped
 * values back as the same instances (no re-parse, no new garbage, and
 * downstream equality checks short-circuit). Not thread-safe: one flow's
 * sequential map.
 */
internal class MappedRows<R : Any, V>(private val key: (R) -> String, private val transform: (R) -> V) {
    private var last: Map<String, Pair<R, V>> = emptyMap()

    /** Rows mapped since the cache was made (the memory-budget tests read it). */
    var mapped: Int = 0
        private set

    fun read(rows: List<R>): List<V> {
        val next = HashMap<String, Pair<R, V>>(rows.size * 2)
        val out = ArrayList<V>(rows.size)
        for (row in rows) {
            val id = key(row)
            val previous = last[id]
            val entry = if (previous != null && previous.first == row) {
                previous
            } else {
                mapped++
                row to transform(row)
            }
            next[id] = entry
            out.add(entry.second)
        }
        last = next
        return out
    }
}
