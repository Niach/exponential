package com.exponential.app.domain

import org.junit.Assert.assertEquals
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
}
