package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
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
        assertEquals(
            mapOf("plan" to NeedsYou.Private.PLAN, "question" to NeedsYou.Private.QUESTION),
            fixture.getValue("private").jsonObject.mapValues { it.value.jsonPrimitive.content },
        )
    }

    @Test
    fun `replays every case`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        val prCases = fixture.getValue("prCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        assertTrue(prCases.isNotEmpty())
        for (case in cases + prCases) {
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
                        busy = it["busy"]?.jsonPrimitive?.boolean ?: false,
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
                pullRequests = input["pullRequests"]?.jsonArray.orEmpty().map { it.jsonObject }.map {
                    NeedsYou.PullRequestInput(
                        id = it.str("id")!!,
                        rootSessionId = it.str("rootSessionId"),
                        number = it.getValue("number").jsonPrimitive.long,
                        title = it.str("title")!!,
                        state = it.str("state")!!,
                        checksState = it.str("checksState"),
                        basePullRequestId = it.str("basePullRequestId"),
                        createdAt = it.str("createdAt")!!,
                    )
                },
                mergeMode = input.str("mergeMode"),
            )
            assertEquals(case.str("name"), case.getValue("expected"), JsonArray(rows.map(::encode)))
        }
    }

    /** A digest's `ask` card keeps its origin's ids through the decode (chats.ts
     *  `digestMessage`), so the fold can name the origin's card. */
    @Test
    fun `a digest keeps the origin ids of an ask`() {
        val digest = Chats.parseDigest(
            """{"first": null, "preview": null, "waiting": {"id": "mirror", "authorKind": "agent",
               "authorSessionId": "c1", "body": "Asks: Ship it?",
               "card": {"kind": "ask", "sessionId": "c1", "messageId": "q2"},
               "answeredAt": null, "createdAt": "2027-01-15T07:55:00.000Z"},
               "agentAt": null, "agentTimes": []}""",
        )
        val kept = Chats.digestMessages("r1", digest)
        assertEquals(1, kept.size)
        val mirror = kept.first()
        assertEquals("c1", mirror.card!!.str("sessionId"))
        assertEquals("q2", mirror.card.str("messageId"))
        fun session(id: String) = NeedsYou.SessionInput(id, "r1", id, "idle", null, "me")
        val rows = NeedsYou.needsYou(
            now = 1_800_000_000_000L,
            me = "me",
            sessions = listOf(session("r1"), session("c1")),
            messages = listOf(
                NeedsYou.MessageInput(
                    id = mirror.id,
                    sessionId = mirror.sessionId,
                    authorSessionId = mirror.authorSessionId,
                    body = mirror.body,
                    createdAt = mirror.createdAt,
                    answeredAt = mirror.answeredAt,
                    card = mirror.card,
                ),
            ),
            mergeMode = null,
        )
        assertEquals(listOf("q2"), rows.map { it.messageId })
        assertEquals(listOf("c1"), rows.map { it.sessionId })
    }
}
