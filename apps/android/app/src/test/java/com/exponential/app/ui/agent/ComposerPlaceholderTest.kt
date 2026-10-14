package com.exponential.app.ui.agent

import com.exponential.app.data.api.ActionDto
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-825: the composer field's placeholder — web `composerPlaceholder` byte
 * for byte. No subject asks for the message; a picked action with a
 * non-blank hint shows the hint (trimmed); everything else the generic prompt.
 */
class ComposerPlaceholderTest {

    private val hinted = ActionDto(
        id = "a-1",
        teamId = "t-1",
        name = "Release",
        promptPlaceholder = "Scope: which platforms, which version",
    )
    private val plain = ActionDto(id = "a-2", teamId = "t-1", name = "Sweep")

    @Test
    fun `no subject asks the agent`() {
        assertEquals("Ask the agent…", composerPlaceholder(null, null))
        // A stale selected row never leaks onto a subject-less composer.
        assertEquals("Ask the agent…", composerPlaceholder(null, hinted))
    }

    @Test
    fun `the agent home asks composer-home json's question`() {
        // EXP-1253: the home column's placeholder; a subject still reads its own.
        assertEquals("What do you want to build?", composerPlaceholder(null, null, home = true))
        assertEquals(
            "Additional instructions (optional)…",
            composerPlaceholder(ComposerSubject.Issues(listOf("i-1")), null, home = true),
        )
    }

    @Test
    fun `the starting row's clock reads tenths of a second`() {
        // EXP-1271: web `startingElapsed` — "1.4 s", never negative.
        assertEquals("0.0 s", startingElapsed(-50))
        assertEquals("1.4 s", startingElapsed(1_420))
        assertEquals("30.0 s", startingElapsed(30_000))
    }

    @Test
    fun `issue chips ask for optional instructions`() {
        val issues = ComposerSubject.Issues(listOf("i-1"))
        assertEquals("Additional instructions (optional)…", composerPlaceholder(issues, null))
        assertEquals("Additional instructions (optional)…", composerPlaceholder(issues, hinted))
        assertEquals(
            "Additional instructions (optional)…",
            composerPlaceholder(ComposerSubject.Issues(listOf("i-1", "i-2")), null),
        )
    }

    @Test
    fun `a picked action shows its hint or the generic prompt`() {
        val picked = ComposerSubject.Action("a-1", emptyMap())
        assertEquals("Scope: which platforms, which version", composerPlaceholder(picked, hinted))
        assertEquals("Additional instructions (optional)…", composerPlaceholder(picked, plain))
        // Not synced yet: the generic prompt, never a crash.
        assertEquals("Additional instructions (optional)…", composerPlaceholder(picked, null))
    }

    @Test
    fun `blank hints fall back and padded hints trim`() {
        val picked = ComposerSubject.Action("a-1", emptyMap())
        assertEquals(
            "Additional instructions (optional)…",
            composerPlaceholder(picked, plain.copy(promptPlaceholder = "  \n ")),
        )
        assertEquals(
            "Which version?",
            composerPlaceholder(picked, plain.copy(promptPlaceholder = "  Which version?  ")),
        )
    }
}
