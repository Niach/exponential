package com.exponential.app.domain

import com.exponential.app.data.db.IssueEntity
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Locks the canonical in-group issue ordering (EXP-38) — the same contract
 * ships on web, iOS, and desktop, so any change here must happen in lockstep:
 * - backlog/todo/in_progress: overdue first, then priority rank, then dueDate
 *   ascending (null last), then `number` ascending NUMERICALLY.
 * - done: (completedAt ?? updatedAt) descending.
 * - cancelled/duplicate: updatedAt descending.
 * Plus the mixed-timestamp-format normalization (Electric "space" format vs
 * the optimistic upserts' ISO "T…Z") that makes the terminal groups correct.
 */
class IssueSortTest {

    private val today = "2026-07-09"

    private fun issue(
        id: String,
        number: Int = 1,
        status: String = "todo",
        priority: String = "none",
        dueDate: String? = null,
        completedAt: String? = null,
        updatedAt: String = "2026-01-01 00:00:00+00",
    ) = IssueEntity(
        id = id,
        projectId = "p1",
        number = number,
        identifier = "EXP-$number",
        title = id,
        status = status,
        priority = priority,
        creatorId = "u1",
        dueDate = dueDate,
        sortOrder = 0.0,
        completedAt = completedAt,
        createdAt = "2026-01-01 00:00:00+00",
        updatedAt = updatedAt,
    )

    private fun sortedIds(status: IssueStatus, issues: List<IssueEntity>): List<String> =
        sortIssuesForGroup(status = status, issues = issues, today = today) { it }.map { it.id }

    // --- backlog/todo/in_progress -------------------------------------------

    @Test
    fun nonTerminalGroupsOrderByPriorityRank() {
        val issues = listOf(
            issue("none", number = 1, priority = "none"),
            issue("low", number = 2, priority = "low"),
            issue("medium", number = 3, priority = "medium"),
            issue("high", number = 4, priority = "high"),
            issue("urgent", number = 5, priority = "urgent"),
        )
        for (status in listOf(IssueStatus.Backlog, IssueStatus.Todo, IssueStatus.InProgress)) {
            assertEquals(
                listOf("urgent", "high", "medium", "low", "none"),
                sortedIds(status, issues),
            )
        }
    }

    @Test
    fun samePriorityOrdersByDueDateAscendingWithNullLast() {
        val issues = listOf(
            issue("no-due", number = 1, priority = "high"),
            issue("later", number = 2, priority = "high", dueDate = "2026-08-01"),
            issue("sooner", number = 3, priority = "high", dueDate = "2026-07-10"),
        )
        assertEquals(listOf("sooner", "later", "no-due"), sortedIds(IssueStatus.Todo, issues))
    }

    @Test
    fun tiebreakIsIssueNumberNumericNotIdentifierString() {
        // "EXP-10" < "EXP-9" lexicographically — the contract demands numeric
        // `number` ordering, so 9 must come before 10.
        val issues = listOf(
            issue("ten", number = 10),
            issue("nine", number = 9),
            issue("two", number = 2),
        )
        assertEquals(listOf("two", "nine", "ten"), sortedIds(IssueStatus.Backlog, issues))
    }

    @Test
    fun overdueIssuesSortBeforeEverythingElseInNonTerminalGroups() {
        // The overdue boost outranks priority: an overdue `low` beats a
        // non-overdue `urgent`. Due today is NOT overdue.
        val issues = listOf(
            issue("urgent-later", number = 1, priority = "urgent", dueDate = "2026-08-01"),
            issue("urgent-today", number = 2, priority = "urgent", dueDate = today),
            issue("low-overdue", number = 3, priority = "low", dueDate = "2026-07-01"),
            issue("none-overdue", number = 4, priority = "none", dueDate = "2026-07-08"),
        )
        assertEquals(
            listOf("low-overdue", "none-overdue", "urgent-today", "urgent-later"),
            sortedIds(IssueStatus.Todo, issues),
        )
    }

    @Test
    fun overdueIssuesKeepPriorityThenDueDateOrderAmongThemselves() {
        val issues = listOf(
            issue("none-old", number = 1, priority = "none", dueDate = "2026-06-01"),
            issue("high-recent", number = 2, priority = "high", dueDate = "2026-07-08"),
            issue("high-old", number = 3, priority = "high", dueDate = "2026-06-01"),
        )
        assertEquals(
            listOf("high-old", "high-recent", "none-old"),
            sortedIds(IssueStatus.InProgress, issues),
        )
    }

    // --- done ----------------------------------------------------------------

    @Test
    fun doneGroupOrdersByCompletedAtDescending() {
        val issues = listOf(
            issue("first", status = "done", completedAt = "2026-07-01 10:00:00+00"),
            issue("latest", status = "done", completedAt = "2026-07-09 10:00:00+00"),
            issue("middle", status = "done", completedAt = "2026-07-05 10:00:00+00"),
        )
        assertEquals(listOf("latest", "middle", "first"), sortedIds(IssueStatus.Done, issues))
    }

    @Test
    fun doneGroupFallsBackToUpdatedAtWhenCompletedAtIsNull() {
        val issues = listOf(
            issue("no-stamp-newer", status = "done", updatedAt = "2026-07-08 10:00:00+00"),
            issue("stamped-older", status = "done", completedAt = "2026-07-02 10:00:00+00"),
        )
        assertEquals(listOf("no-stamp-newer", "stamped-older"), sortedIds(IssueStatus.Done, issues))
    }

    @Test
    fun doneGroupComparesMixedTimestampFormatsCorrectly() {
        // Electric delivers "yyyy-MM-dd HH:mm:ss.ffffff+00"; an optimistic tRPC
        // upsert stores ISO "yyyy-MM-ddTHH:mm:ss.SSSZ". Naively ' ' < 'T' would
        // pin every Electric-format row before every ISO row regardless of time.
        val issues = listOf(
            issue("electric-older", status = "done", completedAt = "2026-07-08 10:00:00.000000+00"),
            issue("iso-newest", status = "done", completedAt = "2026-07-09T12:00:00.000Z"),
            issue("electric-newer", status = "done", completedAt = "2026-07-09 13:00:00.000000+00"),
        )
        assertEquals(
            listOf("electric-newer", "iso-newest", "electric-older"),
            sortedIds(IssueStatus.Done, issues),
        )
    }

    // --- cancelled / duplicate -----------------------------------------------

    @Test
    fun cancelledAndDuplicateGroupsOrderByUpdatedAtDescending() {
        for (status in listOf(IssueStatus.Cancelled, IssueStatus.Duplicate)) {
            val issues = listOf(
                issue("older", status = status.wire, updatedAt = "2026-07-01 10:00:00+00"),
                issue("newest", status = status.wire, updatedAt = "2026-07-09T10:00:00.000Z"),
                issue("middle", status = status.wire, updatedAt = "2026-07-05 10:00:00+00"),
            )
            assertEquals(listOf("newest", "middle", "older"), sortedIds(status, issues))
        }
    }

    // --- timestamp normalization ---------------------------------------------

    @Test
    fun sortableTimestampNormalizesBothWireFormats() {
        // Same instant in both formats normalizes identically (to micro precision).
        assertEquals(
            sortableTimestamp("2026-07-09 17:52:48.911000+00"),
            sortableTimestamp("2026-07-09T17:52:48.911Z"),
        )
        // Postgres trims trailing fraction zeros — the shorter fraction must not
        // compare against the zone suffix ("...48.9Z" vs "...48.911813+00").
        assertEquals(true, sortableTimestamp("2026-07-09 17:52:48.9+00") < sortableTimestamp("2026-07-09T17:52:48.911813Z"))
        // No fraction at all still yields a fixed-width comparable value.
        assertEquals(true, sortableTimestamp("2026-07-09 17:52:48+00") < sortableTimestamp("2026-07-09 17:52:48.000001+00"))
    }
}
