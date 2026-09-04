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
     * EXP-736: the relation enum AND both label lists are contract-owned — the
     * per-side wording ("blocked by", "sub-issue of") is what every client
     * renders, so a regen that reorders or rewords a value must fail here.
     */
    @Test
    fun issueRelationTypeWireValuesMatchGeneratedContract() {
        assertEquals(
            DomainContract.issueRelationTypeValues,
            IssueRelationType.entries.map { it.wire },
        )
        assertEquals(
            DomainContract.issueRelationTypeForwardLabels,
            IssueRelationType.entries.map { it.forwardLabel },
        )
        assertEquals(
            DomainContract.issueRelationTypeInverseLabels,
            IssueRelationType.entries.map { it.inverseLabel },
        )
        for (value in DomainContract.issueRelationTypeValues) {
            assertEquals(value, IssueRelationType.fromWire(value)?.wire)
        }
        // An unknown value from a newer server degrades instead of guessing.
        assertEquals(null, IssueRelationType.fromWire("mentioned"))
        // The six picker entries only name real types.
        assertEquals(6, relationPicks.size)
        for (pick in relationPicks) {
            assertTrue(pick.title, pick.type.wire in DomainContract.issueRelationTypeValues)
        }
        // Both relation events exist in the contract's event vocabulary.
        assertTrue(DomainContract.issueEventTypeRelationAdded in DomainContract.issueEventTypeValues)
        assertTrue(DomainContract.issueEventTypeRelationRemoved in DomainContract.issueEventTypeValues)
    }

    /** The phrases are byte-identical across all four clients (EXP-736). */
    @Test
    fun relationEventPhrasesMatchTheSharedWording() {
        assertEquals(
            "added related issue EXP-12",
            relationEventPhrase(added = true, type = "related", identifier = "EXP-12", direction = "forward"),
        )
        assertEquals(
            "removed related issue EXP-12",
            relationEventPhrase(added = false, type = "related", identifier = "EXP-12", direction = "inverse"),
        )
        assertEquals(
            "marked as blocks EXP-3",
            relationEventPhrase(added = true, type = "blocks", identifier = "EXP-3", direction = "forward"),
        )
        assertEquals(
            "no longer blocked by EXP-3",
            relationEventPhrase(added = false, type = "blocks", identifier = "EXP-3", direction = "inverse"),
        )
        assertEquals(
            "marked as sub-issue of EXP-3",
            relationEventPhrase(added = true, type = "parent", identifier = "EXP-3", direction = "inverse"),
        )
        // Too thin to phrase richly: both degrade paths mirror the web
        // `relationEventPhrase` — a missing counterpart is named "an issue",
        // an unknown/missing type reads as the symmetric `related`.
        assertEquals(
            "marked as blocks an issue",
            relationEventPhrase(added = true, type = "blocks", identifier = null, direction = "forward"),
        )
        assertEquals(
            "marked as blocked by an issue",
            relationEventPhrase(added = true, type = "blocks", identifier = "  ", direction = "inverse"),
        )
        assertEquals(
            "added related issue EXP-3",
            relationEventPhrase(added = true, type = "mentioned", identifier = "EXP-3", direction = "inverse"),
        )
        assertEquals(
            "removed related issue EXP-3",
            relationEventPhrase(added = false, type = null, identifier = "EXP-3", direction = "forward"),
        )
        // The web's `relationEventPhrase('relation_added', {})`.
        assertEquals(
            "added related issue an issue",
            relationEventPhrase(added = true, type = null, identifier = null, direction = null),
        )
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
