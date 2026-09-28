package com.exponential.app.domain

import com.exponential.app.domain.CodingReadinessRepoPicker.Board
import com.exponential.app.domain.CodingReadinessRepoPicker.Repo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Test

/** EXP-1121: the inline repository picker's order, search and tags (iOS mirrors it). */
class CodingReadinessRepoPickerTest {

    private val board = Board(id = "b-mobile", name = "Mobile")

    private val repos = listOf(
        Repo("r1", "acme/web", listOf(Board("b-web", "Web"))),
        Repo("r2", "acme/api", emptyList()),
        Repo("r3", "acme/Mobile", emptyList()),
        Repo("r4", "acme/mobile-app", listOf(board, Board("b-ios", "iOS"))),
        Repo("r5", "other/app", emptyList()),
    )

    private fun ids(query: String = "", slug: String = "mobile") =
        CodingReadinessRepoPicker.rows(repos, board.id, board.name, slug, query).map { it.repo.id }

    @Test
    fun matchingReposLeadInTheirOriginalOrder() {
        assertEquals(listOf("r3", "r1", "r2", "r4", "r5"), ids())
    }

    @Test
    fun theSlugMatchesToo() {
        val slugOnly = CodingReadinessRepoPicker.rows(repos, board.id, "Nope", "app", "").map { it.repo.id }
        assertEquals(listOf("r5", "r1", "r2", "r3", "r4"), slugOnly)
        // The name matches r3, the slug r5: both lead, in the API's order.
        assertEquals(listOf("r3", "r5", "r1", "r2", "r4"), ids(slug = "app"))
    }

    @Test
    fun matchComparesTheNameHalfCaseInsensitively() {
        assertTrue(CodingReadinessRepoPicker.matchesBoard("ACME/MOBILE", "mobile", "x"))
        assertFalse(CodingReadinessRepoPicker.matchesBoard("mobile/web", "mobile", "mobile"))
        assertTrue(CodingReadinessRepoPicker.matchesBoard("solo", "Solo", "x"))
    }

    @Test
    fun searchIsACaseInsensitiveSubstringOfTheFullName() {
        assertEquals(listOf("r3", "r4"), ids(query = "MOB"))
        assertEquals(listOf("r5"), ids(query = "other/"))
        assertEquals(emptyList<String>(), ids(query = "zzz"))
        assertEquals(5, ids(query = "   ").size)
    }

    @Test
    fun tagsNameTheMatchOrTheFirstOtherBoard() {
        val tags = CodingReadinessRepoPicker.rows(repos, board.id, board.name, "mobile", "")
            .associate { it.repo.id to it.tag }
        assertEquals(CodingReadiness.Copy.PICKER_MATCHES_BOARD, tags["r3"])
        assertEquals(CodingReadiness.pickerUsedBy("Web"), tags["r1"])
        assertEquals(null, tags["r2"])
        // The current board is skipped: the FIRST OTHER board names it.
        assertEquals(CodingReadiness.pickerUsedBy("iOS"), tags["r4"])
        assertEquals(null, tags["r5"])
    }

    @Test
    fun aMatchWinsOverUsedBy() {
        val rows = CodingReadinessRepoPicker.rows(
            listOf(Repo("r", "acme/mobile", listOf(Board("b2", "Other")))),
            board.id, board.name, "mobile", "",
        )
        assertEquals(CodingReadiness.Copy.PICKER_MATCHES_BOARD, rows.single().tag)
    }

    /** The shared contract fixture (`picker`), ×4 with web, desktop and iOS. */
    @Test
    fun matchesTheSharedFixture() {
        val fixture = Json.parseToJsonElement(contractFixtureJson("coding-readiness.json")).jsonObject
        val cases = fixture.getValue("picker").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val board = case.getValue("board").jsonObject
            val repos = case.getValue("repos").jsonArray.map { repo ->
                val obj = repo.jsonObject
                Repo(
                    obj.getValue("id").jsonPrimitive.content,
                    obj.getValue("fullName").jsonPrimitive.content,
                    obj.getValue("boards").jsonArray.map {
                        Board(it.jsonObject.getValue("id").jsonPrimitive.content, it.jsonObject.getValue("name").jsonPrimitive.content)
                    },
                )
            }
            val rows = CodingReadinessRepoPicker.rows(
                repos,
                board.getValue("id").jsonPrimitive.content,
                board.getValue("name").jsonPrimitive.content,
                board.getValue("slug").jsonPrimitive.content,
                case.getValue("query").jsonPrimitive.content,
            )
            val expected = case.getValue("expected").jsonArray.map { it.jsonObject }
            val name = case.getValue("name").jsonPrimitive.content
            assertEquals(name, expected.map { it.getValue("id").jsonPrimitive.content }, rows.map { it.repo.id })
            assertEquals(
                name,
                expected.map { it["tag"]?.takeUnless { tag -> tag is JsonNull }?.jsonPrimitive?.content },
                rows.map { it.tag },
            )
        }
    }
}
