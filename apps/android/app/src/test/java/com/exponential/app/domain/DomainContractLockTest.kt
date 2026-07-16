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
}
