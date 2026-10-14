package com.exponential.app.domain

import com.exponential.app.data.db.PullRequestEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1291 Threads v2: the session tree is fixture-locked ×4 — every case of
 * `session-card.json`, `session-thread.json` (cases + guidePullRequestCases +
 * composerCases) and `threads-list.json` (cases +
 * agentCases) runs here (TS `session-tree.test.ts`, desktop
 * `domain::session_tree`, iOS `SessionTreeTests`). Outputs are re-encoded in
 * the fixture's own JSON shape and compared as a WHOLE, so a field the port
 * forgets fails too. The embedded copy is proved equal to the fixtures'.
 */
class SessionTreeTest {

    private fun fixture(name: String): JsonObject =
        Json.parseToJsonElement(contractFixtureJson(name)).jsonObject

    private val cardFixture by lazy { fixture("session-card.json") }
    private val threadFixture by lazy { fixture("session-thread.json") }
    private val listFixture by lazy { fixture("threads-list.json") }

    // ── decode (the fixture's input shapes) ─────────────────────────────────

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.bool(key: String): Boolean = getValue(key).jsonPrimitive.boolean

    private fun session(json: JsonObject) = SessionTree.Session(
        id = json.str("id")!!,
        teamId = json.str("teamId")!!,
        parentId = json.str("parentId"),
        rootId = json.str("rootId")!!,
        kind = json.str("kind")!!,
        userId = json.str("userId")!!,
        status = json.str("status")!!,
        busy = json.bool("busy"),
        needsInput = json.bool("needsInput"),
        blocked = (json["blocked"] as? JsonObject)?.let { SessionTree.Blocked(it.str("window"), it.str("resetsAt")) },
        shared = json.bool("shared"),
        title = json.str("title"),
        guide = (json["guide"] as? JsonArray)?.map { entry ->
            val obj = entry.jsonObject
            SessionTree.GuideEntry(text = obj.str("text"), label = obj.str("label"))
        },
        actionName = json.str("actionName"),
        name = json.str("name"),
        icon = json.str("icon"),
        color = json.str("color"),
        sortOrder = json.getValue("sortOrder").jsonPrimitive.double,
        startedAt = json.str("startedAt"),
        endedAt = json.str("endedAt"),
        createdAt = json.str("createdAt")!!,
        updatedAt = json.str("updatedAt")!!,
    )

    private fun sessionIssue(json: JsonObject) = SessionTree.SessionIssue(
        sessionId = json.str("sessionId")!!,
        issueId = json.str("issueId")!!,
        sortOrder = json.getValue("sortOrder").jsonPrimitive.double,
    )

    private fun issue(json: JsonObject) = SessionTree.Issue(
        id = json.str("id")!!,
        identifier = json.str("identifier")!!,
        title = json.str("title")!!,
    )

    private fun message(json: JsonObject) = SessionTree.Message(
        id = json.str("id")!!,
        sessionId = json.str("sessionId")!!,
        authorKind = json.str("authorKind")!!,
        authorId = json.str("authorId"),
        body = json.str("body")!!,
        card = json["card"] as? JsonObject,
        answeredAt = json.str("answeredAt"),
        consumedAt = json.str("consumedAt"),
        createdAt = json.str("createdAt")!!,
    )

    private fun pullRequest(json: JsonObject) = SessionTree.PullRequest(
        id = json.str("id")!!,
        number = json.getValue("number").jsonPrimitive.int,
    )

    private fun JsonObject.list(key: String): List<JsonObject> =
        (this[key] as? JsonArray).orEmpty().map { it.jsonObject }

    // ── encode (the fixture's expected shapes) ──────────────────────────────

    private fun nullable(value: String?): JsonElement = value?.let(::JsonPrimitive) ?: JsonNull

    private fun nullable(value: Int?): JsonElement = value?.let(::JsonPrimitive) ?: JsonNull

    private fun encode(card: SessionTree.SessionCard): JsonObject = buildJsonObject {
        put("sessionId", card.sessionId)
        put("icon", card.icon)
        put("name", card.name)
        put("kindWord", card.kindWord)
        put("state", card.state.wire)
        put("tone", card.tone)
        put("elapsed", nullable(card.elapsed))
        put("running", card.running)
        put("testId", card.testId)
    }

    private fun encode(tab: SessionTree.ThreadTab): JsonObject = when (tab) {
        SessionTree.ThreadTab.Conversation -> buildJsonObject { put("kind", "conversation") }
        is SessionTree.ThreadTab.IssueTab -> buildJsonObject {
            put("kind", "issue")
            put("issueId", tab.issueId)
            put("identifier", tab.identifier)
        }
        SessionTree.ThreadTab.Add -> buildJsonObject { put("kind", "add") }
        is SessionTree.ThreadTab.Guide -> buildJsonObject {
            put("kind", "guide")
            put("dot", tab.dot)
        }
    }

    private fun encode(row: SessionTree.ThreadRow): JsonObject = when (row) {
        is SessionTree.ThreadRow.MessageRow -> buildJsonObject {
            put("type", "message")
            put("id", row.id)
            put("author", row.author)
            put("authorId", nullable(row.authorId))
            put("body", row.body)
            put("queued", row.queued)
        }
        is SessionTree.ThreadRow.SessionRow -> buildJsonObject {
            put("type", "session")
            put("id", row.id)
            put("card", encode(row.card))
        }
        is SessionTree.ThreadRow.Question -> buildJsonObject {
            put("type", "question")
            put("id", row.id)
            put("question", row.question)
            put("options", JsonArray(row.options.map(::JsonPrimitive)))
            put("answer", nullable(row.answer))
            put("answered", row.answered)
        }
        is SessionTree.ThreadRow.Plan -> buildJsonObject {
            put("type", "plan")
            put("id", row.id)
            put("plan", row.plan)
            put("decision", nullable(row.decision))
        }
        is SessionTree.ThreadRow.Subagent -> buildJsonObject {
            put("type", "subagent")
            put("id", row.id)
            put("title", row.title)
            put("status", row.status)
        }
        is SessionTree.ThreadRow.Picture -> buildJsonObject {
            put("type", "picture")
            put("id", row.id)
            put("attachmentId", row.attachmentId)
            put("width", nullable(row.width))
            put("height", nullable(row.height))
            put("caption", nullable(row.caption))
        }
        is SessionTree.ThreadRow.Event -> buildJsonObject {
            put("type", "event")
            put("id", row.id)
            put("event", row.event)
            put("text", row.text)
        }
    }

    private fun encode(view: SessionTree.ThreadView): JsonObject = buildJsonObject {
        put("rootId", view.rootId)
        put("title", view.title)
        put("titleDerived", view.titleDerived)
        put("private", view.private)
        put("tabs", JsonArray(view.tabs.map(::encode)))
        put("rows", JsonArray(view.rows.map(::encode)))
        put(
            "composer",
            buildJsonObject {
                put("sessionId", view.composer.sessionId)
                put("placeholder", view.composer.placeholder)
            },
        )
        put("live", view.live)
        put("busy", view.busy)
        put("needsYou", view.needsYou)
    }

    private fun encode(group: SessionTree.ThreadsListGroup): JsonObject = buildJsonObject {
        put("key", group.key.wire)
        put("title", group.title)
        put(
            "rows",
            JsonArray(
                group.rows.map { row ->
                    buildJsonObject {
                        put("sessionId", row.sessionId)
                        put("teamId", row.teamId)
                        put("title", row.title)
                        put("private", row.private)
                        put("busy", row.busy)
                        put("needsYou", row.needsYou)
                        put("activityAt", row.activityAt)
                    }
                },
            ),
        )
    }

    private fun encode(agent: SessionTree.AgentRow): JsonObject = buildJsonObject {
        put("sessionId", agent.sessionId)
        put("teamId", agent.teamId)
        put("name", agent.name)
        put("icon", agent.icon)
        put("color", agent.color)
        put("busy", agent.busy)
        put("needsYou", agent.needsYou)
    }

    // ── the cases ───────────────────────────────────────────────────────────

    @Test
    fun `every session-card case matches the fixture`() {
        val cases = cardFixture.list("cases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = SessionTree.sessionCard(
                session(input.getValue("session").jsonObject),
                input.getValue("ordinal").jsonPrimitive.int,
                input.str("now")!!,
            )
            assertEquals(case.str("name"), case.getValue("expected"), encode(actual))
        }
    }

    @Test
    fun `every session-thread case matches the fixture`() {
        val cases = threadFixture.list("cases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = SessionTree.buildSessionThread(
                root = session(input.getValue("root").jsonObject),
                sessions = input.list("sessions").map(::session),
                sessionIssues = input.list("sessionIssues").map(::sessionIssue),
                issues = input.list("issues").map(::issue),
                messages = input.list("messages").map(::message),
                pullRequests = input.list("pullRequests").map(::pullRequest),
                now = input.str("now")!!,
            )
            assertEquals(case.str("name"), case.getValue("expected"), encode(actual))
        }
    }

    @Test
    fun `every threads-list case matches the fixture`() {
        val cases = listFixture.list("cases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = SessionTree.partitionThreads(
                sessions = input.list("sessions").map(::session),
                sessionIssues = input.list("sessionIssues").map(::sessionIssue),
                issues = input.list("issues").map(::issue),
                messages = input.list("messages").map(::message),
                teamId = input.str("teamId"),
            )
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(actual.map(::encode)))
        }
    }

    @Test
    fun `every agents case matches the fixture`() {
        val cases = listFixture.list("agentCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = SessionTree.listAgents(input.list("sessions").map(::session), input.str("teamId"))
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(actual.map(::encode)))
        }
    }

    @Test
    fun `every guide pull request case matches the fixture`() {
        val cases = threadFixture.list("guidePullRequestCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val pulls = case.list("pullRequests").map { json ->
                PullRequestEntity(
                    id = json.str("id")!!,
                    number = json.getValue("number").jsonPrimitive.int,
                    state = json.str("state")!!,
                    createdAt = json.str("createdAt")!!,
                )
            }
            assertEquals(case.str("name"), case.str("expected"), SessionTree.guidePullRequest(pulls)?.id)
        }
    }

    @Test
    fun `every composer case matches the fixture`() {
        val cases = threadFixture.list("composerCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            assertEquals(
                case.str("name"),
                case.bool("expected"),
                SessionTree.composerYieldsToCard(live = input.bool("live"), pendingAsk = input.bool("pendingAsk")),
            )
        }
    }

    @Test
    fun `the elapsed ladder`() {
        assertEquals("37s", SessionTree.formatElapsed(37_000))
        assertEquals("2m 04s", SessionTree.formatElapsed(124_000))
        assertEquals("1h 03m", SessionTree.formatElapsed(3_780_000))
        assertEquals("0s", SessionTree.formatElapsed(-5_000))
    }

    // ── the embedded copy IS the fixtures' ──────────────────────────────────

    @Test
    fun `the card copy, icons, tones and test id equal the fixture`() {
        val states = cardFixture.getValue("copy").jsonObject.getValue("states").jsonObject
        val icons = cardFixture.getValue("icons").jsonObject
        val tones = cardFixture.getValue("tones").jsonObject
        assertEquals(cardFixture.getValue("copy").jsonObject.str("session"), SessionTree.CardCopy.SESSION)
        assertEquals(states.keys, SessionTree.SessionState.entries.map { it.wire }.toSet())
        for (state in SessionTree.SessionState.entries) {
            assertEquals(states.str(state.wire), SessionTree.CardCopy.STATES[state])
            assertEquals(icons.str(state.wire), SessionTree.CardCopy.ICONS[state])
            assertEquals(tones.str(state.wire), SessionTree.CardCopy.TONES[state])
        }
        assertEquals(
            cardFixture.getValue("testIds").jsonObject.str("card"),
            SessionTree.CardCopy.TEST_ID_CARD,
        )
    }

    @Test
    fun `the thread copy equals the fixture`() {
        val copy = threadFixture.getValue("copy").jsonObject
        assertEquals(copy.str("untitled"), SessionTree.ThreadCopy.UNTITLED)
        assertEquals(copy.str("placeholder"), SessionTree.ThreadCopy.PLACEHOLDER)
        assertEquals(copy.str("conversation"), SessionTree.ThreadCopy.CONVERSATION)
        assertEquals(copy.str("guide"), SessionTree.ThreadCopy.GUIDE)
        val events = copy.getValue("events").jsonObject
        assertEquals(events.keys, SessionTree.ThreadCopy.EVENTS.keys)
        for ((key, value) in events) assertEquals(value.jsonPrimitive.content, SessionTree.ThreadCopy.EVENTS[key])
        assertEquals(DomainContract.messageEventKindValues.toSet(), SessionTree.ThreadCopy.EVENTS.keys)
        assertEquals(threadFixture.getValue("titleMax").jsonPrimitive.int, SessionTree.ThreadCopy.TITLE_MAX)
        val testIds = threadFixture.getValue("testIds").jsonObject
        assertEquals(testIds.str("tabs"), SessionTree.ThreadCopy.TEST_ID_TABS)
        assertEquals(testIds.str("conversation"), SessionTree.ThreadCopy.TEST_ID_CONVERSATION)
        assertEquals(testIds.str("composer"), SessionTree.ThreadCopy.TEST_ID_COMPOSER)
        assertEquals(testIds.str("guideStack"), SessionTree.ThreadCopy.TEST_ID_GUIDE_STACK)
        assertEquals(testIds.str("guideMerge"), SessionTree.ThreadCopy.TEST_ID_GUIDE_MERGE)
        assertEquals(copy.str("stack"), SessionTree.ThreadCopy.STACK)
    }

    @Test
    fun `the list copy equals the fixture`() {
        val copy = listFixture.getValue("copy").jsonObject
        assertEquals(copy.str("title"), SessionTree.ListCopy.TITLE)
        assertEquals(copy.str("agents"), SessionTree.ListCopy.AGENTS)
        assertEquals(copy.str("empty"), SessionTree.ListCopy.EMPTY)
        val groups = copy.getValue("groups").jsonObject
        assertEquals(groups.keys, SessionTree.ThreadsGroupKey.entries.map { it.wire }.toSet())
        for (key in SessionTree.ThreadsGroupKey.entries) {
            assertEquals(groups.str(key.wire), SessionTree.ListCopy.GROUPS[key])
        }
        val defaults = listFixture.getValue("agentDefaults").jsonObject
        assertEquals(defaults.str("icon"), SessionTree.ListCopy.AGENT_DEFAULT_ICON)
        assertEquals(defaults.str("color"), SessionTree.ListCopy.AGENT_DEFAULT_COLOR)
        val testIds = listFixture.getValue("testIds").jsonObject
        assertEquals(testIds.str("list"), SessionTree.ListCopy.TEST_ID_LIST)
        assertEquals(testIds.str("row"), SessionTree.ListCopy.TEST_ID_ROW)
        assertEquals(testIds.str("agentRow"), SessionTree.ListCopy.TEST_ID_AGENT_ROW)
    }
}
