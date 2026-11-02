package at.exponential.ui.host

import at.exponential.ui.json.JsonValue
import java.util.TimeZone

/**
 * VAPP-191 (eApps Q2): the expression language (a CEL subset with maths,
 * date/time and duration functions; `catalog/expressions.json`) through the
 * core's one evaluator and type checker over the FFI. The TS reference, the
 * Rust core and Swift's `Expressions` answer the same
 * (`fixtures/expressions.json`).
 */
object Expressions {
    /** An evaluation: the value, or why it has none (`column N: …`). */
    sealed class Outcome {
        data class Value(val value: JsonValue) : Outcome()
        data class Error(val message: String) : Outcome()
    }

    /** A type check: the expression's static type and every problem. */
    data class Check(val type: String, val issues: List<String>)

    /**
     * [source] over [vars] (a JSON object; a missing name reads null).
     * `today()`/`now()` read [nowMs] (default: the wall clock) in the team
     * zone [timeZone] (default UTC).
     */
    fun eval(source: String, vars: JsonValue = JsonValue.Obj(emptyMap()), nowMs: Long? = null, timeZone: TimeZone? = null): Outcome =
        eval(source, vars, nowMs?.toDouble(), timeZone?.getOffset(nowMs ?: System.currentTimeMillis())?.div(60_000))

    /** The same with the clock as epoch ms and the zone as a fixed UTC offset. */
    fun eval(source: String, vars: JsonValue, nowMs: Double?, offsetMinutes: Int?): Outcome {
        val out = runCatching { at.exponential.ui.ffi.evalExpressionJson(source, vars.json, nowMs, offsetMinutes) }.getOrNull()
            ?: return Outcome.Error("vars are a JSON object")
        val parsed = JsonValue.parse(out)
        parsed["error"]?.string?.let { return Outcome.Error(it) }
        return Outcome.Value(parsed["value"] ?: JsonValue.Null)
    }

    /** Check [source] against declared [types] (name → `number`, `list<entry>`, a kind's name…) and the package's record [kinds]. */
    fun check(source: String, types: Map<String, String> = emptyMap(), kinds: JsonValue = JsonValue.Obj(emptyMap())): Check {
        val typesJson = JsonValue.Obj(types.mapValues { JsonValue.Str(it.value) }).json
        val out = runCatching { at.exponential.ui.ffi.checkExpressionJson(source, typesJson, kinds.json) }.getOrNull()
            ?: return Check("dyn", listOf("kinds are a JSON object"))
        val parsed = JsonValue.parse(out)
        return Check(parsed["type"]?.string ?: "dyn", parsed["issues"]?.array.orEmpty().mapNotNull { it.string })
    }

    /** One function in its A2UI call form (`mul {a, b}`, `expr {source, vars}`…); null when the call yields nothing. */
    fun call(name: String, args: JsonValue, nowMs: Double? = null, offsetMinutes: Int? = null): JsonValue? =
        runCatching { at.exponential.ui.ffi.callExpressionJson(name, args.json, nowMs, offsetMinutes) }.getOrNull()?.let { JsonValue.parse(it) }
}
