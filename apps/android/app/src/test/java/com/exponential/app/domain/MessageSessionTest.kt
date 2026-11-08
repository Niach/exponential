package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1730: [MessageSession.keyMessagesBySession] replays
 * `fixtures/message-session.json` (TS `message-session.test.ts`, desktop
 * `domain::message_session`, iOS `MessageSessionTests`).
 */
class MessageSessionTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("message-session.json")).jsonObject
    }

    private data class Row(val id: String, val sessionId: String?, val legacyConversationId: String?)

    @Test
    fun `every message-session case matches the fixture`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val name = case.getValue("name").jsonPrimitive.content
            val messages = case.getValue("messages").jsonArray.map { it.jsonObject }.map {
                Row(
                    id = it.getValue("id").jsonPrimitive.content,
                    sessionId = it["sessionId"]?.takeUnless { v -> v is JsonNull }?.jsonPrimitive?.content,
                    legacyConversationId = it["conversationId"]?.takeUnless { v -> v is JsonNull }?.jsonPrimitive?.content,
                )
            }
            val expected = case.getValue("expected").jsonArray.map { it.jsonObject }.map {
                it.getValue("id").jsonPrimitive.content to it.getValue("sessionId").jsonPrimitive.content
            }
            val keyed = MessageSession.keyMessagesBySession(messages) { it.sessionId }
            assertEquals(name, expected, keyed.map { (row, sessionId) -> row.id to sessionId })
        }
    }
}
