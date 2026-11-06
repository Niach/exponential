// #VAPP-265 (Engine E5, capabilities v1): the native half of a capability on
// Android. packages/eengine/fixtures/capabilities.json is the contract: the
// Engine's bridge validates the app's arguments, maps them to the NATIVE
// arguments (bytes as base64 `data`) and stores the bytes a result carries;
// a capability here only talks to the platform and answers an MCP call result
// whose `structuredContent` matches the tool's `nativeOutputSchema`.
//
// THREADING: the Engine calls `call` on its worker thread and the call BLOCKS
// until the person is done with the picker / sheet / dialog. Never call a
// capability on the main thread: it answers `failed: main thread` there.
package at.exponential.engine.capabilities

import at.exponential.engine.ffi.Capability
import java.time.Instant
import java.time.OffsetDateTime
import java.util.Base64

/** Which thread a call is on (the main looper on Android; a fake in tests). */
fun interface MainThreadCheck {
    fun isMainThread(): Boolean
}

/** A tool answers this to end with `{content: [{type: text, text: reason}], isError: true}`. */
class ToolFailure(val reason: String) : Exception(reason) {
    companion object {
        fun notFound() = ToolFailure("notFound")
        fun unavailable() = ToolFailure("unavailable")
        fun failed(message: String) = ToolFailure("failed: $message")
    }
}

/** Thrown by an [ActivityHost] asked to block on the main thread. */
class MainThreadCallException : IllegalStateException("a capability must not run on the main thread")

/**
 * One capability under its catalog name. Subclasses answer [handle] with the
 * structuredContent; this base decodes the arguments, refuses the main
 * thread, shapes the MCP result and turns every failure into `isError`
 * (`call` never throws, so nothing crosses the FFI as an exception).
 */
abstract class NativeCapability(
    private val capabilityName: String,
    private val mainThread: MainThreadCheck,
) : Capability {
    override fun name(): String = capabilityName

    override fun toolsJson(): String = ToolCatalog.tools[capabilityName] ?: "[]"

    /** The tools this capability serves (the catalog's native tools). */
    protected abstract val toolNames: Set<String>

    /** One tool call: the structuredContent, or a [ToolFailure]. */
    protected abstract fun handle(tool: String, args: Args): Map<String, Any?>

    override fun call(tool: String, argumentsJson: String): String {
        if (tool !in toolNames) return McpResult.error("unavailable")
        if (mainThread.isMainThread()) return McpResult.error("failed: main thread")
        return try {
            val parsed = if (argumentsJson.isBlank()) emptyMap<String, Any?>() else Json.parse(argumentsJson)
            val args = Args(parsed as? Map<*, *> ?: throw ToolFailure.failed("arguments are not an object"), argumentsJson)
            McpResult.ok(handle(tool, args))
        } catch (failure: ToolFailure) {
            McpResult.error(failure.reason)
        } catch (_: MainThreadCallException) {
            McpResult.error("failed: main thread")
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
            McpResult.error("failed: interrupted")
        } catch (e: Json.ParseError) {
            McpResult.error("failed: arguments are not JSON")
        } catch (e: SecurityException) {
            McpResult.error("failed: permission denied")
        } catch (e: Exception) {
            McpResult.error("failed: ${short(e)}")
        }
    }

    private fun short(e: Exception): String = (e.message ?: e.javaClass.simpleName).lineSequence().first().take(200)
}

/** MCP call results (the shape the bridge checks). */
object McpResult {
    fun ok(structured: Map<String, Any?>): String {
        val text = Json.write(structured)
        return Json.write(linkedMapOf("content" to listOf(linkedMapOf("type" to "text", "text" to text)), "structuredContent" to RawJson(text)))
    }

    fun error(reason: String): String =
        Json.write(linkedMapOf("content" to listOf(linkedMapOf("type" to "text", "text" to reason)), "isError" to true))
}

/** The native arguments (validated by the bridge already; decoding stays lenient and typed). */
class Args(private val map: Map<*, *>, private val raw: String? = null) {
    fun has(key: String): Boolean = map[key] != null

    /** A top-level member's JSON text exactly as it arrived (null = absent or null). */
    fun rawMember(key: String): String? = raw?.let { Json.rawMembers(it)[key] }?.takeIf { it != "null" }

    fun string(key: String): String = optString(key) ?: throw ToolFailure.failed("missing $key")

    fun optString(key: String): String? = when (val v = map[key]) {
        null -> null
        is String -> v
        else -> throw ToolFailure.failed("$key is not a string")
    }

    fun int(key: String, default: Int): Int = when (val v = map[key]) {
        null -> default
        is Long -> v.coerceIn(Int.MIN_VALUE.toLong(), Int.MAX_VALUE.toLong()).toInt()
        is Double -> if (v == Math.floor(v)) v.toInt() else throw ToolFailure.failed("$key is not an integer")
        else -> throw ToolFailure.failed("$key is not an integer")
    }

    fun bool(key: String, default: Boolean): Boolean = when (val v = map[key]) {
        null -> default
        is Boolean -> v
        else -> throw ToolFailure.failed("$key is not a boolean")
    }

    fun strings(key: String): List<String> = when (val v = map[key]) {
        null -> emptyList()
        is List<*> -> v.map { it as? String ?: throw ToolFailure.failed("$key holds a non-string") }
        else -> throw ToolFailure.failed("$key is not an array")
    }

    fun obj(key: String): Args? = when (val v = map[key]) {
        null -> null
        is Map<*, *> -> Args(v)
        else -> throw ToolFailure.failed("$key is not an object")
    }

    /** `{name, mimeType, data}` (bytes as standard base64). */
    fun file(key: String): NativeFile? = obj(key)?.let { NativeFile(it.string("name"), it.string("mimeType"), Bytes.decode(it.string("data"))) }

    fun asFile(): NativeFile = NativeFile(string("name"), string("mimeType"), Bytes.decode(string("data")))
}

/** A file the bridge hands in: its name, MIME type and bytes. */
class NativeFile(val name: String, val mimeType: String, val bytes: ByteArray)

object Bytes {
    /** Standard, padded base64 (what the bridge's `format: base64` takes). */
    fun encode(bytes: ByteArray): String = Base64.getEncoder().encodeToString(bytes)

    fun decode(data: String): ByteArray = try {
        Base64.getDecoder().decode(data)
    } catch (_: IllegalArgumentException) {
        throw ToolFailure.failed("data is not base64")
    }
}

object Text {
    /** At most [max] code points (the schemas' maxLength counts code points). */
    fun clip(s: String, max: Int): String {
        if (s.codePointCount(0, s.length) <= max) return s
        return s.substring(0, s.offsetByCodePoints(0, max))
    }

    /** A non-empty name of at most 255 code points. */
    fun name(raw: String?, fallback: String): String = clip(raw?.trim()?.takeIf { it.isNotEmpty() } ?: fallback, 255)
}

object Times {
    /** RFC 3339 in UTC (`2026-11-01T10:00:00Z`), null for an unknown time. */
    fun rfc3339(epochMillis: Long?): String? = epochMillis?.takeIf { it > 0 }?.let { Instant.ofEpochMilli(it).toString() }

    /** An RFC 3339 time with a zone as epoch millis (`at`). */
    fun parse(text: String): Long = try {
        OffsetDateTime.parse(text.uppercase()).toInstant().toEpochMilli()
    } catch (_: Exception) {
        throw ToolFailure.failed("at is not an RFC 3339 time")
    }
}

/** capabilities.json `v1.mimeByExtension` (+ a few more), for names without a known type. */
object Mime {
    val byExtension: Map<String, String> = mapOf(
        "csv" to "text/csv", "gif" to "image/gif", "heic" to "image/heic", "heif" to "image/heif",
        "html" to "text/html", "jpeg" to "image/jpeg", "jpg" to "image/jpeg", "json" to "application/json",
        "md" to "text/markdown", "pdf" to "application/pdf", "png" to "image/png", "svg" to "image/svg+xml",
        "txt" to "text/plain", "webp" to "image/webp", "zip" to "application/zip",
        "avif" to "image/avif", "dng" to "image/x-adobe-dng", "bmp" to "image/bmp",
    )

    fun extension(name: String): String? = name.substringAfterLast('.', "").lowercase().takeIf { it.isNotEmpty() && name.contains('.') }

    fun forName(name: String, lookup: (String) -> String? = { null }): String? =
        extension(name)?.let { ext -> byExtension[ext] ?: lookup(ext) }

    /** A platform-reported type if it is a real one, else the name's, else [fallback]. */
    fun pick(reported: String?, name: String, fallback: String, lookup: (String) -> String? = { null }): String {
        val given = reported?.trim()?.lowercase()?.takeIf { it.contains('/') && it.length <= 100 && it != "application/octet-stream" }
        return given ?: forName(name, lookup) ?: reported?.takeIf { it.contains('/') && it.length <= 100 } ?: fallback
    }

    // files.open's `accept` (MIME types like image/png or image/<wildcard>, or
    // extensions like `.csv`) as the MIME types ACTION_OPEN_DOCUMENT filters
    // by: empty = every type; an extension without a known type widens the
    // filter to every type (the document picker filters by MIME type only).
    fun acceptToMimeTypes(accept: List<String>, lookup: (String) -> String? = { null }): List<String> {
        if (accept.isEmpty()) return listOf("*/*")
        val out = LinkedHashSet<String>()
        for (raw in accept) {
            val a = raw.trim().lowercase()
            val mime = when {
                a.startsWith(".") -> byExtension[a.drop(1)] ?: lookup(a.drop(1))
                a.contains('/') -> a
                else -> byExtension[a] ?: lookup(a)
            } ?: return listOf("*/*")
            out.add(mime)
        }
        return if ("*/*" in out) listOf("*/*") else out.toList()
    }
}
