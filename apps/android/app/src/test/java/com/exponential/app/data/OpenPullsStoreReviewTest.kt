package com.exponential.app.data

import com.exponential.app.data.api.OpenPull
import com.exponential.app.data.api.OpenPullsRepo
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

/** EXP-1244: the app-wide openPulls store behind the Reviews screen + its tab dot. */
class OpenPullsStoreReviewTest {

    private var clock = 1_000_000L
    private val calls = mutableListOf<Pair<String, String>>()
    private var gate: CompletableDeferred<Unit>? = null

    private val store = OpenPullsStore(
        fetch = { accountId, teamId ->
            calls += accountId to teamId
            gate?.await()
            listOf(OpenPullsRepo("repo-$teamId", "acme/$teamId", listOf(OpenPull(1, "u1"), OpenPull(2, "u2"))))
        },
        now = { clock },
        scope = CoroutineScope(Dispatchers.Unconfined),
    )

    @Test
    fun `a fresh team is not refetched unless forced`() = runBlocking {
        store.refresh("a", "t1")
        store.refresh("a", "t1")
        assertEquals(1, calls.size)
        clock += OpenPullsStore.STALE_MS - 1
        store.watch("a", listOf("t1"))
        assertEquals(1, calls.size)
        store.refresh("a", "t1", force = true)
        assertEquals(2, calls.size)
        clock += OpenPullsStore.STALE_MS
        store.onForeground()
        assertEquals(3, calls.size)
        assertEquals(listOf("repo-t1"), store.pulls("a", listOf("t1")).first().map { it.repositoryId })
    }

    @Test
    fun `an in-flight fetch is joined, even by a forced refresh`() = runBlocking {
        gate = CompletableDeferred()
        store.refresh("a", "t1")
        store.refresh("a", "t1", force = true)
        assertEquals(1, calls.size)
        gate!!.complete(Unit)
        assertEquals(1, store.pulls("a", listOf("t1")).first().size)
    }

    @Test
    fun `entries are keyed by account and a merged pull drops`() = runBlocking {
        store.refresh("a", "t1")
        store.refresh("b", "t1")
        assertEquals(2, calls.size)
        store.removePull("a", "repo-t1", 1)
        assertEquals(listOf(2), store.pulls("a", listOf("t1")).first().single().pulls.map { it.number })
        assertEquals(listOf(1, 2), store.pulls("b", listOf("t1")).first().single().pulls.map { it.number })
        assertEquals(emptyList<Any>(), store.pulls(null, listOf("t1")).first())
    }
}
