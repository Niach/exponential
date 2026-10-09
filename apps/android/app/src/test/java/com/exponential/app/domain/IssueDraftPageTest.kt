package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1170: the New issue page's copy + autosave debounce, locked ×4 against
 * `issue-draft.json` (web `issue-draft-page.test.ts`, iOS, desktop).
 */
class IssueDraftPageTest {

    private fun fixture(): JsonObject =
        Json.parseToJsonElement(contractFixtureJson("issue-draft.json")).jsonObject

    private fun assertCopy(expected: Map<String, String>, copy: JsonObject) {
        assertEquals(expected.keys, copy.keys)
        for ((key, value) in expected) {
            assertEquals(key, copy.getValue(key).jsonPrimitive.content, value)
        }
    }

    @Test
    fun `the copy is the contract's`() {
        val copy = fixture().getValue("copy").jsonObject
        val nested = setOf("leave")
        assertEquals(
            setOf("header", "titlePlaceholder", "descriptionPlaceholder", "create", "untitled", "discardedElsewhere") + nested,
            copy.keys,
        )
        assertCopy(
            mapOf(
                "header" to IssueDraftPage.HEADER,
                "titlePlaceholder" to IssueDraftPage.TITLE_PLACEHOLDER,
                "descriptionPlaceholder" to IssueDraftPage.DESCRIPTION_PLACEHOLDER,
                "create" to IssueDraftPage.CREATE,
                "untitled" to IssueDraftPage.UNTITLED,
                "discardedElsewhere" to IssueDraftPage.DISCARDED_ELSEWHERE,
            ),
            JsonObject(copy.filterKeys { it !in nested }),
        )
    }

    // EXP-1247: the close button and its confirm are gone (Back + Create only).
    @Test
    fun `the copy carries no discard button or discard confirm`() {
        val copy = fixture().getValue("copy").jsonObject
        assertFalse(copy.containsKey("discard"))
        assertFalse(copy.containsKey("discardConfirm"))
    }

    @Test
    fun `the leave prompt copy is the contract's`() {
        assertCopy(
            mapOf(
                "title" to IssueDraftPage.LEAVE_TITLE,
                "discard" to IssueDraftPage.LEAVE_DISCARD,
                "keep" to IssueDraftPage.LEAVE_KEEP,
                "create" to IssueDraftPage.LEAVE_CREATE,
            ),
            fixture().getValue("copy").jsonObject.getValue("leave").jsonObject,
        )
    }

    @Test
    fun `the autosave debounce is the contract's`() {
        val autosave = fixture().getValue("autosave").jsonObject
        assertEquals(autosave.getValue("debounceMs").jsonPrimitive.long, IssueDraftPage.AUTOSAVE_DEBOUNCE_MS)
    }

    @Test
    fun `a title, description or attachment is content`() {
        assertFalse(IssueDraftPage.hasContent("", "", 0))
        assertFalse(IssueDraftPage.hasContent("  ", "\n ", 0))
        assertTrue(IssueDraftPage.hasContent("Fix it", "", 0))
        assertTrue(IssueDraftPage.hasContent("", "Steps", 0))
        assertTrue(IssueDraftPage.hasContent("", "", 1))
    }

    @Test
    fun `a draft with content never goes silently`() {
        assertEquals(IssueDraftPage.Prompt.None, IssueDraftPage.prompt(false))
        assertEquals(IssueDraftPage.Prompt.Leave, IssueDraftPage.prompt(true))
    }

    @Test
    fun `a reopened draft counts as content until its files are known`() {
        assertTrue(IssueDraftPage.hasContent("", "", 0, attachmentsKnown = false))
        assertFalse(IssueDraftPage.hasContent("", "", 0, attachmentsKnown = true))
        assertEquals(
            IssueDraftPage.Prompt.Leave,
            IssueDraftPage.prompt(IssueDraftPage.hasContent("", "", 0, attachmentsKnown = false)),
        )
    }

    @Test
    fun `nothing is asked while a create is in flight`() {
        assertEquals(IssueDraftPage.Prompt.None, IssueDraftPage.prompt(true, creating = true))
    }

    @Test
    fun `a mode that writes no draft row offers no keep`() {
        assertEquals(
            listOf(IssueDraftPage.LeaveChoice.Discard, IssueDraftPage.LeaveChoice.Keep, IssueDraftPage.LeaveChoice.Create),
            IssueDraftPage.leaveChoices(canKeep = true),
        )
        assertEquals(
            listOf(IssueDraftPage.LeaveChoice.Discard, IssueDraftPage.LeaveChoice.Create),
            IssueDraftPage.leaveChoices(canKeep = false),
        )
    }

    @Test
    fun `create issue is the leave default unless it is disabled`() {
        val withKeep = IssueDraftPage.leaveChoices(canKeep = true)
        val noKeep = IssueDraftPage.leaveChoices(canKeep = false)
        assertEquals(IssueDraftPage.LeaveChoice.Create, IssueDraftPage.leaveDefault(withKeep, createEnabled = true))
        assertEquals(IssueDraftPage.LeaveChoice.Keep, IssueDraftPage.leaveDefault(withKeep, createEnabled = false))
        assertEquals(IssueDraftPage.LeaveChoice.Create, IssueDraftPage.leaveDefault(noKeep, createEnabled = true))
        // Never Discard: a disabled Create with no Keep has no default.
        assertEquals(null, IssueDraftPage.leaveDefault(noKeep, createEnabled = false))
    }

    @Test
    fun `create needs a title, a board and an idle page`() {
        assertTrue(IssueDraftPage.createEnabled("Fix it", hasBoard = true, creating = false, uploadsInFlight = 0))
        assertFalse(IssueDraftPage.createEnabled(" ", hasBoard = true, creating = false, uploadsInFlight = 0))
        assertFalse(IssueDraftPage.createEnabled("Fix it", hasBoard = false, creating = false, uploadsInFlight = 0))
        assertFalse(IssueDraftPage.createEnabled("Fix it", hasBoard = true, creating = true, uploadsInFlight = 0))
        assertFalse(IssueDraftPage.createEnabled("Fix it", hasBoard = true, creating = false, uploadsInFlight = 1))
    }

    @Test
    fun `the discarded grace is the contract's`() {
        val concurrency = fixture().getValue("concurrency").jsonObject
        assertEquals(concurrency.getValue("discardedGraceMs").jsonPrimitive.long, IssueDraftPage.DISCARDED_GRACE_MS)
    }

    // EXP-1231: an issue carrying the draft id wins; a seen row that is gone
    // is Gone; a never-seen row is never gone.
    @Test
    fun `the draft's fate follows the synced store`() {
        val open = IssueDraftPage.Fate.Open
        val gone = IssueDraftPage.Fate.Gone
        val created = IssueDraftPage.Fate.Created("issue-1")
        assertEquals(open, IssueDraftPage.fate(seen = false, present = false, createdIssueId = null))
        assertEquals(open, IssueDraftPage.fate(seen = true, present = true, createdIssueId = null))
        assertEquals(gone, IssueDraftPage.fate(seen = true, present = false, createdIssueId = null))
        assertEquals(created, IssueDraftPage.fate(seen = true, present = false, createdIssueId = "issue-1"))
        assertEquals(created, IssueDraftPage.fate(seen = true, present = true, createdIssueId = "issue-1"))
        assertEquals(created, IssueDraftPage.fate(seen = false, present = false, createdIssueId = "issue-1"))
    }
}
