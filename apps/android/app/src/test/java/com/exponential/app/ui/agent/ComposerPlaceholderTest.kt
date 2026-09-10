package com.exponential.app.ui.agent

import com.exponential.app.data.api.ActionDto
import com.exponential.app.data.api.builtinCreateAction
import com.exponential.app.data.api.builtinFixConflictsAction
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

    @Test
    fun `the builtins follow the same rule`() {
        val create = builtinCreateAction("t-1")
        assertEquals(
            "Describe the action — what it should do, and its name if you have one…",
            composerPlaceholder(ComposerSubject.Action(create.id, emptyMap()), create),
        )
        val fix = builtinFixConflictsAction("t-1")
        assertEquals(
            "Additional instructions (optional)…",
            composerPlaceholder(ComposerSubject.Action(fix.id, emptyMap()), fix),
        )
    }
}
