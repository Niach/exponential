package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-262: the NOTICES.txt splitter. The renderer's format contract
 * (packages/licenses/src/render.ts): headings are rule sandwiches — a
 * full-width rule (78 `=` or `-`), one or two non-empty title lines, a
 * matching closing rule. Licence bodies are verbatim, so anything else —
 * short dashes, unmatched rules, markdown `##` — must stay body text.
 */
class ThirdPartyNoticesTest {
    private val eq = "=".repeat(78)
    private val dash = "-".repeat(78)

    @Test
    fun preambleOnlyBecomesOneTitlelessSection() {
        val sections = ThirdPartyNotices.parse("Some preamble text.\n\nMore preamble.\n")
        assertEquals(1, sections.size)
        assertEquals("", sections[0].title)
        assertEquals("Some preamble text.\n\nMore preamble.", sections[0].body)
    }

    @Test
    fun splitsOnRuleSandwiches() {
        val sections = ThirdPartyNotices.parse(
            """
            $eq
            EXPONENTIAL — THIRD-PARTY NOTICES
            Android application
            $eq

            Preamble prose.

            $eq
            1. Open-source components
            $eq

            Intro.

            $dash
            MIT
            $dash

            MIT License text.
            """.trimIndent(),
        )
        assertEquals(3, sections.size)
        assertEquals("EXPONENTIAL — THIRD-PARTY NOTICES — Android application", sections[0].title)
        assertEquals("Preamble prose.", sections[0].body)
        assertEquals("1. Open-source components", sections[1].title)
        assertEquals("Intro.", sections[1].body)
        assertEquals("MIT", sections[2].title)
        assertEquals("MIT License text.", sections[2].body)
    }

    @Test
    fun shortOrUnmatchedRulesStayBodyText() {
        val sections = ThirdPartyNotices.parse(
            """
            $dash
            Apache-2.0
            $dash

            A licence body with markdown-ish content:

            ## not a heading
            ---
            --------
            $eq
            (an unmatched full-width rule stays body text too)
            """.trimIndent(),
        )
        assertEquals(1, sections.size)
        assertEquals("Apache-2.0", sections[0].title)
        assertTrue(sections[0].body.contains("## not a heading"))
        assertTrue(sections[0].body.contains("--------"))
        assertTrue(sections[0].body.contains(eq))
    }

    @Test
    fun mismatchedSandwichCharactersDoNotSplit() {
        // An `=` rule closed by a `-` rule is not a heading.
        val sections = ThirdPartyNotices.parse("$eq\nnot a title\n$dash\nbody")
        assertEquals(1, sections.size)
        assertEquals("", sections[0].title)
        assertTrue(sections[0].body.contains("not a title"))
    }

    @Test
    fun toleratesCrlfLineEndings() {
        val sections = ThirdPartyNotices.parse("Preamble.\r\n$dash\r\nMIT\r\n$dash\r\nLicence body.\r\n")
        assertEquals(2, sections.size)
        assertEquals("Preamble.", sections[0].body)
        assertEquals("MIT", sections[1].title)
        assertEquals("Licence body.", sections[1].body)
    }
}
