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
 *
 * NEW-2 (owner 2026-10-21): the catalog = the contract builtins ONLY
 * (compact, model, effort; clear and btw are gone) and commands the agent CLI
 * advertises (`config_state.commands`) are never merged in: the API has no
 * path for them at all.
 */
class SlashCommandsTest {

    @Test
    fun `the catalog is the generated contract, filtered by the run's agent`() {
        assertEquals(DomainContract.steerCommandNames, SlashCommands.all.map { it.name })
        assertEquals(listOf("compact", "model", "effort"), SlashCommands.all.map { it.name })
        // A row with no agent recorded ran the default one.
        assertEquals(
            SlashCommands.catalogFor("claude").map { it.name },
            SlashCommands.catalogFor(null).map { it.name },
        )
        assertEquals("claude", DomainContract.codingAgentValues.first())
        assertEquals(listOf("compact", "model", "effort"), SlashCommands.catalogFor("claude").map { it.name })
        assertEquals(listOf("compact", "model", "effort"), SlashCommands.catalogFor("codex").map { it.name })
        // A retired / unknown agent id's catalog is EMPTY rather than claude's.
        assertTrue(SlashCommands.catalogFor("pi").isEmpty())
        // Nothing the catalog offers discards context any more.
        assertTrue(SlashCommands.all.none { it.confirm })
        // clear and btw are gone.
        assertNull(SlashCommands.commandFor("/clear", "claude"))
        assertNull(SlashCommands.commandFor("/btw why?", "claude"))
    }

    @Test
    fun `the menu opens only while the whole draft is a command being typed`() {
        // A bare slash offers every row for the agent.
        assertEquals(SlashCommands.catalogFor("claude"), SlashCommands.matches("/", "claude"))
        // Case-insensitive name PREFIX, in catalog order.
        assertEquals(listOf("compact"), SlashCommands.matches("/comp", "claude").map { it.name })
        assertEquals(listOf("compact"), SlashCommands.matches("/COMP", "claude").map { it.name })
        assertEquals(listOf("model"), SlashCommands.matches("/mo", "claude").map { it.name })
        assertEquals(listOf("effort"), SlashCommands.matches("/e", "codex").map { it.name })
        // Not a prefix of anything.
        assertTrue(SlashCommands.matches("/zzz", "claude").isEmpty())
        // Outside the catalog, whatever the CLI ships.
        assertTrue(SlashCommands.matches("/new", "codex").isEmpty())
        assertTrue(SlashCommands.matches("/cl", "claude").isEmpty())
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
        for (command in SlashCommands.all) {
            val expected = if (command.argHint.isEmpty()) "/${command.name}" else "/${command.name} "
            assertEquals(expected, command.insertion)
        }
        assertEquals("/compact ", SlashCommands.all.single { it.name == "compact" }.insertion)
    }

    @Test
    fun `a message is a command iff its first token is a name for the agent`() {
        assertEquals("compact", SlashCommands.commandFor("/compact", "claude")?.name)
        assertEquals("compact", SlashCommands.commandFor("/compact keep the diff", "claude")?.name)
        assertEquals("compact", SlashCommands.commandFor("  /Compact  ", "codex")?.name)
        assertEquals("model", SlashCommands.commandFor("/model opus", "claude")?.name)
        // A run with no recorded agent is claude's.
        assertEquals("effort", SlashCommands.commandFor("/effort high", null)?.name)
        // Not in the catalog, whatever the CLI ships.
        assertNull(SlashCommands.commandFor("/cost", "claude"))
        assertNull(SlashCommands.commandFor("/new", "codex"))
        // A retired agent id names no command at all.
        assertNull(SlashCommands.commandFor("/compact", "pi"))
        // Prose, paths and a bare slash are never commands.
        assertNull(SlashCommands.commandFor("fix /compact later", "claude"))
        assertNull(SlashCommands.commandFor("/api/foo", "claude"))
        assertNull(SlashCommands.commandFor("/", "claude"))
        assertNull(SlashCommands.commandFor("//compact", "claude"))
        assertNull(SlashCommands.commandFor("compact", "claude"))
    }

    /**
     * EXP-746, mirrored ×4 under this name: "no agent" alone does not mean
     * claude. An EXTERNAL ACP agent syncs no agent either, so its catalog is
     * empty — and with agent commands never merged (NEW-2) its menu is shut.
     */
    @Test
    fun `an agent-less acp run is an external agent`() {
        assertFalse(SlashCommands.EXTERNAL_AGENT in DomainContract.codingAgentValues)
        assertEquals(SlashCommands.EXTERNAL_AGENT, SlashCommands.agentId(null, acp = true))
        assertEquals(SlashCommands.EXTERNAL_AGENT, SlashCommands.agentId("  ", acp = true))
        // A named agent keeps its own catalog on the ACP path.
        assertEquals("codex", SlashCommands.agentId("codex", acp = true))
        assertEquals("claude", SlashCommands.agentId(null, acp = false))
        assertEquals("claude", SlashCommands.agentId("", acp = false))

        val external = SlashCommands.agentId(null, acp = true)
        assertTrue(SlashCommands.catalogFor(external).isEmpty())
        assertTrue(SlashCommands.matches("/", external).isEmpty())
        assertNull(SlashCommands.commandFor("/compact", external))
    }

    @Test
    fun `the confirm copy is the one every client shows`() {
        assertEquals("Run /clear?", SlashCommands.confirmTitle("clear"))
        assertEquals("Run /clear", SlashCommands.confirmButton("clear"))
        assertEquals("Run /compact?", SlashCommands.confirmTitle("compact"))
        assertEquals(
            "The agent forgets everything in this session so far. " +
                "Files in the worktree are kept.",
            SlashCommands.CONFIRM_BODY,
        )
    }

    // NEW-2: the team's prompts join the `/` menu after the builtins, keyed
    // by their slash name (web `promptSlashName` / `filterSlashPrompts`).
    @Test
    fun `a prompt answers to its name lowercased with dashes`() {
        assertEquals("fix-flaky-tests", SlashCommands.promptSlashName("Fix flaky tests"))
        assertEquals("fix-flaky-tests", SlashCommands.promptSlashName("  Fix   flaky\ttests "))
        assertEquals("review", SlashCommands.promptSlashName("Review"))
    }

    @Test
    fun `prompts filter by slash-name prefix in team order`() {
        val prompts = listOf(
            SlashPrompt("p1", "Fix flaky tests"),
            SlashPrompt("p2", "Review PR"),
            SlashPrompt("p3", "Fix lint"),
        )
        assertEquals(listOf("p1", "p2", "p3"), SlashCommands.promptMatches("/", prompts).map { it.id })
        assertEquals(listOf("p1", "p3"), SlashCommands.promptMatches("/fix", prompts).map { it.id })
        assertEquals(listOf("p1"), SlashCommands.promptMatches("/FIX-FL", prompts).map { it.id })
        // An argument (a space) or prose closes the menu.
        assertTrue(SlashCommands.promptMatches("/fix ", prompts).isEmpty())
        assertTrue(SlashCommands.promptMatches("x /fix", prompts).isEmpty())
    }

    @Test
    fun `the menu lists the builtins then the prompts`() {
        val prompts = listOf(SlashPrompt("p1", "Compact notes"))
        val rows = SlashCommands.candidates("/comp", "claude", prompts)
        assertEquals(listOf("command:compact", "prompt:p1"), rows.map { it.key })
    }
}
