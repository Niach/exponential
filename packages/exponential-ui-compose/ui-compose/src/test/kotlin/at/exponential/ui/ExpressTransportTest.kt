package at.exponential.ui

import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.InferenceFormat
import at.exponential.ui.host.JsonlStreamTransport
import at.exponential.ui.host.McpTransport
import at.exponential.ui.host.SseTransport
import at.exponential.ui.host.Transport
import at.exponential.ui.host.TransportStatus
import at.exponential.ui.host.WebSocketConnection
import at.exponential.ui.host.WebSocketListener
import at.exponential.ui.host.WebSocketTransport
import at.exponential.ui.json.JsonValue
import com.sun.net.httpserver.HttpServer
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.File
import java.net.InetSocketAddress
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * #AXC-12: the transports read the Express inference format (catalog/host.json
 * `inference`): an `application/a2ui+express` body, `a2ui-express` SSE events,
 * `{express}` WebSocket frames, Express MCP resources. Messages go to
 * `receive`, the prose to `text` (the host's `onText`). Driven by
 * `fixtures/host-transport.json` over a local HTTP server (plain JVM).
 */
class ExpressTransportTest {
    private lateinit var server: HttpServer
    private val fixture by lazy { JsonValue.parse(File(Fixtures.dir, "host-transport.json").readText()) }

    private fun case(section: String, name: String): JsonValue = fixture[section]!!.array!!.first { it["name"]?.string == name }

    @Before
    fun start() {
        Fixtures.require()
        server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.start()
    }

    @After
    fun stop() = server.stop(0)

    private fun url(path: String) = "http://127.0.0.1:${server.address.port}$path"

    private fun serve(path: String, contentType: String, body: (String) -> String) {
        server.createContext(path) { ex ->
            val request = ex.requestBody.readBytes().toString(Charsets.UTF_8)
            ex.responseHeaders.add("content-type", contentType)
            ex.responseHeaders.add("x-accept", ex.requestHeaders.getFirst("accept") ?: "")
            val bytes = body(request).toByteArray(Charsets.UTF_8)
            ex.sendResponseHeaders(200, bytes.size.toLong())
            ex.responseBody.use { it.write(bytes) }
        }
    }

    /** What a transport delivered until it reported `closed` (or `open`, for MCP). */
    private class Run(transport: Transport, until: TransportStatus, close: Boolean = true) {
        val messages = CopyOnWriteArrayList<JsonValue>()
        val text = CopyOnWriteArrayList<String>()
        private val done = CountDownLatch(1)

        init {
            transport.start({ messages.add(it) }, { s, _ -> if (s == until || s == TransportStatus.Error) done.countDown() }, { text.add(it) })
            assertTrue("the transport finished", done.await(20, TimeUnit.SECONDS))
            if (close) transport.close()
        }
    }

    /** Components by id, last definition wins (the same surface however the stream was chunked). */
    private fun settle(messages: List<JsonValue>): Map<String, String> {
        val out = LinkedHashMap<String, String>()
        for (m in messages) {
            val body = (m as JsonValue.Obj).v.entries.first { it.key != "version" }.value
            for (c in body["components"]?.array ?: emptyList()) out["${body["surfaceId"]?.string}/${c["id"]?.string}"] = c.json
        }
        return out
    }

    @Test
    fun anExpressBodyStreamsThroughTheJsonlTransport() {
        val c = case("express", "prose and a block split inside the tags, an identifier and a string")
        val body = c["chunks"]!!.array!!.joinToString("") { it.string!! }
        serve("/stream", "application/a2ui+express; charset=utf-8") { body }
        val run = Run(JsonlStreamTransport(url("/stream"), reconnectMs = 0, format = InferenceFormat.Express), TransportStatus.Closed)
        assertEquals(settle(c["expected"]!!["messages"]!!.array!!), settle(run.messages))
        assertEquals(c["expected"]!!["text"]!!.array!!.joinToString("") { it.string!! }, run.text.joinToString(""))
    }

    @Test
    fun aJsonBodyStaysJsonWithoutProse() {
        serve("/jsonl", "application/jsonl") { """{"version":"1.0","createSurface":{"surfaceId":"s1","catalogId":"x"}}""" + "\n" }
        val run = Run(JsonlStreamTransport(url("/jsonl"), reconnectMs = 0), TransportStatus.Closed)
        assertEquals(1, run.messages.size)
        assertTrue(run.text.isEmpty())
    }

    @Test
    fun expressSseEventsAreOneResponse() {
        val c = case("expressSse", "a2ui-express events are the chunks of one response, JSON events beside them")
        val body = c["chunks"]!!.array!!.joinToString("") { it.string!! }
        serve("/sse", "text/event-stream") { body }
        val run = Run(SseTransport(url("/sse"), reconnectMs = 0), TransportStatus.Closed)
        assertEquals(settle(c["expected"]!!["messages"]!!.array!!), settle(run.messages))
        assertEquals(c["expected"]!!["text"]!!.array!!.map { it.string }, run.text.toList())
    }

    @Test
    fun expressFramesAreOneResponseFlushedAtClose() {
        val c = case("expressWs", "{express} frames are one response, JSON frames beside them")
        val frames = c["frames"]!!.array!!.map { it.string!! }
        val transport = WebSocketTransport(url("/ws"), { _, listener: WebSocketListener ->
            listener.onOpen()
            frames.forEach(listener::onText)
            listener.onClosed(1000, "")
            object : WebSocketConnection {
                override fun send(text: String) = true
                override fun close() {}
            }
        }, reconnectMs = 0)
        val run = Run(transport, TransportStatus.Closed)
        assertEquals(c["expected"]!!["messages"]!!.array!!.map { it.json }, run.messages.map { it.json })
        assertEquals(c["expected"]!!["text"]!!.array!!.map { it.string }, run.text.toList())
    }

    @Test
    fun anExpressMcpResourceCompilesForTheTransportsSurface() {
        val c = case("expressMcp", "JSON and Express resources together, in order")
        serve("/mcp", "application/json") { request ->
            val rpc = JsonValue.parse(request)
            val result = if (rpc["method"]?.string == "tools/call") c["result"]!!.json else "{}"
            """{"jsonrpc":"2.0","id":${rpc["id"]!!.json},"result":$result}"""
        }
        val mcp = McpTransport(url("/mcp"), "show", surfaceId = c["surfaceId"]?.string)
        val run = Run(mcp, TransportStatus.Open, close = false)
        // `open` comes just before the messages: wait for them.
        val deadline = System.currentTimeMillis() + 10_000
        while (run.messages.size < 2 && System.currentTimeMillis() < deadline) Thread.sleep(10)
        mcp.close()
        assertEquals(c["expected"]!!["messages"]!!.array!!.map { it.json }, run.messages.map { it.json })
    }

    @Test
    fun theHostHandsTheProseToOnTextAndAdvertisesBothFormats() {
        val c = case("expressWs", "{express} frames are one response, JSON frames beside them")
        val frames = c["frames"]!!.array!!.map { it.string!! }
        val prose = CopyOnWriteArrayList<String>()
        val ws = WebSocketTransport("ws://local", { _, listener ->
            listener.onOpen()
            frames.forEach(listener::onText)
            listener.onClosed(1000, "")
            object : WebSocketConnection {
                override fun send(text: String) = true
                override fun close() {}
            }
        }, reconnectMs = 0)
        val host = ExponentialHost(HostOptions(transport = ws, onText = { prose.add(it) }), CoroutineScope(Dispatchers.Unconfined))
        host.connect()
        assertEquals(c["expected"]!!["text"]!!.array!!.joinToString("") { it.string!! }, prose.joinToString(""))
        assertEquals(listOf("json", "express"), host.rendererCapabilities()["1.0"]?.get("inferenceFormats")?.array?.map { it.string })
        val jsonOnly = ExponentialHost(HostOptions(inferenceFormats = listOf(InferenceFormat.Json)), CoroutineScope(Dispatchers.Unconfined))
        assertEquals(listOf("json"), jsonOnly.rendererCapabilities()["1.0"]?.get("inferenceFormats")?.array?.map { it.string })
    }
}
