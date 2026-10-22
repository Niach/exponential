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
 * EXP-1341: [SessionKind.sessionTraits] replays `fixtures/session-kind.json`
 * (TS `session-kind.test.ts`, desktop `domain::session_kind`, iOS
 * `SessionKindTests`); the embedded `kinds` table equals the fixture's.
 */
class SessionKindTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("session-kind.json")).jsonObject
    }

    private fun traits(json: JsonObject) = SessionTraits(
        persistent = json.getValue("persistent").jsonPrimitive.boolean,
        triggers = json.getValue("triggers").jsonPrimitive.boolean,
    )

    @Test
    fun `every session-kind case matches the fixture`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val kind = case["kind"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
            val expected = traits(case.getValue("expected").jsonObject)
            assertEquals("kind $kind", expected, SessionKind.sessionTraits(kind))
        }
    }

    @Test
    fun `every kindOf case derives the kind from agentId`() {
        val cases = fixture.getValue("kindOfCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val session = case.getValue("session").jsonObject
            val agentId = session["agentId"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
            val expected = case.getValue("expected").jsonPrimitive.content
            assertEquals("session $session", expected, SessionKind.sessionKindOf(agentId))
            assertEquals(
                "session $session",
                SessionKind.sessionTraits(expected).persistent,
                SessionKind.isPersistent(agentId),
            )
        }
    }

    @Test
    fun `the embedded kinds equal the fixture`() {
        val kinds = fixture.getValue("kinds").jsonObject
        assertEquals(kinds.keys, SessionKind.KINDS.keys)
        for ((kind, value) in kinds) assertEquals(kind, traits(value.jsonObject), SessionKind.KINDS[kind])
    }
}
