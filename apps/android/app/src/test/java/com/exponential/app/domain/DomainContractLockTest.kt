package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Locks the hand-maintained enums in IssueDomain.kt to the generated contract
 * constants (DomainContract.generated.kt, regenerated from
 * packages/domain-contract/contract.json) — mirrors the desktop's tests in
 * crates/domain/src/enums.rs. A contract regen that adds/renames/reorders a
 * value fails here until the hand-written enum is updated in lockstep.
 */
class DomainContractLockTest {

    @Test
    fun issueStatusWireValuesMatchGeneratedContract() {
        assertEquals(
            DomainContract.issueStatusValues,
            IssueStatus.entries.map { it.wire },
        )
        // Every canonical value round-trips through fromWire.
        for (value in DomainContract.issueStatusValues) {
            assertEquals(value, IssueStatus.fromWire(value).wire)
        }
    }

    /**
     * EXP-685 retired the `todo` builtin from the contract, but stored timeline
     * payloads still carry the bare wire value — it keeps reading "Todo", while
     * every OTHER unknown value takes the forward-compat path.
     */
    @Test
    fun retiredStatusWireValuesKeepTheirHistoricLabel() {
        assertEquals("Todo", IssueStatus.labelFor("todo"))
        assertEquals(IssueStatus.Backlog, IssueStatus.fromWire("todo"))
        assertEquals(IssueStatus.Backlog, IssueStatus.fromWire("blocked"))
        assertEquals("blocked", IssueStatus.labelFor("blocked"))
    }

    @Test
    fun issueStatusDisplayOrderMatchesGeneratedContract() {
        assertEquals(
            DomainContract.issueStatusDisplayOrder,
            issueStatusOrder.map { it.wire },
        )
    }

    @Test
    fun issuePriorityWireValuesMatchGeneratedContract() {
        assertEquals(
            DomainContract.issuePriorityValues,
            IssuePriority.entries.map { it.wire },
        )
        for (value in DomainContract.issuePriorityValues) {
            assertEquals(value, IssuePriority.fromWire(value).wire)
        }
    }

    @Test
    fun issuePriorityDisplayOrderMatchesGeneratedContract() {
        assertEquals(
            DomainContract.issuePriorityDisplayOrder,
            issuePriorityOrder.map { it.wire },
        )
    }

    @Test
    fun issueSourceWireValuesMatchGeneratedContract() {
        assertEquals(
            listOf("user", "widget", "agent"),
            DomainContract.issueSourceValues,
        )
        // The per-value constants stay in lockstep with the values list.
        assertEquals("user", DomainContract.issueSourceUser)
        assertEquals("widget", DomainContract.issueSourceWidget)
        assertEquals("agent", DomainContract.issueSourceAgent)
    }

    /**
     * EXP-724: `steerCommands` generates as five PARALLEL arrays, which only
     * zip correctly while they are the same length — a regen that adds a field
     * to one row and not the others must fail here, not silently shift every
     * description by one. Every listed agent must also be a real coding agent,
     * or the row is unreachable on every client.
     */
    @Test
    fun steerCommandArraysAreParallelAndNameKnownAgents() {
        val size = DomainContract.steerCommandNames.size
        assertEquals(size, DomainContract.steerCommandDescriptions.size)
        assertEquals(size, DomainContract.steerCommandArgHints.size)
        assertEquals(size, DomainContract.steerCommandAgents.size)
        assertEquals(size, DomainContract.steerCommandConfirm.size)
        // Names are unique, lowercase-kebab, and every row applies somewhere.
        assertEquals(
            DomainContract.steerCommandNames,
            DomainContract.steerCommandNames.distinct(),
        )
        for (name in DomainContract.steerCommandNames) {
            assertTrue(name, Regex("^[a-z][a-z0-9-]*$").matches(name))
        }
        for (row in DomainContract.steerCommandAgents) {
            val agents = row.split(",")
            assertTrue(row, agents.isNotEmpty())
            for (agent in agents) {
                assertTrue(agent, agent in DomainContract.codingAgentValues)
            }
        }
    }
}
