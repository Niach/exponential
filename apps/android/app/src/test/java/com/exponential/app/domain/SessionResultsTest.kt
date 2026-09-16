package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-879: the Results face's pure rules. Test names mirror web
// `session-results.test.ts`, desktop `session_results.rs` and iOS
// `SessionResultsTests.swift`.
class SessionResultsTest {

    @Test
    fun `parses a flat ordered list and drops malformed entries`() {
        val parsed = parseSessionResults(
            """
            [
              {"topic":"chatui","label":"web","attachmentId":"a1","width":1600,"height":900},
              {"topic":"chatui","attachmentId":"a2","width":10,"height":10},
              {"topic":"chatui","label":"ios","width":10,"height":10},
              {"topic":"   ","label":"android","attachmentId":"a3"},
              {"topic":"chatui","label":7,"attachmentId":"a4"},
              {"topic":"chatui","label":"ios","attachmentId":"a5","width":0,"height":-3},
              {"topic":"nav","label":"web","attachmentId":"a6","width":"800","height":null},
              null,
              "nope",
              []
            ]
            """.trimIndent(),
        )
        assertEquals(
            listOf(
                SessionResultEntry("chatui", "web", "a1", 1600, 900),
                SessionResultEntry("chatui", "ios", "a5", null, null),
                SessionResultEntry("nav", "web", "a6", null, null),
            ),
            parsed,
        )
        // Topic, label and id are trimmed.
        assertEquals(
            SessionResultEntry("t", "l", "a", null, null),
            parseSessionResults("""[{"topic":" t ","label":" l ","attachmentId":" a "}]""").single(),
        )
    }

    @Test
    fun `ignores unknown fields and a null or blank blob`() {
        assertEquals(
            listOf(SessionResultEntry("t", "l", "a", 4, 2)),
            parseSessionResults(
                """
                [{"topic":"t","label":"l","attachmentId":"a","width":4,"height":2,
                  "url":"/api/attachments/a","extra":{"nope":true}}]
                """.trimIndent(),
            ),
        )
        assertEquals(emptyList<SessionResultEntry>(), parseSessionResults(null))
        assertEquals(emptyList<SessionResultEntry>(), parseSessionResults(""))
        assertEquals(emptyList<SessionResultEntry>(), parseSessionResults("   "))
        assertEquals(emptyList<SessionResultEntry>(), parseSessionResults("{"))
        assertEquals(emptyList<SessionResultEntry>(), parseSessionResults("""{"topic":"t"}"""))
        assertEquals(emptyList<SessionResultEntry>(), parseSessionResults("42"))
    }

    @Test
    fun `groups by topic in first-seen order`() {
        val entries = parseSessionResults(
            """
            [
              {"topic":"chatui","label":"web","attachmentId":"a1"},
              {"topic":"nav","label":"web","attachmentId":"a2"},
              {"topic":"chatui","label":"ios","attachmentId":"a3"}
            ]
            """.trimIndent(),
        )
        assertEquals(
            listOf(
                SessionResultGroup(
                    "chatui",
                    listOf(
                        SessionResultEntry("chatui", "web", "a1"),
                        SessionResultEntry("chatui", "ios", "a3"),
                    ),
                ),
                SessionResultGroup("nav", listOf(SessionResultEntry("nav", "web", "a2"))),
            ),
            groupSessionResults(entries),
        )
        assertEquals(emptyList<SessionResultGroup>(), groupSessionResults(emptyList()))
    }

    @Test
    fun `caps at 60 entries`() {
        assertEquals(60, MAX_SESSION_RESULTS)
        val many = (0 until 80).joinToString(",") { index ->
            """{"topic":"t","label":"l$index","attachmentId":"a$index"}"""
        }
        val parsed = parseSessionResults("[$many]")
        assertEquals(60, parsed.size)
        assertEquals("l59", parsed[59].label)
    }

    @Test
    // Kotlin forbids `:` inside a backticked JVM method name, so the shared
    // name's `4:3` is written `4-3` here (and nowhere else).
    fun `sizes a tile from the probed aspect, 4-3 without one`() {
        assertEquals(320, SESSION_RESULT_TILE_HEIGHT)
        assertEquals(569, sessionResultTileWidth(entry(1600, 900)))
        assertEquals(148, sessionResultTileWidth(entry(1170, 2532)))
        // Either dimension missing falls back to 4:3.
        assertEquals(427, sessionResultTileWidth(entry(null, 900)))
        assertEquals(427, sessionResultTileWidth(entry(1600, null)))
        assertEquals(160, sessionResultTileWidth(entry(null, null), 120))
        assertEquals(200, sessionResultTileWidth(entry(1000, 1000), 200))
    }

    private fun entry(width: Int?, height: Int?) =
        SessionResultEntry("t", "l", "a", width, height)
}
