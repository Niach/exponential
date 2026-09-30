package com.exponential.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
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

    @Test
    fun `scales every tile down by one factor when the widest overflows the page`() {
        val entries = listOf(
            // 480dp wide at the 320dp base — wider than a phone.
            SessionResultEntry("t", "web", "a1", 1800, 1200),
            SessionResultEntry("t", "ios", "a2", 828, 1800),
        )
        assertEquals(480, sessionResultTileWidth(entries[0]))
        // A 358dp phone column: floor(320 * 358 / 480).
        val height = sessionResultTileHeightFitting(entries, 358)
        assertEquals(238, height)
        // Aspects survive the scale: the one factor is the page's, not a row's.
        assertEquals(357, sessionResultTileWidth(entries[0], height))
        assertEquals(109, sessionResultTileWidth(entries[1], height))
        // A page that already fits — and an unmeasured one — keep the base.
        assertEquals(320, sessionResultTileHeightFitting(entries, 1000))
        assertEquals(320, sessionResultTileHeightFitting(entries, 0))
        assertEquals(320, sessionResultTileHeightFitting(entries, -10))
        assertEquals(320, sessionResultTileHeightFitting(emptyList(), 10))
    }

    // EXP-1128: the tall rule, the fixture's `tiles` cases ×4 — a full-page
    // capture flags tall and takes the 4:3 frame; a phone shot never does.
    @Test
    fun `frames a tall capture at 4-3 and flags it`() {
        val cases = sessionResultsFixture()["tiles"]!!.jsonObject["cases"]!!.jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val name = case["name"]!!.jsonPrimitive.content
            val entry = entry(
                case["width"]!!.takeUnless { it is JsonNull }?.jsonPrimitive?.int,
                case["height"]!!.takeUnless { it is JsonNull }?.jsonPrimitive?.int,
            )
            assertEquals(name, case["tall"]!!.jsonPrimitive.boolean, sessionResultIsTall(entry))
            assertEquals(name, case["widthAt320"]!!.jsonPrimitive.int, sessionResultTileWidth(entry, 320))
        }
    }

    @Test
    fun `splits a tall image into strip ranges`() {
        assertEquals(listOf(0 to 4096), tallImageStripRanges(4096))
        val strips = tallImageStripRanges(25094)
        assertEquals(7, strips.size)
        assertEquals(24576 to 518, strips.last())
        assertEquals(25094, strips.sumOf { it.second })
        assertTrue(tallImageStripRanges(0).isEmpty())
    }

    private fun entry(width: Int?, height: Int?) =
        SessionResultEntry("t", "l", "a", width, height)

    // EXP-933: the shared `session-results.json` fixture, case by case.
    @Test
    fun `every fixture groups case parses byte exact`() {
        val cases = sessionResultsFixture()["groups"]!!.jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val name = case["name"]!!.jsonPrimitive.content
            val raw = case["raw"]!!.let { value ->
                when {
                    value is JsonNull -> null
                    value is JsonPrimitive && value.isString -> value.content
                    else -> value.toString()
                }
            }
            val actual = parseSessionResultGroups(raw).map { group ->
                Triple(group.topic, group.text, group.entries.map { it.label to it.attachmentId })
            }
            val expected = case["expected"]!!.jsonArray.map { groupElement ->
                val group = groupElement.jsonObject
                Triple(
                    group["topic"]!!.jsonPrimitive.content,
                    group["text"]!!.takeUnless { it is JsonNull }?.jsonPrimitive?.content,
                    group["entries"]!!.jsonArray.map {
                        it.jsonObject["label"]!!.jsonPrimitive.content to
                            it.jsonObject["attachmentId"]!!.jsonPrimitive.content
                    },
                )
            }
            assertEquals(name, expected, actual)
            assertEquals(name, expected.isNotEmpty(), hasSessionResults(raw))
        }
    }

    @Test
    fun `report groups keep the 60 picture cap`() {
        val many = (0 until 80).joinToString(",") { index ->
            """{"topic":"t","label":"l$index","attachmentId":"a$index"}"""
        }
        val groups = parseSessionResultGroups("""[$many,{"topic":"late","text":"words"}]""")
        assertEquals(60, groups[0].entries.size)
        assertEquals("words", groups[1].text)
        assertFalse(hasSessionResults("[]"))
    }
}

/** The contract fixture, located relative to the Gradle test working dir. */
internal fun sessionResultsFixture(): JsonObject {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/session-results.json",
        "../../packages/domain-contract/fixtures/session-results.json",
        "packages/domain-contract/fixtures/session-results.json",
    )
    val file = candidates.map(::File).firstOrNull { it.isFile }
        ?: error("session-results.json not found from ${File(".").absolutePath}")
    return Json.parseToJsonElement(file.readText()).jsonObject
}
