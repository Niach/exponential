package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-1170: the New issue page's copy + autosave debounce, locked ×4 against
 * `issue-draft.json` (web `issue-draft-page.test.ts`, iOS, desktop).
 */
class IssueDraftPageTest {

    private fun fixture(): JsonObject =
        Json.parseToJsonElement(contractFixtureJson("issue-draft.json")).jsonObject

    @Test
    fun `the copy is the contract's`() {
        val copy = fixture().getValue("copy").jsonObject
        val expected = mapOf(
            "header" to IssueDraftPage.HEADER,
            "titlePlaceholder" to IssueDraftPage.TITLE_PLACEHOLDER,
            "descriptionPlaceholder" to IssueDraftPage.DESCRIPTION_PLACEHOLDER,
            "create" to IssueDraftPage.CREATE,
            "discard" to IssueDraftPage.DISCARD,
            "untitled" to IssueDraftPage.UNTITLED,
        )
        assertEquals(expected.keys, copy.keys)
        for ((key, value) in expected) {
            assertEquals(key, copy.getValue(key).jsonPrimitive.content, value)
        }
    }

    @Test
    fun `the autosave debounce is the contract's`() {
        val autosave = fixture().getValue("autosave").jsonObject
        assertEquals(autosave.getValue("debounceMs").jsonPrimitive.long, IssueDraftPage.AUTOSAVE_DEBOUNCE_MS)
    }
}
