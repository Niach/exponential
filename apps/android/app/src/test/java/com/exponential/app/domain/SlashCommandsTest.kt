package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-724: the `/` menu's matching rules and its copy. All four clients
 * implement the SAME rules over the SAME contract rows (desktop
 * `steer::commands`, web `lib/steer-commands.ts`, iOS `SlashCommands.swift`),
 * so this file is the Android half of a four-way lock — the literals below are
 * asserted byte-for-byte on purpose.
 */
class SlashCommandsTest {

    @Test
    fun `the catalog is the contract, filtered by the run's agent`() {
        assertEquals(DomainContract.steerCommandNames, SlashCommands.all.map { it.name })
        // A row with no agent recorded ran the default one.
        assertEquals(
            SlashCommands.catalogFor("claude").map { it.name },
            SlashCommands.catalogFor(null).map { it.name },
        )
        assertEquals("claude", DomainContract.codingAgentValues.first())

        val claude = SlashCommands.catalogFor("claude").map { it.name }
        val codex = SlashCommands.catalogFor("codex").map { it.name }
        val pi = SlashCommands.catalogFor("pi").map { it.name }
        // /compact is the one every agent has.
        assertTrue("compact" in claude && "compact" in codex && "compact" in pi)
        // /clear is claude's name for it, /new is codex's and pi's — both
        // discard the conversation, so both confirm.
        assertTrue("clear" in claude)
        assertFalse("new" in claude)
        assertTrue("new" in codex)
        assertFalse("clear" in codex)
        assertTrue(SlashCommands.all.single { it.name == "clear" }.confirm)
        assertTrue(SlashCommands.all.single { it.name == "new" }.confirm)
        assertFalse(SlashCommands.all.single { it.name == "compact" }.confirm)
        // Contract order is menu order.
        assertEquals(claude, DomainContract.steerCommandNames.filter { it in claude })
    }

    @Test
    fun `the menu opens only while the whole draft is a command being typed`() {
        // A bare slash offers every row for the agent.
        assertEquals(
            SlashCommands.catalogFor("claude"),
            SlashCommands.matches("/", "claude"),
        )
        // Case-insensitive name PREFIX, in catalog order.
        assertEquals(listOf("compact"), SlashCommands.matches("/comp", "claude").map { it.name })
        assertEquals(listOf("compact"), SlashCommands.matches("/COMP", "claude").map { it.name })
        // Contract order, not alphabetical.
        assertEquals(listOf("compact", "clear"), SlashCommands.matches("/c", "claude").map { it.name })
        // Not a prefix of anything.
        assertTrue(SlashCommands.matches("/zzz", "claude").isEmpty())
        // Not this agent's command.
        assertTrue(SlashCommands.matches("/clear", "codex").isEmpty())
        // The slash must open the draft, and the first whitespace closes the
        // menu — the argument is being typed now.
        assertTrue(SlashCommands.matches("", "claude").isEmpty())
        assertTrue(SlashCommands.matches("hi /comp", "claude").isEmpty())
        assertTrue(SlashCommands.matches(" /comp", "claude").isEmpty())
        assertTrue(SlashCommands.matches("/compact ", "claude").isEmpty())
        assertTrue(SlashCommands.matches("/compact keep the diff", "claude").isEmpty())
        assertTrue(SlashCommands.matches("/api/foo", "claude").isEmpty())
    }

    @Test
    fun `accepting a row inserts a trailing space only when there is an argument`() {
        assertEquals("/compact ", SlashCommands.all.single { it.name == "compact" }.insertion)
        assertEquals("/model ", SlashCommands.all.single { it.name == "model" }.insertion)
        assertEquals("/clear", SlashCommands.all.single { it.name == "clear" }.insertion)
        assertEquals("/init", SlashCommands.all.single { it.name == "init" }.insertion)
    }

    @Test
    fun `a message is a command iff its first token is a name for the agent`() {
        assertEquals("compact", SlashCommands.commandFor("/compact", "claude")?.name)
        assertEquals("compact", SlashCommands.commandFor("/compact keep the diff", "claude")?.name)
        assertEquals("compact", SlashCommands.commandFor("  /Compact  ", "pi")?.name)
        // A run with no recorded agent is claude's.
        assertEquals("clear", SlashCommands.commandFor("/clear", null)?.name)
        // Not in the catalog, or not for this agent.
        assertNull(SlashCommands.commandFor("/cost", "claude"))
        assertNull(SlashCommands.commandFor("/clear", "codex"))
        assertNull(SlashCommands.commandFor("/model opus", "codex"))
        assertEquals("model", SlashCommands.commandFor("/model opus", "claude")?.name)
        // Prose, paths and a bare slash are never commands.
        assertNull(SlashCommands.commandFor("fix /compact later", "claude"))
        assertNull(SlashCommands.commandFor("/api/foo", "claude"))
        assertNull(SlashCommands.commandFor("/", "claude"))
        assertNull(SlashCommands.commandFor("//compact", "claude"))
        assertNull(SlashCommands.commandFor("compact", "claude"))
    }

    @Test
    fun `the confirm copy is the one every client shows`() {
        assertEquals("Run /clear?", SlashCommands.confirmTitle("clear"))
        assertEquals("Run /clear", SlashCommands.confirmButton("clear"))
        assertEquals("Run /new?", SlashCommands.confirmTitle("new"))
        assertEquals(
            "The agent forgets everything in this session so far. " +
                "Files in the worktree are kept.",
            SlashCommands.CONFIRM_BODY,
        )
    }
}
