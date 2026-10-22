package com.exponential.app.ui.agent

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * EXP-825: the composer field's placeholder — web `composerPlaceholder` byte
 * for byte. No subject asks for the message; any subject (issue chips or,
 * NEW-2, a picked prompt, which carries no hint any more) the generic prompt.
 */
class ComposerPlaceholderTest {

    @Test
    fun `no subject asks the agent`() {
        assertEquals("Ask the agent…", composerPlaceholder(null))
    }

    @Test
    fun `the agent home asks composer-home json's question`() {
        // EXP-1253: the home column's placeholder; a subject still reads its own.
        assertEquals("What do you want to build?", composerPlaceholder(null, home = true))
        assertEquals(
            "Additional instructions (optional)…",
            composerPlaceholder(ComposerSubject.Issues(listOf("i-1")), home = true),
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
        assertEquals("Additional instructions (optional)…", composerPlaceholder(issues))
        assertEquals(
            "Additional instructions (optional)…",
            composerPlaceholder(ComposerSubject.Issues(listOf("i-1", "i-2"))),
        )
    }

    @Test
    fun `a picked prompt asks for optional instructions`() {
        val picked = ComposerSubject.Prompt("p-1")
        assertEquals("Additional instructions (optional)…", composerPlaceholder(picked))
        assertEquals("Additional instructions (optional)…", composerPlaceholder(picked, home = true))
    }
}
