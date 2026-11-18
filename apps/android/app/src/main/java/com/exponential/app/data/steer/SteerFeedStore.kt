package com.exponential.app.data.steer

import com.exponential.app.domain.TailEvent

/**
 * #EDE-114 (chat-store.json S3): one session view's side of the chat store.
 * [record] takes every feed event the view receives (replay, live, the
 * device's history pages) without blocking; [older] pages the store below a
 * seq (the session's current device), before the device is asked.
 */
interface SteerFeedStore {
    fun record(events: List<TailEvent>)

    /** The newest [limit] stored events below [beforeSeq], oldest first; empty = none, null = no store. */
    suspend fun older(beforeSeq: Long, limit: Int): List<TailEvent>?
}
