package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** EXP-1686: [SessionBrief] replays `session-brief.json` `forwardTextCases`. */
class SessionBriefTest {
    @Test
    fun `every forwardText case matches the fixture`() {
        val fixture = Json.parseToJsonElement(contractFixtureJson("session-brief.json")).jsonObject
        val cases = fixture.getValue("forwardTextCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                case.getValue("expected").jsonPrimitive.content,
                SessionBrief.agentForwardText(
                    case.getValue("text").jsonPrimitive.content,
                    case.getValue("identifiers").jsonArray.map { it.jsonPrimitive.content },
                ),
            )
        }
    }
}
