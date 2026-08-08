package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.IssueStatusEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Locks the custom-issue-status resolution contract (EXP-314). The identical
 * table + chain ships on web, iOS and desktop — the literal icon-name lists
 * below are the cross-client parity anchor, so a change here must happen in
 * lockstep with the other three clients' tests.
 */
class IssueStatusResolutionTest {

    private fun row(
        id: String,
        category: String,
        name: String = id,
        color: String? = "#123456",
        sortOrder: Double = 1.0,
        builtinKey: String? = null,
        createdAt: String = "2026-01-01 00:00:00+00",
    ) = IssueStatusEntity(
        id = id,
        teamId = "t1",
        category = category,
        name = name,
        color = color,
        sortOrder = sortOrder,
        builtinKey = builtinKey,
        createdAt = createdAt,
        updatedAt = createdAt,
    )

    private fun issue(status: String, statusId: String? = null) = IssueEntity(
        id = "i1",
        boardId = "b1",
        number = 1,
        identifier = "EXP-1",
        title = "t",
        status = status,
        statusId = statusId,
        priority = "none",
        sortOrder = 0.0,
        createdAt = "2026-01-01 00:00:00+00",
        updatedAt = "2026-01-01 00:00:00+00",
    )

    // --- contract locks -----------------------------------------------------

    @Test
    fun categoryWireValuesMatchGeneratedContract() {
        assertEquals(
            DomainContract.issueStatusCategoryValues,
            IssueStatusCategory.entries.map { it.wire },
        )
        for (value in DomainContract.issueStatusCategoryValues) {
            assertEquals(value, IssueStatusCategory.fromWire(value)?.wire)
        }
        // Tolerant: absent / newer-server values resolve to null, never a throw.
        assertNull(IssueStatusCategory.fromWire(null))
        assertNull(IssueStatusCategory.fromWire("blocked"))
    }

    @Test
    fun categoryOrderMatchesGeneratedContract() {
        // EXP-448: ONE order — list groups, pickers and settings sections.
        assertEquals(
            listOf("backlog", "unstarted", "started", "completed", "cancelled", "duplicate"),
            issueStatusCategoryDisplayOrder.map { it.wire },
        )
        assertEquals(
            DomainContract.issueStatusCategoryDisplayOrder,
            issueStatusCategoryDisplayOrder.map { it.wire },
        )
    }

    @Test
    fun builtinDefaultsMirrorTheGeneratedContract() {
        val byKey = IssueStatusResolver.builtinDefaults.associateBy { it.builtinKey?.wire }
        assertEquals(DomainContract.issueStatusDefaultKeys.size, IssueStatusResolver.builtinDefaults.size)
        DomainContract.issueStatusDefaultKeys.forEachIndexed { i, key ->
            val resolved = byKey.getValue(key)
            assertEquals(DomainContract.issueStatusDefaultNames[i], resolved.name)
            assertEquals(DomainContract.issueStatusDefaultCategories[i], resolved.category.wire)
            assertEquals(DomainContract.issueStatusDefaultColors[i], resolved.colorHex)
            assertEquals("builtin:$key", resolved.id)
            assertNull(resolved.rowId)
        }
    }

    /**
     * The constructed fallbacks must render EXACTLY the glyphs the app used
     * before custom statuses existed (semantic status-in-progress /
     * status-in-review are the N=2 clocks).
     */
    @Test
    fun builtinDefaultGlyphsAreTheProductionStatusGlyphs() {
        val byKey = IssueStatusResolver.builtinDefaults.associateBy { it.builtinKey }
        assertEquals("circle-dashed", byKey.getValue(IssueStatus.Backlog).iconName)
        assertEquals("circle", byKey.getValue(IssueStatus.Todo).iconName)
        assertEquals("progress-2-4", byKey.getValue(IssueStatus.InProgress).iconName)
        assertEquals("progress-3-4", byKey.getValue(IssueStatus.InReview).iconName)
        assertEquals("circle-check", byKey.getValue(IssueStatus.Done).iconName)
        assertEquals("circle-x", byKey.getValue(IssueStatus.Cancelled).iconName)
        assertEquals("copy", byKey.getValue(IssueStatus.Duplicate).iconName)
    }

    // --- started clock table ------------------------------------------------

    @Test
    fun startedClockTableIsTheCrossClientContract() {
        // count <= 2
        assertEquals("progress-2-4", IssueStatusResolver.startedClockIcon(0, 1))
        assertEquals("progress-2-4", IssueStatusResolver.startedClockIcon(0, 2))
        assertEquals("progress-3-4", IssueStatusResolver.startedClockIcon(1, 2))
        // count == 3
        assertEquals("progress-1-4", IssueStatusResolver.startedClockIcon(0, 3))
        assertEquals("progress-2-4", IssueStatusResolver.startedClockIcon(1, 3))
        assertEquals("progress-3-4", IssueStatusResolver.startedClockIcon(2, 3))
        // count >= 4
        assertEquals("progress-1-5", IssueStatusResolver.startedClockIcon(0, 4))
        assertEquals("progress-2-5", IssueStatusResolver.startedClockIcon(1, 4))
        assertEquals("progress-3-5", IssueStatusResolver.startedClockIcon(2, 4))
        assertEquals("progress-4-5", IssueStatusResolver.startedClockIcon(3, 4))
    }

    @Test
    fun startedClockIndexesAreClamped() {
        // A racing create can transiently exceed the started cap.
        assertEquals("progress-4-5", IssueStatusResolver.startedClockIcon(4, 5))
        assertEquals("progress-4-5", IssueStatusResolver.startedClockIcon(9, 9))
        assertEquals("progress-3-4", IssueStatusResolver.startedClockIcon(7, 2))
        assertEquals("progress-2-4", IssueStatusResolver.startedClockIcon(-3, 2))
    }

    // --- ordering -----------------------------------------------------------

    @Test
    fun teamStatusesOrderByCategoryThenSortOrderThenCreatedAtThenId() {
        val rows = listOf(
            row("dup", "duplicate"),
            row("done", "completed"),
            row("backlog", "backlog"),
            row("todo", "unstarted"),
            row("cancelled", "cancelled"),
            row("review", "started", sortOrder = 2.0),
            row("progress", "started", sortOrder = 1.0),
        )
        assertEquals(
            listOf("backlog", "todo", "progress", "review", "done", "cancelled", "dup"),
            IssueStatusResolver.teamStatuses(rows).map { it.id },
        )
    }

    @Test
    fun equalSortOrdersBreakTiesByCreatedAtThenId() {
        val rows = listOf(
            row("zzz", "started", sortOrder = 1.0, createdAt = "2026-01-01 00:00:00+00"),
            row("aaa", "started", sortOrder = 1.0, createdAt = "2026-01-01 00:00:00+00"),
            row("older", "started", sortOrder = 1.0, createdAt = "2025-01-01 00:00:00+00"),
        )
        assertEquals(
            listOf("older", "aaa", "zzz"),
            IssueStatusResolver.teamStatuses(rows).map { it.id },
        )
    }

    @Test
    fun startedGlyphsFollowPositionAmongStartedRows() {
        val rows = listOf(
            row("s1", "started", sortOrder = 1.0),
            row("s2", "started", sortOrder = 2.0),
            row("s3", "started", sortOrder = 3.0),
            row("backlog", "backlog"),
        )
        val glyphs = IssueStatusResolver.teamStatuses(rows).associate { it.id to it.iconName }
        assertEquals("progress-1-4", glyphs["s1"])
        assertEquals("progress-2-4", glyphs["s2"])
        assertEquals("progress-3-4", glyphs["s3"])
        assertEquals("circle-dashed", glyphs["backlog"])
    }

    @Test
    fun unknownCategoryDegradesToTheBacklogTreatment() {
        val resolved = IssueStatusResolver.teamStatuses(listOf(row("weird", "blocked"))).single()
        // RENDER degradation: neutral dashed glyph + the backlog category, so
        // the row also lands on the ACTIVE in-group sort branch.
        assertEquals(IssueStatusCategory.Backlog, resolved.category)
        assertEquals("circle-dashed", resolved.iconName)
        assertEquals("weird", resolved.name)
        assertTrue(
            "unknown categories use the active in-group sort branch",
            resolved.category in listOf(
                IssueStatusCategory.Backlog,
                IssueStatusCategory.Unstarted,
                IssueStatusCategory.Started,
            ),
        )
    }

    @Test
    fun unknownCategorySortsLastAfterDuplicate() {
        // A category this client cannot interpret must not wedge itself into
        // the middle of the team's list — it goes to the very end, after
        // duplicate (the cross-platform UNKNOWN CATEGORY rule).
        val rows = listOf(
            row("weird", "blocked"),
            row("dup", "duplicate"),
            row("done", "completed"),
            row("progress", "started"),
            row("backlog", "backlog"),
        )
        assertEquals(
            listOf("backlog", "progress", "done", "dup", "weird"),
            IssueStatusResolver.teamStatuses(rows).map { it.id },
        )
    }

    @Test
    fun unknownCategoryDoesNotStealAStartedClock() {
        // The started clock ramp counts only REAL started rows.
        val rows = listOf(
            row("s1", "started", sortOrder = 1.0),
            row("s2", "started", sortOrder = 2.0),
            row("weird", "blocked"),
        )
        val resolved = IssueStatusResolver.teamStatuses(rows).associate { it.id to it.iconName }
        assertEquals("progress-2-4", resolved["s1"])
        assertEquals("progress-3-4", resolved["s2"])
        assertEquals("circle-dashed", resolved["weird"])
    }

    // --- resolution chain ---------------------------------------------------

    @Test
    fun resolvesByStatusIdFirst() {
        val team = IssueStatusResolver.teamStatuses(
            listOf(
                row("custom", "started", name = "Reviewing", sortOrder = 2.0),
                row("bi", "started", name = "In Progress", builtinKey = "in_progress"),
            )
        )
        // The anchor says in_progress, but the row id wins.
        val resolved = IssueStatusResolver.resolve(issue("in_progress", statusId = "custom"), team)
        assertEquals("custom", resolved.id)
        assertEquals("Reviewing", resolved.name)
        assertNull(resolved.builtinKey)
    }

    @Test
    fun fallsBackToTheAnchorRowWhenStatusIdIsMissingOrStale() {
        val team = IssueStatusResolver.teamStatuses(
            listOf(row("bi", "started", name = "In Progress", builtinKey = "in_progress"))
        )
        assertEquals("bi", IssueStatusResolver.resolve(issue("in_progress"), team).id)
        assertEquals(
            "bi",
            IssueStatusResolver.resolve(issue("in_progress", statusId = "deleted-row"), team).id,
        )
    }

    @Test
    fun fallsBackToTheConstructedDefaultWhenTheTeamHasNoRows() {
        val resolved = IssueStatusResolver.resolve(issue("done"), emptyList())
        assertEquals("builtin:done", resolved.id)
        assertNull(resolved.rowId)
        assertEquals(IssueStatus.Done, resolved.builtinKey)
        assertEquals("circle-check", resolved.iconName)
    }

    @Test
    fun anUnknownAnchorResolvesToTheBacklogDefault() {
        val resolved = IssueStatusResolver.resolve(issue("blocked"), emptyList())
        assertEquals("builtin:backlog", resolved.id)
        assertEquals(IssueStatus.Backlog, resolved.builtinKey)
    }

    @Test
    fun anUnknownAnchorJoinsTheTeamsRealBacklogRow() {
        // The UNKNOWN ANCHOR rule: normalize to backlog BEFORE the team-row
        // lookup, so a forward-compat status joins the team's REAL Backlog
        // group instead of spawning a second, constructed one.
        val team = IssueStatusResolver.teamStatuses(
            listOf(
                row("bl", "backlog", name = "Backlog", builtinKey = "backlog"),
                row("bi", "started", name = "In Progress", builtinKey = "in_progress"),
            )
        )
        val resolved = IssueStatusResolver.resolve(issue("blocked"), team)
        assertEquals("bl", resolved.id)
        assertEquals("bl", resolved.rowId)
        assertEquals(IssueStatus.Backlog, resolved.builtinKey)
        // Only a team with NO synced rows degrades to the constructed default.
        assertEquals("builtin:backlog", IssueStatusResolver.resolve(issue("blocked"), emptyList()).id)
    }

    @Test
    fun constructedFallbacksCarryTheBuiltinIdPrefix() {
        assertTrue(
            IssueStatusResolver.builtinDefaults.all {
                it.id.startsWith(IssueStatusResolver.BUILTIN_ID_PREFIX)
            }
        )
        assertNotNull(IssueStatusResolver.builtinDefaults.firstOrNull { it.builtinKey == IssueStatus.Todo })
    }
}
