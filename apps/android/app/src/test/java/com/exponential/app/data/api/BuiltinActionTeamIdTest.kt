package com.exponential.app.data.api

import com.exponential.app.domain.DomainContract
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-270: every runAction call site keys the steer `teamId` on
 * `action.teamId.takeIf { action.isBuiltin }` — the server REQUIRES teamId for
 * every builtin (there is no DB row to derive the team from) and forbids it
 * otherwise. This locks the predicate: BOTH virtual builtins ("Create action"
 * AND "Fix merge conflicts") must key as builtin — keying on the create-action
 * id alone regressed fix-conflicts starts to BAD_REQUEST.
 */
class BuiltinActionTeamIdTest {

    private val teamId = "team-1"

    @Test
    fun `every builtin row keys teamId for the steer start`() {
        val builtins = builtinActions(teamId)
        assertEquals(2, builtins.size)
        builtins.forEach { action ->
            assertTrue("${action.id} must report isBuiltin", action.isBuiltin)
            // The exact keying expression used by all four runAction call sites.
            assertEquals(teamId, action.teamId.takeIf { action.isBuiltin })
        }
    }

    @Test
    fun `fix-conflicts is builtin even though it is not the create-action id`() {
        val fixConflicts = builtinFixConflictsAction(teamId)
        assertTrue(fixConflicts.isBuiltin)
        assertTrue(fixConflicts.id != DomainContract.builtinCreateActionId)
        assertEquals(DomainContract.builtinFixConflictsId, fixConflicts.id)
    }

    /**
     * EXP-615: the chat builtin is HIDDEN. It keys a teamId like every other
     * builtin (the server has no row to derive the team from), but it must
     * never reach a list — a "Chat" row in the Actions picker would start a
     * run no old desktop can take.
     */
    @Test
    fun `chat is a builtin that never appears in a list`() {
        val chat = builtinChatAction(teamId)
        assertTrue(chat.isBuiltin)
        assertEquals(DomainContract.builtinChatId, chat.id)
        assertEquals(teamId, chat.teamId.takeIf { chat.isBuiltin })
        assertTrue(builtinActions(teamId).none { it.id == chat.id })
    }

    /** Its two inputs are the server's, byte for byte (web builtin-actions.ts). */
    @Test
    fun `chat declares a required prompt and repository`() {
        val inputs = builtinChatAction(teamId).inputs.orEmpty()
        assertEquals(listOf("prompt", "repo"), inputs.map { it.key })
        assertTrue(inputs.all { it.required })
        assertEquals("Prompt", inputs[0].label)
        assertEquals("What should the agent do?", inputs[0].placeholder)
        assertEquals("textarea", inputs[0].type)
        assertEquals("Repository", inputs[1].label)
        assertEquals("repo", inputs[1].type)
    }

    /** EXP-615: the optional name rides between description and repo. */
    @Test
    fun `create action offers an optional name input`() {
        val inputs = builtinCreateAction(teamId).inputs.orEmpty()
        assertEquals(listOf("description", "name", "repo", "icon"), inputs.map { it.key })
        val name = inputs[1]
        assertEquals("Name", name.label)
        assertEquals("text", name.type)
        assertEquals(false, name.required)
        assertEquals("Name (optional)", name.placeholder)
    }

    @Test
    fun `synced rows never key a teamId`() {
        val synced = ActionDto(id = "a3f0c9d2-0000-0000-0000-000000000000", teamId = teamId, name = "Deploy")
        assertNull(synced.teamId.takeIf { synced.isBuiltin })
    }
}
