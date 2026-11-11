package com.exponential.app.domain

import com.exponential.app.data.db.PullRequestEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.booleanOrNull
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
 * EXP-1291: the session tree is fixture-locked ×4 — every case of
 * `session-card.json`, `session-chat.json` (cases + guidePullRequestCases)
 * and `chat-list.json` (cases +
 * agentCases + markCases + unreadCases + recentCases + treeStateCases) runs here (TS `session-tree.test.ts`, desktop
 * `domain::session_tree`, iOS `SessionTreeTests`). Outputs are re-encoded in
 * the fixture's own JSON shape and compared as a WHOLE, so a field the port
 * forgets fails too. The embedded copy is proved equal to the fixtures'.
 */
class SessionTreeTest {

    private fun fixture(name: String): JsonObject =
        Json.parseToJsonElement(contractFixtureJson(name)).jsonObject

    private val cardFixture by lazy { fixture("session-card.json") }
    private val chatFixture by lazy { fixture("session-chat.json") }
    private val listFixture by lazy { fixture("chat-list.json") }

    // ── decode (the fixture's input shapes) ─────────────────────────────────

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.bool(key: String): Boolean = getValue(key).jsonPrimitive.boolean

    private fun session(json: JsonObject) = SessionTree.Session(
        id = json.str("id")!!,
        teamId = json.str("teamId")!!,
        parentId = json.str("parentId"),
        rootId = json.str("rootId")!!,
        userId = json.str("userId")!!,
        status = json.str("status")!!,
        busy = json.bool("busy"),
        needsInput = json.bool("needsInput"),
        blocked = blocked(json),
        shared = json.bool("shared"),
        title = json.str("title"),
        guide = (json["guide"] as? JsonArray)?.map { entry ->
            val obj = entry.jsonObject
            SessionTree.GuideEntry(text = obj.str("text"), label = obj.str("label"))
        },
        startedAt = json.str("startedAt"),
        endedAt = json.str("endedAt"),
        createdAt = json.str("createdAt")!!,
        updatedAt = json.str("updatedAt")!!,
        deviceId = json.str("deviceId"),
        agentId = json.str("agentId"),
        startedByAgentId = json.str("startedByAgentId"),
        background = (json["background"] as? JsonPrimitive)?.booleanOrNull ?: false,
    )

    private fun blocked(json: JsonObject) = (json["blocked"] as? JsonObject)?.let {
        SessionTree.Blocked(
            it.str("window"),
            it.str("resetsAt"),
            it.str("kind"),
            (it["resume"] as? JsonPrimitive)?.booleanOrNull,
            it.str("resumeSince"),
        )
    }

    /** EXP-1374: a `bannerCases` session carries only the state's fields. */
    private fun bannerSession(json: JsonObject) = SessionTree.Session(
        id = "s1", teamId = "t1", parentId = null, rootId = "s1", userId = "u1",
        status = json.str("status")!!,
        busy = json.bool("busy"),
        needsInput = json.bool("needsInput"),
        blocked = blocked(json),
        shared = true, title = null, guide = null, startedAt = null, endedAt = null,
        createdAt = "2026-10-13T10:00:00.000Z", updatedAt = "2026-10-13T10:00:00.000Z",
        background = (json["background"] as? JsonPrimitive)?.booleanOrNull ?: false,
    )

    private fun encode(banner: SessionTree.SessionBanner?): JsonElement = banner?.let {
        buildJsonObject {
            put("id", it.id)
            put("icon", it.icon)
            put("label", it.label)
            put("text", it.text)
            put("tone", it.tone)
            put("action", nullable(it.action))
        }
    } ?: JsonNull

    private fun read(json: JsonObject) = SessionTree.SessionRead(
        sessionId = json.str("sessionId")!!,
        readAt = json.str("readAt")!!,
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
        parentId = json.str("parentId"),
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
        authorSessionId = json.str("authorSessionId"),
        locked = json.str("locked"),
        attachmentIds = (json["attachmentIds"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.content },
    )

    private fun pullRequest(json: JsonObject) = SessionTree.PullRequest(
        id = json.str("id")!!,
        number = json.getValue("number").jsonPrimitive.int,
        state = json.str("state"),
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

    private fun encode(tab: SessionTree.ChatTab): JsonObject = when (tab) {
        SessionTree.ChatTab.Conversation -> buildJsonObject { put("kind", "conversation") }
        is SessionTree.ChatTab.IssueTab -> buildJsonObject {
            put("kind", "issue")
            put("issueId", tab.issueId)
            put("identifier", tab.identifier)
            put(
                "nested",
                JsonArray(
                    tab.nested.map {
                        buildJsonObject {
                            put("issueId", it.issueId)
                            put("identifier", it.identifier)
                            put("depth", it.depth)
                        }
                    },
                ),
            )
        }
        SessionTree.ChatTab.Add -> buildJsonObject { put("kind", "add") }
        SessionTree.ChatTab.Results -> buildJsonObject { put("kind", "results") }
        SessionTree.ChatTab.Plan -> buildJsonObject { put("kind", "plan") }
        is SessionTree.ChatTab.SessionTab -> buildJsonObject {
            put("kind", "session")
            put("sessionId", tab.sessionId)
            put("name", tab.name)
            put("markState", tab.markState.wire)
            put(
                "nested",
                JsonArray(
                    tab.nested.map {
                        buildJsonObject {
                            put("sessionId", it.sessionId)
                            put("name", it.name)
                            put("markState", it.markState.wire)
                            put("depth", it.depth)
                        }
                    },
                ),
            )
        }
    }

    private fun encode(row: SessionTree.SessionChatRow): JsonObject = when (row) {
        is SessionTree.SessionChatRow.MessageRow -> buildJsonObject {
            put("type", "message")
            put("id", row.id)
            put("author", row.author)
            put("authorId", nullable(row.authorId))
            put("body", row.body)
            put("queued", row.queued)
            put("authorSessionId", nullable(row.authorSessionId))
            put("xref", encode(row.xref))
        }
        is SessionTree.SessionChatRow.SessionRow -> buildJsonObject {
            put("type", "session")
            put("id", row.id)
            put("card", encode(row.card))
        }
        is SessionTree.SessionChatRow.Report -> buildJsonObject {
            put("type", "report")
            put("id", row.id)
            put("sessionId", row.sessionId)
            put("text", row.text)
            put("summary", row.summary)
            put("xref", encode(row.xref))
        }
        is SessionTree.SessionChatRow.Suggestions -> buildJsonObject {
            put("type", "suggestions")
            put("id", row.id)
            put("sessionId", row.sessionId)
            put("header", row.header)
            put("startAll", row.startAll)
            put(
                "items",
                JsonArray(
                    row.items.map { item ->
                        buildJsonObject {
                            put("id", item.id)
                            put("title", item.title)
                            put("text", item.text)
                            put("state", item.state.wire)
                            put("card", item.card?.let(::encode) ?: JsonNull)
                            put("issueId", nullable(item.issueId))
                            put("identifier", nullable(item.identifier))
                        }
                    },
                ),
            )
        }
        is SessionTree.SessionChatRow.Question -> buildJsonObject {
            put("type", "question")
            put("id", row.id)
            put("sessionId", row.sessionId)
            put("question", row.question)
            put(
                "options",
                JsonArray(
                    row.options.map { option ->
                        encode(option)
                    },
                ),
            )
            put("custom", row.custom)
            put("multi", row.multi)
            put("surface", encode(row.surface))
            put("answer", nullable(row.answer))
            put("answers", row.answers?.let { list -> JsonArray(list.map(::JsonPrimitive)) } ?: JsonNull)
            put("surfaceData", row.surfaceData ?: JsonNull)
            put("answered", row.answered)
            row.dismissed?.let { put("dismissed", it) }
        }
        is SessionTree.SessionChatRow.Plan -> buildJsonObject {
            put("type", "plan")
            put("id", row.id)
            put("sessionId", row.sessionId)
            put("plan", row.plan)
            put(
                "questions",
                JsonArray(
                    row.questions.map { question ->
                        buildJsonObject {
                            put("id", question.id)
                            put("prompt", question.prompt)
                            put("multi", question.multi)
                            put("custom", question.custom)
                            put("options", JsonArray(question.options.map { encode(it) }))
                        }
                    },
                ),
            )
            put("surface", encode(row.surface))
            put("decision", nullable(row.decision))
            put("fresh", row.fresh)
            put(
                "answers",
                row.answers?.let { answers ->
                    JsonObject(answers.mapValues { (_, labels) -> JsonArray(labels.map(::JsonPrimitive)) })
                } ?: JsonNull,
            )
            put("feedback", nullable(row.feedback))
            put("surfaceData", row.surfaceData ?: JsonNull)
        }
        is SessionTree.SessionChatRow.Subagent -> buildJsonObject {
            put("type", "subagent")
            put("id", row.id)
            put("title", row.title)
            put("status", row.status)
        }
        is SessionTree.SessionChatRow.Picture -> buildJsonObject {
            put("type", "picture")
            put("id", row.id)
            put("attachmentId", row.attachmentId)
            put("width", nullable(row.width))
            put("height", nullable(row.height))
            put("caption", nullable(row.caption))
        }
        is SessionTree.SessionChatRow.Event -> buildJsonObject {
            put("type", "event")
            put("id", row.id)
            put("event", row.event)
            put("text", row.text)
            put("sessionId", nullable(row.sessionId))
        }
        is SessionTree.SessionChatRow.Locked -> buildJsonObject {
            put("type", "locked")
            put("id", row.id)
            put("reason", row.reason)
            put("author", row.author)
        }
        is SessionTree.SessionChatRow.Divider -> buildJsonObject {
            put("type", "divider")
            put("id", row.id)
            put("kind", row.kind)
            put("at", row.at)
        }
        is SessionTree.SessionChatRow.Surface -> buildJsonObject {
            put("type", "surface")
            put("id", row.id)
            put("sessionId", row.sessionId)
            put("title", nullable(row.title))
            put("surface", encode(row.surface))
        }
    }

    private fun encode(option: SessionTree.SessionChatRow.QuestionChoice): JsonObject = buildJsonObject {
        put("label", option.label)
        put("description", nullable(option.description))
        put("attachmentId", nullable(option.attachmentId))
    }

    @org.junit.Test
    fun `cardSurface keeps a 1_0 card's version`() {
        val root = kotlinx.serialization.json.buildJsonObject { put("id", kotlinx.serialization.json.JsonPrimitive("a")) }
        val v10 = kotlinx.serialization.json.buildJsonObject {
            put("version", kotlinx.serialization.json.JsonPrimitive("1.0"))
            put("root", root)
        }
        org.junit.Assert.assertEquals("1.0", SessionTree.cardSurface(v10)?.version)
        org.junit.Assert.assertNull(SessionTree.cardSurface(kotlinx.serialization.json.buildJsonObject { put("root", root) })?.version)
    }

    private fun encode(surface: CardSurface?): JsonElement = surface?.let {
        buildJsonObject {
            put("root", it.root)
            put("data", it.data ?: JsonNull)
        }
    } ?: JsonNull

    private fun encode(view: SessionTree.ChatView): JsonObject = buildJsonObject {
        put("rootId", view.rootId)
        put("title", view.title)
        put("private", view.private)
        put("focusId", nullable(view.focusId))
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
        put("liveSessionId", nullable(view.liveSessionId))
        put("busy", view.busy)
        put("needsYou", view.needsYou)
        put("mainSessionId", nullable(view.mainSessionId))
        put("origin", encode(view.origin))
    }

    private fun encode(xref: SessionTree.SessionXref?): JsonElement = xref?.let {
        buildJsonObject {
            put("kind", it.kind)
            put("sessionId", it.sessionId)
            put("name", it.name)
            put("agentId", nullable(it.agentId))
        }
    } ?: JsonNull

    private fun encode(group: SessionTree.ChatsListGroup): JsonObject = buildJsonObject {
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
                        put("busy", row.busy)
                        put("needsYou", row.needsYou)
                        put("activityAt", row.activityAt)
                        put("deviceId", nullable(row.deviceId))
                        put("depth", row.depth)
                        put("parentId", nullable(row.parentId))
                        put("rootId", row.rootId)
                        put("unread", row.unread)
                        // #EXP-1703: set only when true.
                        if (row.offline) put("offline", true)
                    }
                },
            ),
        )
    }

    private fun encode(agent: SessionTree.AgentRow): JsonObject = buildJsonObject {
        put("agentId", agent.agentId)
        put("teamId", agent.teamId)
        put("name", agent.name)
        put("icon", agent.icon)
        put("color", agent.color)
        put("busy", agent.busy)
        put("needsYou", agent.needsYou)
        put("assistant", agent.assistant)
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
                input.str("title")!!,
                input.str("now")!!,
            )
            assertEquals(case.str("name"), case.getValue("expected"), encode(actual))
        }
    }

    // EXP-1374 (TS `sessionBanner`): the Dock banner of an interrupted or
    // background session, whole-object compare.
    @Test
    fun `every session-card banner case matches the fixture`() {
        val cases = cardFixture.list("bannerCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            // #EXP-1703: a case may name the host device (`deviceName`).
            val actual = SessionTree.sessionBanner(bannerSession(case.getValue("session").jsonObject), case.str("deviceName"))
            assertEquals(case.str("name"), case.getValue("expected"), encode(actual))
        }
    }

    /** EXP-1459: sealed-content.json `placeholder.rowCases` over session-chat's first input. */
    @Test
    fun `a locked message is the locked row (sealed-content placeholder rowCases)`() {
        val cases = fixture("sealed-content.json").getValue("placeholder").jsonObject.list("rowCases")
        assertTrue(cases.isNotEmpty())
        val input = chatFixture.list("cases").first().getValue("input").jsonObject
        val root = session(input.getValue("root").jsonObject)
        for (case in cases) {
            val m = case.getValue("message").jsonObject
            val json = kotlinx.serialization.json.JsonObject(
                mapOf(
                    "sessionId" to kotlinx.serialization.json.JsonPrimitive(root.id),
                    "authorId" to kotlinx.serialization.json.JsonNull,
                    "body" to kotlinx.serialization.json.JsonPrimitive(""),
                    "card" to kotlinx.serialization.json.JsonNull,
                    "answeredAt" to kotlinx.serialization.json.JsonNull,
                    "consumedAt" to kotlinx.serialization.json.JsonPrimitive("2026-10-13T10:00:00.000Z"),
                    "createdAt" to kotlinx.serialization.json.JsonPrimitive("2026-10-13T10:05:00.000Z"),
                ) + m,
            )
            val view = SessionTree.buildSessionChat(
                root = root,
                sessions = input.list("sessions").map(::session),
                sessionIssues = input.list("sessionIssues").map(::sessionIssue),
                issues = input.list("issues").map(::issue),
                messages = listOf(message(json)),
                pullRequests = input.list("pullRequests").map(::pullRequest),
                now = input.str("now")!!,
            )
            val rows = view.rows.filter { it.id == m.str("id") }.map(::encode)
            assertEquals(case.str("name"), listOf(case.getValue("expected")), rows)
        }
    }

    @Test
    fun `every session-chat case matches the fixture`() {
        val cases = chatFixture.list("cases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = SessionTree.buildSessionChat(
                root = session(input.getValue("root").jsonObject),
                sessions = input.list("sessions").map(::session),
                sessionIssues = input.list("sessionIssues").map(::sessionIssue),
                issues = input.list("issues").map(::issue),
                messages = input.list("messages").map(::message),
                pullRequests = input.list("pullRequests").map(::pullRequest),
                now = input.str("now")!!,
                focus = (case["options"] as? JsonObject)?.str("focus"),
            )
            assertEquals(case.str("name"), case.getValue("expected"), encode(actual))
        }
    }

    @Test
    fun `every chats-list case matches the fixture`() {
        val cases = listFixture.list("cases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = SessionTree.partitionChats(
                sessions = input.list("sessions").map(::session),
                sessionIssues = input.list("sessionIssues").map(::sessionIssue),
                issues = input.list("issues").map(::issue),
                messages = input.list("messages").map(::message),
                teamId = input.str("teamId"),
                reads = input.list("reads").map(::read),
                now = input.str("now"),
            )
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(actual.map(::encode)))
        }
    }

    @Test
    fun `every recent window case matches the fixture`() {
        val cases = listFixture.list("recentCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val groups = SessionTree.partitionChats(
                sessions = input.list("sessions").map(::session),
                sessionIssues = input.list("sessionIssues").map(::sessionIssue),
                issues = input.list("issues").map(::issue),
                messages = input.list("messages").map(::message),
                teamId = input.str("teamId"),
                reads = input.list("reads").map(::read),
                now = input.str("now"),
            )
            val recent = groups.firstOrNull { it.key == SessionTree.ChatsGroupKey.Recent }
            val actual = recent?.rows.orEmpty().filter { it.depth == 0 }.map { it.sessionId }
            val expected = case.getValue("expected").jsonArray.map { it.jsonPrimitive.content }
            assertEquals(case.str("name"), expected, actual)
        }
    }

    @Test
    fun `every tree state case matches the fixture`() {
        val cases = listFixture.list("treeStateCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = SessionTree.treeState(
                tree = input.list("tree").map(::session),
                pullRequests = input.list("pullRequests").map(::pullRequest),
                now = input.str("now")!!,
            )
            assertEquals(case.str("name"), case.str("expected"), actual.wire)
        }
    }

    // #EXP-1703 (TS `treeOffline`): a tree on a person only because its device stayed away.
    @Test
    fun `every tree offline case matches the fixture`() {
        val cases = listFixture.list("offlineCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val tree = case.getValue("tree").jsonArray.map { session(it.jsonObject) }
            assertEquals(case.str("name"), case.bool("expected"), SessionTree.treeOffline(tree))
        }
    }

    @Test
    fun `every nav dot case matches the fixture`() {
        val cases = listFixture.list("unreadCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val groups = case.list("groups").map { group ->
                SessionTree.ChatsListGroup(
                    key = SessionTree.ChatsGroupKey.entries.first { it.wire == group.str("key") },
                    title = group.str("title")!!,
                    rows = group.list("rows").map { row ->
                        SessionTree.ChatsListRow(
                            sessionId = row.str("sessionId")!!,
                            teamId = row.str("teamId")!!,
                            title = row.str("title")!!,
                            busy = row.bool("busy"),
                            needsYou = row.bool("needsYou"),
                            activityAt = row.str("activityAt")!!,
                            deviceId = row.str("deviceId"),
                            depth = row.getValue("depth").jsonPrimitive.int,
                            parentId = row.str("parentId"),
                            rootId = row.str("rootId")!!,
                            unread = row.bool("unread"),
                        )
                    },
                )
            }
            assertEquals(case.str("name"), case.bool("expected"), SessionTree.chatsUnread(groups))
        }
    }

    @Test
    fun `every agents case matches the fixture`() {
        val cases = listFixture.list("agentCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val agents = input.list("agents").map {
                SessionTree.AgentDefinition(
                    id = it.str("id")!!,
                    teamId = it.str("teamId")!!,
                    name = it.str("name")!!,
                    icon = it.str("icon"),
                    color = it.str("color"),
                    sortOrder = it.getValue("sortOrder").jsonPrimitive.double,
                    role = it.str("role"),
                    ownerId = it.str("ownerId"),
                )
            }
            val actual = SessionTree.listAgents(
                agents,
                input.list("sessions").map(::session),
                input.str("teamId"),
                input.str("me"),
            )
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(actual.map(::encode)))
        }
    }

    @Test
    fun `every row mark case matches the fixture`() {
        val cases = listFixture.list("markCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = SessionTree.chatRowMarkState(input.bool("needsYou"), input.bool("busy"))
            assertEquals(case.str("name"), case.str("expected"), SessionTree.chatRowMarkWire(actual))
        }
    }

    @Test
    fun `every chat mode case matches the fixture (S-3)`() {
        val cases = chatFixture.list("chatModeCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val root = case.getValue("root").jsonObject
            assertEquals(case.str("name"), case.str("expected"), SessionTree.chatMode(root.str("agentId")).wire)
        }
    }

    @Test
    fun `every guide pull request case matches the fixture`() {
        val cases = chatFixture.list("guidePullRequestCases")
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
    fun `every replies case matches the fixture`() {
        val cases = chatFixture.list("repliesCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val actual = SessionTree.composerReplies(
                sessionId = input.str("sessionId")!!,
                messages = input.list("messages").map(::message),
            )?.let { replies ->
                buildJsonObject {
                    put("messageId", replies.messageId)
                    put(
                        "items",
                        JsonArray(replies.items.map { buildJsonObject { put("id", it.id); put("text", it.text) } }),
                    )
                }
            } ?: JsonNull
            assertEquals(case.str("name"), case.getValue("expected"), actual)
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
        assertTrue("copy.session is gone (EXP-1341)", "session" !in cardFixture.getValue("copy").jsonObject)
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
        val blockedKinds = cardFixture.getValue("copy").jsonObject.getValue("blockedKinds").jsonObject
        assertEquals(blockedKinds.keys, SessionTree.CardCopy.BLOCKED_KINDS.keys)
        // S-3: the interruptions (device_offline, restarted) have no word.
        assertTrue(DomainContract.codingSessionBlockedKinds.toSet().containsAll(SessionTree.CardCopy.BLOCKED_KINDS.keys))
        for ((key, value) in blockedKinds) {
            assertEquals(value.jsonPrimitive.content, SessionTree.CardCopy.BLOCKED_KINDS[key])
        }
        assertEquals(setOf("states", "blockedKinds"), cardFixture.getValue("copy").jsonObject.keys)
        val banner = cardFixture.getValue("banner").jsonObject
        assertEquals(setOf("background", "queued", "offline"), banner.keys)
        // #EXP-1703: a device that stayed away, with Move.
        assertEquals(banner.str("offline"), SessionTree.CardCopy.BANNER_OFFLINE)
        assertEquals(cardFixture.str("bannerDevice"), SessionTree.CardCopy.BANNER_DEVICE)
        val bannerActions = cardFixture.getValue("bannerActions").jsonObject
        assertEquals(bannerActions.keys, SessionTree.CardCopy.BANNER_ACTIONS.keys)
        for ((key, value) in bannerActions) assertEquals(value.jsonPrimitive.content, SessionTree.CardCopy.BANNER_ACTIONS[key])
        assertEquals(banner.str("background"), SessionTree.CardCopy.BANNER_BACKGROUND)
        // #EXP-1405: a queued row waits for its device.
        assertEquals(banner.str("queued"), SessionTree.CardCopy.BANNER_QUEUED)
        val bannerIcons = cardFixture.getValue("bannerIcons").jsonObject
        assertEquals(bannerIcons.keys, SessionTree.CardCopy.BANNER_ICONS.keys)
        for ((key, value) in bannerIcons) assertEquals(value.jsonPrimitive.content, SessionTree.CardCopy.BANNER_ICONS[key])
        val bannerLabels = cardFixture.getValue("bannerLabels").jsonObject
        assertEquals(bannerLabels.keys, SessionTree.CardCopy.BANNER_LABELS.keys)
        for ((key, value) in bannerLabels) assertEquals(value.jsonPrimitive.content, SessionTree.CardCopy.BANNER_LABELS[key])
    }

    @Test
    fun `the chat copy equals the fixture`() {
        val copy = chatFixture.getValue("copy").jsonObject
        assertEquals(copy.str("untitled"), SessionTree.ChatCopy.UNTITLED)
        assertEquals(copy.str("placeholder"), SessionTree.ChatCopy.PLACEHOLDER)
        assertEquals(copy.str("conversation"), SessionTree.ChatCopy.CONVERSATION)
        assertEquals(copy.str("guide"), SessionTree.ChatCopy.GUIDE)
        val events = copy.getValue("events").jsonObject
        // #EXP-1772: `trigger_schedule` is a schedule firing's marker, no event kind.
        assertEquals(events.str("trigger_schedule"), SessionTree.ChatCopy.TRIGGER_SCHEDULE)
        assertEquals(events.keys - "trigger_schedule", SessionTree.ChatCopy.EVENTS.keys)
        for ((key, value) in events) {
            if (key != "trigger_schedule") assertEquals(value.jsonPrimitive.content, SessionTree.ChatCopy.EVENTS[key])
        }
        // S-3: device_offline and session_interrupted draw nothing.
        assertEquals(
            DomainContract.messageEventKindValues.toSet() - setOf("device_offline", "session_interrupted"),
            SessionTree.ChatCopy.EVENTS.keys,
        )
        val testIds = chatFixture.getValue("testIds").jsonObject
        assertEquals(testIds.str("tabs"), SessionTree.ChatCopy.TEST_ID_TABS)
        assertEquals(testIds.str("conversation"), SessionTree.ChatCopy.TEST_ID_CONVERSATION)
        assertEquals(testIds.str("composer"), SessionTree.ChatCopy.TEST_ID_COMPOSER)
        assertEquals(testIds.str("guideStack"), SessionTree.ChatCopy.TEST_ID_GUIDE_STACK)
        assertEquals(testIds.str("guideMerge"), SessionTree.ChatCopy.TEST_ID_GUIDE_MERGE)
        assertEquals(testIds.str("guideMenu"), SessionTree.ChatCopy.TEST_ID_GUIDE_MENU)
        assertEquals(copy.str("stack"), SessionTree.ChatCopy.STACK)
        assertEquals(copy.str("report"), SessionTree.ChatCopy.REPORT)
        val xref = copy.getValue("xref").jsonObject
        assertEquals(xref.keys, SessionTree.ChatCopy.XREF.keys)
        for ((key, value) in xref) assertEquals(value.jsonPrimitive.content, SessionTree.ChatCopy.XREF[key])
        assertEquals(testIds.str("xref"), SessionTree.ChatCopy.TEST_ID_XREF)
        val suggestions = copy.getValue("suggestions").jsonObject
        val embedded = mapOf(
            "one" to SessionTree.SuggestionsCopy.ONE,
            "many" to SessionTree.SuggestionsCopy.MANY,
            "started" to SessionTree.SuggestionsCopy.STARTED,
            "startAll" to SessionTree.SuggestionsCopy.START_ALL,
            "start" to SessionTree.SuggestionsCopy.START,
            "here" to SessionTree.SuggestionsCopy.HERE,
            "options" to SessionTree.SuggestionsCopy.OPTIONS,
            "startOn" to SessionTree.SuggestionsCopy.START_ON,
            "startWith" to SessionTree.SuggestionsCopy.START_WITH,
            "file" to SessionTree.SuggestionsCopy.FILE,
            "dismiss" to SessionTree.SuggestionsCopy.DISMISS,
            "sentHere" to SessionTree.SuggestionsCopy.SENT_HERE,
            "filed" to SessionTree.SuggestionsCopy.FILED,
        )
        assertEquals(suggestions.keys, embedded.keys)
        for ((key, value) in embedded) assertEquals(suggestions.str(key), value)
        val replies = copy.getValue("replies").jsonObject
        assertEquals(setOf("label", "dismiss"), replies.keys)
        assertEquals(replies.str("label"), SessionTree.RepliesCopy.LABEL)
        assertEquals(replies.str("dismiss"), SessionTree.RepliesCopy.DISMISS)
        assertEquals(testIds.str("replies"), SessionTree.RepliesCopy.TEST_ID)
    }

    @Test
    fun `the list copy equals the fixture`() {
        val copy = listFixture.getValue("copy").jsonObject
        assertEquals(copy.str("title"), SessionTree.ListCopy.TITLE)
        assertEquals(copy.str("agents"), SessionTree.ListCopy.AGENTS)
        assertEquals(copy.str("empty"), SessionTree.ListCopy.EMPTY)
        assertEquals(copy.str("rail"), SessionTree.ListCopy.RAIL)
        val words = copy.getValue("words").jsonObject
        // EXP-1339: `merged` carries no text (it only turns the lead violet).
        assertEquals(words.keys, SessionTree.ChatRowWord.entries.filter { it.text != null }.map { it.wire }.toSet())
        assertEquals(null, SessionTree.ChatRowWord.Merged.text)
        for (word in SessionTree.ChatRowWord.entries) assertEquals(words.str(word.wire), word.text)
        val groups = copy.getValue("groups").jsonObject
        assertEquals(groups.keys, SessionTree.ChatsGroupKey.entries.map { it.wire }.toSet())
        for (key in SessionTree.ChatsGroupKey.entries) {
            assertEquals(groups.str(key.wire), SessionTree.ListCopy.GROUPS[key])
        }
        val defaults = listFixture.getValue("agentDefaults").jsonObject
        assertEquals(defaults.str("icon"), SessionTree.ListCopy.AGENT_DEFAULT_ICON)
        assertEquals(defaults.str("color"), SessionTree.ListCopy.AGENT_DEFAULT_COLOR)
        val testIds = listFixture.getValue("testIds").jsonObject
        assertEquals(testIds.str("list"), SessionTree.ListCopy.TEST_ID_LIST)
        assertEquals(testIds.str("row"), SessionTree.ListCopy.TEST_ID_ROW)
        assertEquals(testIds.str("agentRow"), SessionTree.ListCopy.TEST_ID_AGENT_ROW)
        val recent = listFixture.getValue("recent").jsonObject
        assertEquals(recent.getValue("hours").jsonPrimitive.int, SessionTree.ListCopy.RECENT_HOURS)
        assertEquals(recent.getValue("max").jsonPrimitive.int, SessionTree.ListCopy.RECENT_MAX)
        assertTrue("treeState.staleHours is gone (S-3)", "treeState" !in listFixture)
        assertEquals(listFixture.getValue("recentChildMinutes").jsonPrimitive.int, SessionTree.ListCopy.RECENT_CHILD_MINUTES)
        assertTrue("copy.earlier is gone (#EXP-1461)", "earlier" !in copy)
    }

    // NEW-1 / S-3 (TS `sessionKindWord`, session-card.json `copy.blockedKinds`):
    // a flagged session that needs you names WHY; an unknown kind = "Needs you";
    // an interruption (device_offline, restarted) is invisible (no needsInput).
    private fun blockedSession(kind: String?) = SessionTree.Session(
        id = "s1", teamId = "t1", parentId = null, rootId = "s1", userId = "u1",
        status = "running", busy = false, needsInput = kind != "device_offline" && kind != "restarted",
        blocked = SessionTree.Blocked(kind = kind),
        shared = true, title = null, guide = null, startedAt = null, endedAt = null,
        createdAt = "2026-10-21T10:00:00.000Z", updatedAt = "2026-10-21T10:00:00.000Z",
    )

    private fun blockedWord(kind: String?): String {
        val session = blockedSession(kind)
        return SessionTree.sessionCard(session, "S", "2026-10-21T10:05:00.000Z").kindWord
    }

    @Test
    fun blockedKindUsageWallReadsUsageLimit() = assertEquals("Usage limit", blockedWord("rate_limit"))

    @Test
    fun blockedKindMissingReadsNeedsYou() = assertEquals("Needs you", blockedWord(null))

    @Test
    // S-3: an offline device is invisible: the session reads idle.
    fun blockedKindDeviceOfflineReadsIdle() = assertEquals("Idle", blockedWord("device_offline"))

    @Test
    fun blockedKindRestartedReadsIdle() = assertEquals("Idle", blockedWord("restarted"))

    @Test
    fun blockedKindUnclaimedReadsItsWord() = assertEquals("Not picked up", blockedWord("unclaimed"))

    @Test
    fun blockedKindStuckReadsItsWord() = assertEquals("Stuck", blockedWord("stuck"))

    @Test
    // EXP-1536: a failed start reads as failed, never Idle.
    fun blockedKindStartFailedReadsItsWord() = assertEquals("Couldn't start", blockedWord("start_failed"))

    @Test
    fun blockedKindUnknownReadsNeedsYou() = assertEquals("Needs you", blockedWord("quota"))

    // #EXP-1710 (merge-train.json `copy.events`): the train's `pr_merge` line
    // reads by its card's `mergeOutcome`; none (or an unknown one) keeps
    // `Auto-merge of PR {pr}`.
    @Test
    fun `the pr_merge line reads by mergeOutcome`() {
        val root = SessionTree.Session(
            id = "r1", teamId = "t1", parentId = null, rootId = "r1", userId = "me", status = "running", busy = false,
            needsInput = false, blocked = null, shared = true, title = "Root", guide = null, startedAt = null, endedAt = null,
            createdAt = "2027-01-15T07:00:00.000Z", updatedAt = "2027-01-15T07:00:00.000Z",
        )
        fun line(outcome: String?): String? {
            val card = Json.parseToJsonElement(
                if (outcome == null) """{"kind":"event","event":"pr_merge","pullRequestId":"pr1"}"""
                else """{"kind":"event","event":"pr_merge","pullRequestId":"pr1","mergeOutcome":"$outcome"}""",
            ).jsonObject
            val message = SessionTree.Message(
                id = "m1", sessionId = "r1", authorKind = "user", authorId = "me", body = "", card = card,
                answeredAt = null, consumedAt = null, createdAt = "2027-01-15T08:00:00.000Z", authorSessionId = null,
            )
            val view = SessionTree.buildSessionChat(
                root = root, sessions = listOf(root), sessionIssues = emptyList(), issues = emptyList(),
                messages = listOf(message), pullRequests = listOf(SessionTree.PullRequest("pr1", 12, "open")),
                now = "2027-01-15T09:00:00.000Z",
            )
            return view.rows.filterIsInstance<SessionTree.SessionChatRow.Event>().singleOrNull()?.text
        }
        assertEquals("Merged PR #12", line("merged"))
        assertEquals("Conflict on PR #12", line("conflict"))
        assertEquals("Checks failing on PR #12", line("failing"))
        assertEquals("Auto-merge stopped on PR #12", line("held"))
        assertEquals("Auto-merge of PR #12", line(null))
        assertEquals("Auto-merge of PR #12", line("later"))
    }
}
