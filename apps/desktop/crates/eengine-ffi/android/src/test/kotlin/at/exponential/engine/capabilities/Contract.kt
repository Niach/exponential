// #VAPP-265: the contract the tests check against: capabilities.json (read from
// the repo, `eengine.fixtures` set by build.gradle.kts) and a minimal JSON
// Schema checker for the keywords `nativeOutputSchema` uses.
package at.exponential.engine.capabilities

import java.io.File
import java.time.OffsetDateTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail

object Contract {
    val fixture: Map<*, *> by lazy {
        val dir = System.getProperty("eengine.fixtures") ?: "../../../../../packages/eengine/fixtures"
        Json.parse(File(dir, "capabilities.json").readText()) as Map<*, *>
    }

    val tools: List<Map<*, *>> get() = (fixture["tools"] as List<*>).map { it as Map<*, *> }

    fun tool(name: String): Map<*, *> = tools.first { it["name"] == name }

    fun nativeOutputSchema(name: String): Map<*, *> = tool(name)["nativeOutputSchema"] as Map<*, *>

    val bridgeCases: List<Map<*, *>> get() = (fixture["bridgeCases"] as List<*>).map { it as Map<*, *> }

    /** The violations of [value] against [schema] (empty = valid). */
    fun violations(schema: Map<*, *>, value: Any?, path: String = ""): List<String> {
        val out = ArrayList<String>()
        val at = path.ifEmpty { "/" }
        val types = when (val t = schema["type"]) { null -> null; is String -> listOf(t); else -> (t as List<*>).map { it as String } }
        if (types != null && types.none { typeMatches(it, value) }) return listOf("$at type")
        when (value) {
            is Map<*, *> -> {
                (schema["required"] as List<*>?)?.forEach { if (it !in value.keys) out.add("$path/$it required") }
                val props = schema["properties"] as Map<*, *>?
                if (schema["additionalProperties"] == false) value.keys.filter { props == null || it !in props.keys }.forEach { out.add("$path/$it additionalProperties") }
                props?.forEach { (k, s) -> if (k in value.keys) out += violations(s as Map<*, *>, value[k], "$path/$k") }
            }
            is List<*> -> {
                (schema["minItems"] as Long?)?.let { if (value.size < it) out.add("$at minItems") }
                (schema["maxItems"] as Long?)?.let { if (value.size > it) out.add("$at maxItems") }
                (schema["items"] as Map<*, *>?)?.let { s -> value.forEachIndexed { i, x -> out += violations(s, x, "$path/$i") } }
            }
            is String -> {
                val n = value.codePointCount(0, value.length)
                (schema["minLength"] as Long?)?.let { if (n < it) out.add("$at minLength") }
                (schema["maxLength"] as Long?)?.let { if (n > it) out.add("$at maxLength") }
                when (schema["format"]) {
                    "date-time" -> if (!isDateTime(value)) out.add("$at format")
                    "base64" -> if (!Regex("^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$").matches(value)) out.add("$at format")
                }
            }
        }
        return out
    }

    private fun typeMatches(type: String, v: Any?) = when (type) {
        "object" -> v is Map<*, *>
        "array" -> v is List<*>
        "string" -> v is String
        "boolean" -> v is Boolean
        "null" -> v == null
        "integer" -> v is Long || (v is Double && v == Math.floor(v))
        "number" -> v is Number
        else -> false
    }

    private fun isDateTime(s: String): Boolean =
        Regex("^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}(\\.\\d+)?(Z|[+-]\\d{2}:\\d{2})$").matches(s) && runCatching { OffsetDateTime.parse(s) }.isSuccess
}

/** A parsed MCP call result. */
class Result(text: String) {
    val json = Json.parse(text) as Map<*, *>
    val isError get() = json["isError"] == true
    val structured get() = json["structuredContent"] as Map<*, *>
    val text get() = ((json["content"] as List<*>)[0] as Map<*, *>)["text"] as String

    /** Valid for [tool] (`photos.pick`): content = the structuredContent's JSON, the schema holds. */
    fun assertValid(tool: String): Result {
        assertTrue("$tool answered an error: ${json}", !isError)
        assertEquals(setOf("content", "structuredContent"), json.keys)
        assertEquals(structured, Json.parse(text))
        val problems = Contract.violations(Contract.nativeOutputSchema(tool), structured)
        if (problems.isNotEmpty()) fail("$tool: $problems in $json")
        return this
    }

    fun assertError(reason: String): Result {
        assertEquals(setOf("content", "isError"), json.keys)
        assertTrue(isError)
        assertEquals(reason, text)
        return this
    }
}

val offMain = MainThreadCheck { false }
val onMain = MainThreadCheck { true }
