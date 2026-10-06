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
        val nested = setOf("discardConfirm", "leave")
        assertEquals(
            setOf("header", "titlePlaceholder", "descriptionPlaceholder", "create", "discard", "untitled") + nested,
            copy.keys,
        )
        assertCopy(
            mapOf(
                "header" to IssueDraftPage.HEADER,
                "titlePlaceholder" to IssueDraftPage.TITLE_PLACEHOLDER,
                "descriptionPlaceholder" to IssueDraftPage.DESCRIPTION_PLACEHOLDER,
                "create" to IssueDraftPage.CREATE,
                "discard" to IssueDraftPage.DISCARD,
                "untitled" to IssueDraftPage.UNTITLED,
            ),
            JsonObject(copy.filterKeys { it !in nested }),
        )
    }

    @Test
    fun `the discard confirm copy is the contract's`() {
        assertCopy(
            mapOf(
                "title" to IssueDraftPage.DISCARD_CONFIRM_TITLE,
                "confirm" to IssueDraftPage.DISCARD_CONFIRM,
            ),
            fixture().getValue("copy").jsonObject.getValue("discardConfirm").jsonObject,
        )
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
        val exits = IssueDraftPage.Exit.entries
        for (exit in exits) assertEquals(IssueDraftPage.Prompt.None, IssueDraftPage.prompt(false, exit))
        assertEquals(IssueDraftPage.Prompt.DiscardConfirm, IssueDraftPage.prompt(true, IssueDraftPage.Exit.Discard))
        assertEquals(IssueDraftPage.Prompt.Leave, IssueDraftPage.prompt(true, IssueDraftPage.Exit.Leave))
    }

    @Test
    fun `a reopened draft counts as content until its files are known`() {
        assertTrue(IssueDraftPage.hasContent("", "", 0, attachmentsKnown = false))
        assertFalse(IssueDraftPage.hasContent("", "", 0, attachmentsKnown = true))
        assertEquals(
            IssueDraftPage.Prompt.Leave,
            IssueDraftPage.prompt(IssueDraftPage.hasContent("", "", 0, attachmentsKnown = false), IssueDraftPage.Exit.Leave),
        )
    }

    @Test
    fun `nothing is asked while a create is in flight`() {
        for (exit in IssueDraftPage.Exit.entries) {
            assertEquals(IssueDraftPage.Prompt.None, IssueDraftPage.prompt(true, exit, creating = true))
        }
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
        // Its close button still confirms a discard with content.
        assertEquals(IssueDraftPage.Prompt.DiscardConfirm, IssueDraftPage.prompt(true, IssueDraftPage.Exit.Discard))
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
}
