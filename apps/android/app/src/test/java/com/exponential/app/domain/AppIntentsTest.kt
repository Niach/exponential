package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.double
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// #EXP-1640: App Actions, replayed off `app-intents.json`
// (TS `app-intents.test.ts`, same case names).
class AppIntentsTest {
    private val fixture: JsonObject = Json.parseToJsonElement(contractFixtureJson("app-intents.json")).jsonObject

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    @Test
    fun `copy, capabilities and limits are the fixture's`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("created"), AppIntents.CREATED)
        assertEquals(copy.str("askTitle"), AppIntents.ASK_TITLE)
        assertEquals(copy.str("noBoard"), AppIntents.NO_BOARD)
        assertEquals(copy.str("noAgent"), AppIntents.NO_AGENT)
        assertEquals(copy.str("sent"), AppIntents.SENT)
        assertEquals(copy.str("signedOut"), AppIntents.SIGNED_OUT)
        assertEquals(copy.str("createIssueTitle"), AppIntents.CREATE_ISSUE_TITLE)
        assertEquals(copy.str("askAgentTitle"), AppIntents.ASK_AGENT_TITLE)
        val caps = fixture.getValue("androidCapabilities").jsonObject
        assertEquals(caps.str("createIssue"), AppIntents.CAPABILITY_CREATE_ISSUE)
        assertEquals(caps.str("askAgent"), AppIntents.CAPABILITY_ASK_AGENT)
        assertEquals(fixture.getValue("titleMax").jsonPrimitive.int, AppIntents.TITLE_MAX)
    }

    @Test
    fun `titleCases match the fixture`() {
        for (case in cases("titleCases")) {
            assertEquals(case.str("name"), case.str("expected"), AppIntents.intentIssueTitle(case.str("input")!!))
        }
    }

    @Test
    fun `boardCases match the fixture`() {
        for (case in cases("boardCases")) {
            val input = case.getValue("input").jsonObject
            val boards = input.getValue("boards").jsonArray.map {
                val b = it.jsonObject
                AppIntents.Board(b.str("id")!!, b.getValue("sortOrder").jsonPrimitive.double)
            }
            assertEquals(case.str("name"), case.str("expected"), AppIntents.intentBoard(boards, input.str("lastBoardId"), input.str("pickedId")))
        }
    }

    @Test
    fun `agentCases match the fixture`() {
        for (case in cases("agentCases")) {
            val input = case.getValue("input").jsonObject
            val agents = input.getValue("agents").jsonArray.map {
                val a = it.jsonObject
                AppIntents.Agent(a.str("id")!!, a.str("name")!!, a.getValue("sortOrder").jsonPrimitive.int, a.str("role"), a.str("ownerId"))
            }
            assertEquals(case.str("name"), case.str("expected"), AppIntents.matchIntentAgent(agents, input.str("me"), input.str("spoken")!!))
        }
    }
}
