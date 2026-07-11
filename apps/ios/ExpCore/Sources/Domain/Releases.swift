import Foundation

// Release progress + ordering (EXP-56 §10.2/§10.3) — the cross-platform
// contract, mirroring apps/web/src/lib/releases.ts exactly. Pure functions
// shared by the releases list, the release detail, and the issue picker.

/// Progress over a release's member issues. `cancelled`/`duplicate` issues
/// are *dropped*, not shipped — they leave the denominator instead of
/// counting as progress.
public struct ReleaseProgress: Sendable, Equatable {
    public let total: Int
    public let done: Int
    public let dropped: Int
    public let denominator: Int
    public let fraction: Double
    /// "Ready to ship": every non-dropped issue is done. Independent of
    /// shipped_at — shipping early (or an empty release) is allowed, it just
    /// never reads as Ready.
    public let isComplete: Bool
}

/// Compute progress from the member issues' raw status strings.
public func releaseProgress(statuses: [String]) -> ReleaseProgress {
    let total = statuses.count
    var done = 0
    var dropped = 0
    for status in statuses {
        if status == IssueStatus.done.rawValue {
            done += 1
        } else if status == IssueStatus.cancelled.rawValue
            || status == IssueStatus.duplicate.rawValue {
            dropped += 1
        }
    }
    let denominator = total - dropped
    return ReleaseProgress(
        total: total,
        done: done,
        dropped: dropped,
        denominator: denominator,
        fraction: denominator > 0 ? Double(done) / Double(denominator) : 0,
        isComplete: denominator > 0 && done == denominator
    )
}

public func releaseProgress(issues: [IssueEntity]) -> ReleaseProgress {
    releaseProgress(statuses: issues.map(\.status))
}

/// Canonical release ordering: unshipped before shipped; unshipped by
/// targetDate asc with nulls LAST (a dated release is more urgent than an
/// undated one) then createdAt desc; shipped by shippedAt desc (most recently
/// shipped first). String comparisons are safe — targetDate is a plain DATE
/// and the timestamps arrive as sortable ISO-8601 text.
public func compareReleases(_ a: ReleaseEntity, _ b: ReleaseEntity) -> Bool {
    let aShipped = a.shippedAt != nil
    let bShipped = b.shippedAt != nil
    if aShipped != bShipped {
        return !aShipped
    }

    if let aShippedAt = a.shippedAt, let bShippedAt = b.shippedAt {
        if aShippedAt != bShippedAt { return aShippedAt > bShippedAt }
        return a.createdAt > b.createdAt
    }

    switch (a.targetDate, b.targetDate) {
    case let (aDate?, bDate?) where aDate != bDate:
        return aDate < bDate
    case (.some, .none):
        return true
    case (.none, .some):
        return false
    default:
        return a.createdAt > b.createdAt
    }
}
