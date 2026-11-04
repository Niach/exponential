package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** [NeedsYou] replays `fixtures/needs-you.json` `cases`, comparing the WHOLE
 *  expected array (the TS reference's own test). */
class NeedsYouTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("needs-you.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun encode(row: NeedsYou.Row) = buildJsonObject {
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

    @Test
    fun `constants match the fixture`() {
        assertEquals(fixture.str("route"), NeedsYou.ROUTE)
        assertEquals(
            mapOf("entry" to NeedsYou.Copy.ENTRY, "empty" to NeedsYou.Copy.EMPTY),
            fixture.getValue("copy").jsonObject.mapValues { it.value.jsonPrimitive.content },
        )
        assertEquals(
            mapOf("question" to NeedsYou.Prefix.QUESTION),
            fixture.getValue("prefix").jsonObject.mapValues { it.value.jsonPrimitive.content },
        )
        assertEquals(
            mapOf("entry" to NeedsYou.TestIds.ENTRY, "row" to NeedsYou.TestIds.ROW, "list" to NeedsYou.TestIds.LIST),
            fixture.getValue("testIds").jsonObject.mapValues { it.value.jsonPrimitive.content },
        )
    }

    @Test
    fun `replays every case`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val rows = NeedsYou.needsYou(
                now = input.getValue("now").jsonPrimitive.long,
                me = input.str("me"),
                sessions = input.getValue("sessions").jsonArray.map { it.jsonObject }.map {
                    NeedsYou.SessionInput(
                        id = it.str("id")!!,
                        rootId = it.str("rootId")!!,
                        name = it.str("name")!!,
                        status = it.str("status")!!,
                        agentId = it.str("agentId"),
                        userId = it.str("userId"),
                    )
                },
                messages = input.getValue("messages").jsonArray.map { it.jsonObject }.map {
                    NeedsYou.MessageInput(
                        id = it.str("id")!!,
                        sessionId = it.str("sessionId")!!,
                        authorSessionId = it.str("authorSessionId"),
                        body = it.str("body")!!,
                        createdAt = it.str("createdAt")!!,
                        answeredAt = it.str("answeredAt"),
                        card = it["card"] as? JsonObject,
                    )
                },
            )
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(rows.map(::encode)))
        }
    }
}
