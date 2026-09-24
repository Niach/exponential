package com.exponential.app.data.db

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Wire-decode vectors for the issues shape row. EXP-897 added
 * `pr_base_branch` (the stack edge) to the allowlist: a row that carries it
 * must land it, and a row from an older server that does not must still
 * decode — a required field missing on the wire drops the row forever.
 */
class IssueEntityDecodeTest {

    // Mirrors HttpClientProvider's shared Json.
    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    private fun row(extra: String) = """
        {
          "id": "issue-1",
          "board_id": "board-1",
          "number": 12,
          "identifier": "APP-12",
          "title": "Stack the pull requests",
          "status": "in_progress",
          "priority": "none",
          "sort_order": 1,
          "pr_url": "https://github.com/o/r/pull/7",
          "pr_number": 7,
          "pr_state": "open",
          "branch": "exp/APP-12"$extra,
          "created_at": "2026-09-15 10:00:00+00",
          "updated_at": "2026-09-15 10:00:00+00"
        }
    """.trimIndent()

    @Test
    fun `a row carrying pr_base_branch lands the stack edge`() {
        val entity = json.decodeFromString(
            IssueEntity.serializer(),
            row(",\n  \"pr_base_branch\": \"exp/APP-11\""),
        )
        assertEquals("exp/APP-11", entity.prBaseBranch)
        assertEquals("exp/APP-12", entity.branch)
        assertEquals("APP-12", entity.identifier)
    }

    @Test
    fun `a row without pr_base_branch still decodes`() {
        val entity = json.decodeFromString(IssueEntity.serializer(), row(""))
        assertNull(entity.prBaseBranch)
        assertEquals("exp/APP-12", entity.branch)
    }

    @Test
    fun `an explicit null pr_base_branch decodes as no stack edge`() {
        val entity = json.decodeFromString(
            IssueEntity.serializer(),
            row(",\n  \"pr_base_branch\": null"),
        )
        assertNull(entity.prBaseBranch)
    }

    // EXP-630: `estimate` joined the issues shape allowlist — a point number
    // on the wire lands, and a row without it (older server) or with an
    // explicit null still decodes as "not estimated".

    @Test
    fun `a row carrying estimate lands the points`() {
        val entity = json.decodeFromString(IssueEntity.serializer(), row(",\n  \"estimate\": 5"))
        assertEquals(5, entity.estimate)
    }

    @Test
    fun `a row without estimate decodes as not estimated`() {
        assertNull(json.decodeFromString(IssueEntity.serializer(), row("")).estimate)
        assertNull(
            json.decodeFromString(IssueEntity.serializer(), row(",\n  \"estimate\": null")).estimate,
        )
    }
}
