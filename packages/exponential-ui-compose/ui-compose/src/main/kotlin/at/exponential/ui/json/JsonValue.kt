package at.exponential.ui.json

import kotlin.math.abs
import kotlin.math.floor

/**
 * A JSON value (props, payloads, data) the painter reads and the host
 * receives. Parsed from the facade's JSON strings once per structure.
 * Booleans are [Bool], never [Num].
 */
sealed class JsonValue {
    /** `null`. */
    data object Null : JsonValue()

    /** `true` / `false`. */
    data class Bool(val v: Boolean) : JsonValue()

    /** Any number (a Double, like JavaScript). */
    data class Num(val v: Double) : JsonValue()

    /** A string. */
    data class Str(val v: String) : JsonValue()

    /** An array. */
    data class Arr(val v: List<JsonValue>) : JsonValue()

    /** An object. */
    data class Obj(val v: Map<String, JsonValue>) : JsonValue()

    /** Serialized: compact, object keys SORTED, integers without `.0`. */
    val json: String get() = StringBuilder().also { write(this, it, sorted = true) }.toString()

    /** Serialized with object keys in their PARSED order (where order is
     *  meaningful, e.g. a theme whose issues are reported in key order). */
    val orderedJson: String get() = StringBuilder().also { write(this, it, sorted = false) }.toString()

    override fun toString(): String = json

    /** The member `key` of an object (null otherwise). */
    operator fun get(key: String): JsonValue? = (this as? Obj)?.v?.get(key)

    /** The string, when this is one. */
    val string: String? get() = (this as? Str)?.v

    /** A scalar as display text (`3` → "3"; the React `String(v)`). */
    val displayText: String
        get() = when (this) {
            Null -> ""
            is Str -> v
            is Bool -> if (v) "true" else "false"
            is Num -> formatNumber(v)
            is Arr, is Obj -> json
        }

    /** A number (numeric strings and booleans coerce). */
    val number: Double?
        get() = when (this) {
            is Num -> v
            is Str -> v.trim().toDoubleOrNull()
            is Bool -> if (v) 1.0 else 0.0
            else -> null
        }

    /** A boolean (`"true"` / `"false"` strings coerce). */
    val bool: Boolean?
        get() = when (this) {
            is Bool -> v
            is Str -> if (v == "true") true else if (v == "false") false else null
            else -> null
        }

    /** The elements, when this is an array. */
    val array: List<JsonValue>? get() = (this as? Arr)?.v

    /** The members, when this is an object. */
    val obj: Map<String, JsonValue>? get() = (this as? Obj)?.v

    /** Is this `null`? */
    val isNull: Boolean get() = this is Null

    companion object {
        /** Parse a JSON document ([Null] on failure). */
        fun parse(text: String): JsonValue = try {
            val p = Parser(text)
            val v = p.value()
            p.ws()
            if (p.i != text.length) Null else v
        } catch (_: RuntimeException) {
            Null
        }

        /** Wrap a Kotlin value (Map/List/String/Number/Boolean/null/JsonValue). */
        fun of(any: Any?): JsonValue = when (any) {
            null -> Null
            is JsonValue -> any
            is Boolean -> Bool(any)
            is Number -> Num(any.toDouble())
            is String -> Str(any)
            is Map<*, *> -> Obj(any.entries.associate { (k, v) -> k.toString() to of(v) })
            is List<*> -> Arr(any.map { of(it) })
            else -> Str(any.toString())
        }

        internal fun formatNumber(n: Double): String =
            if (n.isNaN() || n.isInfinite()) "null"
            else if (n == floor(n) && abs(n) < 1e15) n.toLong().toString()
            else n.toString()

        private fun write(v: JsonValue, sb: StringBuilder, sorted: Boolean) {
            when (v) {
                Null -> sb.append("null")
                is Bool -> sb.append(if (v.v) "true" else "false")
                is Num -> sb.append(formatNumber(v.v))
                is Str -> writeString(v.v, sb)
                is Arr -> {
                    sb.append('[')
                    v.v.forEachIndexed { i, x -> if (i > 0) sb.append(','); write(x, sb, sorted) }
                    sb.append(']')
                }
                is Obj -> {
                    sb.append('{')
                    (if (sorted) v.v.keys.sorted() else v.v.keys).forEachIndexed { i, k ->
                        if (i > 0) sb.append(',')
                        writeString(k, sb)
                        sb.append(':')
                        write(v.v.getValue(k), sb, sorted)
                    }
                    sb.append('}')
                }
            }
        }

        private fun writeString(s: String, sb: StringBuilder) {
            sb.append('"')
            for (c in s) {
                when (c) {
                    '"' -> sb.append("\\\"")
                    '\\' -> sb.append("\\\\")
                    '\n' -> sb.append("\\n")
                    '\r' -> sb.append("\\r")
                    '\t' -> sb.append("\\t")
                    '\b' -> sb.append("\\b")
                    '\u000c' -> sb.append("\\f")
                    else -> if (c < ' ') sb.append("\\u").append(String.format("%04x", c.code)) else sb.append(c)
                }
            }
            sb.append('"')
        }
    }

    private class Parser(val s: String) {
        var i = 0

        fun ws() {
            while (i < s.length && (s[i] == ' ' || s[i] == '\n' || s[i] == '\r' || s[i] == '\t')) i++
        }

        fun fail(): Nothing = throw IllegalArgumentException("bad JSON at $i")

        fun literal(word: String, v: JsonValue): JsonValue {
            if (!s.startsWith(word, i)) fail()
            i += word.length
            return v
        }

        fun value(): JsonValue {
            ws()
            if (i >= s.length) fail()
            return when (s[i]) {
                '{' -> {
                    i++
                    val m = LinkedHashMap<String, JsonValue>()
                    ws()
                    if (i < s.length && s[i] == '}') { i++; return Obj(m) }
                    while (true) {
                        ws()
                        if (i >= s.length || s[i] != '"') fail()
                        val k = str()
                        ws()
                        if (i >= s.length || s[i] != ':') fail()
                        i++
                        m[k] = value()
                        ws()
                        if (i >= s.length) fail()
                        if (s[i] == ',') { i++; continue }
                        if (s[i] == '}') { i++; return Obj(m) }
                        fail()
                    }
                    @Suppress("UNREACHABLE_CODE")
                    Null
                }
                '[' -> {
                    i++
                    val l = ArrayList<JsonValue>()
                    ws()
                    if (i < s.length && s[i] == ']') { i++; return Arr(l) }
                    while (true) {
                        l.add(value())
                        ws()
                        if (i >= s.length) fail()
                        if (s[i] == ',') { i++; continue }
                        if (s[i] == ']') { i++; return Arr(l) }
                        fail()
                    }
                    @Suppress("UNREACHABLE_CODE")
                    Null
                }
                '"' -> Str(str())
                't' -> literal("true", Bool(true))
                'f' -> literal("false", Bool(false))
                'n' -> literal("null", Null)
                else -> {
                    val start = i
                    while (i < s.length && (s[i].isDigit() || s[i] in "+-.eE")) i++
                    if (start == i) fail()
                    Num(s.substring(start, i).toDoubleOrNull() ?: fail())
                }
            }
        }

        fun str(): String {
            i++
            val sb = StringBuilder()
            while (true) {
                if (i >= s.length) fail()
                val c = s[i]
                if (c == '"') break
                if (c == '\\') {
                    i++
                    if (i >= s.length) fail()
                    when (s[i]) {
                        'n' -> sb.append('\n')
                        't' -> sb.append('\t')
                        'r' -> sb.append('\r')
                        'b' -> sb.append('\b')
                        'f' -> sb.append('\u000c')
                        'u' -> {
                            if (i + 5 > s.length) fail()
                            sb.append(s.substring(i + 1, i + 5).toInt(16).toChar())
                            i += 4
                        }
                        else -> sb.append(s[i])
                    }
                    i++
                } else {
                    sb.append(c)
                    i++
                }
            }
            i++
            return sb.toString()
        }
    }
}

/** A node's props with typed readers. */
typealias Props = Map<String, JsonValue>

/** The string at `key` ("" otherwise). */
fun Props.str(key: String): String = this[key]?.string ?: ""

/** The scalar at `key` as display text ("" otherwise). */
fun Props.text(key: String): String = this[key]?.displayText ?: ""

/** The number at `key`. */
fun Props.num(key: String): Double? = this[key]?.number

/** The boolean at `key` (false otherwise). */
fun Props.flag(key: String): Boolean = this[key]?.bool ?: false

/** The array at `key` (empty otherwise). */
fun Props.list(key: String): List<JsonValue> = this[key]?.array ?: emptyList()

/** A CSS-ish length prop in px (number / `"Npx"`); percentages return null. */
fun Props.px(key: String): Float? = when (val v = this[key]) {
    is JsonValue.Num -> v.v.toFloat()
    is JsonValue.Str -> {
        val t = v.v.trim()
        if (t.endsWith("%")) null else (if (t.endsWith("px")) t.dropLast(2) else t).trim().toDoubleOrNull()?.toFloat()
    }
    else -> null
}

/** A `"N%"` prop as a fraction. */
fun Props.percent(key: String): Float? {
    val s = (this[key] as? JsonValue.Str)?.v ?: return null
    if (!s.endsWith("%")) return null
    return s.dropLast(1).toDoubleOrNull()?.let { (it / 100).toFloat() }
}

/** The props serialized (sorted keys). */
val Props.json: String get() = JsonValue.Obj(this).json
