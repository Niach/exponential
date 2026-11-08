package com.exponential.app.ui.chats

import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.domain.Chats
import com.exponential.app.domain.NeedsYou
import com.exponential.app.domain.ResultsTab
import com.exponential.app.domain.SessionScreen
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.contractFixtureJson
import com.exponential.app.navigation.Routes
import com.exponential.app.ui.styleguide.NEEDS_YOU_SPECIMEN_ROWS
import com.exponential.app.ui.styleguide.NEEDS_YOU_SPECIMEN_STUCK
import com.exponential.app.ui.styleguide.RESULTS_SPECIMEN
import com.exponential.app.ui.chat.resultPictureSet
import com.exponential.app.ui.chat.resultThumbWidth
import com.exponential.app.ui.chats.chatDisplayedRows
import com.exponential.app.ui.chats.needsYouFold
import com.exponential.app.ui.chats.openerOfflineSince
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test

/** #FEED-191 the Android hosts of needs-you.json + results-tab.json: where a row
 *  opens, the older tab links, the specimens = the fixtures (their `specimen`). */
class NeedsYouResultsUiTest {

    private fun row(opens: NeedsYou.Opens) = NeedsYou.Row(
        messageId = "m1", sessionId = "s1", rootId = "r1", kind = NeedsYou.Kind.Question, opens = opens,
        title = "t", chat = "c", meta = "", age = "1m", createdAt = "2027-01-15T07:36:00.000Z",
    )

    @Test
    fun `a row opens by opens`() {
        assertEquals("chat/r1?tab=plan&plan=m1", Routes.needsYou(row(NeedsYou.Opens.Plan)))
        assertEquals("chat/r1?card=m1", Routes.needsYou(row(NeedsYou.Opens.Card)))
        assertEquals("chat/r1?tab=results", Routes.needsYou(row(NeedsYou.Opens.Results)))
        assertEquals("chat/r1", Routes.needsYou(row(NeedsYou.Opens.Chat)))
    }

    @Test
    fun `an older stories or guide link lands on Results`() {
        // #EXP-1732: THE session screen's `legacyTabs` (session-screen.json).
        fun active(tab: String?) = SessionScreen.screen(SessionScreen.Kind.Session, false, tab, null, SessionScreen.Counts()).active
        assertEquals(SessionScreen.Tab.Results, active("stories"))
        assertEquals(SessionScreen.Tab.Results, active("guide"))
        assertEquals(SessionScreen.Tab.Results, active("results"))
        assertEquals(SessionScreen.Tab.Chat, active("plan"))
        assertEquals(SessionScreen.Tab.Chat, active(null))
    }

    @Test
    fun `a thumbnail is as wide as its aspect, capped, 4 by 3 unknown`() {
        fun picture(width: Double?, height: Double?) = ResultsTab.Picture("p", "a", width, height, null)
        assertEquals(320, resultThumbWidth(picture(1600.0, 1000.0)))
        assertEquals(111, resultThumbWidth(picture(1179.0, 2556.0)))
        assertEquals(320, resultThumbWidth(picture(null, null)))
        assertEquals(240, resultThumbWidth(picture(1000.0, 1000.0)))
    }

    @Test
    fun `a picture opens THE lightbox over that result's pictures at its index`() {
        val set = resultPictureSet(RESULTS_SPECIMEN[0], 1)
        assertEquals(1, set.start)
        assertEquals(listOf("/api/attachments/att-w", "/api/attachments/att-i"), set.items.map { it.src })
    }

    private fun nullable(value: String?): JsonElement = value?.let(::JsonPrimitive) ?: JsonNull

    @Test
    fun `the needs-you specimen is cases 1 expected`() {
        val fixture = Json.parseToJsonElement(contractFixtureJson("needs-you.json")).jsonObject
        val expected = fixture.getValue("cases").jsonArray[1].jsonObject.getValue("expected")
        val actual = JsonArray(
            NEEDS_YOU_SPECIMEN_ROWS.map { row ->
                buildJsonObject {
                    put("messageId", row.messageId)
                    put("sessionId", row.sessionId)
                    put("rootId", row.rootId)
                    put("kind", row.kind.wire)
                    put("opens", row.opens.wire)
                    put("title", row.title)
                    put("chat", row.chat)
                    put("meta", row.meta)
                    put("age", row.age)
                    put("createdAt", row.createdAt)
                }
            },
        )
        assertEquals(expected, actual)
    }

    // #EXP-1710: the specimen's ONE stuck row = merge-train.json `stuckRow`
    // cases[0] (drawn under cases[1]'s rows).
    @Test
    fun `the needs-you specimen's stuck row is merge-train stuckRow cases 0`() {
        val fixture = Json.parseToJsonElement(contractFixtureJson("merge-train.json")).jsonObject
        val expected = fixture.getValue("stuckRow").jsonArray[0].jsonObject.getValue("expected").jsonObject
        val row = NEEDS_YOU_SPECIMEN_STUCK
        assertEquals(expected.getValue("title").jsonPrimitive.content, row.title)
        assertEquals(expected.getValue("meta").jsonPrimitive.content, row.meta)
        assertEquals(expected.getValue("actions").jsonArray.map { it.jsonPrimitive.content }, row.actions.map { it.wire })
        assertEquals(NeedsYou.Kind.Pr, row.kind)
        assertEquals(NeedsYou.Opens.Results, row.opens)
    }

    @Test
    fun `the results-tab specimen is cases 1 expected`() {
        val fixture = Json.parseToJsonElement(contractFixtureJson("results-tab.json")).jsonObject
        val expected = fixture.getValue("cases").jsonArray[1].jsonObject.getValue("expected")
        val actual = JsonArray(
            RESULTS_SPECIMEN.map { result ->
                buildJsonObject {
                    put("id", result.id)
                    put("sessionId", result.sessionId)
                    put("sessionName", result.sessionName)
                    put("createdAt", result.createdAt)
                    put("age", result.age)
                    put("title", result.title)
                    put("description", nullable(result.description))
                    put(
                        "pictures",
                        JsonArray(
                            result.pictures.map { picture ->
                                buildJsonObject {
                                    put("label", picture.label)
                                    put("attachmentId", picture.attachmentId)
                                    put("width", picture.width?.toLong())
                                    put("height", picture.height?.toLong())
                                    put("caption", nullable(picture.caption))
                                }
                            },
                        ),
                    )
                    put("surface", result.surface)
                }
            },
        )
        assertEquals(expected, actual)
    }

    private fun chat(id: String, group: SessionTree.ChatsGroupKey, unread: Int = 0) = Chats.ChatRow(
        sessionId = id, teamId = "t", title = id, deviceId = null, group = group, lead = Chats.ChatLead.Waiting,
        preview = Chats.ChatPreview("text", ""), activityAt = "", time = "", unreadCount = unread,
        needsYou = false, busy = false, word = null,
    )

    @Test
    fun `selection order is the displayed order, the Archived page flat`() {
        val rows = listOf(
            chat("plain", SessionTree.ChatsGroupKey.Recent),
            chat("unread", SessionTree.ChatsGroupKey.Recent, unread = 1),
            chat("working", SessionTree.ChatsGroupKey.Running),
            chat("waiting", SessionTree.ChatsGroupKey.NeedsYou),
        )
        assertEquals(
            listOf("waiting", "working", "unread", "plain"),
            chatDisplayedRows(rows, Chats.ChatListView.List).map { it.sessionId },
        )
        assertEquals(
            listOf("plain", "unread", "working", "waiting"),
            chatDisplayedRows(rows, Chats.ChatListView.Archived).map { it.sessionId },
        )
    }

    private fun session(id: String, team: String = "t1", agentId: String? = null) = SessionTree.Session(
        id = id, teamId = team, parentId = null, rootId = id, userId = "me", status = "running", busy = false,
        needsInput = false, blocked = null, shared = true, title = id, guide = null, startedAt = null, endedAt = null,
        createdAt = "2027-01-15T07:00:00.000Z", updatedAt = "2027-01-15T07:00:00.000Z", agentId = agentId,
    )

    private fun waiting(id: String, sessionId: String, kind: String) = SessionTree.Message(
        id = id, sessionId = sessionId, authorKind = "agent", authorId = null, body = "",
        card = Json.parseToJsonElement("""{"kind":"$kind","question":"Which?","options":["A"],"plan":"# P"}""").jsonObject,
        answeredAt = null, consumedAt = null, createdAt = "2027-01-15T07:00:00.000Z", authorSessionId = sessionId,
    )

    @Test
    fun `the fold reads the active team, names an agent conversation by its agent, counts ready PRs`() {
        val rows = needsYouFold(
            sessions = listOf(session("s1"), session("elsewhere", team = "t2"), session("agent", agentId = "a1")),
            links = emptyList(),
            issues = emptyList(),
            messages = listOf(
                waiting("q1", "s1", "question"),
                waiting("q2", "elsewhere", "question"),
                waiting("q3", "agent", "question"),
            ),
            definitions = listOf(SessionTree.AgentDefinition("a1", "t1", "Triage", null, null, 0.0)),
            pullRequests = listOf(
                PullRequestEntity(
                    id = "pr1", teamId = "t1", rootSessionId = "s1", number = 7, title = "Ready",
                    checksState = "success", createdAt = "2027-01-15T07:30:00.000Z",
                ),
                PullRequestEntity(
                    id = "pr2", teamId = "t2", rootSessionId = "elsewhere", number = 8, title = "Other team",
                    checksState = "success", createdAt = "2027-01-15T07:30:00.000Z",
                ),
            ),
            team = "t1",
            me = "me",
            now = 1_800_000_000_000L,
            // #EXP-1710: ready PR rows hold only in `approval` mode.
            mergeMode = "approval",
        )
        assertEquals(listOf("q3", "q1", "pr1"), rows.map { it.messageId })
        assertEquals(NeedsYou.Kind.Pr, rows[2].kind)
        assertEquals("#7", rows[2].meta)
        assertEquals("Triage", rows[0].chat)
    }

    // #EXP-1710 (merge-train.json NEEDS YOU): `auto` mode drops the ready
    // rows; a PR held by the limits, or a conflict whose opener's device has
    // been offline 2 h+, is the stuck row with its actions (owners: Merge now).
    @Test
    fun `auto mode keeps only the stuck PRs, off the opener's device presence`() {
        val now = com.exponential.app.domain.WireTimestamps.parseEpochMs("2027-01-15T10:00:00.000Z")!!
        val opener = session("s1").copy(deviceId = "dev-1")
        val child = session("c1").copy(rootId = "s1", parentId = "s1", deviceId = "dev-1")
        val ready = PullRequestEntity(
            id = "ready", teamId = "t1", rootSessionId = "s1", sessionId = "c1", number = 6, title = "Ready",
            checksState = "success", createdAt = "2027-01-15T07:30:00.000Z", updatedAt = "2027-01-15T08:30:00.000Z",
        )
        val held = ready.copy(id = "held", number = 7, mergeState = "held", mergeHold = "bounces")
        val conflict = ready.copy(id = "conflict", number = 8, mergeState = "conflict", checksState = "failure")
        val devices = listOf(
            com.exponential.app.data.db.DeviceEntity(
                id = "d1", userId = "me", deviceId = "dev-1", lastSeenAt = "2027-01-15T07:59:00.000Z",
            ),
        )
        fun fold(mode: String, owner: Boolean, devs: List<com.exponential.app.data.db.DeviceEntity>) = needsYouFold(
            sessions = listOf(opener, child), links = emptyList(), issues = emptyList(), messages = emptyList(),
            definitions = emptyList(), pullRequests = listOf(ready, held, conflict), team = "t1", me = "me", now = now,
            mergeMode = mode, owner = owner, devices = devs,
        )
        val auto = fold("auto", owner = true, devs = devices)
        assertEquals(setOf("held", "conflict"), auto.map { it.messageId }.toSet())
        val stuck = auto.first { it.messageId == "held" }
        assertEquals("Auto-merge stuck on #7", stuck.title)
        assertEquals("3 bounces", stuck.meta)
        assertEquals(listOf("retry", "merge-now", "close-pr"), stuck.actions.map { it.wire })
        assertEquals("Device offline", auto.first { it.messageId == "conflict" }.meta)
        // merge-train.json `offlineSince`: no device row synced = never
        // online, offline since the PR's updatedAt (08:30: 1.5 h, waits); a
        // member gets no Merge now.
        val noDevice = fold("auto", owner = false, devs = emptyList())
        assertEquals(listOf("held"), noDevice.map { it.messageId })
        assertEquals(listOf("retry", "close-pr"), noDevice.single().actions.map { it.wire })
        // An online opener device: the agent fixes it.
        val online = listOf(devices.single().copy(lastSeenAt = "2027-01-15T09:59:30.000Z"))
        assertEquals(listOf("held"), fold("auto", owner = true, devs = online).map { it.messageId })
        // `approval` keeps the ready row beside the stuck ones.
        assertEquals(setOf("ready", "held", "conflict"), fold("approval", owner = true, devs = devices).map { it.messageId }.toSet())
        // #EXP-1710: only the `approval` ready row's press asks Needs
        // approval; stuck rows keep opening results.
        assertEquals(
            setOf("ready"),
            fold("approval", owner = true, devs = devices).filter { it.needsApproval }.map { it.messageId }.toSet(),
        )
    }

    // #EXP-1710 (merge-train.json `offlineSince`): the opener = the PR's
    // `sessionId` session, else its root; its device's presence decides; an
    // unknown last-seen = the PR's updatedAt, never online.
    @Test
    fun `openerOfflineSince routes through the domain offlineSince`() {
        val now = com.exponential.app.domain.WireTimestamps.parseEpochMs("2027-01-15T10:00:00.000Z")!!
        val root = session("s1").copy(deviceId = "dev-root")
        val child = session("c1").copy(rootId = "s1", parentId = "s1", deviceId = "dev-1")
        val sessions = listOf(root, child).associateBy { it.id }
        val pr = PullRequestEntity(
            id = "pr", teamId = "t1", rootSessionId = "s1", sessionId = "c1", number = 8,
            createdAt = "2027-01-15T07:00:00.000Z", updatedAt = "2027-01-15T08:00:00.000Z",
        )
        fun device(id: String, seen: String?) = com.exponential.app.data.db.DeviceEntity(
            id = "row-$id", userId = "me", deviceId = id, lastSeenAt = seen,
        )
        fun since(p: PullRequestEntity, vararg devs: com.exponential.app.data.db.DeviceEntity) =
            openerOfflineSince(p, sessions, devs.associateBy { it.deviceId }, now)
        // Online (THE presence window): none.
        assertEquals(null, since(pr, device("dev-1", "2027-01-15T09:59:30.000Z")))
        // Offline, last seen known: since the last heartbeat.
        assertEquals("2027-01-15T07:30:00.000Z", since(pr, device("dev-1", "2027-01-15T07:30:00.000Z")))
        // Offline, no heartbeat: since the PR went conflict/failing.
        assertEquals("2027-01-15T08:00:00.000Z", since(pr, device("dev-1", null)))
        // No device row synced: never online.
        assertEquals("2027-01-15T08:00:00.000Z", since(pr))
        // No `sessionId`: the root opened it (its own device).
        assertEquals(
            "2027-01-15T06:00:00.000Z",
            since(pr.copy(sessionId = null), device("dev-root", "2027-01-15T06:00:00.000Z"), device("dev-1", "2027-01-15T09:59:30.000Z")),
        )
        // No opener synced, or an opener without a device: none.
        assertEquals(null, since(pr.copy(sessionId = "gone", rootSessionId = "gone")))
        assertEquals(null, openerOfflineSince(pr, mapOf("c1" to child.copy(deviceId = null)), emptyMap(), now))
    }
}
