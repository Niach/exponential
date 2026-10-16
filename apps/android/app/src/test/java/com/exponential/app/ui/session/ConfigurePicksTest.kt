package com.exponential.app.ui.session

import com.exponential.app.data.db.SessionEntity
import com.exponential.app.ui.session.AgentSessionViewModel.ConfigurePicks
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1334: the session composer's configure picks over the synced row. A
 * pick leads the row, then goes once the row says it (web `dropSettled`);
 * the MCP pick has no row column and stays.
 */
class ConfigurePicksTest {

    private fun row(
        planMode: Boolean = false,
        effort: String? = null,
        ultracode: Boolean = false,
        computerUse: Boolean = false,
    ) = SessionEntity(id = "s1", planMode = planMode, effort = effort, ultracode = ultracode, computerUse = computerUse)

    @Test
    fun `a change lays its keys over the picks and keeps the rest`() {
        val current = ConfigurePicks(planMode = true, mcpServerIds = listOf("a"))
        assertEquals(
            ConfigurePicks(planMode = true, effort = "high", mcpServerIds = listOf("a")),
            current.overriddenBy(ConfigurePicks(effort = "high")),
        )
        assertEquals(
            ConfigurePicks(planMode = false, mcpServerIds = listOf("a")),
            current.overriddenBy(ConfigurePicks(planMode = false)),
        )
    }

    @Test
    fun `the row confirming a pick drops it`() {
        val picks = ConfigurePicks(planMode = true, effort = "high")
        assertEquals(ConfigurePicks(effort = "high"), picks.dropSettledBy(row(planMode = true)))
        assertEquals(ConfigurePicks(), picks.dropSettledBy(row(planMode = true, effort = "high")))
        // The row still says the old value: the pick leads.
        assertEquals(picks, picks.dropSettledBy(row()))
    }

    @Test
    fun `the CLI default effort is the row's null effort`() {
        val picks = ConfigurePicks(effort = "")
        assertEquals(ConfigurePicks(), picks.dropSettledBy(row(effort = null)))
        assertEquals(picks, picks.dropSettledBy(row(effort = "high")))
    }

    @Test
    fun `a later pick survives the earlier one settling`() {
        val current = ConfigurePicks(planMode = false)
        assertEquals(current, current.dropSettled(ConfigurePicks(planMode = true)))
        assertEquals(ConfigurePicks(), current.dropSettled(ConfigurePicks(planMode = false)))
    }

    @Test
    fun `the MCP pick never settles`() {
        val picks = ConfigurePicks(ultracode = true, mcpServerIds = listOf("a", "b"))
        assertEquals(
            ConfigurePicks(mcpServerIds = listOf("a", "b")),
            picks.dropSettledBy(row(ultracode = true)),
        )
        assertEquals(
            ConfigurePicks(mcpServerIds = listOf("a", "b")),
            picks.dropSettled(ConfigurePicks(ultracode = true, mcpServerIds = listOf("a", "b"))),
        )
        assertFalse(ConfigurePicks(mcpServerIds = listOf("a")).hasRowPick())
        assertTrue(ConfigurePicks(computerUse = true).hasRowPick())
        assertEquals(ConfigurePicks(computerUse = true), ConfigurePicks(computerUse = true, mcpServerIds = listOf("a")).rowPicks())
    }
}
