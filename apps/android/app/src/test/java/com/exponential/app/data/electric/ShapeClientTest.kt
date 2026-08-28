package com.exponential.app.data.electric

import com.exponential.app.data.db.ElectricOffsetDao
import com.exponential.app.data.db.ElectricOffsetEntity
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.MockRequestHandleScope
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.HttpTimeoutCapability
import io.ktor.client.request.HttpRequestData
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Run-loop resilience tests for [ShapeClient] (EXP-61):
 *  - a CancellationException surfacing from the HTTP layer while the loop's
 *    own job is alive must NOT kill the loop (ktor CIO's engine timeout
 *    cancels the call job — before the fix this silently froze sync forever),
 *  - HTTP 400 (Electric "shape definition and handle do not match") must reset
 *    the shape like a 409 instead of retrying the identical request forever,
 *  - shape polls must carry a request-timeout budget above the live-poll hold.
 */
class ShapeClientTest {

    @Serializable
    private data class Row(val id: String, val name: String)

    private class FakeOffsetDao : ElectricOffsetDao {
        val map = mutableMapOf<String, ElectricOffsetEntity>()
        override suspend fun get(shape: String): ElectricOffsetEntity? = map[shape]
        override fun observeIsLive(shape: String): Flow<Boolean?> = flowOf(map[shape]?.isLive)
        override suspend fun upsert(item: ElectricOffsetEntity) { map[item.shape] = item }
        override suspend fun clear() { map.clear() }
    }

    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

    /**
     * Stand-in for SystemClock.elapsedRealtime, which is a constant 0 under
     * `unitTests.isReturnDefaultValues` — the kick freshness window needs a
     * clock the test can move.
     */
    private class FakeClock(start: Long = 10_000L) : () -> Long {
        private val now = java.util.concurrent.atomic.AtomicLong(start)
        override fun invoke(): Long = now.get()
        fun advance(ms: Long) { now.addAndGet(ms) }
    }

    private fun shapeHeaders(handle: String = "h1", offset: String = "0_0") = headersOf(
        "electric-handle" to listOf(handle),
        "electric-offset" to listOf(offset),
        "Content-Type" to listOf("application/json"),
    )

    private val insertAndUpToDateBody = """
        [
          {"headers":{"operation":"insert"},"key":"\"public\".\"rows\"/\"r1\"","value":{"id":"r1","name":"one"}},
          {"headers":{"control":"up-to-date"}}
        ]
    """.trimIndent()

    private fun client(
        dao: FakeOffsetDao,
        onMessages: suspend (List<ShapeMessage<Row>>) -> Unit,
        onError: (Boolean, String?, Boolean) -> Unit = { _, _, _ -> },
        onSuccess: () -> Unit = {},
        onReset: suspend () -> Unit = {},
        onUnauthorized: () -> Unit = {},
        // A real advancing clock by default, so kicks behave as in production;
        // the freshness test swaps in a clock it can hold still.
        nowMs: () -> Long = { System.currentTimeMillis() },
        // App-lifecycle gate — open unless a test drives it (REV2-38).
        active: StateFlow<Boolean> = MutableStateFlow(true),
        handler: suspend MockRequestHandleScope.(HttpRequestData) -> io.ktor.client.request.HttpResponseData,
    ): ShapeClient<Row> {
        val engine = MockEngine { request -> handler(request) }
        val http = HttpClient(engine) { install(HttpTimeout) }
        return ShapeClient(
            client = http,
            baseUrlProvider = { "http://test" },
            tokenProvider = { "token" },
            shapeName = "rows",
            urlPath = "/api/shapes/rows",
            valueSerializer = Row.serializer(),
            offsetDao = dao,
            json = json,
            onMessages = onMessages,
            onError = onError,
            onSuccess = onSuccess,
            onReset = onReset,
            onUnauthorized = onUnauthorized,
            nowMs = nowMs,
            active = active,
        )
    }

    /**
     * The dead-session split: a shape poll's 401 means the bearer this loop
     * presented was rejected, which signs the account out (SessionInvalidator);
     * a 403 is an authorization verdict on a LIVE session and must keep backing
     * off instead of ejecting a working account.
     */
    @Test
    fun unauthorizedSignsTheAccountOutButForbiddenDoesNot() = runBlocking {
        val dao = FakeOffsetDao()
        val signOuts = java.util.concurrent.atomic.AtomicInteger(0)
        val requests = java.util.concurrent.atomic.AtomicInteger(0)
        val signOutsAfterForbidden = java.util.concurrent.atomic.AtomicInteger(-1)

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            onUnauthorized = { signOuts.incrementAndGet() },
            handler = {
                val n = requests.incrementAndGet()
                if (n == 1) {
                    respond("", HttpStatusCode.Forbidden)
                } else {
                    // Sampled before answering: the 403 has fully unwound by now.
                    if (n == 2) signOutsAfterForbidden.set(signOuts.get())
                    respond("", HttpStatusCode.Unauthorized)
                }
            },
        )

        val job = launch { shapeClient.run() }
        withTimeout(10_000) {
            while (signOuts.get() == 0) {
                kotlinx.coroutines.delay(20)
            }
        }
        job.cancel()
        job.join()

        assertEquals("a 403 must not sign anyone out", 0, signOutsAfterForbidden.get())
        assertTrue("a 401 must sign the account out", signOuts.get() >= 1)
    }

    @Test
    fun httpLayerCancellationDoesNotKillTheLoop() = runBlocking {
        val dao = FakeOffsetDao()
        val errors = CopyOnWriteArrayList<String?>()
        val applied = CopyOnWriteArrayList<ShapeMessage<Row>>()
        var calls = 0

        val shapeClient = client(
            dao = dao,
            onMessages = { applied.addAll(it) },
            onError = { _, message, _ -> errors.add(message) },
            handler = {
                calls++
                if (calls == 1) {
                    // Mimic ktor CIO's engine-level request timeout, which
                    // cancels the call job — surfaces to the caller as a
                    // CancellationException, not an IOException.
                    throw CancellationException("Request is timed out")
                }
                respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
            },
        )

        val job = launch { shapeClient.run() }
        withTimeout(10_000) {
            while (applied.none { it is ShapeMessage.Insert }) {
                kotlinx.coroutines.delay(20)
            }
        }
        job.cancel()
        job.join()

        assertTrue("loop must survive the cancellation and keep polling", calls >= 2)
        assertEquals(listOf("Request is timed out"), errors)
        assertTrue(applied.any { it is ShapeMessage.UpToDate })
        assertEquals(true, dao.map["rows"]?.isLive)
    }

    @Test
    fun realJobCancellationStillExitsTheLoop() = runBlocking {
        val dao = FakeOffsetDao()
        val shapeClient = client(
            dao = dao,
            onMessages = {},
            handler = { respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders()) },
        )
        val job = launch { shapeClient.run() }
        // Let it poll at least once, then cancel: join() must complete.
        kotlinx.coroutines.delay(200)
        job.cancel()
        val joined = withTimeoutOrNull(5_000) { job.join(); true }
        assertNotNull("run() must exit on real cancellation", joined)
    }

    /**
     * The must-refetch reaction (EXP-264): a 409/400 only MARKS the shape —
     * the rows and the offset row survive, and no MustRefetch reaches the
     * apply layer yet. The next poll re-snapshots and carries the wipe at the
     * head of its own batch, so SyncManager's single transaction turns the
     * refetch into an atomic swap instead of "empty for a second".
     */
    private fun assertMarksRefetchInsteadOfWiping(status: HttpStatusCode, body: String) = runBlocking {
        val dao = FakeOffsetDao()
        dao.map["rows"] = ElectricOffsetEntity(shape = "rows", handle = "stale", offset = "5_1", isLive = true)
        val batches = CopyOnWriteArrayList<List<ShapeMessage<Row>>>()
        val requests = CopyOnWriteArrayList<io.ktor.http.Url>()
        val markerAfterFirst = java.util.concurrent.atomic.AtomicReference<ElectricOffsetEntity?>()
        val batchesAfterFirst = java.util.concurrent.atomic.AtomicInteger(-1)

        val shapeClient = client(
            dao = dao,
            onMessages = { batches.add(it) },
            handler = { request ->
                requests.add(request.url)
                if (requests.size == 1) {
                    respond(body, status, headersOf("electric-handle" to listOf("h2")))
                } else {
                    // Snapshot the reaction to the reset BEFORE answering.
                    if (requests.size == 2) {
                        markerAfterFirst.set(dao.map["rows"])
                        batchesAfterFirst.set(batches.size)
                    }
                    respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
                }
            },
        )

        val job = launch { shapeClient.run() }
        withTimeout(10_000) {
            while (batches.flatten().none { it is ShapeMessage.Insert }) {
                kotlinx.coroutines.delay(20)
            }
        }
        job.cancel()
        job.join()

        // Nothing was applied in reaction to the reset — in particular no
        // MustRefetch, which would have emptied the table on its own.
        assertEquals(0, batchesAfterFirst.get())
        // The cursor row stays, flagged for a refetch and pointing at -1.
        val marker = markerAfterFirst.get()
        assertNotNull("the offset row must survive the reset", marker)
        assertTrue(marker!!.needsRefetch)
        assertEquals("-1", marker.offset)
        assertEquals("h2", marker.handle)
        assertFalse(marker.isLive)

        // The follow-up poll is a snapshot that carries the rotated handle.
        val second = requests[1]
        assertEquals("-1", second.parameters["offset"])
        assertEquals("h2", second.parameters["handle"])
        assertFalse(second.parameters.contains("live"))

        // …and its batch wipes and repopulates in one go.
        val refetchBatch = batches.first()
        assertEquals(ShapeMessage.MustRefetch, refetchBatch.first())
        assertTrue(refetchBatch.any { it is ShapeMessage.Insert })

        // The refetch is over: fresh cursor, flag cleared, live re-earned.
        assertEquals("h1", dao.map["rows"]?.handle)
        assertEquals(false, dao.map["rows"]?.needsRefetch)
        assertEquals(true, dao.map["rows"]?.isLive)
    }

    @Test
    fun badRequestMarksAnAtomicRefetch() {
        // Electric's deterministic definition error (e.g. "shape definition and
        // handle do not match" after a where-clause rotation under a persisted
        // handle).
        assertMarksRefetchInsteadOfWiping(HttpStatusCode.BadRequest, "definition mismatch")
    }

    @Test
    fun conflictMarksAnAtomicRefetch() {
        assertMarksRefetchInsteadOfWiping(
            HttpStatusCode.Conflict,
            """[{"headers":{"control":"must-refetch"}}]""",
        )
    }

    @Test
    fun inlineMustRefetchMarksTheShapeInsteadOfForwardingTheWipe() = runBlocking {
        val dao = FakeOffsetDao()
        dao.map["rows"] = ElectricOffsetEntity(shape = "rows", handle = "stale", offset = "5_1", isLive = true)
        val batches = CopyOnWriteArrayList<List<ShapeMessage<Row>>>()
        val requests = CopyOnWriteArrayList<io.ktor.http.Url>()

        val shapeClient = client(
            dao = dao,
            onMessages = { batches.add(it) },
            handler = { request ->
                requests.add(request.url)
                if (requests.size == 1) {
                    // A must-refetch inside a 200 body.
                    respond(
                        """[{"headers":{"control":"must-refetch"}}]""",
                        HttpStatusCode.OK,
                        shapeHeaders(),
                    )
                } else {
                    respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
                }
            },
        )

        val job = launch { shapeClient.run() }
        withTimeout(10_000) {
            while (batches.flatten().none { it is ShapeMessage.Insert }) {
                kotlinx.coroutines.delay(20)
            }
        }
        job.cancel()
        job.join()

        // The bare wipe never reached the apply layer; the first batch is the
        // refetch snapshot, wipe included.
        assertEquals(ShapeMessage.MustRefetch, batches.first().first())
        assertTrue(batches.first().any { it is ShapeMessage.Insert })
        assertEquals("-1", requests[1].parameters["offset"])
    }

    @Test
    fun kickInterruptsABackoffWaitAndResetsIt() = runBlocking {
        val dao = FakeOffsetDao()
        val clock = FakeClock()
        val requestAt = CopyOnWriteArrayList<Long>()
        val errors = CopyOnWriteArrayList<String?>()

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            nowMs = clock,
            onError = { _, message, _ -> errors.add(message) },
            handler = {
                requestAt.add(System.currentTimeMillis())
                respond("boom", HttpStatusCode.InternalServerError)
            },
        )

        val job = launch { shapeClient.run() }
        // Three failures grow the backoff to a 2s wait (500 → 1000 → 2000).
        withTimeout(10_000) {
            while (errors.size < 3) kotlinx.coroutines.delay(10)
        }
        // Construction seeds the freshness window (a fresh client's first poll
        // is imminent by definition) — step past it so the kick isn't dropped.
        clock.advance(KICK_FRESHNESS_MS + 1)
        val kickedAt = System.currentTimeMillis()
        shapeClient.kick()
        withTimeout(10_000) {
            while (requestAt.size < 4) kotlinx.coroutines.delay(10)
        }
        val afterKick = requestAt[3] - kickedAt
        // …and the backoff is back at its floor, so the NEXT retry is quick
        // too (it would be a 4s wait had the kick only skipped one round).
        withTimeout(10_000) {
            while (requestAt.size < 5) kotlinx.coroutines.delay(10)
        }
        val afterReset = requestAt[4] - requestAt[3]
        job.cancel()
        job.join()

        assertTrue("kick must cut the 2s backoff short (was ${afterKick}ms)", afterKick < 1_000)
        assertTrue("backoff must reset to its floor (was ${afterReset}ms)", afterReset < 1_500)
    }

    @Test
    fun kickCancelsAnInFlightPollAndRepollsWithoutAnError() = runBlocking {
        val dao = FakeOffsetDao()
        val clock = FakeClock()
        val started = CopyOnWriteArrayList<Int>()
        val errors = CopyOnWriteArrayList<String?>()
        val applied = CopyOnWriteArrayList<ShapeMessage<Row>>()

        val shapeClient = client(
            dao = dao,
            onMessages = { applied.addAll(it) },
            nowMs = clock,
            onError = { _, message, _ -> errors.add(message) },
            handler = {
                started.add(started.size + 1)
                if (started.size == 1) {
                    // A live long-poll holding open with nothing to say.
                    kotlinx.coroutines.delay(30_000)
                }
                respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
            },
        )

        val job = launch { shapeClient.run() }
        withTimeout(10_000) {
            while (started.isEmpty()) kotlinx.coroutines.delay(10)
        }
        // Step past the construction-seeded freshness window so the kick lands.
        clock.advance(KICK_FRESHNESS_MS + 1)
        shapeClient.kick()
        withTimeout(10_000) {
            while (applied.none { it is ShapeMessage.Insert }) kotlinx.coroutines.delay(10)
        }
        job.cancel()
        job.join()

        assertTrue("the kicked poll must be re-issued", started.size >= 2)
        // An interrupted poll is not a failure — it must not touch the shape's
        // error health or its diagnostics row.
        assertTrue("a kick must not report an error, got $errors", errors.isEmpty())
    }

    @Test
    fun kickIsANoOpRightAfterASuccessfulPoll() = runBlocking {
        val dao = FakeOffsetDao()
        val clock = FakeClock()
        val started = CopyOnWriteArrayList<Int>()

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            nowMs = clock,
            handler = {
                started.add(started.size + 1)
                // Everything after the first poll is an idle live hold.
                if (started.size > 1) kotlinx.coroutines.delay(30_000)
                respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
            },
        )

        val job = launch { shapeClient.run() }
        // Wait for the first poll to succeed and the second (live) one to start.
        withTimeout(10_000) {
            while (started.size < 2) kotlinx.coroutines.delay(10)
        }

        // The live hold IS the freshest state available, so this kick is dropped.
        shapeClient.kick()
        kotlinx.coroutines.delay(300)
        assertEquals("a kick inside the freshness window must not re-poll", 2, started.size)

        // Past the window it takes effect again.
        clock.advance(KICK_FRESHNESS_MS + 1)
        shapeClient.kick()
        withTimeout(10_000) {
            while (started.size < 3) kotlinx.coroutines.delay(10)
        }
        job.cancel()
        job.join()
    }

    /**
     * EXP-304: a live cursor's first poll after a kick must NOT go live.
     *
     * A `live=true` request is held open by Electric until something changes,
     * so nothing ever observes that we caught up — which is why the "Syncing…"
     * chip used to clear on a 15s timer and pull-to-refresh always burned its
     * full 5s. One non-live poll answers in a single round-trip; the poll after
     * it goes live as usual.
     */
    @Test
    fun theFirstPollAfterAKickConfirmsFreshnessNonLive() = runBlocking {
        val dao = FakeOffsetDao()
        val clock = FakeClock()
        val requests = CopyOnWriteArrayList<HttpRequestData>()

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            nowMs = clock,
            handler = { request ->
                requests.add(request)
                // Poll 2 is the live hold; poll 3 (post-kick) must not be.
                if (requests.size == 2) kotlinx.coroutines.delay(30_000)
                respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
            },
        )

        val job = launch { shapeClient.run() }
        withTimeout(10_000) {
            while (requests.size < 2) kotlinx.coroutines.delay(10)
        }
        // Poll 1 is the cold snapshot (offset=-1, never live); poll 2 rides the
        // live cursor it just earned.
        assertFalse("the snapshot is never live", requests[0].url.parameters.contains("live"))
        assertEquals("true", requests[1].url.parameters["live"])

        // Past the freshness window so the kick isn't dropped as redundant.
        clock.advance(KICK_FRESHNESS_MS + 1)
        shapeClient.kick()
        withTimeout(10_000) {
            while (requests.size < 4) kotlinx.coroutines.delay(10)
        }
        job.cancel()
        job.join()

        assertFalse(
            "the poll a kick asks for must come back, not park in a live hold",
            requests[2].url.parameters.contains("live"),
        )
        assertEquals(
            "and the poll after it goes live again",
            "true",
            requests[3].url.parameters["live"],
        )
        assertEquals("the cursor stays live throughout", true, dao.map["rows"]?.isLive)
    }

    /**
     * EXP-304: DNS/connect-class failures mean "the network isn't usable yet"
     * (a radio waking, a VPN establishing), not "the server is unhappy". They
     * clear in a second or two, so the first few retries stay flat and short
     * instead of climbing the 500ms→30s ladder — which used to leave a shape
     * parked for tens of seconds AFTER connectivity had come back.
     */
    @Test
    fun dnsFailuresRetryFastInsteadOfClimbingTheBackoffLadder() = runBlocking {
        val dao = FakeOffsetDao()
        val attempts = CopyOnWriteArrayList<Int>()

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            handler = {
                attempts.add(attempts.size + 1)
                if (attempts.size <= 4) throw java.nio.channels.UnresolvedAddressException()
                respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
            },
        )

        val started = System.currentTimeMillis()
        val job = launch { shapeClient.run() }
        withTimeout(20_000) {
            while (attempts.size < 5) kotlinx.coroutines.delay(10)
        }
        val elapsed = System.currentTimeMillis() - started
        job.cancel()
        job.join()

        // Flat 750ms x 4 ≈ 3s. The old ladder (0.5 + 1 + 2 + 4) would be 7.5s
        // and keeps doubling from there.
        assertTrue(
            "four DNS failures must not cost more than the flat burst (was ${elapsed}ms)",
            elapsed < 5_000,
        )
    }

    /**
     * REV2-38: backgrounding the app parks the loop. The in-flight long-poll is
     * cancelled (the socket is released instead of being held for the rest of
     * its 90s budget), no further polls go out — not even for a kick, which is
     * what an FCM push does to a cached process — and none of it is reported as
     * an error. Foregrounding resumes polling.
     */
    @Test
    fun backgroundingCancelsTheInFlightPollAndParksTheLoop() = runBlocking {
        val dao = FakeOffsetDao()
        val clock = FakeClock()
        val active = MutableStateFlow(true)
        val started = CopyOnWriteArrayList<Int>()
        val errors = CopyOnWriteArrayList<String?>()
        val droppedInFlight = java.util.concurrent.atomic.AtomicBoolean(false)

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            nowMs = clock,
            onError = { _, message, _ -> errors.add(message) },
            active = active,
            handler = {
                started.add(started.size + 1)
                if (started.size == 1) {
                    // A live long-poll holding open with nothing to say.
                    try {
                        kotlinx.coroutines.delay(30_000)
                    } catch (cancel: CancellationException) {
                        droppedInFlight.set(true)
                        throw cancel
                    }
                }
                respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
            },
        )

        val job = launch { shapeClient.run() }
        withTimeout(10_000) {
            while (started.isEmpty()) kotlinx.coroutines.delay(10)
        }

        active.value = false
        withTimeout(10_000) {
            while (!droppedInFlight.get()) kotlinx.coroutines.delay(10)
        }
        val parkedAt = started.size
        // Step past the construction-seeded freshness window so the kick is
        // really enqueued (an FCM push into a cached process) instead of being
        // dropped as redundant — otherwise this leg would prove nothing.
        clock.advance(KICK_FRESHNESS_MS + 1)
        shapeClient.kick()
        kotlinx.coroutines.delay(300)
        assertEquals("a parked loop must not poll", parkedAt, started.size)
        assertTrue("parking is not a failure, got $errors", errors.isEmpty())

        active.value = true
        withTimeout(10_000) {
            while (started.size <= parkedAt) kotlinx.coroutines.delay(10)
        }
        job.cancel()
        job.join()

        assertTrue("foregrounding must resume polling", errors.isEmpty())
    }

    /**
     * A process started headlessly (an FCM push wakes it with no Activity) has
     * never been in the foreground, so the gate is still closed: the loop must
     * issue nothing at all until the app is actually opened.
     */
    @Test
    fun aLoopStartedBackgroundedIssuesNoRequestsUntilForeground() = runBlocking {
        val dao = FakeOffsetDao()
        val active = MutableStateFlow(false)
        val started = CopyOnWriteArrayList<Int>()

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            active = active,
            handler = {
                started.add(started.size + 1)
                respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
            },
        )

        val job = launch { shapeClient.run() }
        kotlinx.coroutines.delay(300)
        assertTrue("a backgrounded process must not poll", started.isEmpty())

        active.value = true
        withTimeout(10_000) {
            while (started.isEmpty()) kotlinx.coroutines.delay(10)
        }
        job.cancel()
        job.join()
    }

    /**
     * EXP-656: the gate reopening wakes a loop that is WAITING, not just one
     * that is polling.
     *
     * Parking cancels an in-flight request, but a loop sitting on the backoff
     * ladder (or the DNS burst, or the pacing delay) used to notice the gate
     * again only when that wait expired — up to 30s of stale content on an app
     * the user is looking at, rescued only by a kick the 1s debounce could
     * swallow. The gate's false→true edge is now itself the kick.
     */
    @Test
    fun reopeningTheGateWakesALoopParkedInBackoff() = runBlocking {
        val dao = FakeOffsetDao()
        val active = MutableStateFlow(true)
        val errors = CopyOnWriteArrayList<String?>()
        val requestAt = CopyOnWriteArrayList<Long>()

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            onError = { _, message, _ -> errors.add(message) },
            active = active,
            handler = {
                requestAt.add(System.currentTimeMillis())
                respond("boom", HttpStatusCode.InternalServerError)
            },
        )

        val job = launch { shapeClient.run() }
        // Three failures put the loop into a 2s wait (500 → 1000 → 2000).
        withTimeout(10_000) {
            while (errors.size < 3) kotlinx.coroutines.delay(10)
        }
        val parkedRequests = requestAt.size
        active.value = false
        // Nothing goes out while parked, however long the wait had left.
        kotlinx.coroutines.delay(200)
        assertEquals("a parked loop must not poll", parkedRequests, requestAt.size)

        val reopenedAt = System.currentTimeMillis()
        active.value = true
        withTimeout(10_000) {
            while (requestAt.size <= parkedRequests) kotlinx.coroutines.delay(10)
        }
        val afterResume = requestAt.last() - reopenedAt
        job.cancel()
        job.join()

        assertTrue(
            "the gate edge must cut the backoff short (was ${afterResume}ms)",
            afterResume < 1_000,
        )
    }

    /**
     * EXP-656: the gate edge queues a self-kick, and a queued kick must never
     * cancel the very poll it asked for — `pollOnce` drains pending kicks
     * before its request. One resume, one request.
     */
    @Test
    fun reopeningTheGateIssuesExactlyOneRequest() = runBlocking {
        val dao = FakeOffsetDao()
        val active = MutableStateFlow(true)
        val started = CopyOnWriteArrayList<Int>()
        val errors = CopyOnWriteArrayList<String?>()

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            onError = { _, message, _ -> errors.add(message) },
            active = active,
            handler = {
                started.add(started.size + 1)
                // Everything after the first poll holds open like a live poll,
                // so a self-cancel would show up as an extra request here.
                if (started.size > 1) kotlinx.coroutines.delay(30_000)
                respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
            },
        )

        val job = launch { shapeClient.run() }
        withTimeout(10_000) {
            while (started.size < 2) kotlinx.coroutines.delay(10)
        }
        active.value = false
        kotlinx.coroutines.delay(100)
        val parked = started.size

        active.value = true
        withTimeout(10_000) {
            while (started.size <= parked) kotlinx.coroutines.delay(10)
        }
        kotlinx.coroutines.delay(400)
        job.cancel()
        job.join()

        assertEquals("the resume must cost exactly one request", parked + 1, started.size)
        assertTrue("a resume is not a failure, got $errors", errors.isEmpty())
    }

    /**
     * EXP-656: the catch-up poll a resume issues is the one request that can
     * land on a pooled socket the radio killed, and the only one the user is
     * waiting on — so it carries the SHORT budget, not the 90s live-hold one.
     */
    @Test
    fun theResumeCatchUpPollUsesTheConfirmTimeout() = runBlocking {
        val dao = FakeOffsetDao()
        val active = MutableStateFlow(true)
        val requests = CopyOnWriteArrayList<HttpRequestData>()
        val budgets = CopyOnWriteArrayList<Long?>()

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            active = active,
            handler = { request ->
                requests.add(request)
                budgets.add(request.getCapabilityOrNull(HttpTimeoutCapability)?.requestTimeoutMillis)
                // Poll 2 is the live hold the park interrupts.
                if (requests.size == 2) kotlinx.coroutines.delay(30_000)
                respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
            },
        )

        val job = launch { shapeClient.run() }
        withTimeout(10_000) {
            while (requests.size < 2) kotlinx.coroutines.delay(10)
        }
        active.value = false
        kotlinx.coroutines.delay(100)
        active.value = true
        withTimeout(10_000) {
            while (requests.size < 3) kotlinx.coroutines.delay(10)
        }
        job.cancel()
        job.join()

        // The snapshot and the live hold keep the long budget…
        assertTrue("the snapshot keeps the live-safe budget", budgets[0]!! >= 75_000)
        assertEquals("true", requests[1].url.parameters["live"])
        assertTrue("the live poll keeps the live-safe budget", budgets[1]!! >= 75_000)
        // …and the resume's catch-up is non-live on the short one.
        assertFalse(
            "the resume must confirm freshness non-live",
            requests[2].url.parameters.contains("live"),
        )
        assertEquals(CONFIRM_TIMEOUT_MS, budgets[2])
    }

    @Test
    fun shapePollsCarryALongPollSafeTimeoutBudget() = runBlocking {
        val dao = FakeOffsetDao()
        var requestTimeout: Long? = null
        var socketTimeout: Long? = null

        var connectTimeout: Long? = null

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            handler = { request ->
                val config = request.getCapabilityOrNull(HttpTimeoutCapability)
                requestTimeout = config?.requestTimeoutMillis
                socketTimeout = config?.socketTimeoutMillis
                connectTimeout = config?.connectTimeoutMillis
                respond(insertAndUpToDateBody, HttpStatusCode.OK, shapeHeaders())
            },
        )

        val job = launch { shapeClient.run() }
        withTimeout(10_000) {
            while (requestTimeout == null) {
                kotlinx.coroutines.delay(20)
            }
        }
        job.cancel()
        job.join()

        // Must exceed the server's live long-poll hold window (~60s worst
        // case per long-poll-canary.md; desktop asserts >= 75s the same way).
        assertTrue("request timeout must exceed the live hold", requestTimeout!! >= 75_000)
        assertTrue("socket timeout must exceed the live hold", socketTimeout!! >= 75_000)
        // Connect is set EXPLICITLY: a per-request timeout block replaces the
        // whole config, so leaving it out kept the client-wide 10s — the ~10s
        // of stale content on app open (EXP-264).
        assertEquals(5_000L, connectTimeout)
    }
}
