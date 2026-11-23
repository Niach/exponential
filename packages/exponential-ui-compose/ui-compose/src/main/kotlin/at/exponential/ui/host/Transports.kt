package at.exponential.ui.host

import at.exponential.ui.ffi.ExpressDecoder
import at.exponential.ui.ffi.FrameDecoder
import at.exponential.ui.ffi.JsonlDecoder
import at.exponential.ui.ffi.SseDecoder
import at.exponential.ui.ffi.decodeJsonlJson
import at.exponential.ui.ffi.hostContractJson
import at.exponential.ui.ffi.inferenceFormatOf
import at.exponential.ui.ffi.mcpActionCallJson
import at.exponential.ui.ffi.messagesFromMcpResultWithJson
import at.exponential.ui.json.JsonValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URL
import java.nio.ByteBuffer
import java.nio.CharBuffer
import java.nio.charset.CodingErrorAction
import java.util.concurrent.CopyOnWriteArrayList

// VAPP-91: the transport adapters the Kotlin SDK ships (the twins of the TS
// reference `src/host/transports.ts`). Each speaks A2UI messages in and
// client messages out; the PARSING is the core's (`JsonlDecoder`,
// `SseDecoder`, `ExpressDecoder`, `decodeJsonlJson`, `messagesFromMcpResultJson`,
// `mcpActionCallJson` through the facade), never re-implemented here.
// No HTTP library: `HttpURLConnection` on Dispatchers.IO.
//
// #AXC-12: each reads the Express inference format too (catalog/host.json
// `inference`): an HTTP body by its Content-Type, `a2ui-express` SSE events,
// `{express}` WebSocket frames, Express MCP resources. The prose goes to
// `start`'s `text` (the host's `onText`); the SDK draws none.

/** A transport's connection state. */
enum class TransportStatus(val wire: String) { Connecting("connecting"), Open("open"), Closed("closed"), Error("error") }

/** #AXC-12: what an agent writes: A2UI JSON messages (the default) or Express (compiled by the core against the core catalog). */
enum class InferenceFormat(val wire: String) { Json("json"), Express("express") }

/** #AXC-12: the `inference` keys of the core's contract, read once. */
internal object InferenceWire {
    private val express by lazy { JsonValue.parse(hostContractJson())["inference"]?.get("express") }
    val mimeType: String by lazy { express?.get("mimeType")?.string ?: "application/a2ui+express" }
    val defaultSurfaceId: String by lazy { express?.get("defaultSurfaceId")?.string ?: "main" }
}

/** Messages in, client messages out. Callbacks may come from any thread; [ExponentialHost] hops to its own scope. */
interface Transport {
    /** Start delivering server messages; report the connection state. */
    fun start(receive: (JsonValue) -> Unit, status: (TransportStatus, String?) -> Unit)

    /** #AXC-12: [start] plus [text], the prose of an Express response piece by piece (append them). The default drops the prose. */
    fun start(receive: (JsonValue) -> Unit, status: (TransportStatus, String?) -> Unit, text: ((String) -> Unit)?) = start(receive, status)

    /** One client message (A2UI `action` / `error`). */
    suspend fun send(message: JsonValue)

    /** Round 9 §1.3: a message with the `sendDataModel` metadata beside it: `{"message", "metadata"}` on the wire (the MCP carrier sends it beside). */
    suspend fun send(message: JsonValue, metadata: JsonValue) = send(JsonValue.Obj(mapOf("message" to message, "metadata" to metadata)))

    fun close()
}

/** The messages of a decoder result `{"messages": [...], "issues": [...]}`. */
internal fun decodedMessages(json: String): List<JsonValue> = JsonValue.parse(json)["messages"]?.array ?: emptyList()

/** #AXC-12: the prose pieces of a decoder result (`text`: an Express response's prose around its `<a2ui>` blocks; never for JSON). Append them in order; the SDK draws none. */
internal fun decodedText(json: String): List<String> = JsonValue.parse(json)["text"]?.array?.mapNotNull { it.string } ?: emptyList()

/** One decoder result: messages to [receive], prose to [text]. */
internal fun deliver(json: String, receive: (JsonValue) -> Unit, text: ((String) -> Unit)?) {
    val v = JsonValue.parse(json)
    v["messages"]?.array?.forEach(receive)
    if (text != null) v["text"]?.array?.forEach { piece -> piece.string?.let(text) }
}

/** In-memory: [feed] messages in, read what the host sent from [sent]. */
class MemoryTransport : Transport {
    /** Every client message the host sent, in order. */
    val sent: MutableList<JsonValue> = CopyOnWriteArrayList()

    /** Observes every sent message. */
    var onSend: ((JsonValue) -> Unit)? = null

    private var receiveFn: ((JsonValue) -> Unit)? = null
    private val pending = ArrayList<JsonValue>()

    override fun start(receive: (JsonValue) -> Unit, status: (TransportStatus, String?) -> Unit) {
        val queued: List<JsonValue>
        synchronized(this) {
            receiveFn = receive
            queued = pending.toList()
            pending.clear()
        }
        status(TransportStatus.Open, null)
        queued.forEach(receive)
    }

    /** Server messages (queued until [start]). */
    fun feed(vararg messages: JsonValue) {
        val r = synchronized(this) { receiveFn ?: run { pending.addAll(messages); null } } ?: return
        messages.forEach(r)
    }

    /** Server messages as JSON text (one message, an array or JSONL). */
    fun feedJsonl(text: String) = feed(*decodedMessages(decodeJsonlJson(text)).toTypedArray())

    override suspend fun send(message: JsonValue) {
        sent.add(message)
        onSend?.invoke(message)
    }

    override fun close() {
        synchronized(this) { receiveFn = null }
    }
}

/** A streamed HTTP transport's options. */
data class HttpTransportOptions(
    /** The stream to read (GET). */
    val url: String,
    /** Where client messages go (POST, JSON body); default [url]. */
    val postUrl: String? = null,
    val headers: Map<String, String> = emptyMap(),
    /** Reconnect after a drop, ms (0 = never). */
    val reconnectMs: Long = 2000,
    val connectTimeoutMs: Int = 15_000,
    /** #AXC-12: [InferenceFormat.Express] asks for Express (`accept`). Either way an `application/a2ui+express` body (and SSE `a2ui-express` events) is read as Express: the wire decides. */
    val format: InferenceFormat = InferenceFormat.Json,
    /** The surface Express statements before any `surface("…")` target (null = `main`). */
    val surfaceId: String? = null,
)

/** A chunked decoder of the facade (JSONL, SSE or Express). */
internal interface ChunkDecoder {
    fun push(chunk: String): String
    fun end(): String
}

/** UTF-8 bytes → text chunks, a multi-byte sequence split across reads stays whole. */
internal class Utf8Chunks {
    private val decoder = Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPLACE).onUnmappableCharacter(CodingErrorAction.REPLACE)
    private var carry = ByteArray(0)

    fun decode(bytes: ByteArray, n: Int): String {
        val input = ByteBuffer.allocate(carry.size + n).put(carry).put(bytes, 0, n)
        input.flip()
        val out = CharBuffer.allocate(input.remaining() + 1)
        decoder.decode(input, out, false)
        carry = ByteArray(input.remaining()).also { input.get(it) }
        out.flip()
        return out.toString()
    }
}

/** #AXC-12: the core's Express decoder as a [ChunkDecoder] (`wrapped` = prose with `<a2ui>` blocks, else one bare body). */
internal fun expressChunkDecoder(wrapped: Boolean, surfaceId: String): ChunkDecoder = object : ChunkDecoder {
    val d = ExpressDecoder(wrapped, surfaceId)
    override fun push(chunk: String) = d.push(chunk)
    override fun end() = d.end().also { d.close() }
}

/** Read a stream through a decoder, delivering every message (and, to [text], an Express stream's prose). */
internal fun pump(stream: InputStream, decoder: ChunkDecoder, receive: (JsonValue) -> Unit, text: ((String) -> Unit)? = null, alive: () -> Boolean) {
    val chars = Utf8Chunks()
    val buf = ByteArray(8192)
    while (alive()) {
        val n = stream.read(buf)
        if (n < 0) break
        if (n == 0) continue
        deliver(decoder.push(chars.decode(buf, n)), receive, text)
    }
    deliver(decoder.end(), receive, text)
}

/** POST a JSON body; returns (status, content type, body text, headers). */
internal fun postJson(url: String, body: String, headers: Map<String, String>, timeoutMs: Int = 15_000): HttpReply {
    val conn = URL(url).openConnection() as HttpURLConnection
    try {
        conn.requestMethod = "POST"
        conn.doOutput = true
        conn.connectTimeout = timeoutMs
        conn.readTimeout = 60_000
        conn.setRequestProperty("content-type", "application/json")
        for ((k, v) in headers) conn.setRequestProperty(k, v)
        conn.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
        val code = conn.responseCode
        val stream = if (code in 200..299) conn.inputStream else conn.errorStream
        val text = stream?.use { it.readBytes().toString(Charsets.UTF_8) } ?: ""
        return HttpReply(code, conn.contentType ?: "", text, conn.getHeaderField("mcp-session-id"))
    } finally {
        conn.disconnect()
    }
}

internal data class HttpReply(val code: Int, val contentType: String, val body: String, val mcpSession: String?)

/** The shared GET-stream loop of [JsonlStreamTransport] and [SseTransport]. */
abstract class StreamTransport(val options: HttpTransportOptions) : Transport {
    private var scope: CoroutineScope? = null
    @Volatile private var closed = false
    @Volatile private var connection: HttpURLConnection? = null

    /** The body's decoder; `express` = its Content-Type is Express. */
    internal abstract fun decoder(express: Boolean): ChunkDecoder
    protected abstract val accept: String

    override fun start(receive: (JsonValue) -> Unit, status: (TransportStatus, String?) -> Unit) = start(receive, status, null)

    override fun start(receive: (JsonValue) -> Unit, status: (TransportStatus, String?) -> Unit, text: ((String) -> Unit)?) {
        closed = false
        val s = CoroutineScope(SupervisorJob() + Dispatchers.IO)
        scope = s
        s.launch {
            while (isActive && !closed) {
                status(TransportStatus.Connecting, null)
                try {
                    val conn = URL(options.url).openConnection() as HttpURLConnection
                    connection = conn
                    conn.connectTimeout = options.connectTimeoutMs
                    conn.readTimeout = 0
                    conn.setRequestProperty("accept", if (options.format == InferenceFormat.Express) "${InferenceWire.mimeType}, $accept" else accept)
                    for ((k, v) in options.headers) conn.setRequestProperty(k, v)
                    val code = conn.responseCode
                    if (code !in 200..299) throw java.io.IOException("HTTP $code")
                    status(TransportStatus.Open, null)
                    val express = inferenceFormatOf(conn.contentType ?: "") == InferenceFormat.Express.wire
                    conn.inputStream.use { pump(it, decoder(express), receive, text) { !closed } }
                    if (!closed) status(TransportStatus.Closed, null)
                } catch (e: Exception) {
                    if (closed) return@launch
                    status(TransportStatus.Error, e.message ?: e.toString())
                } finally {
                    connection?.disconnect()
                    connection = null
                }
                if (options.reconnectMs <= 0 || closed) break
                delay(options.reconnectMs)
            }
        }
    }

    override suspend fun send(message: JsonValue) {
        withContext(Dispatchers.IO) { postJson(options.postUrl ?: options.url, message.json, options.headers) }
    }

    override fun close() {
        closed = true
        runCatching { connection?.disconnect() }
        scope?.cancel()
        scope = null
    }
}

/** A2UI JSONL over a streamed HTTP response (`application/jsonl`), through the core's JSONL decoder; an `application/a2ui+express` response is ONE Express response (prose + `<a2ui>` blocks). */
class JsonlStreamTransport(options: HttpTransportOptions) : StreamTransport(options) {
    constructor(url: String, postUrl: String? = null, headers: Map<String, String> = emptyMap(), reconnectMs: Long = 2000, format: InferenceFormat = InferenceFormat.Json, surfaceId: String? = null) :
        this(HttpTransportOptions(url, postUrl, headers, reconnectMs, format = format, surfaceId = surfaceId))

    override val accept = "application/jsonl, application/x-ndjson"

    override fun decoder(express: Boolean): ChunkDecoder = if (express) expressChunkDecoder(true, options.surfaceId ?: InferenceWire.defaultSurfaceId) else object : ChunkDecoder {
        val d = JsonlDecoder()
        override fun push(chunk: String) = d.push(chunk)
        override fun end() = d.end().also { d.close() }
    }
}

/** Server-Sent Events (`text/event-stream`) over a streamed GET, through the core's SSE decoder; `a2ui-express` events are the chunks of the stream's ONE Express response. */
class SseTransport(options: HttpTransportOptions) : StreamTransport(options) {
    constructor(url: String, postUrl: String? = null, headers: Map<String, String> = emptyMap(), reconnectMs: Long = 2000, format: InferenceFormat = InferenceFormat.Json, surfaceId: String? = null) :
        this(HttpTransportOptions(url, postUrl, headers, reconnectMs, format = format, surfaceId = surfaceId))

    override val accept = "text/event-stream"

    override fun decoder(express: Boolean): ChunkDecoder = object : ChunkDecoder {
        val d = SseDecoder.withSurface(options.surfaceId)
        override fun push(chunk: String) = d.push(chunk)
        override fun end() = d.end().also { d.close() }
    }
}

/** What a WebSocket implementation reports to [WebSocketTransport]. Any thread. */
interface WebSocketListener {
    fun onOpen()

    /** One text frame. */
    fun onText(text: String)

    fun onClosed(code: Int, reason: String)

    fun onFailure(error: Throwable)
}

/** An open socket. */
interface WebSocketConnection {
    /** Send one text frame; false when the socket is not open. */
    fun send(text: String): Boolean

    fun close()
}

/**
 * The app's WebSocket client (the AAR ships none). An OkHttp app:
 * ```
 * val connector = WebSocketConnector { url, listener ->
 *     val ws = client.newWebSocket(Request.Builder().url(url).build(), object : okhttp3.WebSocketListener() {
 *         override fun onOpen(webSocket: WebSocket, response: Response) = listener.onOpen()
 *         override fun onMessage(webSocket: WebSocket, text: String) = listener.onText(text)
 *         override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = listener.onClosed(code, reason)
 *         override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) = listener.onFailure(t)
 *     })
 *     object : WebSocketConnection {
 *         override fun send(text: String) = ws.send(text)
 *         override fun close() { ws.close(1000, null) }
 *     }
 * }
 * ```
 */
fun interface WebSocketConnector {
    fun connect(url: String, listener: WebSocketListener): WebSocketConnection
}

/** One A2UI message (or JSONL) per text frame; client messages go back as frames. #AXC-12: `{"express": "…"}` frames are the chunks of the connection's ONE Express response (flushed at close). */
class WebSocketTransport(
    val url: String,
    private val connector: WebSocketConnector,
    /** Reconnect after a close, ms (0 = never). */
    val reconnectMs: Long = 2000,
    /** #AXC-12: [InferenceFormat.Express] = the agent writes Express; the frames decide either way (nothing to negotiate on a socket). */
    val format: InferenceFormat = InferenceFormat.Json,
    /** The surface Express statements before any `surface("…")` target (null = `main`). */
    val surfaceId: String? = null,
) : Transport {
    @Volatile private var closed = false
    @Volatile private var socket: WebSocketConnection? = null
    private var retry: Job? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun start(receive: (JsonValue) -> Unit, status: (TransportStatus, String?) -> Unit) = start(receive, status, null)

    override fun start(receive: (JsonValue) -> Unit, status: (TransportStatus, String?) -> Unit, text: ((String) -> Unit)?) {
        closed = false
        status(TransportStatus.Connecting, null)
        // One decoder per connection: its `{express}` frames are ONE response.
        val frames = FrameDecoder(surfaceId)
        val prose = text
        var ended = false
        fun end() = synchronized(frames) {
            if (ended) return@synchronized
            ended = true
            deliver(frames.end(), receive, text)
            frames.close()
        }
        val listener = object : WebSocketListener {
            override fun onOpen() = status(TransportStatus.Open, null)

            override fun onText(text: String) {
                val out = synchronized(frames) { if (ended) null else frames.push(text) } ?: return
                deliver(out, receive, prose)
            }

            override fun onClosed(code: Int, reason: String) {
                end()
                status(TransportStatus.Closed, reason.ifEmpty { null })
                reconnect(receive, status, text)
            }

            override fun onFailure(error: Throwable) {
                end()
                status(TransportStatus.Error, error.message ?: "websocket error")
                reconnect(receive, status, text)
            }
        }
        socket = try {
            connector.connect(url, listener)
        } catch (e: Exception) {
            listener.onFailure(e)
            null
        }
    }

    private fun reconnect(receive: (JsonValue) -> Unit, status: (TransportStatus, String?) -> Unit, text: ((String) -> Unit)?) {
        if (closed || reconnectMs <= 0) return
        retry?.cancel()
        retry = scope.launch {
            delay(reconnectMs)
            if (!closed) start(receive, status, text)
        }
    }

    override suspend fun send(message: JsonValue) {
        socket?.send(message.json)
    }

    override fun close() {
        closed = true
        retry?.cancel()
        socket?.close()
        socket = null
    }
}

/**
 * A2UI over MCP (streamable HTTP, JSON-RPC over POST): `initialize`, then
 * `tools/call` [tool] once; the A2UI resources of its result are the
 * surface. Every client message goes back as a `tools/call` to
 * [actionTool] (default `a2ui_event`), whose result may carry more
 * messages. An SSE response body is read through the core's SSE decoder.
 */
class McpTransport(
    val url: String,
    val tool: String,
    val arguments: JsonValue = JsonValue.Obj(emptyMap()),
    val actionTool: String? = null,
    val headers: Map<String, String> = emptyMap(),
    /** #AXC-12: [InferenceFormat.Express] = the agent writes Express; the result decides either way (an `application/a2ui+express` resource, `structuredContent.a2uiExpress`). */
    val format: InferenceFormat = InferenceFormat.Json,
    /** The surface Express statements before any `surface("…")` target (null = `main`). */
    val surfaceId: String? = null,
) : Transport {
    private var id = 0
    @Volatile private var session: String? = null
    @Volatile private var receiveFn: ((JsonValue) -> Unit)? = null
    @Volatile private var textFn: ((String) -> Unit)? = null
    private var scope: CoroutineScope? = null

    override fun start(receive: (JsonValue) -> Unit, status: (TransportStatus, String?) -> Unit) = start(receive, status, null)

    override fun start(receive: (JsonValue) -> Unit, status: (TransportStatus, String?) -> Unit, text: ((String) -> Unit)?) {
        receiveFn = receive
        textFn = text
        val s = CoroutineScope(SupervisorJob() + Dispatchers.IO)
        scope = s
        status(TransportStatus.Connecting, null)
        s.launch {
            try {
                rpc(
                    "initialize",
                    JsonValue.parse("""{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"exponential-ui-compose","version":"0.1.0"}}"""),
                )
                val result = rpc("tools/call", JsonValue.Obj(mapOf("name" to JsonValue.Str(tool), "arguments" to arguments)))
                status(TransportStatus.Open, null)
                deliverResult(result)
            } catch (e: Exception) {
                status(TransportStatus.Error, e.message ?: e.toString())
            }
        }
    }

    override suspend fun send(message: JsonValue) = call(message, null)

    override suspend fun send(message: JsonValue, metadata: JsonValue) = call(message, metadata)

    private suspend fun call(message: JsonValue, metadata: JsonValue?) {
        val call = JsonValue.parse(mcpActionCallJson(message.json, actionTool, metadata?.json))
        val result = withContext(Dispatchers.IO) { rpc(call["method"]?.string ?: "tools/call", call["params"] ?: JsonValue.Null) }
        deliverResult(result)
    }

    /** A tool result's messages (Express resources compiled) and prose. */
    private fun deliverResult(result: JsonValue) {
        val r = receiveFn ?: return
        deliver(messagesFromMcpResultWithJson(result.json, surfaceId), r, textFn)
    }

    override fun close() {
        receiveFn = null
        textFn = null
        scope?.cancel()
        scope = null
    }

    private fun rpc(method: String, params: JsonValue): JsonValue {
        val myId = synchronized(this) { ++id }
        val body = JsonValue.Obj(
            mapOf("jsonrpc" to JsonValue.Str("2.0"), "id" to JsonValue.Num(myId.toDouble()), "method" to JsonValue.Str(method), "params" to params),
        ).json
        val h = LinkedHashMap<String, String>()
        h["accept"] = "application/json, text/event-stream"
        session?.let { h["mcp-session-id"] = it }
        h.putAll(headers)
        val reply = postJson(url, body, h)
        reply.mcpSession?.let { session = it }
        if (reply.code !in 200..299) throw java.io.IOException("MCP HTTP ${reply.code}")
        val replies = if (reply.contentType.contains("text/event-stream")) {
            val d = SseDecoder()
            try {
                decodedMessages(d.push(reply.body)) + decodedMessages(d.end())
            } finally {
                d.close()
            }
        } else {
            listOf(JsonValue.parse(reply.body))
        }
        val r = replies.firstOrNull { it["id"]?.number == myId.toDouble() } ?: throw java.io.IOException("MCP: no reply to $method")
        r["error"]?.let { throw java.io.IOException("MCP: ${it["message"]?.string ?: "error"}") }
        return r["result"] ?: JsonValue.Null
    }
}
