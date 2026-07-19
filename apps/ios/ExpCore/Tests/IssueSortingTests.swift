import Foundation
import XCTest
@testable import ExpCore

// Locks the canonical in-group issue ordering (EXP-38) — the cross-platform
// contract shared with web/Android/desktop. One test per group class, plus the
// gotchas the contract calls out explicitly: the overdue-first boost, numeric
// (never identifier-string) number ordering, and nil-last placements.
final class IssueSortingTests: XCTestCase {
    private let today = "2026-07-09"

    private func makeIssue(
        id: String,
        number: Int? = nil,
        identifier: String? = nil,
        status: IssueStatus = .todo,
        priority: IssuePriority = .none,
        dueDate: String? = nil,
        sortOrder: Double? = nil,
        completedAt: String? = nil,
        updatedAt: String = "2026-07-01 10:00:00+00"
    ) -> IssueEntity {
        IssueEntity(
            id: id,
            boardId: "p1",
            number: number,
            identifier: identifier,
            title: id,
            description: nil,
            status: status.rawValue,
            priority: priority.rawValue,
            assigneeId: nil,
            creatorId: nil,
            dueDate: dueDate,
            dueTime: nil,
            endTime: nil,
            sortOrder: sortOrder,
            completedAt: completedAt,
            archivedAt: nil,
            duplicateOfId: nil,
            prUrl: nil,
            prNumber: nil,
            prState: nil,
            branch: nil,
            prMergedAt: nil,
            createdAt: "2026-06-01 10:00:00+00",
            updatedAt: updatedAt
        )
    }

    private func sortedIds(_ issues: [IssueEntity], status: IssueStatus) -> [String] {
        IssueSorting.sorted(issues, status: status, today: today).map(\.id)
    }

    // Non-terminal: an overdue low-priority issue beats a non-overdue urgent
    // one (web board-view.test.ts parity), and due-today is NOT overdue.
    func testOverdueBoostBeatsPriority() {
        let overdueLow = makeIssue(id: "overdue-low", number: 1, priority: .low, dueDate: "2026-07-01")
        let urgentNoDue = makeIssue(id: "urgent-no-due", number: 2, priority: .urgent)
        let mediumToday = makeIssue(id: "medium-today", number: 3, priority: .medium, dueDate: today)
        let nonePriority = makeIssue(id: "none-priority", number: 4, priority: .none)

        XCTAssertEqual(
            sortedIds([nonePriority, mediumToday, urgentNoDue, overdueLow], status: .todo),
            ["overdue-low", "urgent-no-due", "medium-today", "none-priority"]
        )
    }

    // Non-terminal: full priority ladder urgent → high → medium → low → none.
    func testPriorityRankOrdering() {
        let issues = [
            makeIssue(id: "none", number: 1, priority: .none),
            makeIssue(id: "low", number: 2, priority: .low),
            makeIssue(id: "medium", number: 3, priority: .medium),
            makeIssue(id: "high", number: 4, priority: .high),
            makeIssue(id: "urgent", number: 5, priority: .urgent),
        ]
        XCTAssertEqual(
            sortedIds(issues, status: .backlog),
            ["urgent", "high", "medium", "low", "none"]
        )
    }

    // Non-terminal: same priority → dueDate ascending, nil due dates LAST.
    func testDueDateAscendingWithNilLast() {
        let issues = [
            makeIssue(id: "no-due", number: 1, priority: .high),
            makeIssue(id: "later", number: 2, priority: .high, dueDate: "2026-08-01"),
            makeIssue(id: "sooner", number: 3, priority: .high, dueDate: "2026-07-10"),
        ]
        XCTAssertEqual(
            sortedIds(issues, status: .inProgress),
            ["sooner", "later", "no-due"]
        )
    }

    // Non-terminal: final tie-break is `number` ascending NUMERICALLY — an
    // identifier-string sort would put EXP-10 before EXP-9. Nil number last.
    // sortOrder must be ignored entirely (it's set to contradict the order).
    func testNumberTieBreakIsNumericNeverIdentifierString() {
        let issues = [
            makeIssue(id: "ten", number: 10, identifier: "EXP-10", sortOrder: 1),
            makeIssue(id: "nine", number: 9, identifier: "EXP-9", sortOrder: 2),
            makeIssue(id: "no-number", number: nil, identifier: nil, sortOrder: 0),
        ]
        XCTAssertEqual(
            sortedIds(issues, status: .todo),
            ["nine", "ten", "no-number"]
        )
    }

    // Done: (completedAt ?? updatedAt) DESCENDING — latest completed first,
    // with updatedAt as the fallback key for rows missing completedAt.
    func testDoneGroupLatestCompletedFirst() {
        let issues = [
            makeIssue(
                id: "old-done", number: 1, status: .done,
                completedAt: "2026-07-01 09:00:00+00"
            ),
            makeIssue(
                id: "new-done", number: 2, status: .done,
                completedAt: "2026-07-08 09:00:00+00"
            ),
            makeIssue(
                id: "fallback-updated", number: 3, status: .done,
                completedAt: nil, updatedAt: "2026-07-05 09:00:00+00"
            ),
        ]
        XCTAssertEqual(
            sortedIds(issues, status: .done),
            ["new-done", "fallback-updated", "old-done"]
        )
    }

    // Cancelled and duplicate: updatedAt DESCENDING.
    func testCancelledAndDuplicateGroupsLatestUpdatedFirst() {
        let issues = [
            makeIssue(id: "older", number: 1, status: .cancelled, updatedAt: "2026-07-02 09:00:00+00"),
            makeIssue(id: "newer", number: 2, status: .cancelled, updatedAt: "2026-07-07 09:00:00+00"),
        ]
        XCTAssertEqual(sortedIds(issues, status: .cancelled), ["newer", "older"])
        XCTAssertEqual(sortedIds(issues, status: .duplicate), ["newer", "older"])
    }

    // The overdue check is a plain string comparison against the local
    // yyyy-MM-dd date: strictly-before is overdue, today/future are not.
    func testOverdueIsStrictlyBeforeToday() {
        XCTAssertTrue(IssueSorting.isOverdue(makeIssue(id: "a", dueDate: "2026-07-08"), today: today))
        XCTAssertFalse(IssueSorting.isOverdue(makeIssue(id: "b", dueDate: today), today: today))
        XCTAssertFalse(IssueSorting.isOverdue(makeIssue(id: "c", dueDate: "2026-07-10"), today: today))
        XCTAssertFalse(IssueSorting.isOverdue(makeIssue(id: "d", dueDate: nil), today: today))
    }

    func testTodayStringFormat() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let date = Date(timeIntervalSince1970: 1_772_000_000) // 2026-02-25 UTC
        XCTAssertEqual(IssueSorting.todayString(now: date, calendar: calendar), "2026-02-25")
    }

    // QA1480 regression: a Buddhist-calendar device (Thailand default) must not
    // produce "2569-…" in the yyyy-MM-dd wire string space — that would mark
    // every synced due date overdue.
    func testTodayStringIsGregorianOnNonGregorianDeviceCalendar() {
        var buddhist = Calendar(identifier: .buddhist)
        buddhist.timeZone = TimeZone(identifier: "UTC")!
        let date = Date(timeIntervalSince1970: 1_772_000_000) // 2026-02-25 UTC
        XCTAssertEqual(IssueSorting.todayString(now: date, calendar: buddhist), "2026-02-25")
        var japanese = Calendar(identifier: .japanese)
        japanese.timeZone = TimeZone(identifier: "UTC")!
        XCTAssertEqual(IssueSorting.todayString(now: date, calendar: japanese), "2026-02-25")
    }
}
