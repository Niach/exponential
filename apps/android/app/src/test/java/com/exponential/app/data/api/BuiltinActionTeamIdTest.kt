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

    @Test
    fun `synced rows never key a teamId`() {
        val synced = ActionDto(id = "a3f0c9d2-0000-0000-0000-000000000000", teamId = teamId, name = "Deploy")
        assertNull(synced.teamId.takeIf { synced.isBuiltin })
    }
}
