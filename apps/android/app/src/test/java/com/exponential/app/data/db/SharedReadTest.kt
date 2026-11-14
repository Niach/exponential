package com.exponential.app.data.db

import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * #ERR-23 heap guard: a whole-table read is ONE upstream query and ONE list
 * however many screens and services observe it (it was one copy per reader,
 * ten on a large account's chat screen, and the 256 MB heap ran out).
 */
class SharedReadTest {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    @After
    fun tearDown() = scope.cancel()

    /** A stand-in for a Room observe query: a fresh list per run and per invalidation. */
    private class Table {
        val runs = AtomicInteger()
        val invalidations = MutableStateFlow(0)
        val query = invalidations.map { gen -> runs.incrementAndGet(); List(1_000) { "row-$gen-$it" } }
    }

    @Test
    fun tenReadersShareOneQueryAndOneList() = runBlocking {
        val table = Table()
        val shared = table.query.sharedRead(scope)
        val lists = (1..10).map { async(Dispatchers.Default) { shared.first() } }.awaitAll()
        assertEquals("one query for ten readers", 1, table.runs.get())
        lists.forEach { assertSame("every reader holds the same list", lists[0], it) }
    }

    @Test
    fun anInvalidationReRunsTheQueryOnceForAllReaders() = runBlocking {
        val table = Table()
        val shared = table.query.sharedRead(scope)
        val readers = (1..10).map { async(Dispatchers.Default) { shared.first { it[0].startsWith("row-1-") } } }
        // Every reader is subscribed (the first list landed) before the write.
        withTimeout(5_000) { while (table.runs.get() < 1) delay(5) }
        delay(100)
        table.invalidations.value = 1
        val lists = readers.awaitAll()
        assertEquals("one re-run per invalidation", 2, table.runs.get())
        lists.forEach { assertSame(lists[0], it) }
    }

    @Test
    fun theCopyIsDroppedAfterTheLastReaderAndNeverReplayedStale() = runBlocking {
        val table = Table()
        val shared = table.query.sharedRead(scope, stopTimeoutMs = 50)
        val first = shared.first()
        delay(300) // past the stop timeout: the upstream stopped, its replay reset
        val second = shared.first()
        assertEquals("a returning reader re-queries", 2, table.runs.get())
        assertTrue(first !== second)
    }

    @Test
    fun aFailureReachesTheReaderLikeAnUnsharedQuery() = runBlocking {
        val shared = flow<List<String>> { throw IllegalStateException("closed") }
            .sharedRead(scope)
        try {
            shared.first()
            fail("expected the query's failure")
        } catch (e: IllegalStateException) {
            assertEquals("closed", e.message)
        }
    }

    @Test
    fun aWriteBurstIsReadOnceAtOnceThenOncePerIntervalWithTheNewestRows() = runBlocking {
        // The invalidation tracker: one emission per synced write.
        val writes = MutableSharedFlow<Int>(extraBufferCapacity = 64)
        val table = AtomicInteger(0)
        val reads = AtomicInteger()
        val read = writes.onSubscriptionStart(0).throttledRead(minIntervalMs = 300) { reads.incrementAndGet(); table.get() }
        val seen = async(Dispatchers.Default) { read.take(2).toList() }
        withTimeout(5_000) { while (reads.get() < 1 || writes.subscriptionCount.value < 1) delay(5) }
        // Fifty writes inside one interval: ONE more read, of the last state.
        repeat(50) { table.set(it + 1); writes.emit(it + 1) }
        val values = withTimeout(5_000) { seen.await() }
        assertEquals("the initial read + one read for the whole burst", 2, reads.get())
        assertEquals(listOf(0, 50), values)
    }

    /** The tracker's initial-state emission, then [this]. */
    private fun MutableSharedFlow<Int>.onSubscriptionStart(initial: Int) =
        kotlinx.coroutines.flow.flow { emit(initial); this@onSubscriptionStart.collect { emit(it) } }
}
