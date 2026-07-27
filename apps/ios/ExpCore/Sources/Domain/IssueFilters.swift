import Foundation

// Mirrored from apps/web/src/lib/filters.ts. The filter shape and matching
// semantics must match the web mapping; if you change one, change the others
// (apps/web/src/lib/filters.ts, apps/android/.../domain/IssueFilters.kt).
public struct IssueFilters: Equatable, Sendable {
    /// EXP-314: status GROUP KEYS — an `issue_statuses` row id, or the
    /// synthetic `builtin:<key>` of a constructed default while the statuses
    /// shape hasn't synced. Same semantics as `labelIds`.
    public var statusIds: Set<String> = []
    public var priorities: Set<IssuePriority> = []
    public var labelIds: Set<String> = []

    public init(statusIds: Set<String> = [], priorities: Set<IssuePriority> = [], labelIds: Set<String> = []) {
        self.statusIds = statusIds
        self.priorities = priorities
        self.labelIds = labelIds
    }

    public var isEmpty: Bool { statusIds.isEmpty && priorities.isEmpty && labelIds.isEmpty }
    public var count: Int { statusIds.count + priorities.count + labelIds.count }

    /// True when the status filter set selects this resolved status, honoring
    /// the `builtin:<key>` ↔ synced-row equivalence (see
    /// `statusMatchesFilterToken`).
    public func selectsStatus(_ status: ResolvedIssueStatus) -> Bool {
        statusIds.contains { statusMatchesFilterToken(status, token: $0) }
    }

    /// Toggle a status in the filter set. Turning one OFF drops every token
    /// equivalent to it, so a stale `builtin:<key>` token can't survive an
    /// un-check of the synced row it re-keyed into.
    public mutating func toggleStatus(_ status: ResolvedIssueStatus) {
        let equivalent = statusIds.filter { statusMatchesFilterToken(status, token: $0) }
        if equivalent.isEmpty {
            statusIds.insert(status.id)
        } else {
            statusIds.subtract(equivalent)
        }
    }
}

/// FILTER TOKENS (EXP-314 cross-platform rule) — a stored status token must
/// survive the fallback→synced re-key. Filters hold GROUP KEYS: a real
/// `issue_statuses` row id, or the synthetic `builtin:<key>` of a constructed
/// default picked while the statuses shape hadn't synced yet. When the shape
/// lands mid-session those groups re-key to row uuids, so a `builtin:<key>`
/// token also matches the SYNCED row whose `builtinKey` is `<key>`; a row-uuid
/// token matches ONLY that row. (Web mirrors this in `statusOptionMatchesToken`
/// — it additionally accepts legacy bare-enum tokens because its filters ride
/// the URL; native filters only ever write group keys.)
public func statusMatchesFilterToken(_ status: ResolvedIssueStatus, token: String) -> Bool {
    if token == status.id { return true }
    guard token.hasPrefix(IssueStatusResolver.builtinIdPrefix) else { return false }
    let key = String(token.dropFirst(IssueStatusResolver.builtinIdPrefix.count))
    return status.builtinKey?.rawValue == key
}

/// `status` is the issue's RESOLVED status (`IssueStatusResolver.resolve`), not
/// the raw `issues.status_id` column — a pre-backfill row still matches the
/// builtin group it resolves into.
public func matchesFilters(
    status: ResolvedIssueStatus,
    priority: IssuePriority,
    issueLabelIds: Set<String>,
    filters: IssueFilters
) -> Bool {
    if !filters.statusIds.isEmpty && !filters.selectsStatus(status) { return false }
    if !filters.priorities.isEmpty && !filters.priorities.contains(priority) { return false }
    if !filters.labelIds.isEmpty && filters.labelIds.isDisjoint(with: issueLabelIds) { return false }
    return true
}
