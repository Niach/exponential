package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-1456: [InboxRoute] replays `fixtures/inbox-route.json` (TS
 * `inbox-route.test.ts`, desktop `domain::inbox_route`, iOS `InboxRoute.swift`).
 */
class InboxRouteTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("inbox-route.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonElement?.orNullStr(): String? = this?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    @Test
    fun `every row case matches the fixture`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val row = case.getValue("row").jsonObject
            val e = case.getValue("expected").jsonObject
            val expected = when (e.str("kind")) {
                "issue" -> InboxRoute.RowTarget.Issue(e.str("issueId")!!, results = e["face"].orNullStr() == "results")
                "session" -> InboxRoute.RowTarget.Session(e.str("sessionId")!!)
                "none" -> InboxRoute.RowTarget.None
                else -> error("unknown kind in ${case.str("name")}")
            }
            assertEquals(
                case.str("name"),
                expected,
                InboxRoute.inboxRowTarget(row.str("type"), row.str("issueId"), row.str("sessionId")),
            )
        }
    }

    @Test
    fun `every session case matches the fixture`() {
        val cases = fixture.getValue("sessionCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val s = case.getValue("session").jsonObject
            val e = case.getValue("expected").jsonObject
            val expected = when (e.str("kind")) {
                "agent" -> InboxRoute.SessionTarget.Agent(e.str("agentId")!!)
                "thread" -> InboxRoute.SessionTarget.Thread(e.str("threadId")!!, e.str("sessionId"))
                "session" -> InboxRoute.SessionTarget.Session(e.str("sessionId")!!)
                else -> error("unknown kind in ${case.str("name")}")
            }
            assertEquals(
                case.str("name"),
                expected,
                InboxRoute.sessionTarget(
                    id = s.str("id")!!,
                    rootId = s.str("rootId"),
                    agentId = s.str("agentId"),
                    userId = s.str("userId"),
                    viewerId = case.str("viewerId"),
                ),
            )
        }
    }
}
