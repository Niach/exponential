package at.exponential.ui

import at.exponential.ui.ffi.HostZone
import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.ffi.validatePackageJson
import at.exponential.ui.host.Expression
import at.exponential.ui.host.TimeZoneOffset
import at.exponential.ui.json.JsonValue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant
import java.util.TimeZone

/**
 * VAPP-191: `fixtures/expressions.json` (`eval`, `check`, `packages`) through
 * [Expression] (the core's one evaluator over the FFI). The TS reference, the
 * Rust core, the FFI facade and Swift's `ExpressionsTests` replay the same
 * cases; the team zone is a [HostZone] built from the fixture's transitions
 * (checked against java.util's IANA zone).
 */
class ExpressionsTest {
    private class FixtureZone(z: JsonValue?) : HostZone {
        private val base = (z?.get("offset")?.number ?: 0.0).toInt()
        private val steps = z?.get("transitions")?.array.orEmpty().map { t -> instant(t.array!![0].string!!) to t.array!![1].number!!.toInt() }

        override fun offsetMinutes(epochMs: Double): Int = steps.lastOrNull { epochMs >= it.first }?.second ?: base
    }

    private fun same(got: JsonValue, want: JsonValue, name: String) =
        assertTrue("$name: differs at ${jsonDiff(got.json, want.json)}: got ${got.json.take(300)}", jsonEqual(got.json, want.json))

    private fun asJson(o: Expression.Outcome, key: String): JsonValue = when (o) {
        is Expression.Outcome.Value -> JsonValue.Obj(mapOf(key to o.value))
        is Expression.Outcome.Error -> JsonValue.Obj(mapOf("error" to JsonValue.Str(o.message)))
    }

    private fun want(c: JsonValue, key: String): JsonValue =
        c["error"]?.let { JsonValue.Obj(mapOf("error" to it)) } ?: JsonValue.Obj(mapOf(key to (c[key] ?: JsonValue.Null)))

    private fun fixture(): JsonValue {
        Fixtures.require()
        return Fixtures.json("expressions.json")
    }

    /** VAPP-206: `fixtures/host-router.json` `hostParams`. */
    @Test
    fun hostParams() {
        Fixtures.require()
        for (c in Fixtures.json("host-router.json")["hostParams"]?.array.orEmpty()) {
            val got = at.exponential.ui.host.HostParams.values(instant(c["now"]!!.string!!), FixtureZone(c))
            same(got, c["expected"]!!, "hostParams: ${c["name"]?.string}")
        }
    }

    @Test
    fun eval() {
        val f = fixture()
        val vars = f["vars"]?.obj.orEmpty()
        val cases = f["eval"]?.array.orEmpty()
        assertTrue(cases.size > 150)
        for (c in cases) {
            val now = instant(c["now"]?.string ?: f["env"]!!["now"]!!.string!!)
            val zone = FixtureZone(f["zones"]?.get(c["zone"]?.string ?: f["env"]!!["zone"]!!.string!!))
            val got = Expression.evaluate(c["expr"]!!.string!!, vars, now, zone, c["maxSteps"]?.number?.toUInt())
            same(asJson(got, "value"), want(c, "value"), "eval: ${c["name"]?.string}")
        }
    }

    @Test
    fun check() {
        val f = fixture()
        val types = f["checkTypes"]?.obj.orEmpty()
        for (c in f["check"]?.array.orEmpty()) same(asJson(Expression.check(c["expr"]!!.string!!, types), "type"), want(c, "type"), "check: ${c["name"]?.string}")
    }

    @Test
    fun packages() {
        val f = fixture()
        for (c in f["packages"]?.array.orEmpty()) same(JsonValue.parse(validatePackageJson(c["package"]!!.json, null)), c["issues"]!!, "package: ${c["name"]?.string}")
    }

    @Test
    fun theFixtureZoneIsJavas() {
        val vienna = fixture()["zones"]?.get("vienna")
        val ours = FixtureZone(vienna)
        val theirs = TimeZoneOffset(TimeZone.getTimeZone(vienna!!["name"]!!.string!!))
        for (t in vienna["transitions"]?.array.orEmpty()) {
            val ms = instant(t.array!![0].string!!)
            for (at in listOf(ms - 1, ms, ms + 1)) assertEquals(theirs.offsetMinutes(at), ours.offsetMinutes(at))
        }
    }

    private companion object {
        fun instant(s: String): Double = Instant.parse(s).toEpochMilli().toDouble()
    }
}
