package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.float
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [AskDock] replays `fixtures/ask-dock.json` (every case array), comparing
 * the WHOLE expected object like the TS reference's own test.
 */
class AskDockTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("ask-dock.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun array(key: String): List<JsonObject> = fixture.getValue(key).jsonArray.map { it.jsonObject }

    private fun nullable(value: String?): JsonElement = value?.let { JsonPrimitive(it) } ?: JsonNull

    private fun kind(wire: String): AskDock.Kind = AskDock.Kind.entries.first { it.wire == wire }

    private fun pendingJson(p: AskDock.Pending) = buildJsonObject {
        put("id", p.id)
        put("kind", p.kind.wire)
        put("text", p.text)
        put("sessionName", nullable(p.sessionName))
        put("createdAt", p.createdAt)
        p.questions?.let { put("questions", it) }
    }

    @Test
    fun `constants match the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("planReady"), AskDock.Copy.PLAN_READY)
        assertEquals(copy.str("question"), AskDock.Copy.QUESTION)
        assertEquals(copy.str("back"), AskDock.Copy.BACK)
        assertEquals(copy.str("feedbackPlaceholder"), AskDock.Copy.FEEDBACK_PLACEHOLDER)
        assertEquals(copy.str("cancel"), AskDock.Copy.CANCEL)
        assertEquals(setOf("planReady", "question", "back", "feedbackPlaceholder", "cancel"), copy.keys)
        val icons = fixture.getValue("icons").jsonObject
        assertEquals(mapOf("plan" to AskDock.Icons.PLAN, "question" to AskDock.Icons.QUESTION), icons.mapValues { it.value.jsonPrimitive.content })
        val options = array("planOptions").map {
            AskDock.PlanOption(
                key = it.str("key")!!,
                label = it.str("label")!!,
                description = it.str("description"),
                decision = it.str("decision")!!,
                fresh = it.getValue("fresh").jsonPrimitive.boolean,
            )
        }
        assertEquals(options, AskDock.PLAN_OPTIONS)
    }

    @Test
    fun `title cases`() {
        for (case in array("titleCases")) {
            assertEquals(case.str("text"), case.str("expected"), AskDock.askDockTitle(case.str("text")!!))
        }
    }

    @Test
    fun `decision label cases`() {
        for (case in array("decisionLabelCases")) {
            val actual = AskDock.planDecisionLabel(case.str("decision")!!, case.getValue("fresh").jsonPrimitive.boolean)
            assertEquals(case.toString(), case.str("expected"), actual)
        }
    }

    private fun threadRow(json: JsonObject): SessionTree.ThreadRow = when (json.str("type")) {
        "plan" -> SessionTree.ThreadRow.Plan(
            id = json.str("id")!!,
            sessionId = json.str("sessionId")!!,
            plan = json.str("plan")!!,
            decision = json.str("decision"),
            fresh = json.getValue("fresh").jsonPrimitive.boolean,
        )
        "question" -> SessionTree.ThreadRow.Question(
            id = json.str("id")!!,
            sessionId = json.str("sessionId")!!,
            question = json.str("question")!!,
            options = (json["options"] as? JsonArray)?.map { SessionTree.ThreadRow.QuestionChoice(it.jsonPrimitive.content, null) }
                ?: emptyList(),
            custom = json.getValue("custom").jsonPrimitive.boolean,
            answer = json.str("answer"),
            answered = json.getValue("answered").jsonPrimitive.boolean,
        )
        else -> SessionTree.ThreadRow.MessageRow(
            id = json.str("id")!!,
            author = "user",
            authorId = null,
            body = "",
            queued = false,
        )
    }

    @Test
    fun `thread cases`() {
        val cases = array("threadCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val names = case.getValue("sessionNames").jsonObject
            val actual = AskDock.threadPendingAsks(case.getValue("rows").jsonArray.map { threadRow(it.jsonObject) }) { id ->
                names.str(id)
            }
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(actual.map { pendingJson(it) }))
        }
    }

    @Test
    fun `session name cases`() {
        val cases = array("sessionNameCases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val tabs = case.getValue("tabs").jsonArray.mapNotNull { tab ->
                val json = tab.jsonObject
                if (json.str("kind") != "session") return@mapNotNull null
                SessionTree.ThreadTab.SessionTab(
                    sessionId = json.str("sessionId")!!,
                    name = json.str("name")!!,
                    markState = SessionTree.SessionState.Ended,
                    nested = json.getValue("nested").jsonArray.map { nested ->
                        val n = nested.jsonObject
                        SessionTree.NestedSession(n.str("sessionId")!!, n.str("name")!!, SessionTree.SessionState.Ended, 1)
                    },
                )
            }
            val name = AskDock.threadSessionNames(tabs)
            for ((id, expected) in case.getValue("lookups").jsonObject) {
                assertEquals("${case.str("name")}: $id", (expected as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content, name(id))
            }
        }
    }
}
