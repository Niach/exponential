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
        override suspend fun deleteShape(shape: String) { map.remove(shape) }
        override suspend fun clear() { map.clear() }
    }

    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

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
        )
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

    @Test
    fun badRequestResetsTheShapeLikeMustRefetch() = runBlocking {
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
                    // Electric's deterministic definition error (e.g. "shape
                    // definition and handle do not match" after a where-clause
                    // rotation under a persisted handle).
                    respond("definition mismatch", HttpStatusCode.BadRequest)
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

        // First reaction to the 400: cursor dropped + MustRefetch (table wipe).
        assertEquals(listOf<ShapeMessage<Row>>(ShapeMessage.MustRefetch), batches.first())
        // The follow-up poll is a fresh initial snapshot: offset=-1, no handle.
        val second = requests[1]
        assertEquals("-1", second.parameters["offset"])
        assertNull(second.parameters["handle"])
        assertFalse(second.parameters.contains("live"))
        // And the snapshot lands + the cursor is re-established.
        assertTrue(batches.flatten().any { it is ShapeMessage.Insert })
        assertEquals("h1", dao.map["rows"]?.handle)
    }

    @Test
    fun conflictResetsTheShape() = runBlocking {
        val dao = FakeOffsetDao()
        dao.map["rows"] = ElectricOffsetEntity(shape = "rows", handle = "stale", offset = "5_1", isLive = true)
        val batches = CopyOnWriteArrayList<List<ShapeMessage<Row>>>()
        var calls = 0

        val shapeClient = client(
            dao = dao,
            onMessages = { batches.add(it) },
            handler = {
                calls++
                if (calls == 1) {
                    respond("""[{"headers":{"control":"must-refetch"}}]""", HttpStatusCode.Conflict)
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

        assertEquals(listOf<ShapeMessage<Row>>(ShapeMessage.MustRefetch), batches.first())
        assertTrue(batches.flatten().any { it is ShapeMessage.Insert })
    }

    @Test
    fun shapePollsCarryALongPollSafeTimeoutBudget() = runBlocking {
        val dao = FakeOffsetDao()
        var requestTimeout: Long? = null
        var socketTimeout: Long? = null

        val shapeClient = client(
            dao = dao,
            onMessages = {},
            handler = { request ->
                val config = request.getCapabilityOrNull(HttpTimeoutCapability)
                requestTimeout = config?.requestTimeoutMillis
                socketTimeout = config?.socketTimeoutMillis
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
    }
}
