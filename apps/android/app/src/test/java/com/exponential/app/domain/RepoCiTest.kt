package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** [RepoCi] replays `fixtures/repo-ci.json` (runRows, pipelineRows,
 *  requiredSuggestions, secretName, switches), comparing the WHOLE result;
 *  the copy, testIds, limits and the styleguide specimen equal the fixture's. */
class RepoCiTest {

    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("repo-ci.json")).jsonObject }

    private fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.contentOrNull
    private fun JsonObject.obj(key: String): JsonObject = getValue(key).jsonObject
    private fun JsonObject.cases(key: String): List<JsonObject> = getValue(key).jsonArray.map { it.jsonObject }
    private fun nullable(value: String?): JsonElement = value?.let(::JsonPrimitive) ?: JsonNull

    private fun run(o: JsonObject) = RepoCi.Run(
        id = o.str("id")!!,
        number = o.getValue("number").jsonPrimitive.int,
        name = o.str("name")!!,
        sha = o.str("sha") ?: "",
        ref = o.str("ref"),
        trigger = o.str("trigger") ?: "",
        state = o.str("state") ?: "pending",
        startedAt = o.str("startedAt"),
        completedAt = o.str("completedAt"),
    )

    private fun job(o: JsonObject) = RepoCi.Job(
        runId = o.str("runId")!!,
        status = o.str("status") ?: "",
        conclusion = o.str("conclusion"),
        check = o.str("check") ?: "",
    )

    private fun runs(c: JsonObject) = c.getValue("runs").jsonArray.map { run(it.jsonObject) }
    private fun jobs(c: JsonObject) = (c["jobs"] as? JsonArray).orEmpty().map { job(it.jsonObject) }

    private fun rowJson(r: RepoCi.RunRow) = buildJsonObject {
        put("id", r.id)
        put("identifier", r.identifier)
        put("title", r.title)
        put("status", r.status.wire)
        put("statusLabel", r.statusLabel)
        put("glyph", r.glyph.wire)
        put("tone", r.tone.wire)
        put("commit", r.commit)
        put("ref", r.ref)
        put("trigger", r.trigger)
        put("subtitle", r.subtitle)
        put("duration", nullable(r.duration))
        put("startedAt", nullable(r.startedAt))
    }

    @Test
    fun `runRows cases`() {
        val all = fixture.cases("runRows")
        assertTrue(all.isNotEmpty())
        for (c in all) {
            val now = WireTimestamps.parseEpochMs(c.str("now")!!)!!
            val actual = JsonArray(RepoCi.runRows(runs(c), jobs(c), now).map(::rowJson))
            assertEquals(c.str("name"), c.getValue("expected"), actual)
        }
    }

    @Test
    fun `pipelineRows cases`() {
        for (c in fixture.cases("pipelineRows")) {
            val actual = JsonArray(
                RepoCi.pipelineRows(runs(c)).map { buildJsonObject { put("name", it.name); put("runId", it.runId); put("state", it.state) } },
            )
            assertEquals(c.str("name"), c.getValue("expected"), actual)
        }
    }

    @Test
    fun `requiredSuggestions cases`() {
        val all = fixture.cases("requiredSuggestions")
        assertTrue(all.isNotEmpty())
        for (c in all) {
            val required = c.getValue("required").jsonArray.map { it.jsonPrimitive.content }
            val actual = JsonArray(RepoCi.requiredSuggestions(runs(c), jobs(c), required).map(::JsonPrimitive))
            assertEquals(c.str("name"), c.getValue("expected"), actual)
        }
    }

    @Test
    fun `secretName cases`() {
        for (c in fixture.cases("secretName")) {
            assertEquals(c.str("input"), c.getValue("valid").jsonPrimitive.boolean, RepoCi.secretName(c.str("input")!!))
        }
    }

    @Test
    fun `switches cases`() {
        for (c in fixture.cases("switches")) {
            val actual = JsonArray(RepoCi.switches(c.str("provider")).map { JsonPrimitive(it.wire) })
            assertEquals(c.str("provider"), c.getValue("expected"), actual)
        }
    }

    @Test
    fun `copy, testIds and limits are the fixture's`() {
        val copy = fixture.obj("copy")
        val runs = copy.obj("runs")
        assertEquals(runs.str("noRef"), RepoCi.Copy.Runs.NO_REF)
        val cols = runs.obj("columns")
        assertEquals(
            listOf("status", "run", "commit", "ref", "trigger", "duration", "started").map { cols.str(it) },
            with(RepoCi.Copy.Runs) { listOf(COL_STATUS, COL_RUN, COL_COMMIT, COL_REF, COL_TRIGGER, COL_DURATION, COL_STARTED) },
        )
        assertEquals(copy.obj("triggers").mapValues { it.value.jsonPrimitive.content }, RepoCi.Copy.TRIGGERS)
        val status = copy.obj("status")
        for ((key, label) in CiRun.Copy.STATUS) assertEquals(status.str(key.wire), label)
        assertEquals(status.size, CiRun.Copy.STATUS.size)
        val ci = copy.obj("ci")
        assertEquals(ci.str("section"), RepoCi.Copy.Ci.SECTION)
        assertEquals(ci.str("noPipeline"), RepoCi.Copy.Ci.NO_PIPELINE)
        val required = ci.obj("required")
        assertEquals(required.str("label"), RepoCi.Copy.Ci.REQUIRED_LABEL)
        assertEquals(required.str("description"), RepoCi.Copy.Ci.REQUIRED_DESCRIPTION)
        assertEquals(required.str("placeholder"), RepoCi.Copy.Ci.REQUIRED_PLACEHOLDER)
        val s = ci.obj("secrets")
        with(RepoCi.Copy.Secrets) {
            assertEquals(s.str("title"), TITLE)
            assertEquals(s.obj("columns").str("name"), COL_NAME)
            assertEquals(s.obj("columns").str("updated"), COL_UPDATED)
            assertEquals(s.str("add"), ADD)
            assertEquals(s.str("name"), NAME)
            assertEquals(s.str("value"), VALUE)
            assertEquals(s.str("save"), SAVE)
            assertEquals(s.str("cancel"), CANCEL)
            assertEquals(s.str("delete"), DELETE)
            assertEquals(s.str("deleteTitle"), DELETE_TITLE)
            assertEquals(s.str("deleteConfirm"), DELETE_CONFIRM)
            assertEquals(s.str("empty"), EMPTY)
            assertEquals(s.str("invalid"), INVALID)
        }
        for ((switch, text) in RepoCi.Copy.SWITCHES) {
            assertEquals(ci.obj(switch.wire).str("label"), text.label)
            assertEquals(ci.obj(switch.wire).str("description"), text.description)
        }
        val ids = fixture.obj("testIds")
        with(RepoCi.TestIds) {
            assertEquals(
                listOf(RUNS, RUN_ROW, CI, PIPELINES, REQUIRED, SECRETS, ADD_SECRET, GITHUB_STATUSES, ACTIONS_RUNNERS),
                listOf("runs", "runRow", "ci", "pipelines", "required", "secrets", "addSecret", "githubStatuses", "actionsRunners")
                    .map { ids.str(it) },
            )
        }
        assertEquals(fixture.obj("limits").getValue("runs").jsonPrimitive.int, RepoCi.MAX_RUNS)
    }

    @Test
    fun `the specimen is runRows' first case`() {
        val c = fixture.cases("runRows").first()
        assertEquals(c.str("now"), RepoCi.SPECIMEN_NOW)
        assertEquals(runs(c), RepoCi.SPECIMEN_RUNS)
        assertEquals(jobs(c).map { it.copy(check = "") }, RepoCi.SPECIMEN_JOBS.map { it.copy(check = "") })
    }
}
