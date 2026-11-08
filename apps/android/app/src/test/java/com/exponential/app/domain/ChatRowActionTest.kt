package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1464: [ChatRowAction] replays `fixtures/chat-row-action.json`
 * (TS `chat-row-action.test.ts`, desktop `domain::chat_row_action`, iOS
 * `ChatRowAction.swift`), comparing the WHOLE expected object.
 */
class ChatRowActionTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("chat-row-action.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    @Test
    fun `copy and test ids match the fixture`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("openChat"), ChatRowAction.Copy.OPEN_CHAT)
        assertEquals(copy.str("answered"), ChatRowAction.Copy.ANSWERED)
        val ids = fixture.getValue("testIds").jsonObject
        assertEquals(ids.str("openChat"), ChatRowAction.TestIds.OPEN_CHAT)
        assertEquals(ids.str("expanded"), ChatRowAction.TestIds.EXPANDED)
        assertEquals(ids.str("event"), ChatRowAction.TestIds.EVENT)
    }

    @Test
    fun `every case matches the fixture`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val e = case.getValue("expected").jsonObject
            val expected = ChatRowAction.Action(
                action = ChatRowAction.Kind.entries.first { it.wire == e.str("action") },
                sessionId = e.str("sessionId"),
                chevron = e.getValue("chevron").jsonPrimitive.boolean,
                kindWord = e.str("kindWord"),
                tone = e.str("tone"),
            )
            val state = input.str("state")?.let { wire ->
                SessionTree.SessionState.entries.first { it.wire == wire }
            }
            assertEquals(
                case.str("name"),
                expected,
                ChatRowAction.chatRowAction(
                    type = input.str("type")!!,
                    sessionId = input.str("sessionId"),
                    rootId = input.str("rootId")!!,
                    state = state,
                    blocked = input.getValue("blocked").jsonPrimitive.boolean,
                    inline = input.getValue("inline").jsonPrimitive.boolean,
                    answered = input.getValue("answered").jsonPrimitive.boolean,
                ),
            )
        }
    }

    @Test
    fun `every waiting case matches the fixture`() {
        val cases = fixture.getValue("waitingCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val messages = case.getValue("messages").jsonArray.map { it.jsonObject }.map { m ->
                SessionTree.Message(
                    id = m.str("id")!!,
                    sessionId = m.str("sessionId")!!,
                    authorKind = "agent",
                    authorId = null,
                    body = "",
                    card = m["card"]?.takeUnless { it is JsonNull }?.jsonObject,
                    answeredAt = m.str("answeredAt"),
                    consumedAt = null,
                    createdAt = m.str("createdAt")!!,
                )
            }
            assertEquals(
                case.str("name"),
                case.str("expected"),
                ChatRowAction.sessionWaitingCard(case.str("sessionId")!!, messages),
            )
        }
    }
}
