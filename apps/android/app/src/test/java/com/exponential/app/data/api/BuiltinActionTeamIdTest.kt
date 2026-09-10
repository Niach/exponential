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

    /**
     * Its one input is the server's, byte for byte (web builtin-actions.ts).
     * EXP-825: the chat text is the start's `prompt`, never an input; EXP-739
     * made the repo OPTIONAL: a repo-less chat runs worktree-less in a
     * scratch dir.
     */
    @Test
    fun `chat declares only an optional repository`() {
        val inputs = builtinChatAction(teamId).inputs.orEmpty()
        assertEquals(listOf("repo"), inputs.map { it.key })
        assertEquals(false, inputs[0].required)
        assertEquals("Repository", inputs[0].label)
        assertEquals("repo", inputs[0].type)
    }

    /**
     * EXP-825: the request (and a stated name) is the start's `prompt`; the
     * creator keeps only the two PICKS it cannot derive from prose. Every
     * builtin input is a pick type the contract still knows.
     */
    @Test
    fun `create action offers only the repo and icon picks`() {
        val inputs = builtinCreateAction(teamId).inputs.orEmpty()
        assertEquals(listOf("repo", "icon"), inputs.map { it.key })
        assertEquals(listOf("Repository", "Icon"), inputs.map { it.label })
        assertEquals(listOf("repo", "icon"), inputs.map { it.type })
        assertTrue(inputs.none { it.required })
        val every = builtinActions(teamId) + builtinChatAction(teamId)
        every.flatMap { it.inputs.orEmpty() }.forEach { def ->
            assertTrue(def.key, def.type in DomainContract.actionInputTypeValues)
        }
    }

    @Test
    fun `synced rows never key a teamId`() {
        val synced = ActionDto(id = "a3f0c9d2-0000-0000-0000-000000000000", teamId = teamId, name = "Deploy")
        assertNull(synced.teamId.takeIf { synced.isBuiltin })
    }
}
