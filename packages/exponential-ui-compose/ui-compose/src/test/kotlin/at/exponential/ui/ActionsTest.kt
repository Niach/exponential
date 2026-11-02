package at.exponential.ui

import at.exponential.ui.ffi.HostZone
import at.exponential.ui.ffi.checkExpressionJson
import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.ffi.validatePackageJson
import at.exponential.ui.host.Actions
import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.FunctionCallInfo
import at.exponential.ui.host.FunctionOutcome
import at.exponential.ui.host.FunctionPolicy
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostPolicy
import at.exponential.ui.json.JsonValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

/**
 * VAPP-194: `fixtures/actions.json` (`effects`, `check`, `validate`, `run`,
 * `batch`) through [Actions] (the core's one action runner over the FFI),
 * plus the host's all-or-nothing commit of `actions.<name>`. The TS
 * reference, the Rust core, the FFI facade and Swift's `ActionsTests`
 * replay the same cases.
 */
class ActionsTest {
    private class FixedZone(private val offset: Int) : HostZone {
        override fun offsetMinutes(epochMs: Double): Int = offset
    }

    private fun same(got: JsonValue, want: JsonValue, name: String) =
        assertTrue("$name: differs at ${jsonDiff(got.json, want.json)}: got ${got.json.take(300)}", jsonEqual(got.json, want.json))

    private fun want(c: JsonValue, key: String): JsonValue =
        c["error"]?.let { JsonValue.Obj(mapOf("error" to it)) } ?: JsonValue.Obj(mapOf(key to (c[key] ?: JsonValue.Null)))

    private fun fixture(): JsonValue {
        Fixtures.require()
        return Fixtures.json("actions.json")
    }

    @Test
    fun effects() {
        val cases = fixture()["effects"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        for (c in cases) assertEquals("effect: ${c["name"]?.string}", c["effect"]?.string, Actions.effect(c["name"]!!.string!!).wire)
    }

    @Test
    fun check() {
        for (c in fixture()["check"]?.array.orEmpty()) {
            same(JsonValue.parse(checkExpressionJson(c["expr"]!!.string!!, "{}")), want(c, "type"), "check: ${c["name"]?.string}")
        }
    }

    @Test
    fun validate() {
        val f = fixture()
        val base = f["package"]!!
        for (c in f["validate"]?.array.orEmpty()) {
            val pkg = validateCasePackage(base, c["with"])
            same(JsonValue.parse(validatePackageJson(pkg.json, null)), c["issues"] ?: JsonValue.Arr(emptyList()), "validate: ${c["name"]?.string}")
        }
    }

    @Test
    fun run() {
        val f = fixture()
        val base = f["package"]!!
        val now = instant(f["env"]!!["now"]!!.string!!)
        val zone = FixedZone(f["env"]!!["offset"]!!.number!!.toInt())
        val cases = f["run"]?.array.orEmpty()
        assertTrue(cases.size > 10)
        for (c in cases) {
            val got = Actions.run(base, c["action"]!!.string!!, c["params"], c["data"], c["me"]?.string, now, zone, c["maxSteps"]?.number?.toUInt())
            val json = when (got) {
                is Actions.Outcome.Planned -> JsonValue.Obj(mapOf("plan" to got.plan.json))
                is Actions.Outcome.Error -> JsonValue.Obj(mapOf("error" to JsonValue.Str(got.message)))
            }
            same(json, want(c, "plan"), "run: ${c["name"]?.string}")
        }
    }

    @Test
    fun batch() {
        val b = fixture()["batch"]!!
        for (c in b["cases"]?.array.orEmpty()) {
            val got = Actions.applyBatch(
                b["rows"]?.array.orEmpty(),
                c["writes"]?.array.orEmpty(),
                b["records"]!!,
                b["me"]?.string,
                b["now"]!!.string!!,
                b["newIds"]?.array.orEmpty().mapNotNull { it.string },
            )
            val json = when (got) {
                is Actions.BatchOutcome.Applied -> JsonValue.Obj(mapOf("rows" to JsonValue.Arr(got.rows), "ids" to JsonValue.Arr(got.ids.map { JsonValue.Str(it) })))
                is Actions.BatchOutcome.Error -> JsonValue.Obj(mapOf("error" to JsonValue.Str(got.message)))
            }
            val expected = c["error"]?.let { JsonValue.Obj(mapOf("error" to it)) }
                ?: JsonValue.Obj(mapOf("rows" to (c["rows"] ?: JsonValue.Null), "ids" to (c["ids"] ?: JsonValue.Null)))
            same(json, expected, "batch: ${c["name"]?.string}")
        }
    }

    // A package action on a host (plain JVM, an unconfined scope).

    private class Surface(f: JsonValue) {
        var rows: List<JsonValue> = f["batch"]!!["rows"]?.array.orEmpty()
        var batches = 0
        val toasts = ArrayList<String>()
        val failed = ArrayList<String>()
        var consent = true
        val host: ExponentialHost

        init {
            val b = f["batch"]!!
            val pkg = f["package"]!!
            host = ExponentialHost(
                HostOptions(
                    functions = mapOf(
                        Actions.BATCH_FUNCTION to { args, _ ->
                            val fresh = (0 until 10).map { "new$batches-$it" }
                            when (val out = Actions.applyBatch(rows, args["writes"]?.array.orEmpty(), pkg["records"]!!, b["me"]?.string, b["now"]!!.string!!, fresh)) {
                                is Actions.BatchOutcome.Error -> error(out.message)
                                is Actions.BatchOutcome.Applied -> {
                                    rows = out.rows
                                    batches++
                                    mapOf("ids" to out.ids)
                                }
                            }
                        },
                        "harness.toast" to { args, _ -> toasts.add(args["message"]?.string.orEmpty()) },
                    ),
                    packages = listOf(pkg.json),
                    policy = HostPolicy(functions = FunctionPolicy(ask = listOf("harness.toast")), onFunctionCall = { consent }),
                    me = b["me"]?.string,
                    now = { instant(f["env"]!!["now"]!!.string!!) },
                    zone = FixedZone(f["env"]!!["offset"]!!.number!!.toInt()),
                    onActionFailed = { failed.add(it.message) },
                ),
                CoroutineScope(Dispatchers.Unconfined),
            )
            host.receive(JsonValue.parse("""{"version":"v0.9","applyTemplate":{"surfaceId":"s","packageId":"${pkg["id"]!!.string}","templateId":"month"}}"""))
        }

        val formHours: JsonValue? get() = host.surface("s")?.data?.get("form")?.get("hours")

        suspend fun call(name: String, args: Map<String, JsonValue>): FunctionOutcome = host.callFunction(FunctionCallInfo("s", "log", name, args))
    }

    private fun logArgs(hours: Double) = mapOf("date" to JsonValue.Str("2026-11-03"), "hours" to JsonValue.Num(hours))

    @Test
    fun thePlanLandsOneBatchTheSetsThenTheCalls() = runBlocking {
        val s = Surface(fixture())
        assertNotNull(s.host.surface("s"))
        val out = s.call("actions.log", logArgs(2.0))
        assertNull(out.error)
        assertEquals(1, s.batches)
        val logged = JsonValue.Obj(mapOf("date" to JsonValue.Str("2026-11-03"), "hours" to JsonValue.Num(2.0), "note" to JsonValue.Null))
        assertTrue(s.rows.any { it["data"] == logged })
        assertEquals(JsonValue.Num(0.0), s.formHours)
        assertEquals(listOf("Logged 2:00"), s.toasts)
    }

    @Test
    fun aFalseRequireLandsNothingAndIsTheFailuresMessage() = runBlocking {
        val s = Surface(fixture())
        val before = s.rows
        val out = s.call("actions.log", logArgs(0.0))
        assertEquals("Log more than 0 hours", out.error)
        assertEquals(listOf("Log more than 0 hours"), s.failed)
        assertSame(before, s.rows)
        assertEquals(JsonValue.Num(7.5), s.formHours)
    }

    @Test
    fun aRefusedWriteLandsNothingTheBatchIsOneUnit() = runBlocking {
        val s = Surface(fixture())
        val before = s.rows
        val out = s.call("actions.swap", mapOf("a" to JsonValue.Str("e1"), "b" to JsonValue.Str("missing")))
        assertEquals("write 1: record missing not found", out.error)
        assertSame(before, s.rows)
    }

    @Test
    fun aHostCallThePersonRefusesLandsNothing() = runBlocking {
        val s = Surface(fixture())
        s.consent = false
        val before = s.rows
        val out = s.call("actions.log", logArgs(2.0))
        assertEquals("harness.toast was not allowed", out.error)
        assertSame(before, s.rows)
        assertEquals(0, s.batches)
        assertEquals(JsonValue.Num(7.5), s.formHours)
    }

    private companion object {
        fun instant(s: String): Double = Instant.parse(s).toEpochMilli().toDouble()

        /** A validate case's package: the base with its `with` keys replaced (`addActions` = actions added to the base's). */
        fun validateCasePackage(base: JsonValue, with: JsonValue?): JsonValue {
            val pkg = LinkedHashMap(base.obj.orEmpty())
            for ((k, v) in with?.obj.orEmpty()) if (k != "addActions") pkg[k] = v
            with?.get("addActions")?.obj?.let { add -> pkg["actions"] = JsonValue.Obj(base["actions"]?.obj.orEmpty() + add) }
            return JsonValue.Obj(pkg)
        }
    }
}
