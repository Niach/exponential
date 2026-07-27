package com.exponential.app.ui.theme

import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.domain.IssueStatusResolver
import com.exponential.app.domain.ResolvedIssueStatus
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

/**
 * REV2-85: the status/priority COLOR IDENTITY is a cross-client contract. The
 * desktop IDE palette is the source of truth (web spells the same roles as
 * Tailwind classes): cancelled is a muted terminal RESOLUTION, not an error,
 * and todo carries the brighter foreground tint that separates it from
 * backlog's neutral gray (Android also renders a dashed-circle backlog glyph —
 * `ExpIcons.statusBacklog`, EXP-273).
 */
class StatusColorsTest {

    private val isoDate = DateTimeFormatter.ofPattern("yyyy-MM-dd")

    @Test
    fun cancelledIsMutedNotAnErrorState() {
        assertEquals(DesignTokens.Semantic.Neutral, statusColor(IssueStatus.Cancelled))
        assertNotEquals(DesignTokens.Semantic.Red, statusColor(IssueStatus.Cancelled))
        // Same treatment the other terminal resolution already had.
        assertEquals(statusColor(IssueStatus.Duplicate), statusColor(IssueStatus.Cancelled))
    }

    @Test
    fun todoIsBrighterThanBacklog() {
        assertEquals(DesignTokens.Palette.Foreground, statusColor(IssueStatus.Todo))
        assertEquals(DesignTokens.Semantic.Neutral, statusColor(IssueStatus.Backlog))
        assertNotEquals(statusColor(IssueStatus.Backlog), statusColor(IssueStatus.Todo))
    }

    @Test
    fun activeStatusAccentsMatchTheSharedTokens() {
        assertEquals(DesignTokens.Semantic.Yellow, statusColor(IssueStatus.InProgress))
        assertEquals(DesignTokens.Semantic.Green, statusColor(IssueStatus.InReview))
        assertEquals(DesignTokens.Semantic.Blue, statusColor(IssueStatus.Done))
    }

    /**
     * EXP-314: a RESOLVED builtin row (and every constructed fallback) renders
     * the same design token as before custom statuses existed — the synced
     * near-neutral seed hex is deliberately NOT used for builtins.
     */
    @Test
    fun resolvedBuiltinStatusesKeepTheirTokenColors() {
        val byKey = IssueStatusResolver.builtinDefaults.associateBy { it.builtinKey }
        for (status in IssueStatus.entries) {
            assertEquals(statusColor(status), resolvedStatusColor(byKey.getValue(status)))
        }
    }

    /** A custom row renders its own hex, never a status token. */
    @Test
    fun resolvedCustomStatusesUseTheirOwnColor() {
        val custom = ResolvedIssueStatus(
            id = "s1",
            rowId = "s1",
            name = "Blocked",
            category = IssueStatusCategory.Started,
            colorHex = "#EF4444",
            builtinKey = null,
            iconName = "progress-2-4",
        )
        assertNotEquals(statusColor(IssueStatus.InProgress), resolvedStatusColor(custom))
    }

    /** A custom row without a usable color stays neutral rather than blank. */
    @Test
    fun resolvedCustomStatusWithoutAColorIsNeutral() {
        val custom = ResolvedIssueStatus(
            id = "s2",
            rowId = "s2",
            name = "Blocked",
            category = IssueStatusCategory.Backlog,
            colorHex = null,
            builtinKey = null,
            iconName = "circle-dashed",
        )
        assertEquals(DesignTokens.Semantic.Neutral, resolvedStatusColor(custom))
    }

    @Test
    fun priorityAccentsMatchTheSharedTokens() {
        assertEquals(DesignTokens.Semantic.Red, priorityColor(IssuePriority.Urgent))
        assertEquals(DesignTokens.Semantic.Orange, priorityColor(IssuePriority.High))
        assertEquals(DesignTokens.Semantic.Yellow, priorityColor(IssuePriority.Medium))
        assertEquals(DesignTokens.Semantic.Blue, priorityColor(IssuePriority.Low))
        assertEquals(DesignTokens.Semantic.Neutral, priorityColor(IssuePriority.None))
    }

    /** REV2-48: due today WINS over overdue — the rule web/desktop now share. */
    @Test
    fun dueDateColorIsRedWhenOverdueAndOrangeToday() {
        val today = LocalDate.now()
        assertEquals(DesignTokens.Semantic.Red, dueDateColor(today.minusDays(1).format(isoDate)))
        assertEquals(DesignTokens.Semantic.Orange, dueDateColor(today.format(isoDate)))
        assertEquals(
            DesignTokens.Semantic.Neutral.copy(alpha = TextEmphasis.Tertiary),
            dueDateColor(today.plusDays(1).format(isoDate)),
        )
        // Unparseable / absent dates stay muted rather than reading as overdue.
        assertEquals(
            DesignTokens.Semantic.Neutral.copy(alpha = TextEmphasis.Tertiary),
            dueDateColor(null),
        )
    }
}
