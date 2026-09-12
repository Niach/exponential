package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * EXP-846: the rules that decide whether a transcript row is OURS, and what it
 * then says. The recognition half is byte-identical with the engine's
 * `exp_tool_row` (mapper.rs) — its cases are mirrored here on purpose.
 */
class ExpToolDisplayTest {

    @Test
    fun `our tools are recognised through every namespace`() {
        assertEquals("issues_create", ExpToolDisplay.row("mcp__exponential__exponential_issues_create"))
        assertEquals("pr_open", ExpToolDisplay.row("exponential_pr_open"))
        assertEquals("issues_list", ExpToolDisplay.row("exponential.exponential_issues_list"))
        // Another server's tool of the same name, a bare name, an ordinary
        // tool, and a name our contract does not carry: all none of ours.
        assertNull(ExpToolDisplay.row("mcp__linear__issues_create"))
        assertNull(ExpToolDisplay.row("issues_create"))
        assertNull(ExpToolDisplay.row("Bash"))
        assertNull(ExpToolDisplay.row("exponential_issues_invented"))
        assertNull(ExpToolDisplay.row(null))
        assertNull(ExpToolDisplay.row(" "))
    }

    @Test
    fun `the caption follows the tense of the call`() {
        val running = ExpToolDisplay.forName("mcp__exponential__exponential_issues_create", settled = false)
        assertEquals("Creating issue", running?.caption)
        // The subject is the INPUT field the contract names — a created
        // issue's title — and the answer is an issue, so the row previews one.
        assertEquals("title", running?.subjectKey)
        assertEquals(ExpToolDisplay.RESULT_ISSUE, running?.result)
        assertEquals(
            "Created issue",
            ExpToolDisplay.forName("exponential_issues_create", settled = true)?.caption,
        )
    }

    /** A list answers with rows and names no subject; `none` previews nothing. */
    @Test
    fun `list and subject-less tools say so`() {
        val list = ExpToolDisplay.forName("exponential_issues_list", settled = true)
        assertEquals("Listed issues", list?.caption)
        assertEquals("", list?.subjectKey)
        assertEquals(ExpToolDisplay.RESULT_LIST, list?.result)
        val none = ExpToolDisplay.forName("exponential_issues_delete", settled = true)
        assertEquals(ExpToolDisplay.RESULT_NONE, none?.result)
    }

    /** Every contract row resolves — a table that drifted out of step with
     *  `expToolNames` would silently caption a call with its own row name. */
    @Test
    fun `every contract row has a caption, a subject key and a result`() {
        DomainContract.expToolNames.forEach { row ->
            val display = ExpToolDisplay.forName("exponential_$row", settled = true)
                ?: error("$row is not recognised")
            assertEquals(row, display.row)
            org.junit.Assert.assertNotEquals(row, display.caption)
            org.junit.Assert.assertTrue(
                "$row has an unknown result kind: ${display.result}",
                display.result in DomainContract.expToolResultKinds,
            )
        }
    }
}
