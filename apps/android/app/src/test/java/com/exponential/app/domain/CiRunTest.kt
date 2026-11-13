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
 * #EXP-1617 / #EXP-1804: `packages/domain-contract/fixtures/ci-run.json` and
 * `runner-settings.json` replayed against [CiRun] and [RunnerSettings]
 * (TS `ci-run.test.ts`, desktop + iOS mirrors).
 */
class CiRunTest {

    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("ci-run.json")).jsonObject }
    private val runner: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("runner-settings.json")).jsonObject }

    private fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.contentOrNull
    private fun JsonObject.obj(key: String): JsonObject = getValue(key).jsonObject
    private fun JsonObject.cases(key: String): List<JsonObject> = getValue(key).jsonArray.map { it.jsonObject }
    private fun JsonObject.long(key: String): Long? = (this[key] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.longOrNull
    private fun epoch(iso: String) = WireTimestamps.parseEpochMs(iso)!!

    private fun job(json: JsonElement): CiRun.Job {
        val o = json.jsonObject
        return CiRun.Job(
            id = o.str("id") ?: "",
            check = o.str("check") ?: "",
            position = (o["position"] as? JsonPrimitive)?.int ?: 0,
            status = o.str("status")!!,
            conclusion = o.str("conclusion"),
            reason = o.str("reason"),
            runnerLabel = o.str("runnerLabel"),
            startedAt = o.str("startedAt"),
            completedAt = o.str("completedAt"),
            steps = (o["steps"] as? JsonArray)?.map(::step).orEmpty(),
        )
    }

    private fun step(json: JsonElement): CiRun.Step {
        val o = json.jsonObject
        return CiRun.Step(o.str("name")!!, o.str("status")!!, o.str("conclusion"), o.long("startedAt"), o.long("completedAt"))
    }

    private fun run(o: JsonObject) = CiRun.Run(
        id = o.str("id") ?: "",
        number = (o["number"] as? JsonPrimitive)?.int ?: 0,
        name = o.str("name") ?: "",
        state = o.str("state")!!,
        error = o.str("error"),
        sha = o.str("sha")!!,
        startedAt = o.str("startedAt"),
        completedAt = o.str("completedAt"),
    )

    private fun row(o: JsonObject) = listOf(o.str("id"), o.str("title"), o.str("subtitle"), o.str("glyph"), o.str("tone"), o.str("duration"))
    private fun row(r: CiRun.JobRow) = listOf(r.id, r.title, r.subtitle, r.glyph.wire, r.tone.wire, r.duration)

    @Test
    fun `copy, testIds and the tail rate are the fixture's`() {
        val copy = fixture.obj("copy")
        assertEquals(copy.str("title"), CiRun.Copy.TITLE)
        assertEquals(copy.str("jobs"), CiRun.Copy.JOBS)
        assertEquals(copy.str("steps"), CiRun.Copy.STEPS)
        assertEquals(copy.str("log"), CiRun.Copy.LOG)
        assertEquals(copy.str("rerunFailed"), CiRun.Copy.RERUN_FAILED)
        assertEquals(copy.str("cancel"), CiRun.Copy.CANCEL)
        assertEquals(copy.str("emptyLog"), CiRun.Copy.EMPTY_LOG)
        assertEquals(copy.str("pipelineError"), CiRun.Copy.PIPELINE_ERROR)
        assertEquals(copy.str("none"), CiRun.Copy.NONE)
        val tiles = copy.obj("tiles")
        assertEquals(
            listOf(tiles.str("status"), tiles.str("duration"), tiles.str("runner"), tiles.str("commit")),
            listOf(CiRun.Copy.TILE_STATUS, CiRun.Copy.TILE_DURATION, CiRun.Copy.TILE_RUNNER, CiRun.Copy.TILE_COMMIT),
        )
        val status = copy.obj("status")
        assertEquals(status.keys, CiRun.RunStatus.entries.map { it.wire }.toSet())
        for (s in CiRun.RunStatus.entries) assertEquals(status.str(s.wire), CiRun.Copy.STATUS[s])
        val reasons = copy.obj("reasons")
        assertEquals(reasons.mapValues { (it.value as JsonPrimitive).content }, CiRun.Copy.REASONS)
        val ids = fixture.obj("testIds")
        assertEquals(
            listOf("page", "tiles", "jobRow", "steps", "log", "rerun", "cancel").map { ids.str(it) },
            listOf(
                CiRun.TestIds.PAGE, CiRun.TestIds.TILES, CiRun.TestIds.JOB_ROW, CiRun.TestIds.STEPS,
                CiRun.TestIds.LOG, CiRun.TestIds.RERUN, CiRun.TestIds.CANCEL,
            ),
        )
        assertEquals(fixture.long("liveTailMs"), CiRun.LIVE_TAIL_MS)
        val lines = fixture.obj("logMaxLines")
        assertEquals(lines.long("wide"), CiRun.LOG_MAX_LINES_WIDE.toLong())
        assertEquals(lines.long("phone"), CiRun.LOG_MAX_LINES_PHONE.toLong())
    }

    @Test
    fun `every runStatus case matches the fixture`() {
        for (case in fixture.cases("runStatus")) {
            val jobs = case.getValue("jobs").jsonArray.map { it.jsonObject.let { o -> o.str("status")!! to o.str("conclusion") } }
            val (status, tone) = CiRun.runStatus(case.str("state")!!, jobs)
            val expected = case.obj("expected")
            assertEquals(case.str("name"), listOf(expected.str("status"), expected.str("tone")), listOf(status.wire, tone.wire))
        }
    }

    @Test
    fun `every tiles case matches the fixture`() {
        for (case in fixture.cases("tiles")) {
            val actual = CiRun.tiles(run(case.obj("run")), case.getValue("jobs").jsonArray.map(::job), epoch(case.str("now")!!))
            val expected = case.getValue("expected").jsonArray.map { it.jsonObject.let { o -> listOf(o.str("label"), o.str("value"), o.str("tone"), o.str("delta")) } }
            assertEquals(case.str("name"), expected, actual.map { listOf(it.label, it.value, it.tone.wire, it.delta) })
        }
    }

    @Test
    fun `every jobRows case matches the fixture`() {
        for (case in fixture.cases("jobRows")) {
            val actual = CiRun.jobRows(case.getValue("jobs").jsonArray.map(::job), epoch(case.str("now")!!))
            assertEquals(case.str("name"), case.getValue("expected").jsonArray.map { row(it.jsonObject) }, actual.map(::row))
        }
    }

    @Test
    fun `every defaultJob case matches the fixture`() {
        for (case in fixture.cases("defaultJob")) {
            val actual = CiRun.defaultJob(case.getValue("jobs").jsonArray.map(::job), case.str("requested"))
            assertEquals(case.str("name"), case.str("expected"), actual)
        }
    }

    @Test
    fun `every steps case matches the fixture`() {
        for (case in fixture.cases("steps")) {
            val actual = CiRun.steps(case.getValue("steps").jsonArray.map(::step), epoch(case.str("now")!!))
            val expected = case.getValue("expected").jsonArray.map { it.jsonObject.let { o -> listOf(o.str("title"), o.str("status"), o.str("description")) } }
            assertEquals(case.str("name"), expected, actual.map { listOf(it.title, it.status.wire, it.description) })
        }
    }

    @Test
    fun `steps parse from their synced jsonb`() {
        val case = fixture.cases("steps").first()
        assertEquals(case.getValue("steps").jsonArray.map(::step), CiRun.parseSteps(case.getValue("steps").toString()))
        assertEquals(emptyList<CiRun.Step>(), CiRun.parseSteps(null))
        assertEquals(emptyList<CiRun.Step>(), CiRun.parseSteps("{not json"))
    }

    @Test
    fun `every logView case matches the fixture`() {
        for (case in fixture.cases("logView")) {
            val expected = case.obj("expected")
            val actual = CiRun.logView(case.str("text")!!)
            assertEquals(case.str("name"), expected.str("code"), actual.code)
            assertEquals(case.str("name"), expected.getValue("highlight").jsonArray.map { it.jsonPrimitive.int }, actual.highlight)
        }
    }

    @Test
    fun `every actions case matches the fixture`() {
        for (case in fixture.cases("actions")) {
            val expected = case.obj("expected")
            val actual = CiRun.actions(case.str("state")!!, case.str("error"))
            assertEquals(
                case.str("name"),
                listOf(expected.getValue("rerunFailed").jsonPrimitive.boolean, expected.getValue("cancel").jsonPrimitive.boolean),
                listOf(actual.rerunFailed, actual.cancel),
            )
        }
    }

    @Test
    fun `the specimen is the fixture's`() {
        val specimen = fixture.obj("specimen")
        assertEquals(specimen.str("now"), CiRun.SPECIMEN_NOW)
        assertEquals(run(specimen.obj("run")), CiRun.SPECIMEN_RUN)
        assertEquals(specimen.getValue("jobs").jsonArray.map(::job), CiRun.SPECIMEN_JOBS)
        assertEquals(specimen.str("selected"), CiRun.SPECIMEN_SELECTED)
        assertEquals(specimen.str("log"), CiRun.SPECIMEN_LOG)
        // The specimen's selection is the rule's own pick too (its first failing job).
        assertEquals(CiRun.SPECIMEN_SELECTED, CiRun.defaultJob(CiRun.SPECIMEN_JOBS, null))
    }

    @Test
    fun `run links parse`() {
        assertEquals("acme" to "r12", CiRun.parsePath("/t/acme/ci/r12?job=j2"))
        assertEquals("j2", CiRun.jobParam("/t/acme/ci/r12?job=j2"))
        assertEquals(null, CiRun.jobParam("/t/acme/ci/r12"))
        assertEquals(null, CiRun.parsePath("/t/acme/ci"))
        assertEquals(null, CiRun.parsePath("/t/acme/sessions/r12"))
        assertTrue(isCiRunLink("https://app.exponential.at/t/acme/ci/r12?job=j2", "https://app.exponential.at"))
        assertEquals(false, isCiRunLink("https://github.com/o/r/actions/runs/1/job/5", "https://app.exponential.at"))
        assertEquals(false, isCiRunLink("https://app.exponential.at/t/acme/sessions/s1", "https://app.exponential.at"))
    }

    // ── runner-settings.json ────────────────────────────────────────────

    @Test
    fun `runner copy, defaults, limits, options and testIds are the fixture's`() {
        val copy = runner.obj("copy")
        assertEquals(copy.str("section"), RunnerSettings.Copy.SECTION)
        assertEquals(copy.obj("enabled").str("label"), RunnerSettings.Copy.ENABLED_LABEL)
        assertEquals(copy.obj("enabled").str("description"), RunnerSettings.Copy.ENABLED_DESCRIPTION)
        val labels = copy.obj("labels")
        assertEquals(labels.str("label"), RunnerSettings.Copy.LABELS_LABEL)
        assertEquals(labels.str("placeholder"), RunnerSettings.Copy.LABELS_PLACEHOLDER)
        assertEquals(labels.str("invalid"), RunnerSettings.Copy.LABELS_INVALID)
        assertEquals(copy.str("slots"), RunnerSettings.Copy.SLOTS)
        assertEquals(copy.str("timeout"), RunnerSettings.Copy.TIMEOUT)
        assertEquals(copy.str("recent"), RunnerSettings.Copy.RECENT)
        assertEquals(copy.str("noJobs"), RunnerSettings.Copy.NO_JOBS)
        assertEquals(copy.str("runSubtitle"), RunnerSettings.Copy.RUN_SUBTITLE)
        val notice = copy.obj("notice")
        assertEquals(notice.keys, RunnerSettings.Notice.entries.map { it.wire }.toSet())
        for (n in RunnerSettings.Notice.entries) {
            val o = notice.obj(n.wire)
            assertEquals(RunnerSettings.NoticeCopy(o.str("type")!!, o.str("title")!!, o.str("message")!!), RunnerSettings.Copy.NOTICE[n])
        }
        val defaults = runner.obj("defaults")
        assertEquals(
            RunnerSettings.Settings(
                enabled = defaults.getValue("enabled").jsonPrimitive.boolean,
                labels = defaults.getValue("labels").jsonArray.map { it.jsonPrimitive.content },
                slots = defaults.getValue("slots").jsonPrimitive.int,
                timeoutMin = defaults.getValue("timeoutMin").jsonPrimitive.int,
            ),
            RunnerSettings.DEFAULTS,
        )
        val limits = runner.obj("limits")
        assertEquals(limits.getValue("labels").jsonPrimitive.int, RunnerSettings.MAX_LABELS)
        assertEquals(limits.obj("slots").getValue("min").jsonPrimitive.int, RunnerSettings.MIN_SLOTS)
        assertEquals(limits.obj("slots").getValue("max").jsonPrimitive.int, RunnerSettings.MAX_SLOTS)
        assertEquals(limits.getValue("recent").jsonPrimitive.int, RunnerSettings.MAX_RECENT)
        assertEquals(
            runner.cases("timeoutOptions").map { it.str("value")!! to it.str("label")!! },
            RunnerSettings.TIMEOUT_OPTIONS,
        )
        val ids = runner.obj("testIds")
        assertEquals(
            listOf("section", "switch", "labels", "slots", "timeout", "recent").map { ids.str(it) },
            listOf(
                RunnerSettings.TestIds.SECTION, RunnerSettings.TestIds.SWITCH, RunnerSettings.TestIds.LABELS,
                RunnerSettings.TestIds.SLOTS, RunnerSettings.TestIds.TIMEOUT, RunnerSettings.TestIds.RECENT,
            ),
        )
    }

    @Test
    fun `every normalizeLabel case matches the fixture`() {
        for (case in runner.cases("normalizeLabel")) {
            assertEquals(case.str("input"), case.str("expected"), RunnerSettings.normalizeLabel(case.str("input")!!))
        }
    }

    @Test
    fun `every notice case matches the fixture`() {
        for (case in runner.cases("notice")) {
            val caps = case.getValue("caps").jsonArray.map { it.jsonPrimitive.content }
            assertEquals(case.str("name"), case.str("expected"), RunnerSettings.notice(case.str("client")!!, caps)?.wire)
        }
    }

    @Test
    fun `every osLabel case matches the fixture`() {
        for (case in runner.cases("osLabel")) {
            assertEquals(case.str("platform"), case.str("expected"), RunnerSettings.osLabel(case.str("platform")))
        }
    }

    private fun runnerJob(json: JsonElement): RunnerSettings.RunnerJob {
        val o = json.jsonObject
        return RunnerSettings.RunnerJob(
            id = o.str("id")!!,
            runnerDeviceId = o.str("runnerDeviceId"),
            check = o.str("check")!!,
            runNumber = o.getValue("runNumber").jsonPrimitive.int,
            repo = o.str("repo")!!,
            status = o.str("status")!!,
            conclusion = o.str("conclusion"),
            startedAt = o.str("startedAt"),
            completedAt = o.str("completedAt"),
        )
    }

    @Test
    fun `every recentJobs case matches the fixture`() {
        val cases = runner.cases("recentJobs")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val actual = RunnerSettings.recentJobs(case.str("deviceId")!!, case.getValue("jobs").jsonArray.map(::runnerJob), epoch(case.str("now")!!))
            assertEquals(case.str("name"), case.getValue("expected").jsonArray.map { row(it.jsonObject) }, actual.map(::row))
        }
    }

    @Test
    fun `the runner specimen is the fixture's (with recentJobs 0)`() {
        val specimen = runner.obj("specimen")
        val device = specimen.obj("device")
        assertEquals(device.str("id"), RunnerSettings.SPECIMEN_DEVICE_ID)
        assertEquals(device.str("label"), RunnerSettings.SPECIMEN_LABEL)
        assertEquals(device.str("platform"), RunnerSettings.SPECIMEN_PLATFORM)
        assertEquals(device.getValue("caps").jsonArray.map { it.jsonPrimitive.content }, RunnerSettings.SPECIMEN_CAPS)
        assertEquals(RunnerSettings.parse(device.getValue("runner").toString()), RunnerSettings.SPECIMEN_SETTINGS)
        val recent = runner.cases("recentJobs").first()
        assertEquals(recent.str("now"), RunnerSettings.SPECIMEN_NOW)
        assertEquals(recent.getValue("jobs").jsonArray.map(::runnerJob), RunnerSettings.SPECIMEN_JOBS)
        assertEquals(null, RunnerSettings.parse(null))
    }
}
