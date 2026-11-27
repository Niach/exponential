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
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-2172: `packages/domain-contract/fixtures/ci-page.json` replayed against
 * [CiPage] (TS `ci-page.test.ts`), comparing the WHOLE result of every case.
 */
class CiPageTest {

    private val fixture: JsonObject by lazy { Json.parseToJsonElement(contractFixtureJson("ci-page.json")).jsonObject }

    companion object {
        private fun JsonObject.str(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.contentOrNull
        private fun JsonObject.list(key: String): List<JsonObject> = (this[key] as? JsonArray)?.map { it.jsonObject }.orEmpty()

        fun run(o: JsonObject) = CiPage.Run(
            id = o.str("id")!!, repositoryId = o.str("repositoryId")!!, number = o.getValue("number").jsonPrimitive.int,
            name = o.str("name")!!, sha = o.str("sha") ?: "", ref = o.str("ref"), trigger = o.str("trigger") ?: "",
            state = o.str("state")!!, pullRequestId = o.str("pullRequestId"), createdByUserId = o.str("createdByUserId"),
            provider = o.str("provider") ?: "exponential", startedAt = o.str("startedAt"), completedAt = o.str("completedAt"),
            createdAt = o.str("createdAt") ?: "",
            title = o.str("title"),
        )

        fun job(o: JsonObject) = CiPage.Job(
            runId = o.str("runId")!!, status = o.str("status")!!, conclusion = o.str("conclusion"),
            runsOn = (o["runsOn"] as? JsonArray)?.map { it.jsonPrimitive.content }.orEmpty(),
            runnerDeviceId = o.str("runnerDeviceId"), provider = o.str("provider") ?: "exponential",
        )

        fun pr(o: JsonObject) = CiPage.Pr(
            id = o.str("id")!!, number = o.getValue("number").jsonPrimitive.int, title = o.str("title") ?: "",
            repositoryId = o.str("repositoryId") ?: "", rootSessionId = o.str("rootSessionId"), base = o.str("base") ?: "",
            mergeState = o.str("mergeState"), mergePosition = (o["mergePosition"] as? JsonPrimitive)?.intOrNull,
            mergeTrainId = o.str("mergeTrainId"),
        )

        fun runner(o: JsonObject) = CiPage.Runner(
            id = o.str("id")!!, label = o.str("label")!!, platform = o.str("platform"),
            labels = (o["labels"] as? JsonArray)?.map { it.jsonPrimitive.content }.orEmpty(),
            slots = o.getValue("slots").jsonPrimitive.int, lastSeenAt = o.str("lastSeenAt"),
        )

        /** ci-page.json's `scenario` (or a badge case's input) as [CiPage.Input]. */
        fun input(o: JsonObject) = CiPage.Input(
            runs = o.list("runs").map(::run),
            jobs = o.list("jobs").map(::job),
            repos = o.list("repos").map { CiPage.Repo(it.str("id")!!, it.str("fullName")!!) },
            pullRequests = o.list("pullRequests").map(::pr),
            sessions = o.list("sessions").map { CiPage.Session(it.str("id")!!, it.str("userId")!!) },
            runners = o.list("runners").map(::runner),
            users = o.list("users").map { CiPage.User(it.str("id")!!, it.str("name")!!, it.str("image")) },
            viewerId = o.str("viewerId") ?: "",
            now = o.str("now") ?: "",
            utcOffsetMinutes = (o["utcOffsetMinutes"] as? JsonPrimitive)?.intOrNull ?: 0,
        )
    }

    private fun s(v: String?): JsonElement = v?.let(::JsonPrimitive) ?: JsonNull

    private fun json(r: CiPage.RunListRow) = buildJsonObject {
        put("id", r.id); put("status", r.status.wire); put("statusLabel", r.statusLabel); put("glyph", r.glyph.wire)
        put("tone", r.tone); put("title", r.title); put("workflow", r.workflow); put("repositoryId", r.repositoryId)
        put("repository", r.repository); put("repo", r.repo); put("ref", r.ref); put("sha", r.sha); put("trigger", r.trigger)
        put("source", r.source); put("sourceLabel", r.sourceLabel); put("sourceIcon", r.sourceIcon)
        put("duration", s(r.duration)); put("at", r.at)
        put("author", r.author?.let { a -> buildJsonObject { put("id", a.id); put("name", a.name); put("image", s(a.image)) } } ?: JsonNull)
        put("subtitle", r.subtitle); put("phoneSubtitle", r.phoneSubtitle); put("phoneMeta", r.phoneMeta)
    }

    private fun json(g: CiPage.RunGroup) = buildJsonObject {
        put("id", g.id); put("title", g.title); put("count", g.count?.let(::JsonPrimitive) ?: JsonNull)
        put("rows", JsonArray(g.rows.map(::json)))
    }

    private fun json(g: CiPage.TrainGroup) = buildJsonObject {
        put("repositoryId", g.repositoryId); put("title", g.title); put("caption", g.caption)
        put("rows", JsonArray(g.rows.map { r ->
            buildJsonObject {
                put("id", r.id); put("identifier", r.identifier); put("title", r.title); put("subtitle", s(r.subtitle))
                put("glyph", r.glyph.wire); put("badge", r.badge); put("pullRequestId", r.pullRequestId)
            }
        }))
    }

    private val scenario by lazy { input(fixture.getValue("scenario").jsonObject) }

    @Test
    fun `copy, testIds and limits are the fixture's`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("title"), CiPage.Copy.TITLE)
        assertEquals(copy.getValue("filters"), JsonObject(CiPage.Copy.FILTERS.mapValues { JsonPrimitive(it.value) }))
        assertEquals(copy.getValue("filterLabels"), JsonObject(CiPage.Copy.FILTER_LABELS.mapValues { JsonPrimitive(it.value) }))
        assertEquals(copy.getValue("triggers"), JsonObject(CiPage.Copy.TRIGGERS.mapValues { JsonPrimitive(it.value) }))
        val sources = copy.getValue("sources").jsonObject
        assertEquals(sources.str("all"), CiPage.Copy.SOURCES_ALL)
        assertEquals(sources.str("exponential"), CiPage.Copy.SOURCES["exponential"])
        assertEquals(sources.str("github"), CiPage.Copy.SOURCES["github"])
        val tabs = copy.getValue("tabs").jsonObject
        assertEquals(listOf(tabs.str("runs"), tabs.str("train"), tabs.str("runners")), listOf(CiPage.Copy.TAB_RUNS, CiPage.Copy.TAB_TRAIN, CiPage.Copy.TAB_RUNNERS))
        assertEquals(
            listOf("inProgress", "today", "yesterday", "queued", "batchTitle", "runnerChip", "waitingForRunner").map { copy.str(it) },
            with(CiPage.Copy) { listOf(IN_PROGRESS, TODAY, YESTERDAY, QUEUED, BATCH_TITLE, RUNNER_CHIP, WAITING_FOR_RUNNER) },
        )
        assertEquals(copy.getValue("months").jsonArray.map { it.jsonPrimitive.content }, CiPage.Copy.MONTHS)
        val tones = fixture.getValue("statusTones").jsonObject
        assertEquals(tones.keys, CiPage.STATUS_TONES.keys.map { it.wire }.toSet())
        for ((status, pair) in CiPage.STATUS_TONES) {
            val t = tones.getValue(status.wire).jsonObject
            assertEquals(listOf(t.str("glyph"), t.str("tone")), listOf(pair.first.wire, pair.second))
        }
        val cols = copy.getValue("columns").jsonObject
        assertEquals(
            listOf("status", "run", "repository", "source", "ref", "trigger", "duration", "started").map { cols.str(it) },
            with(CiPage.Copy) { listOf(COL_STATUS, COL_RUN, COL_REPOSITORY, COL_SOURCE, COL_REF, COL_TRIGGER, COL_DURATION, COL_STARTED) },
        )
        assertEquals(copy.getValue("repositories").jsonObject.str("all"), CiPage.Copy.REPOSITORIES_ALL)
        assertEquals(copy.str("trainSection"), CiPage.Copy.TRAIN_SECTION)
        assertEquals(copy.str("pullRequest"), CiPage.Copy.PULL_REQUEST)
        assertEquals(copy.str("unknownRepository"), CiPage.Copy.UNKNOWN_REPOSITORY)
        assertEquals(copy.getValue("train").jsonObject.str("batch"), CiPage.Copy.TRAIN_BATCH)
        val runners = copy.getValue("runners").jsonObject
        assertEquals(
            listOf(runners.str("busy"), runners.str("queue"), runners.str("offlineSince"), runners.str("noLabels")),
            listOf(CiPage.Copy.RUNNERS_BUSY, CiPage.Copy.RUNNERS_QUEUE, CiPage.Copy.RUNNERS_OFFLINE_SINCE, CiPage.Copy.RUNNERS_NO_LABELS),
        )
        val empty = copy.getValue("empty").jsonObject
        assertEquals(
            listOf(empty.str("runs"), empty.str("filtered"), empty.str("train"), empty.str("runners")),
            listOf(CiPage.Copy.EMPTY_RUNS, CiPage.Copy.EMPTY_FILTERED, CiPage.Copy.EMPTY_TRAIN, CiPage.Copy.EMPTY_RUNNERS),
        )
        val limits = fixture.getValue("limits").jsonObject
        assertEquals(limits.getValue("runs").jsonPrimitive.int, CiPage.LIMIT_RUNS)
        assertEquals(limits.getValue("runnerOnlineMs").jsonPrimitive.content.toLong(), CiPage.RUNNER_ONLINE_MS)
        val ids = fixture.getValue("testIds").jsonObject
        with(CiPage.TestIds) {
            assertEquals(
                listOf("page", "tiles", "tabs", "filters", "statusFilter", "repoFilter", "sourceFilter", "runs", "runRow", "train", "trainRow", "runners", "runnerRow", "runnerChip").map { ids.str(it) },
                listOf(PAGE, TILES, TABS, FILTERS, STATUS_FILTER, REPO_FILTER, SOURCE_FILTER, RUNS, RUN_ROW, TRAIN, TRAIN_ROW, RUNNERS, RUNNER_ROW, RUNNER_CHIP),
            )
        }
    }

    @Test
    fun `every run list case matches the fixture`() {
        val cases = fixture.list("runGroupsCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val f = case.getValue("filter").jsonObject
            val filter = CiPage.RunFilter(f.str("status")!!, f.str("repositoryId"), f.str("source"))
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(CiPage.runGroups(scenario, filter).map(::json)))
        }
        for (case in fixture.list("dayTitleCases")) {
            assertEquals(
                case.str("day"),
                case.str("expected"),
                CiPage.dayTitle(case.str("day")!!, case.str("now")!!, case.getValue("utcOffsetMinutes").jsonPrimitive.int),
            )
        }
    }

    @Test
    fun `repo options, tiles, tab counts and runners match the fixture`() {
        assertEquals(
            fixture.getValue("repoOptions"),
            JsonArray(CiPage.repoOptions(scenario).map { buildJsonObject { put("label", it.label); put("value", it.value) } }),
        )
        val chip = CiPage.runnerChip(scenario)!!
        assertEquals(fixture.getValue("runnerChip"), buildJsonObject { put("label", chip.label); put("online", chip.online) })
        val counts = CiPage.tabCounts(scenario)
        assertEquals(fixture.getValue("tabCounts"), buildJsonObject { put("train", counts.train); put("runners", counts.runners) })
        assertEquals(
            fixture.getValue("runners"),
            JsonArray(CiPage.runnerRows(scenario).map {
                buildJsonObject {
                    put("id", it.id); put("title", it.title); put("subtitle", it.subtitle); put("online", it.online)
                    put("busy", s(it.busy)); put("queue", s(it.queue)); put("offlineSince", s(it.offlineSince))
                }
            }),
        )
    }

    @Test
    fun `every train case matches the fixture`() {
        for (case in fixture.list("trainCases")) {
            val queue = case.getValue("queue").jsonPrimitive.boolean
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(CiPage.trainGroups(scenario, queue).map(::json)))
        }
    }

    @Test
    fun `the shot's specimen is the fixture's scenario`() {
        assertEquals(scenario, CiPageSpecimen.INPUT)
    }

    @Test
    fun `every badge case matches the fixture`() {
        for (case in fixture.list("badgeCases")) {
            val badge = NavBadges.ciBadge(input(case.getValue("input").jsonObject))
            val expected = case.getValue("expected").jsonObject
            assertEquals(case.str("name"), expected.getValue("count").jsonPrimitive.int, badge.count)
            assertEquals(case.str("name"), expected.str("tone"), badge.tone.name.lowercase())
        }
    }
}
