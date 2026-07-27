import Foundation

/// Canonical in-group issue ordering (EXP-38) — the cross-platform contract all
/// four clients implement with identical semantics (web `lib/board-view.ts`,
/// Android, desktop). Group ORDER itself lives elsewhere (EXP-314:
/// `IssueStatusResolver.teamStatuses`); this governs order WITHIN one status
/// group and switches on that group's CATEGORY, so a CUSTOM status sorts
/// exactly like the builtin it anchors to:
///
/// - backlog / unstarted / started: OVERDUE FIRST (dueDate < today), then
///   priority rank urgent(0) → none(4) ascending, then dueDate ascending with
///   nil LAST, then issue `number` ascending NUMERICALLY (nil last — never the
///   identifier string, which sorts "EXP-9" after "EXP-10").
/// - completed: key = (completedAt ?? updatedAt), DESCENDING (latest first).
/// - cancelled / duplicate: updatedAt DESCENDING.
///
/// The fractional `sortOrder` column is deliberately NOT consulted anymore.
/// Timestamps are Postgres text in one consistent wire format per column, so
/// plain string comparison is chronological (InboxViewModel precedent).
public enum IssueSorting {
    /// Today's local calendar date as `yyyy-MM-dd` — the same string space as
    /// `issues.due_date`, so plain string comparison is chronological. The
    /// components are ALWAYS Gregorian (wire strings must never carry
    /// Buddhist/Japanese device-calendar years); only `calendar`'s time zone
    /// is honored — "today" means the local calendar day.
    public static func todayString(now: Date = Date(), calendar: Calendar = .current) -> String {
        var gregorian = Calendar(identifier: .gregorian)
        gregorian.timeZone = calendar.timeZone
        let c = gregorian.dateComponents([.year, .month, .day], from: now)
        return String(format: "%04d-%02d-%02d", c.year ?? 0, c.month ?? 0, c.day ?? 0)
    }

    /// Sort one status group's issues per the canonical contract. `today` is
    /// injectable for tests; callers use the default.
    public static func sorted(
        _ issues: [IssueEntity], status: IssueStatus, today: String = todayString()
    ) -> [IssueEntity] {
        sorted(issues, category: IssueStatusResolver.categoryOf(status), today: today)
    }

    /// EXP-314: the same contract keyed on the group's CATEGORY, so a CUSTOM
    /// status sorts exactly like the builtin it anchors to. The branches are
    /// identical to the enum switch above; an unknown category takes the
    /// active branch.
    public static func sorted(
        _ issues: [IssueEntity], category: IssueStatusCategory, today: String = todayString()
    ) -> [IssueEntity] {
        issues.sorted { compare($0, $1, category: category, today: today) == .orderedAscending }
    }

    static func compare(
        _ a: IssueEntity, _ b: IssueEntity, status: IssueStatus, today: String
    ) -> ComparisonResult {
        compare(a, b, category: IssueStatusResolver.categoryOf(status), today: today)
    }

    static func compare(
        _ a: IssueEntity, _ b: IssueEntity, category: IssueStatusCategory, today: String
    ) -> ComparisonResult {
        switch category {
        case .completed:
            let ka = a.completedAt ?? a.updatedAt
            let kb = b.completedAt ?? b.updatedAt
            if ka != kb { return kb < ka ? .orderedAscending : .orderedDescending }
        case .cancelled, .duplicate:
            if a.updatedAt != b.updatedAt {
                return b.updatedAt < a.updatedAt ? .orderedAscending : .orderedDescending
            }
        case .backlog, .unstarted, .started:
            let aOverdue = isOverdue(a, today: today)
            let bOverdue = isOverdue(b, today: today)
            if aOverdue != bOverdue { return aOverdue ? .orderedAscending : .orderedDescending }

            let pa = priorityRank(a.priority)
            let pb = priorityRank(b.priority)
            if pa != pb { return pa < pb ? .orderedAscending : .orderedDescending }

            switch (a.dueDate, b.dueDate) {
            case let (da?, db?) where da != db:
                return da < db ? .orderedAscending : .orderedDescending
            case (.some, .none): return .orderedAscending // nil due dates last
            case (.none, .some): return .orderedDescending
            default: break
            }
        }
        // Shared final tie-break: `number` ascending numerically, nil last.
        // Swift's sort is not stability-guaranteed, so ties must resolve
        // deterministically (id as the ultimate fallback for cross-board
        // lists where numbers can collide).
        switch (a.number, b.number) {
        case let (na?, nb?) where na != nb:
            return na < nb ? .orderedAscending : .orderedDescending
        case (.some, .none): return .orderedAscending
        case (.none, .some): return .orderedDescending
        default:
            return a.id < b.id ? .orderedAscending : .orderedDescending
        }
    }

    /// Overdue = due strictly before today. Only the non-terminal groups
    /// consult it (terminal groups sort purely by resolution recency).
    static func isOverdue(_ issue: IssueEntity, today: String) -> Bool {
        guard let due = issue.dueDate else { return false }
        return due < today
    }

    /// Rank = index in `IssuePriority.displayOrder`: urgent(0) … none(4).
    static func priorityRank(_ wire: String?) -> Int {
        IssuePriority.displayOrder.firstIndex(of: IssuePriority.from(wire))
            ?? IssuePriority.displayOrder.count
    }
}
