package com.exponential.app.ui.chats

import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.domain.Chats
import com.exponential.app.domain.NeedsYou
import com.exponential.app.domain.ResultsTab
import com.exponential.app.domain.SessionTree
import com.exponential.app.domain.contractFixtureJson
import com.exponential.app.navigation.Routes
import com.exponential.app.ui.styleguide.NEEDS_YOU_SPECIMEN_ROWS
import com.exponential.app.ui.styleguide.RESULTS_SPECIMEN
import com.exponential.app.ui.thread.RESULTS_TAB_KEY
import com.exponential.app.ui.thread.resultPictureSet
import com.exponential.app.ui.thread.resultThumbWidth
import com.exponential.app.ui.thread.threadInitialTabKey
import com.exponential.app.ui.threads.chatDisplayedRows
import com.exponential.app.ui.threads.needsYouFold
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
        assertEquals("thread/r1?tab=plan&plan=m1", Routes.needsYou(row(NeedsYou.Opens.Plan)))
        assertEquals("thread/r1?card=m1", Routes.needsYou(row(NeedsYou.Opens.Card)))
        assertEquals("thread/r1?tab=results", Routes.needsYou(row(NeedsYou.Opens.Results)))
        assertEquals("thread/r1", Routes.needsYou(row(NeedsYou.Opens.Chat)))
    }

    @Test
    fun `an older stories or guide link lands on Results`() {
        assertEquals(RESULTS_TAB_KEY, threadInitialTabKey("stories"))
        assertEquals(RESULTS_TAB_KEY, threadInitialTabKey("guide"))
        assertEquals(RESULTS_TAB_KEY, threadInitialTabKey("results"))
        assertEquals("plan", threadInitialTabKey("plan"))
        assertEquals("conversation", threadInitialTabKey(null))
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

    private fun chat(id: String, group: SessionTree.ThreadsGroupKey, unread: Int = 0) = Chats.ChatRow(
        sessionId = id, teamId = "t", title = id, deviceId = null, group = group, lead = Chats.ChatLead.Waiting,
        preview = Chats.ChatPreview("text", ""), activityAt = "", time = "", unreadCount = unread,
        needsYou = false, busy = false, word = null,
    )

    @Test
    fun `selection order is the displayed order, the Archived page flat`() {
        val rows = listOf(
            chat("plain", SessionTree.ThreadsGroupKey.Recent),
            chat("unread", SessionTree.ThreadsGroupKey.Recent, unread = 1),
            chat("working", SessionTree.ThreadsGroupKey.Running),
            chat("waiting", SessionTree.ThreadsGroupKey.NeedsYou),
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
        )
        assertEquals(listOf("pr1", "q3", "q1"), rows.map { it.messageId })
        assertEquals(NeedsYou.Kind.Pr, rows[0].kind)
        assertEquals("#7", rows[0].meta)
        assertEquals("Triage", rows[1].chat)
    }
}
