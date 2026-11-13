package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1804: [CiRunView] builds the SAME genUI trees as web
 * `apps/web/src/lib/ci-run-view.ts`. `ci-run-view.golden.json` holds web's
 * own output (its `ciRunTree`, `runnerSectionTree` and `runnerSettingsAfter`
 * over the fixtures' specimens and a few variations), replayed here node for
 * node: ids, props and child order.
 */
class CiRunViewTest {

    private val golden: JsonObject by lazy {
        val text = javaClass.classLoader!!.getResourceAsStream("ci-run-view.golden.json")!!.bufferedReader().readText()
        Json.parseToJsonElement(text).jsonObject
    }

    private fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.contentOrNull
    private fun JsonObject.long(key: String): Long? = (this[key] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.longOrNull

    private fun job(json: JsonElement): CiRun.Job {
        val o = json.jsonObject
        return CiRun.Job(
            id = o.str("id")!!,
            check = o.str("check")!!,
            position = o.getValue("position").jsonPrimitive.int,
            status = o.str("status")!!,
            conclusion = o.str("conclusion"),
            reason = o.str("reason"),
            runnerLabel = o.str("runnerLabel"),
            startedAt = o.str("startedAt"),
            completedAt = o.str("completedAt"),
            steps = o.getValue("steps").jsonArray.map { s ->
                val step = s.jsonObject
                CiRun.Step(step.str("name")!!, step.str("status")!!, step.str("conclusion"), step.long("startedAt"), step.long("completedAt"))
            },
        )
    }

    private fun run(o: JsonObject) = CiRun.Run(
        id = o.str("id")!!,
        number = o.getValue("number").jsonPrimitive.int,
        name = o.str("name")!!,
        state = o.str("state")!!,
        error = o.str("error"),
        sha = o.str("sha")!!,
        startedAt = o.str("startedAt"),
        completedAt = o.str("completedAt"),
    )

    private fun settings(o: JsonObject) = RunnerSettings.Settings(
        enabled = o.getValue("enabled").jsonPrimitive.boolean,
        labels = o.getValue("labels").jsonArray.map { it.jsonPrimitive.content },
        slots = o.getValue("slots").jsonPrimitive.int,
        timeoutMin = o.getValue("timeoutMin").jsonPrimitive.int,
    )

    /** Numbers compare by value (web's `2` and Kotlin's `2` alike). */
    private fun normalize(e: JsonElement): JsonElement = when (e) {
        is JsonObject -> JsonObject(e.mapValues { normalize(it.value) })
        is JsonArray -> JsonArray(e.map(::normalize))
        is JsonPrimitive -> if (!e.isString && e !is JsonNull) e.content.toDoubleOrNull()?.let(::JsonPrimitive) ?: e else e
    }

    @Test
    fun `the run page tree is web's`() {
        val cases = golden.getValue("ciRun").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val model = case.getValue("model").jsonObject
            val busy = when (case.str("busy")) {
                "rerun" -> CiRunView.Busy.Rerun
                "cancel" -> CiRunView.Busy.Cancel
                else -> null
            }
            val actual = CiRunView.ciRunTree(
                run = run(model.getValue("run").jsonObject),
                jobs = model.getValue("jobs").jsonArray.map(::job),
                selected = case.str("selected"),
                log = case.str("log"),
                nowMs = WireTimestamps.parseEpochMs(case.str("now")!!)!!,
                phone = case.getValue("phone").jsonPrimitive.boolean,
                busy = busy,
            )
            assertEquals(case.str("name"), normalize(case.getValue("tree")), normalize(actual))
        }
    }

    @Test
    fun `the runner section tree is web's`() {
        val sample = RunnerSettings.recentJobs(
            RunnerSettings.SPECIMEN_DEVICE_ID,
            RunnerSettings.SPECIMEN_JOBS,
            WireTimestamps.parseEpochMs(RunnerSettings.SPECIMEN_NOW)!!,
        )
        for (case in golden.getValue("runner").jsonArray.map { it.jsonObject }) {
            val actual = CiRunView.runnerSectionTree(
                client = case.str("client")!!,
                caps = case.getValue("caps").jsonArray.map { it.jsonPrimitive.content },
                platform = case.str("platform"),
                settings = settings(case.getValue("settings").jsonObject),
                owner = case.getValue("owner").jsonPrimitive.boolean,
                refused = case.getValue("refused").jsonPrimitive.boolean,
                recent = if (case.getValue("recent").jsonPrimitive.boolean) sample else emptyList(),
            )
            assertEquals(case.str("name"), normalize(case.getValue("tree")), normalize(actual))
        }
    }

    @Test
    fun `runner change events write what web writes`() {
        for (case in golden.getValue("after").jsonArray.map { it.jsonObject }) {
            val (next, refused) = CiRunView.runnerSettingsAfter(
                settings(case.getValue("current").jsonObject),
                case.str("event")!!,
                case.getValue("payload").jsonObject,
            )
            val expected = (case["settings"] as? JsonObject)?.let(::settings)
            val name = "${case.str("event")} ${case.getValue("payload")}"
            assertEquals(name, expected, next)
            assertEquals(name, case.getValue("refused").jsonPrimitive.boolean, refused)
        }
    }
}
