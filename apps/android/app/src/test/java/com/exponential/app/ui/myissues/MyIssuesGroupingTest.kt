package com.exponential.app.ui.myissues

import com.exponential.app.domain.IssueStatus
import com.exponential.app.domain.IssueStatusCategory
import com.exponential.app.domain.ResolvedIssueStatus
import org.junit.Assert.assertEquals
import org.junit.Test

/** P14: My issues groups by the RESOLVED team status rows, not the anchor enum. */
class MyIssuesGroupingTest {
    private fun row(id: String, name: String, category: IssueStatusCategory, icon: String = "circle") =
        ResolvedIssueStatus(
            id = id,
            rowId = id,
            name = name,
            category = category,
            colorHex = null,
            builtinKey = null,
            iconName = icon,
        )

    @Test
    fun `a custom status gets its own group, ordered by category then team position`() {
        val backlog = row("b", "Backlog", IssueStatusCategory.Backlog)
        val inProgress = row("p", "In Progress", IssueStatusCategory.Started, "progress-1-3")
        val inQa = row("q", "In QA", IssueStatusCategory.Started, "progress-2-3")
        val done = row("d", "Done", IssueStatusCategory.Completed)
        val groups = groupByResolvedStatus(
            listOf(
                Triple("APP-1", done, 5),
                Triple("APP-2", inQa, 3),
                Triple("APP-3", backlog, 0),
                Triple("APP-4", inProgress, 2),
                Triple("APP-5", inQa, 3),
            ),
        )
        assertEquals(listOf("Backlog", "In Progress", "In QA", "Done"), groups.map { it.first.name })
        assertEquals(listOf("APP-2", "APP-5"), groups[2].second)
        assertEquals("progress-2-3", groups[2].first.iconName)
    }

    @Test
    fun `two teams' rows of the same name share one group`() {
        val teamA = row("a-p", "In Progress", IssueStatusCategory.Started)
        val teamB = row("b-p", "In Progress", IssueStatusCategory.Started)
        val groups = groupByResolvedStatus(listOf(Triple(1, teamA, 1), Triple(2, teamB, 1)))
        assertEquals(1, groups.size)
        assertEquals(listOf(1, 2), groups.single().second)
    }

    @Test
    fun `the anchor labels read as the builtin rows do`() {
        assertEquals("In Progress", IssueStatus.InProgress.label)
        assertEquals("In Review", IssueStatus.InReview.label)
    }
}
