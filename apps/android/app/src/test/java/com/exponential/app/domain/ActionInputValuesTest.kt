package com.exponential.app.domain

import com.exponential.app.data.api.ActionInputDto
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-825: only PICK inputs remain (repo/board/pr/icon). A `text`/`textarea`
 * def — an older row, or a newer server — is UNSUPPORTED and blocks the run;
 * the wire payload carries filled values verbatim (picked ids are never
 * trimmed or rewritten) and drops blanks, so a cleared optional pick never
 * reaches the server as an empty string (EXP-756).
 */
class ActionInputValuesTest {

    private val repo = ActionInputDto(key = "repo", label = "Repository", type = "repo", required = true)
    private val board = ActionInputDto(key = "board", label = "Board", type = "board")
    private val pr = ActionInputDto(key = "pr", label = "Pull request", type = "pr", required = true)
    private val icon = ActionInputDto(key = "icon", label = "Icon", type = "icon")

    @Test
    fun `the four pick types are supported and free text is not`() {
        assertFalse(ActionInputValues.hasUnsupportedType(listOf(repo, board, pr, icon)))
        assertFalse(ActionInputValues.hasUnsupportedType(emptyList()))
        for (type in listOf("text", "textarea", "date", "")) {
            assertTrue(
                type,
                ActionInputValues.hasUnsupportedType(
                    listOf(repo, ActionInputDto(key = "k", label = "L", type = type)),
                ),
            )
        }
    }

    @Test
    fun `required inputs must be non-blank`() {
        assertTrue(ActionInputValues.requiredFilled(listOf(repo, board), mapOf("repo" to "r-1")))
        assertFalse(ActionInputValues.requiredFilled(listOf(repo, board), mapOf("board" to "b-1")))
        assertFalse(ActionInputValues.requiredFilled(listOf(repo), mapOf("repo" to "   ")))
        assertTrue(ActionInputValues.requiredFilled(listOf(board, icon), emptyMap()))
    }

    @Test
    fun `wire values are filled picks in def order, verbatim, blanks dropped`() {
        val values = mapOf(
            "icon" to "sparkles",
            "repo" to " r-1 ",
            "board" to "",
            "pr" to "issue-9",
            "stale" to "leftover",
        )
        val wire = ActionInputValues.wireValues(listOf(repo, board, pr, icon), values)
        assertEquals(listOf("repo", "pr", "icon"), wire.keys.toList())
        assertEquals(" r-1 ", wire["repo"])
        assertEquals("issue-9", wire["pr"])
        assertEquals("sparkles", wire["icon"])
        assertFalse(wire.containsKey("board"))
        assertFalse(wire.containsKey("stale"))
    }

    @Test
    fun `an empty pick set is an empty payload`() {
        assertEquals(emptyMap<String, String>(), ActionInputValues.wireValues(listOf(board), mapOf("board" to "")))
        assertEquals(emptyMap<String, String>(), ActionInputValues.wireValues(emptyList(), mapOf("x" to "y")))
    }
}
