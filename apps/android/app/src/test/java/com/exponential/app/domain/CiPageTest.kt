package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.booleanOrNull
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

        private fun JsonObject.strings(key: String): List<String> = (this[key] as? JsonArray)?.map { it.jsonPrimitive.content }.orEmpty()
        private fun JsonObject.intOr(key: String, default: Int): Int = (this[key] as? JsonPrimitive)?.intOrNull ?: default

        fun run(o: JsonObject) = CiPage.Run(
            id = o.str("id")!!, repositoryId = o.str("repositoryId")!!, number = o.getValue("number").jsonPrimitive.int,
            name = o.str("name")!!, sha = o.str("sha") ?: "", ref = o.str("ref"), trigger = o.str("trigger") ?: "",
            state = o.str("state")!!, pullRequestId = o.str("pullRequestId"), createdByUserId = o.str("createdByUserId"),
            provider = o.str("provider") ?: "exponential", startedAt = o.str("startedAt"), completedAt = o.str("completedAt"),
            createdAt = o.str("createdAt") ?: "",
            title = o.str("title"),
            mergeTrainId = o.str("mergeTrainId"),
        )

        fun job(o: JsonObject) = CiPage.Job(
            id = o.str("id") ?: "", runId = o.str("runId")!!, check = o.str("check") ?: "", status = o.str("status")!!,
            conclusion = o.str("conclusion"), runsOn = o.strings("runsOn"),
            runnerDeviceId = o.str("runnerDeviceId"), provider = o.str("provider") ?: "exponential",
        )

        fun pr(o: JsonObject) = CiPage.Pr(
            id = o.str("id")!!, number = o.getValue("number").jsonPrimitive.int, title = o.str("title") ?: "",
            repositoryId = o.str("repositoryId") ?: "", rootSessionId = o.str("rootSessionId"), base = o.str("base") ?: "",
            state = o.str("state") ?: "open", mergeState = o.str("mergeState"), mergeWait = o.str("mergeWait"),
            mergeHold = o.str("mergeHold"), mergeNow = (o["mergeNow"] as? JsonPrimitive)?.booleanOrNull ?: false,
            mergeAttempts = o.intOr("mergeAttempts", 0), mergePosition = (o["mergePosition"] as? JsonPrimitive)?.intOrNull,
            mergeTrainId = o.str("mergeTrainId"),
        )

        fun runner(o: JsonObject) = CiPage.Runner(
            id = o.str("id")!!, label = o.str("label")!!, platform = o.str("platform"),
            labels = o.strings("labels"),
            slots = o.getValue("slots").jsonPrimitive.int, lastSeenAt = o.str("lastSeenAt"),
        )

        fun train(o: JsonObject) = CiPage.Train(
            id = o.str("id")!!, repositoryId = o.str("repositoryId")!!, base = o.str("base")!!, baseSha = o.str("baseSha"),
            units = o.getValue("units").jsonArray.map { u -> u.jsonArray.map { it.jsonPrimitive.content } },
            state = o.str("state")!!, attempt = o.intOr("attempt", 0), bisectOf = o.str("bisectOf"),
            pending = o.getValue("pending").jsonArray.map { h -> h.jsonArray.map { u -> u.jsonArray.map { it.jsonPrimitive.content } } },
            untested = o.getValue("untested").jsonPrimitive.boolean, testedSha = o.str("testedSha"),
            createdAt = o.str("createdAt")!!, finishedAt = o.str("finishedAt"),
        )

        fun trainEvent(o: JsonObject) = CiPage.TrainEvent(
            id = o.str("id")!!, repositoryId = o.str("repositoryId")!!, pullRequestId = o.str("pullRequestId")!!,
            mergeTrainId = o.str("mergeTrainId"), kind = o.str("kind")!!, base = o.str("base")!!, attempt = o.intOr("attempt", 0),
            reason = o.str("reason"), checks = o.strings("checks"), ciJobId = o.str("ciJobId"), jobUrl = o.str("jobUrl"),
            files = o.strings("files"), sha = o.str("sha"), byUserId = o.str("byUserId"), createdAt = o.str("createdAt")!!,
        )

        fun repo(o: JsonObject) = CiPage.Repo(o.str("id")!!, o.str("fullName")!!, o.str("defaultBranch"))

        /** ci-page.json's `scenario` (or a badge case's input) as [CiPage.Input]. */
        fun input(o: JsonObject) = CiPage.Input(
            runs = o.list("runs").map(::run),
            jobs = o.list("jobs").map(::job),
            repos = o.list("repos").map(::repo),
            pullRequests = o.list("pullRequests").map(::pr),
            sessions = o.list("sessions").map { CiPage.Session(it.str("id")!!, it.str("userId")!!) },
            runners = o.list("runners").map(::runner),
            users = o.list("users").map { CiPage.User(it.str("id")!!, it.str("name")!!, it.str("image")) },
            trains = o.list("trains").map(::train),
            trainEvents = o.list("trainEvents").map(::trainEvent),
            viewerId = o.str("viewerId") ?: "",
            now = o.str("now") ?: "",
            utcOffsetMinutes = (o["utcOffsetMinutes"] as? JsonPrimitive)?.intOrNull ?: 0,
        )
    }

    private fun s(v: String?): JsonElement = v?.let(::JsonPrimitive) ?: JsonNull
    private fun n(v: Number?): JsonElement = v?.let(::JsonPrimitive) ?: JsonNull
    private fun strMap(m: Map<String, String>) = JsonObject(m.mapValues { JsonPrimitive(it.value) })

    private fun json(u: CiPage.User?): JsonElement =
        u?.let { a -> buildJsonObject { put("id", a.id); put("name", a.name); put("image", s(a.image)) } } ?: JsonNull

    private fun json(r: CiPage.RunListRow) = buildJsonObject {
        put("id", r.id); put("status", r.status.wire); put("statusLabel", r.statusLabel); put("glyph", r.glyph.wire)
        put("tone", r.tone); put("title", r.title); put("workflow", r.workflow); put("repositoryId", r.repositoryId)
        put("repository", r.repository); put("repo", r.repo); put("ref", r.ref); put("sha", r.sha); put("trigger", r.trigger)
        put("source", r.source); put("sourceLabel", r.sourceLabel); put("sourceIcon", r.sourceIcon)
        put("duration", s(r.duration)); put("at", r.at)
        put("author", json(r.author))
        put("subtitle", r.subtitle); put("phoneSubtitle", r.phoneSubtitle); put("phoneMeta", r.phoneMeta)
    }

    private fun json(g: CiPage.RunGroup) = buildJsonObject {
        put("id", g.id); put("title", g.title); put("count", n(g.count))
        put("rows", JsonArray(g.rows.map(::json)))
    }

    private fun json(p: CiPage.Pill?): JsonElement = p?.let { buildJsonObject { put("text", it.text); put("tone", it.tone) } } ?: JsonNull

    private fun json(g: CiPage.SubjectGroup) = buildJsonObject {
        put("id", g.id); put("title", g.title); put("open", g.open); put("count", g.count)
        put("rows", JsonArray(g.rows.map { r ->
            buildJsonObject {
                put("id", r.id); put("pullRequestId", s(r.pullRequestId)); put("identifier", s(r.identifier)); put("title", r.title)
                put("status", r.status.wire); put("glyph", r.glyph.wire); put("tone", r.tone)
                put("subtitle", r.subtitle); put("phoneSubtitle", r.phoneSubtitle); put("meta", r.meta)
                put("pill", json(r.pill)); put("runId", r.runId); put("runs", JsonArray(r.runs.map(::json)))
            }
        }))
    }

    private fun json(r: CiPage.QueueRow) = buildJsonObject {
        put("id", r.id); put("pullRequestId", r.pullRequestId); put("identifier", r.identifier); put("title", r.title)
        put("subtitle", s(r.subtitle)); put("glyph", r.glyph); put("tone", r.tone); put("badge", json(r.badge))
        put("at", s(r.at)); put("action", s(r.action)); put("runId", s(r.runId)); put("jobId", s(r.jobId)); put("url", s(r.url))
        put("menu", r.menu)
    }

    private fun json(q: CiPage.QueueRepo) = buildJsonObject {
        put("repositoryId", q.repositoryId); put("title", q.title); put("caption", q.caption)
        put("stats", buildJsonObject {
            put("inQueue", q.stats.inQueue); put("landedToday", q.stats.landedToday)
            put("sentBackToday", q.stats.sentBackToday); put("typicalRun", s(q.stats.typicalRun))
        })
        put("testing", JsonArray(q.testing.map { t ->
            buildJsonObject {
                put("trainId", t.trainId); put("title", t.title); put("subtitle", t.subtitle)
                put("members", JsonArray(t.members.map { m ->
                    buildJsonObject { put("pullRequestId", m.pullRequestId); put("identifier", m.identifier); put("title", m.title); put("author", json(m.author)) }
                }))
                put("status", t.status.wire); put("runId", s(t.runId)); put("elapsed", t.elapsed); put("eta", s(t.eta)); put("progress", n(t.progress))
            }
        }))
        put("upNext", JsonArray(q.upNext.map(::json)))
        put("waiting", JsonArray(q.waiting.map(::json)))
        put("sentBack", JsonArray(q.sentBack.map(::json)))
        put("landed", JsonArray(q.landed.map(::json)))
    }

    private fun json(c: CiPage.RunContext) = buildJsonObject {
        put("kind", c.kind); put("badge", c.badge); put("title", s(c.title)); put("caption", s(c.caption))
        put("outcome", JsonArray(c.outcome.map { o ->
            buildJsonObject {
                put("pullRequestId", o.pullRequestId); put("identifier", o.identifier); put("title", o.title); put("text", o.text)
                put("glyph", o.glyph); put("tone", o.tone); put("jobId", s(o.jobId))
            }
        }))
    }

    /** Numbers compare by value. */
    private fun normalize(e: JsonElement): JsonElement = when (e) {
        is JsonObject -> JsonObject(e.mapValues { normalize(it.value) })
        is JsonArray -> JsonArray(e.map(::normalize))
        is JsonPrimitive -> if (!e.isString && e !is JsonNull) e.content.toDoubleOrNull()?.let(::JsonPrimitive) ?: e else e
    }

    private fun filter(o: JsonObject): CiPage.RunFilter {
        val f = o.getValue("filter").jsonObject
        return CiPage.RunFilter(f.str("status")!!, f.str("repositoryId"), f.str("source"))
    }

    private val scenario by lazy { input(fixture.getValue("scenario").jsonObject) }

    @Test
    fun `copy, testIds and limits are the fixture's`() {
        val copy = fixture.getValue("copy").jsonObject
        val q = CiPage.Copy.Queue
        val queue = buildJsonObject {
            for ((key, value) in listOf(
                "testing" to q.TESTING, "base" to q.BASE, "rerun" to q.RERUN, "split" to q.SPLIT, "untested" to q.UNTESTED,
                "runningFor" to q.RUNNING_FOR, "left" to q.LEFT, "almostDone" to q.ALMOST_DONE, "batch" to q.BATCH,
                "openRun" to q.OPEN_RUN, "upNext" to q.UP_NEXT, "position" to q.POSITION, "mergeNow" to q.MERGE_NOW,
                "notReady" to q.NOT_READY, "sentBack" to q.SENT_BACK, "failedIn" to q.FAILED_IN, "conflictIn" to q.CONFLICT_IN,
                "tries" to q.TRIES, "landed" to q.LANDED, "landedWith" to q.LANDED_WITH, "mergedNow" to q.MERGED_NOW,
                "openLog" to q.OPEN_LOG, "resume" to q.RESUME, "more" to q.MORE, "into" to q.INTO, "whatHappened" to q.WHAT_HAPPENED,
            )) put(key, value)
            put("menu", strMap(q.MENU)); put("stats", strMap(q.STATS)); put("waits", strMap(q.WAITS))
            put("holds", strMap(q.HOLDS)); put("outcome", strMap(q.OUTCOME))
        }
        val c = CiPage.Copy
        val mine = buildJsonObject {
            put("title", c.TITLE); put("tabs", strMap(c.TABS)); put("filters", strMap(c.FILTERS)); put("filterLabels", strMap(c.FILTER_LABELS))
            put("repositories", buildJsonObject { put("all", c.REPOSITORIES_ALL) })
            put("sources", JsonObject(mapOf("all" to JsonPrimitive(c.SOURCES_ALL)) + c.SOURCES.mapValues { JsonPrimitive(it.value) }))
            put("columns", strMap(c.COLUMNS)); put("pullRequest", c.PULL_REQUEST); put("triggers", strMap(c.TRIGGERS))
            put("unknownRepository", c.UNKNOWN_REPOSITORY)
            put("runners", buildJsonObject {
                put("busy", c.RUNNERS_BUSY); put("queue", c.RUNNERS_QUEUE); put("offlineSince", c.RUNNERS_OFFLINE_SINCE); put("noLabels", c.RUNNERS_NO_LABELS)
            })
            put("empty", buildJsonObject {
                put("runs", c.EMPTY_RUNS); put("filtered", c.EMPTY_FILTERED); put("train", c.EMPTY_TRAIN)
                put("prs", c.EMPTY_PRS); put("releases", c.EMPTY_RELEASES); put("runners", c.EMPTY_RUNNERS)
            })
            put("inProgress", c.IN_PROGRESS); put("today", c.TODAY); put("yesterday", c.YESTERDAY)
            put("months", JsonArray(c.MONTHS.map(::JsonPrimitive))); put("queued", c.QUEUED); put("batchTitle", c.BATCH_TITLE)
            put("runnerChip", c.RUNNER_CHIP); put("waitingForRunner", c.WAITING_FOR_RUNNER); put("phoneTabs", strMap(c.PHONE_TABS))
            put("prs", strMap(c.PRS)); put("queue", queue); put("kinds", strMap(c.KINDS)); put("batchOne", c.BATCH_ONE)
        }
        // The WHOLE copy, key order aside.
        assertEquals(copy, mine)
        assertEquals(copy.getValue("tabs").jsonObject.keys.toList(), CiPage.TAB_ORDER)
        val tones = fixture.getValue("statusTones").jsonObject
        assertEquals(tones.keys, CiPage.STATUS_TONES.keys.map { it.wire }.toSet())
        for ((status, pair) in CiPage.STATUS_TONES) {
            val t = tones.getValue(status.wire).jsonObject
            assertEquals(listOf(t.str("glyph"), t.str("tone")), listOf(pair.first.wire, pair.second))
        }
        assertEquals(
            fixture.getValue("limits"),
            buildJsonObject {
                put("runs", CiPage.LIMIT_RUNS); put("runnerOnlineMs", CiPage.RUNNER_ONLINE_MS); put("subjects", CiPage.LIMIT_SUBJECTS)
                put("subjectRuns", CiPage.LIMIT_SUBJECT_RUNS); put("history", CiPage.LIMIT_HISTORY); put("historyHours", CiPage.HISTORY_HOURS)
                put("etaSamples", CiPage.ETA_SAMPLES)
            },
        )
        val ids = fixture.getValue("testIds").jsonObject
        with(CiPage.TestIds) {
            assertEquals(
                ids,
                buildJsonObject {
                    for ((k, v) in listOf(
                        "page" to PAGE, "tabs" to TABS, "filters" to FILTERS, "statusFilter" to STATUS_FILTER, "repoFilter" to REPO_FILTER,
                        "sourceFilter" to SOURCE_FILTER, "runs" to RUNS, "runRow" to RUN_ROW, "runners" to RUNNERS, "runnerRow" to RUNNER_ROW,
                        "runnerChip" to RUNNER_CHIP, "queue" to QUEUE, "queueRepo" to QUEUE_REPO, "queueStats" to QUEUE_STATS,
                        "queueNow" to QUEUE_NOW, "queueRow" to QUEUE_ROW, "prs" to PRS, "prRow" to PR_ROW, "releases" to RELEASES,
                    )) put(k, v)
                },
            )
        }
    }

    @Test
    fun `every run list case matches the fixture`() {
        val cases = fixture.list("runGroupsCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(CiPage.runGroups(scenario, filter(case)).map(::json)))
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
    fun `every release case matches the fixture`() {
        val cases = fixture.list("releaseGroupsCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(CiPage.runGroups(scenario, filter(case), listOf("release")).map(::json)))
        }
    }

    @Test
    fun `every pull requests case matches the fixture`() {
        val cases = fixture.list("prGroupsCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(CiPage.prGroups(scenario, filter(case)).map(::json)))
        }
    }

    @Test
    fun `every merge queue case matches the fixture`() {
        val cases = fixture.list("queueCases")
        assertTrue(cases.isNotEmpty())
        val base = fixture.getValue("scenario").jsonObject
        for (case in cases) {
            val over = case["input"] as? JsonObject
            val world = input(if (over == null) base else JsonObject(base + over))
            assertEquals(case.str("name"), normalize(case.getValue("expected")), normalize(JsonArray(CiPage.queue(world).map(::json))))
        }
    }

    @Test
    fun `kinds, typical runs, ETAs and run contexts match the fixture`() {
        for (case in fixture.list("kindCases")) {
            val r = case.getValue("run").jsonObject
            val kind = CiPage.runKind(
                r.str("trigger") ?: "", r.str("mergeTrainId"), r.str("ref"), r.str("pullRequestId"), r.str("repositoryId") ?: "",
                case.list("repos").map(::repo),
            )
            assertEquals(case.str("name"), case.str("expected"), kind)
        }
        for ((repo, ms) in fixture.getValue("typicalTrainMs").jsonObject) {
            assertEquals(repo, (ms as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content?.toLong(), CiPage.typicalTrainMs(scenario, repo))
        }
        val etas = fixture.list("etaCases")
        assertTrue(etas.isNotEmpty())
        for (case in etas) {
            val typical = (case["typicalMs"] as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content?.toLong()
            val eta = CiPage.trainEta(typical, case.getValue("elapsedMs").jsonPrimitive.content.toLong())
            val actual = eta?.let { buildJsonObject { put("eta", it.eta); put("progress", it.progress) } } ?: JsonNull
            assertEquals("$typical", normalize(case.getValue("expected")), normalize(actual))
        }
        val contexts = fixture.list("runContextCases")
        assertTrue(contexts.isNotEmpty())
        for (case in contexts) {
            val run = scenario.runs.first { it.id == case.str("run") }
            assertEquals(case.str("run"), case.getValue("expected"), json(CiPage.runContext(run, scenario)))
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
        assertEquals(
            fixture.getValue("tabCounts"),
            buildJsonObject { put("train", counts.train); put("prs", counts.prs); put("releases", counts.releases); put("runners", counts.runners) },
        )
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
