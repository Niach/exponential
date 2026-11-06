// #VAPP-265: a tiny JSON reader/writer for the capabilities, so their pure
// parts need neither org.json (an Android stub on the plain JVM) nor a library.
// Values: Map<String, Any?> (insertion order), List<Any?>, String, Long (an
// integral literal), Double, Boolean, null.
package at.exponential.engine.capabilities

internal object Json {
    fun parse(text: String): Any? = Parser(text).run { val v = value(); ws(); if (i != s.length) fail("trailing text"); v }

    /** A top-level object's members as their RAW text (verbatim, for the notification tap). */
    fun rawMembers(text: String): Map<String, String> {
        val p = Parser(text)
        val out = LinkedHashMap<String, String>()
        p.ws()
        if (p.peek() != '{') return out
        p.i++
        p.ws()
        if (p.peek() == '}') return out
        while (true) {
            p.ws()
            val key = p.str()
            p.ws(); p.expect(':'); p.ws()
            val start = p.i
            p.value()
            out[key] = text.substring(start, p.i)
            p.ws()
            if (p.peek() == ',') { p.i++; continue }
            p.expect('}')
            return out
        }
    }

    fun write(v: Any?): String = StringBuilder().also { emit(v, it) }.toString()

    fun quote(s: String): String = StringBuilder().also { emitString(s, it) }.toString()

    private fun emit(v: Any?, sb: StringBuilder) {
        when (v) {
            null -> sb.append("null")
            is RawJson -> sb.append(v.text)
            is String -> emitString(v, sb)
            is Boolean -> sb.append(v)
            is Double -> if (v == Math.floor(v) && !v.isInfinite() && Math.abs(v) < 1e15) sb.append(v.toLong()) else sb.append(v)
            is Float -> emit(v.toDouble(), sb)
            is Number -> sb.append(v.toLong())
            is Map<*, *> -> {
                sb.append('{')
                var first = true
                for ((k, x) in v) {
                    if (!first) sb.append(',')
                    first = false
                    emitString(k as String, sb)
                    sb.append(':')
                    emit(x, sb)
                }
                sb.append('}')
            }
            is List<*> -> {
                sb.append('[')
                v.forEachIndexed { n, x -> if (n > 0) sb.append(','); emit(x, sb) }
                sb.append(']')
            }
            else -> emitString(v.toString(), sb)
        }
    }

    private fun emitString(v: String, sb: StringBuilder) {
        sb.append('"')
        for (c in v) when (c) {
            '"' -> sb.append("\\\"")
            '\\' -> sb.append("\\\\")
            '\n' -> sb.append("\\n")
            '\r' -> sb.append("\\r")
            '\t' -> sb.append("\\t")
            '\b' -> sb.append("\\b")
            '\u000c' -> sb.append("\\f")
            else -> if (c < ' ' || c == ' ' || c == ' ') sb.append(String.format("\\u%04x", c.code)) else sb.append(c)
        }
        sb.append('"')
    }

    class ParseError(message: String) : IllegalArgumentException(message)

    private class Parser(val s: String) {
        var i = 0
        fun fail(what: String): Nothing = throw ParseError("$what at $i")
        fun peek(): Char = if (i < s.length) s[i] else fail("unexpected end")
        fun expect(c: Char) { if (peek() != c) fail("expected '$c'"); i++ }
        fun ws() { while (i < s.length && (s[i] == ' ' || s[i] == '\n' || s[i] == '\r' || s[i] == '\t')) i++ }
        fun word(w: String, v: Any?): Any? { if (!s.startsWith(w, i)) fail("bad literal"); i += w.length; return v }

        fun value(): Any? {
            ws()
            return when (peek()) {
                '{' -> obj()
                '[' -> arr()
                '"' -> str()
                't' -> word("true", true)
                'f' -> word("false", false)
                'n' -> word("null", null)
                else -> number()
            }
        }

        fun obj(): Map<String, Any?> {
            expect('{')
            val m = LinkedHashMap<String, Any?>()
            ws()
            if (peek() == '}') { i++; return m }
            while (true) {
                ws()
                val k = str()
                ws(); expect(':')
                m[k] = value()
                ws()
                if (peek() == ',') { i++; continue }
                expect('}')
                return m
            }
        }

        fun arr(): List<Any?> {
            expect('[')
            val l = ArrayList<Any?>()
            ws()
            if (peek() == ']') { i++; return l }
            while (true) {
                l.add(value())
                ws()
                if (peek() == ',') { i++; continue }
                expect(']')
                return l
            }
        }

        fun number(): Any {
            val start = i
            if (peek() == '-') i++
            while (i < s.length && s[i].isDigit()) i++
            var integral = true
            if (i < s.length && s[i] == '.') { integral = false; i++; while (i < s.length && s[i].isDigit()) i++ }
            if (i < s.length && (s[i] == 'e' || s[i] == 'E')) {
                integral = false
                i++
                if (i < s.length && (s[i] == '+' || s[i] == '-')) i++
                while (i < s.length && s[i].isDigit()) i++
            }
            val text = s.substring(start, i)
            if (text.isEmpty() || text == "-") fail("bad value")
            if (integral) text.toLongOrNull()?.let { return it }
            return text.toDoubleOrNull() ?: fail("bad number")
        }

        fun str(): String {
            expect('"')
            val sb = StringBuilder()
            while (true) {
                val c = peek()
                i++
                when (c) {
                    '"' -> return sb.toString()
                    '\\' -> {
                        val e = peek()
                        i++
                        when (e) {
                            'n' -> sb.append('\n'); 't' -> sb.append('\t'); 'r' -> sb.append('\r')
                            'b' -> sb.append('\b'); 'f' -> sb.append('\u000c'); '/' -> sb.append('/')
                            '\\' -> sb.append('\\'); '"' -> sb.append('"')
                            'u' -> {
                                if (i + 4 > s.length) fail("bad escape")
                                sb.append(s.substring(i, i + 4).toIntOrNull(16)?.toChar() ?: fail("bad escape"))
                                i += 4
                            }
                            else -> fail("bad escape")
                        }
                    }
                    else -> sb.append(c)
                }
            }
        }
    }
}

/** JSON text written as is (a stored tap goes back verbatim). */
internal class RawJson(val text: String)
