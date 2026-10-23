package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * NEW-8: [MessageConversation.keyMessagesBySession] replays
 * `fixtures/message-conversation.json` (TS `message-conversation.test.ts`,
 * desktop `domain::message_conversation`, iOS `MessageConversationTests`).
 */
class MessageConversationTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("message-conversation.json")).jsonObject
    }

    private data class Row(val id: String, val conversationId: String, val legacySessionId: String?)

    private fun conversations(array: JsonArray) = array.map { it.jsonObject }.map {
        MessageConversation.Conversation(
            id = it.getValue("id").jsonPrimitive.content,
            sessionId = it["sessionId"]?.takeUnless { v -> v is JsonNull }?.jsonPrimitive?.content,
        )
    }

    @Test
    fun `every message-conversation case matches the fixture`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        val shared = conversations(fixture.getValue("conversations").jsonArray)
        for (case in cases) {
            val name = case.getValue("name").jsonPrimitive.content
            val convs = case["conversations"]?.jsonArray?.let(::conversations) ?: shared
            val messages = case.getValue("messages").jsonArray.map { it.jsonObject }.map {
                Row(
                    id = it.getValue("id").jsonPrimitive.content,
                    conversationId = it.getValue("conversationId").jsonPrimitive.content,
                    legacySessionId = it["sessionId"]?.takeUnless { v -> v is JsonNull }?.jsonPrimitive?.content,
                )
            }
            val expected = case.getValue("expected").jsonArray.map { it.jsonObject }.map {
                it.getValue("id").jsonPrimitive.content to it.getValue("sessionId").jsonPrimitive.content
            }
            val keyed = MessageConversation.keyMessagesBySession(messages, convs) { it.conversationId }
            assertEquals(name, expected, keyed.map { (row, sessionId) -> row.id to sessionId })
        }
    }
}
