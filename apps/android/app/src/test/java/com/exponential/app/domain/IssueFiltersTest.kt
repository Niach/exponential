package com.exponential.app.domain

import com.exponential.app.data.db.IssueStatusEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The status-filter token contract (EXP-314). A ticked status is stored as its
 * GROUP KEY — a row uuid, or `builtin:<key>` when the pick was made against the
 * CONSTRUCTED fallback set. The rule this locks: a `builtin:<key>` token must
 * survive the fallback→synced re-key (it also matches the synced row carrying
 * that builtin key), while a row-uuid token matches only that row.
 */
class IssueFiltersTest {

    private fun row(
        id: String,
        category: String,
        builtinKey: String? = null,
        name: String = id,
    ) = IssueStatusEntity(
        id = id,
        teamId = "t1",
        category = category,
        name = name,
        color = "#123456",
        sortOrder = 1.0,
        builtinKey = builtinKey,
        createdAt = "2026-01-01 00:00:00+00",
        updatedAt = "2026-01-01 00:00:00+00",
    )

    private val syncedBacklog = IssueStatusResolver.teamStatuses(
        listOf(row("row-backlog", "backlog", builtinKey = "backlog"))
    ).single()

    private val syncedCustom = IssueStatusResolver.teamStatuses(
        listOf(row("row-custom", "started", name = "Reviewing"))
    ).single()

    private val fallbackBacklog =
        IssueStatusResolver.builtinDefaults.first { it.builtinKey == IssueStatus.Backlog }

    @Test
    fun aBuiltinTokenSetPreSyncStillMatchesTheSyncedRow() {
        val filters = IssueFilters(statusIds = setOf("builtin:backlog"))
        assertTrue(filters.isStatusSelected(fallbackBacklog))
        assertTrue(filters.isStatusSelected(syncedBacklog))
        assertTrue(matchesFilters(syncedBacklog, IssuePriority.None, emptyList(), filters))
    }

    @Test
    fun aRowTokenMatchesOnlyThatRow() {
        val filters = IssueFilters(statusIds = setOf("row-backlog"))
        assertTrue(filters.isStatusSelected(syncedBacklog))
        assertFalse(filters.isStatusSelected(syncedCustom))
        // …and it does NOT bleed onto the constructed fallback of the same key.
        assertFalse(filters.isStatusSelected(fallbackBacklog))
        assertFalse(matchesFilters(syncedCustom, IssuePriority.None, emptyList(), filters))
    }

    @Test
    fun aCustomRowHasNoBuiltinEquivalence() {
        assertEquals(setOf("row-custom"), statusFilterTokens(syncedCustom))
        assertFalse(IssueFilters(statusIds = setOf("builtin:in_progress")).isStatusSelected(syncedCustom))
    }

    @Test
    fun untickingDropsTheStalePreSyncTokenToo() {
        // Ticked pre-sync, then unticked after the rows landed: the filter must
        // come out EMPTY, not silently keep filtering on the stale token.
        val ticked = IssueFilters(statusIds = setOf("builtin:backlog"))
        val unticked = ticked.toggleStatus(syncedBacklog)
        assertTrue(unticked.statusIds.isEmpty())
        assertFalse(unticked.isStatusSelected(syncedBacklog))
    }

    @Test
    fun tickingASyncedRowStoresItsRowId() {
        val ticked = IssueFilters().toggleStatus(syncedBacklog)
        assertEquals(setOf("row-backlog"), ticked.statusIds)
        assertTrue(ticked.isStatusSelected(syncedBacklog))
        assertEquals(IssueFilters(), ticked.toggleStatus(syncedBacklog))
    }

    @Test
    fun tickingAConstructedFallbackStoresTheBuiltinToken() {
        val ticked = IssueFilters().toggleStatus(fallbackBacklog)
        assertEquals(setOf("builtin:backlog"), ticked.statusIds)
        assertEquals(IssueFilters(), ticked.toggleStatus(fallbackBacklog))
    }

    @Test
    fun anEmptyStatusFilterMatchesEverything() {
        val filters = IssueFilters()
        assertTrue(matchesFilters(syncedCustom, IssuePriority.None, emptyList(), filters))
        assertTrue(matchesFilters(fallbackBacklog, IssuePriority.None, emptyList(), filters))
    }

    @Test
    fun priorityAndLabelFiltersStillAnd() {
        val filters = IssueFilters(
            statusIds = setOf("builtin:backlog"),
            priorities = setOf(IssuePriority.Urgent),
            labelIds = setOf("l1"),
        )
        assertTrue(matchesFilters(syncedBacklog, IssuePriority.Urgent, listOf("l1", "l2"), filters))
        assertFalse(matchesFilters(syncedBacklog, IssuePriority.Low, listOf("l1"), filters))
        assertFalse(matchesFilters(syncedBacklog, IssuePriority.Urgent, listOf("l2"), filters))
        assertFalse(matchesFilters(syncedCustom, IssuePriority.Urgent, listOf("l1"), filters))
    }
}
