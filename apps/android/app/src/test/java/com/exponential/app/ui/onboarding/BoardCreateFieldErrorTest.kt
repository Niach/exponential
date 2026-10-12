package com.exponential.app.ui.onboarding

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** FEED-79 (web `boardCreateFieldError`): a refused create names a field, or it is a toast. */
class BoardCreateFieldErrorTest {
    @Test
    fun aTakenNameOrPrefixStaysUnderTheField() {
        val name = "A board with this name already exists in this team"
        val prefix = "A board with this prefix already exists in this team"
        assertEquals(name, boardCreateFieldError(name))
        assertEquals(prefix, boardCreateFieldError(prefix))
    }

    @Test
    fun theZodPrefixRuleReadsAsTheRule() {
        val raw = """[{"code":"invalid_string","message":"Prefix must be 1-4 letters or digits, starting with a letter"}]"""
        assertEquals("Prefix must be 1-4 letters or digits, starting with a letter", boardCreateFieldError(raw))
    }

    @Test
    fun anythingElseIsAFailedAction() {
        assertNull(boardCreateFieldError("You're offline. Check your connection and try again."))
        assertNull(boardCreateFieldError(""))
    }
}
