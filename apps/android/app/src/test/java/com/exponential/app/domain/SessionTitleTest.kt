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

/**
 * EXP-1341: [SessionTitle.sessionTitle] replays `fixtures/session-title.json`
 * (TS `session-title.test.ts`, desktop `domain::session_title`, iOS
 * `SessionTitleTests` replay the same cases); the embedded copy equals the
 * fixture's.
 */
class SessionTitleTest {

    private val fixture: JsonObject by lazy {
        Json.parseToJsonElement(contractFixtureJson("session-title.json")).jsonObject
    }

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.list(key: String): List<JsonObject> =
        this[key]?.takeUnless { it is JsonNull }?.jsonArray?.map { it.jsonObject }.orEmpty()

    @Test
    fun `every session-title case matches the fixture`() {
        val cases = fixture.list("cases")
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val input = case.getValue("input").jsonObject
            val session = input.getValue("session").jsonObject
            val actual = SessionTitle.sessionTitle(
                SessionTitle.TitleSession(
                    id = session.str("id")!!,
                    rootId = session.str("rootId")!!,
                    title = session.str("title"),
                ),
                input.list("links").map {
                    SessionTitle.TitleLink(
                        sessionId = it.str("sessionId")!!,
                        issueId = it.str("issueId")!!,
                        sortOrder = it.getValue("sortOrder").jsonPrimitive.double,
                    )
                },
                input.list("issues").map {
                    SessionTitle.TitleIssue(it.str("id")!!, it.str("identifier")!!, it.str("title")!!)
                },
                input.list("messages").map {
                    SessionTitle.TitleMessage(
                        id = it.str("id")!!,
                        sessionId = it.str("sessionId")!!,
                        authorKind = it.str("authorKind")!!,
                        authorSessionId = it.str("authorSessionId"),
                        body = it.str("body")!!,
                        hasCard = it["card"].let { card -> card != null && card !is JsonNull },
                        createdAt = it.str("createdAt")!!,
                    )
                },
            )
            assertEquals(case.str("name"), case.str("expected"), actual)
        }
    }

    @Test
    fun `the embedded copy equals the fixture`() {
        val titles = fixture.getValue("titles").jsonObject
        assertEquals(titles.str("issue"), SessionTitle.TITLE_ISSUE)
        assertEquals(titles.str("issues"), SessionTitle.TITLE_ISSUES)
        assertEquals(fixture.getValue("titleMax").jsonPrimitive.int, SessionTitle.TITLE_MAX)
        assertEquals(fixture.str("untitled"), SessionTitle.UNTITLED)
    }
}
