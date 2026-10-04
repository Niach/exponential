package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-1184: the live-run display rule, replayed from the ×4 contract fixture
// `session-display.json` (web `coding-session-display.test.ts`, iOS, desktop
// the same) — no hand-mirrored cases.
class CodingSessionDisplayTest {

    private val cases by lazy {
        Json.parseToJsonElement(contractFixtureJson("session-display.json"))
            .jsonObject.getValue("cases").jsonArray
    }

    private fun session(status: String, needsInput: Boolean, agentBusy: Boolean) = CodingSessionEntity(
        id = "sess-1",
        issueId = "issue-1",
        teamId = "ws-1",
        userId = "user-1",
        status = status,
        needsInput = needsInput,
        agentBusy = agentBusy,
        startedAt = "2026-07-17T09:00:00Z",
        createdAt = "2026-07-17T09:00:00Z",
        updatedAt = "2026-07-17T11:30:00Z",
    )

    private fun stateKey(state: CodingSessionDisplayState): String = when (state) {
        CodingSessionDisplayState.Working -> "working"
        CodingSessionDisplayState.NeedsInput -> "needs_input"
        CodingSessionDisplayState.Review -> "review"
        CodingSessionDisplayState.Done -> "done"
    }

    private fun toneKey(tone: SessionStatusTone): String = when (tone) {
        SessionStatusTone.Muted -> "muted"
        SessionStatusTone.Amber -> "amber"
        SessionStatusTone.Emerald -> "emerald"
        SessionStatusTone.Sky -> "sky"
    }

    @Test
    fun `every fixture case renders as the contract says`() {
        assertTrue(cases.size >= 10)
        cases.forEach { element ->
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val status = case.getValue("status").jsonPrimitive.content
            val prState = case["prState"]?.takeIf { it !is JsonNull }?.jsonPrimitive?.content
            val row = session(
                status = status,
                needsInput = case.getValue("needsInput").jsonPrimitive.boolean,
                agentBusy = case.getValue("agentBusy").jsonPrimitive.boolean,
            )
            val state = codingSessionDisplayState(row, prState)
            assertEquals("$name: state", case.getValue("state").jsonPrimitive.content, stateKey(state))
            assertEquals(
                "$name: working",
                case.getValue("working").jsonPrimitive.boolean,
                codingSessionIsWorking(status, state),
            )
            assertEquals(
                "$name: statusTone",
                case.getValue("statusTone").jsonPrimitive.content,
                toneKey(sessionStatusTone(state)),
            )
        }
    }
}
