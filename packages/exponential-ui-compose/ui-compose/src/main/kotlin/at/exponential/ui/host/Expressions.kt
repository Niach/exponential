package at.exponential.ui.host

import at.exponential.ui.ffi.HostZone
import at.exponential.ui.json.JsonValue
import java.util.TimeZone

/**
 * VAPP-191 (eApps Q2): expressions, the core's one evaluator
 * (`@exponential_app/eui` `src/cel.ts`, eui-core `cel`,
 * `fixtures/expressions.json`). The string form is a CEL subset (literals,
 * field paths, `+ - * / %`, comparisons, `&& || !`, `?:`, calls, the macros
 * `map/filter/all/exists` one level) over one number type; dates run in the
 * team zone. A surface evaluates the same functions at bind time
 * (`{"call": "expr", …}`); this is the direct entry for a host.
 */
object Expression {
    /** The value ([evaluate]) or the type ([check], spelled as JSON), or the error. */
    sealed interface Outcome {
        data class Value(val value: JsonValue) : Outcome

        data class Error(val message: String) : Outcome
    }

    /** Evaluates [expr] over [vars] at [nowMs] (epoch ms; null = the wall clock) in [zone] (the team zone; null = UTC). */
    fun evaluate(expr: String, vars: Map<String, JsonValue> = emptyMap(), nowMs: Double? = null, zone: HostZone? = null, maxSteps: UInt? = null): Outcome {
        val out = runCatching { at.exponential.ui.ffi.evaluateExpressionJson(expr, JsonValue.Obj(vars).json, nowMs, zone, maxSteps) }.getOrNull()
            ?: return Outcome.Error("vars is an object")
        return outcome(JsonValue.parse(out), "value")
    }

    /** [evaluate] in a `java.util.TimeZone`. */
    fun evaluate(expr: String, vars: Map<String, JsonValue>, nowMs: Double?, timeZone: TimeZone): Outcome = evaluate(expr, vars, nowMs, TimeZoneOffset(timeZone))

    /** Type-checks [expr] against [types] (name → type spelled as JSON: `"number"`, `["date"]`, a row object). */
    fun check(expr: String, types: Map<String, JsonValue>): Outcome {
        val out = runCatching { at.exponential.ui.ffi.checkExpressionJson(expr, JsonValue.Obj(types).json) }.getOrNull()
            ?: return Outcome.Error("types is an object")
        return outcome(JsonValue.parse(out), "type")
    }

    private fun outcome(v: JsonValue, key: String): Outcome = v["error"]?.string?.let { Outcome.Error(it) } ?: Outcome.Value(v[key] ?: JsonValue.Null)
}

/** A `java.util.TimeZone` as the core's [HostZone] (its offset per instant). */
class TimeZoneOffset(private val timeZone: TimeZone) : HostZone {
    override fun offsetMinutes(epochMs: Double): Int = timeZone.getOffset(epochMs.toLong()) / 60_000
}
