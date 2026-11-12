package com.exponential.app.domain

import kotlinx.serialization.json.JsonObject

/** #PERF-9 (steer-enet.json STORED TAIL): one feed event the viewer applied, with its wire seq. */
class TailEvent(val event: JsonObject, val seq: Long?)

/**
 * The stored tail folded into a transcript, exactly like a replay: the same
 * reducer over a fresh state. Only the ROWS are taken: the latest-wins slots
 * (turn, queue, rate limit, compaction…) a tail carries may be stale and the
 * join's replay restates them a moment later.
 */
fun foldStoredTail(events: List<TailEvent>): ActivityFeedState {
    val folded = events.fold(ActivityFeedState()) { state, tail ->
        state.applyActivityEvent(tail.event, { false }, tail.seq)
    }
    return ActivityFeedState(feed = folded.feed, nextEventId = folded.nextEventId, feedBytes = folded.feedBytes)
}

/**
 * The replay that REPLACES a painted stored tail keeps the tail's row ids
 * (its LazyColumn keys) for the rows both hold, matched by `seq`, so the
 * reader's anchor holds across the swap (EXP-656). Rows only the replay has
 * are numbered around them (older ones below, like a prepended page). When
 * the matched rows leave no room to number the rows between them, the
 * replay keeps its own ids (the swap is total, as before).
 */
fun ActivityFeedState.adoptRowIds(previous: List<AgentFeedItem>): ActivityFeedState {
    if (feed.isEmpty() || previous.isEmpty()) return this
    val bySeq = HashMap<Long, AgentFeedItem>(previous.size)
    for (item in previous) item.seq?.let { bySeq.putIfAbsent(it, item) }
    if (bySeq.isEmpty()) return this
    val target = LongArray(feed.size)
    val matched = BooleanArray(feed.size)
    var lastIndex = -1
    var firstIndex = -1
    for ((index, item) in feed.withIndex()) {
        val seq = item.seq ?: continue
        val old = bySeq[seq] ?: continue
        if (old::class != item::class) continue
        if (lastIndex >= 0) {
            // Strictly increasing, with room for the rows in between.
            if (old.id - target[lastIndex] < (index - lastIndex).toLong()) return this
        } else {
            firstIndex = index
        }
        target[index] = old.id
        matched[index] = true
        lastIndex = index
    }
    if (firstIndex < 0) return this
    var anchor = firstIndex
    val renumbered = feed.mapIndexed { index, item ->
        val id = when {
            matched[index] -> target[index].also { anchor = index }
            index < firstIndex -> target[firstIndex] - (firstIndex - index)
            else -> target[anchor] + (index - anchor)
        }
        if (id == item.id) item else item.withId(id)
    }
    return copy(feed = renumbered, nextEventId = renumbered.last().id + 1)
}
