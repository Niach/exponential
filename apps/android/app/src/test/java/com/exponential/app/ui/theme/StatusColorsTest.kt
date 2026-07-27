package com.exponential.app.ui.theme

import com.exponential.app.domain.IssuePriority
import com.exponential.app.domain.IssueStatus
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
