import Foundation

/// EXP-314 — custom issue statuses. The cross-platform resolution contract
/// every client implements byte-identically (web `lib/issue-status.ts`,
/// desktop `domain/src/statuses.rs`, Android `domain/IssueStatusResolution.kt`).
///
/// A team owns a set of `issue_statuses` rows: 7 LOCKED builtins (one per
/// `issue_status` enum value, `builtinKey` non-nil) plus any number of custom
/// rows. `issues.status` stays the builtin ANCHOR — dual-written by the server
/// for every status change — so anchor-keyed surfaces (cross-team grouping,
/// terminal sets, coding gates) keep working verbatim.
///
/// Rendering must NEVER fail: when the statuses shape hasn't synced yet the
/// resolver CONSTRUCTS the builtin defaults from the generated contract, keyed
/// `builtin:<key>`.

// MARK: - Category

public enum IssueStatusCategory: String, CaseIterable, Sendable, Hashable {
    case backlog
    case unstarted
    case started
    case completed
    case cancelled
    case duplicate

    /// Tolerant wire mapping: an unknown category from a NEWER server decodes
    /// to nil, and callers degrade it to `.backlog` (sorting it last) instead
    /// of failing — see `IssueStatusResolver.teamStatuses`.
    public static func from(_ wire: String?) -> IssueStatusCategory? {
        guard let wire else { return nil }
        return IssueStatusCategory(rawValue: wire)
    }

    /// Group order on a board list (locked to `DomainContract` by test).
    public static let displayOrder: [IssueStatusCategory] = [
        .started, .unstarted, .backlog, .completed, .cancelled, .duplicate,
    ]

    /// Section order in a management surface (locked to `DomainContract`).
    public static let settingsOrder: [IssueStatusCategory] = [
        .backlog, .unstarted, .started, .completed, .cancelled, .duplicate,
    ]
}

// MARK: - Resolved status

/// One render-ready status: a real synced row, or a locally-constructed builtin
/// default when the shape hasn't landed yet.
public struct ResolvedIssueStatus: Identifiable, Equatable, Sendable, Hashable {
    /// The GROUP KEY: the row id, or `builtin:<key>` for a constructed default.
    /// Filters, collapse state and list grouping all key on this.
    public let id: String
    /// The real `issue_statuses` row id — nil for a constructed default, which
    /// is exactly the signal that a write must fall back to the anchor enum.
    public let rowId: String?
    public let name: String
    public let category: IssueStatusCategory
    /// Hex swatch. Only CUSTOM rows render it (builtins keep the platform's
    /// design-token colors — see `ResolvedIssueStatus.color` in ExpUI).
    public let colorHex: String?
    /// Non-nil ⇒ a builtin row (or a constructed default) anchored to this
    /// enum value.
    public let builtinKey: IssueStatus?
    /// Shared-registry icon name (EXP-273) — render through `AppIcon`.
    public let iconName: String

    public init(
        id: String,
        rowId: String?,
        name: String,
        category: IssueStatusCategory,
        colorHex: String?,
        builtinKey: IssueStatus?,
        iconName: String
    ) {
        self.id = id
        self.rowId = rowId
        self.name = name
        self.category = category
        self.colorHex = colorHex
        self.builtinKey = builtinKey
        self.iconName = iconName
    }

    /// The anchor enum a write for this status should fall back to when there
    /// is no real row to reference.
    public var anchor: IssueStatus { builtinKey ?? IssueStatusResolver.anchor(for: category) }
}

// MARK: - Resolver

public enum IssueStatusResolver {
    /// Prefix of a constructed default's synthetic group key.
    public static let builtinIdPrefix = "builtin:"

    // MARK: Glyphs

    /// Non-started categories map 1:1 to a registry glyph.
    static func categoryIconName(_ category: IssueStatusCategory) -> String {
        switch category {
        case .backlog: "circle-dashed"
        case .unstarted: "circle"
        case .started: "progress-2-4" // overridden by the clock table
        case .completed: "circle-check"
        case .cancelled: "circle-x"
        case .duplicate: "copy"
        }
    }

    /// The started-category pie clock: a team's started statuses read left to
    /// right as a progress ramp. The literal name lists ARE the cross-platform
    /// contract — every client's unit test locks this exact table.
    public static func startedClockIconName(index: Int, count: Int) -> String {
        if count <= 2 {
            let names = ["progress-2-4", "progress-3-4"]
            return names[min(max(index, 0), names.count - 1)]
        }
        if count == 3 {
            let names = ["progress-1-4", "progress-2-4", "progress-3-4"]
            return names[min(max(index, 0), names.count - 1)]
        }
        let names = ["progress-1-5", "progress-2-5", "progress-3-5", "progress-4-5"]
        return names[min(max(index, 0), names.count - 1)]
    }

    /// Glyph for a status: category-keyed, with started resolved by position
    /// among the team's started rows.
    public static func iconName(
        category: IssueStatusCategory, startedIndex: Int = 0, startedCount: Int = 2
    ) -> String {
        guard category == .started else { return categoryIconName(category) }
        return startedClockIconName(index: startedIndex, count: startedCount)
    }

    // MARK: Anchors

    /// CATEGORY_ANCHOR — which builtin enum a CUSTOM status of this category
    /// anchors to (the server's derive-trigger map, mirrored client-side).
    public static func anchor(for category: IssueStatusCategory) -> IssueStatus {
        switch category {
        case .backlog: .backlog
        case .unstarted: .todo
        case .started: .inProgress
        case .completed: .done
        case .cancelled: .cancelled
        case .duplicate: .duplicate
        }
    }

    // MARK: Builtin defaults

    /// The 7 locked builtin rows, CONSTRUCTED from the generated contract —
    /// the fallback set used until (or when) the statuses shape has no row.
    public static let builtinDefaults: [ResolvedIssueStatus] = {
        let keys = DomainContract.issueStatusDefaultKeys
        let categories = DomainContract.issueStatusDefaultCategories
        let names = DomainContract.issueStatusDefaultNames
        let colors = DomainContract.issueStatusDefaultColors
        let count = min(keys.count, min(categories.count, min(names.count, colors.count)))
        // The generated started defaults (In Progress, In Review) are the N=2
        // clock pair, so their glyphs come out of the same table.
        let startedKeys = (0..<count).filter { categories[$0] == IssueStatusCategory.started.rawValue }
        return (0..<count).map { index -> ResolvedIssueStatus in
            let category = IssueStatusCategory.from(categories[index]) ?? .backlog
            let startedIndex = startedKeys.firstIndex(of: index) ?? 0
            return ResolvedIssueStatus(
                id: builtinIdPrefix + keys[index],
                rowId: nil,
                name: names[index],
                category: category,
                colorHex: colors[index],
                builtinKey: IssueStatus(rawValue: keys[index]),
                iconName: iconName(
                    category: category,
                    startedIndex: startedIndex,
                    startedCount: max(startedKeys.count, 1)
                )
            )
        }
    }()

    /// The constructed default set in the SAME order `teamStatuses` would
    /// produce (category displayOrder, then the contract sort orders) — what a
    /// board list / picker renders while the statuses shape is still syncing.
    public static let builtinFallbackTeam: [ResolvedIssueStatus] = {
        let keys = DomainContract.issueStatusDefaultKeys
        let sortOrders = DomainContract.issueStatusDefaultSortOrders
        return builtinDefaults.enumerated().sorted { a, b in
            let ra = IssueStatusCategory.displayOrder.firstIndex(of: a.element.category)
                ?? IssueStatusCategory.displayOrder.count
            let rb = IssueStatusCategory.displayOrder.firstIndex(of: b.element.category)
                ?? IssueStatusCategory.displayOrder.count
            if ra != rb { return ra < rb }
            let sa = a.offset < sortOrders.count ? sortOrders[a.offset] : 0
            let sb = b.offset < sortOrders.count ? sortOrders[b.offset] : 0
            if sa != sb { return sa < sb }
            return keys[a.offset] < keys[b.offset]
        }.map(\.element)
    }()

    /// A team's render list, degrading to the constructed builtins while the
    /// `issue_statuses` shape has delivered nothing for this team yet.
    public static func teamStatusesOrFallback(_ rows: [IssueStatusEntity]) -> [ResolvedIssueStatus] {
        let resolved = teamStatuses(rows)
        return resolved.isEmpty ? builtinFallbackTeam : resolved
    }

    /// The constructed default for one anchor. Unknown/missing ⇒ backlog,
    /// mirroring `IssueStatus.from(_:)`.
    public static func builtinDefault(for anchor: IssueStatus) -> ResolvedIssueStatus {
        builtinDefaults.first { $0.builtinKey == anchor }
            ?? builtinDefaults.first { $0.builtinKey == .backlog }
            ?? ResolvedIssueStatus(
                id: builtinIdPrefix + IssueStatus.backlog.rawValue,
                rowId: nil,
                name: IssueStatus.backlog.label,
                category: .backlog,
                colorHex: nil,
                builtinKey: .backlog,
                iconName: categoryIconName(.backlog)
            )
    }

    // MARK: Team ordering

    /// One team's statuses in render order: category `displayOrder`, then
    /// `sortOrder` asc, then `createdAt` asc, then `id` — the tie-break chain
    /// every client implements identically. Started glyphs are assigned from
    /// each started row's position in THIS order.
    public static func teamStatuses(_ rows: [IssueStatusEntity]) -> [ResolvedIssueStatus] {
        let ordered = rows.sorted { a, b in
            let ca = categoryRank(a.category)
            let cb = categoryRank(b.category)
            if ca != cb { return ca < cb }
            let sa = a.sortOrder ?? 0
            let sb = b.sortOrder ?? 0
            if sa != sb { return sa < sb }
            if a.createdAt != b.createdAt { return a.createdAt < b.createdAt }
            return a.id < b.id
        }
        let startedIds = ordered
            .filter { IssueStatusCategory.from($0.category) == .started }
            .map(\.id)
        return ordered.map { row in
            let builtinKey = row.builtinKey.flatMap { IssueStatus(rawValue: $0) }
            // UNKNOWN CATEGORY (cross-platform rule): a category from a NEWER
            // server never drops the row — it degrades to `backlog` for every
            // consumer at once. It sorts LAST here (see `categoryRank`), and
            // renders the backlog treatment (dashed circle + neutral color)
            // and takes the ACTIVE in-group sort branch through this
            // `category`. Rendering and sorting must never disagree, so this
            // deliberately does NOT route through the anchor's category.
            let effective = IssueStatusCategory.from(row.category) ?? .backlog
            let startedIndex = startedIds.firstIndex(of: row.id) ?? 0
            return ResolvedIssueStatus(
                id: row.id,
                rowId: row.id,
                name: row.name,
                category: effective,
                colorHex: row.color,
                builtinKey: builtinKey,
                iconName: iconName(
                    category: effective,
                    startedIndex: startedIndex,
                    startedCount: max(startedIds.count, 1)
                )
            )
        }
    }

    /// Unknown categories sort last (stable, never a crash) — the sort half of
    /// the UNKNOWN CATEGORY rule whose render half lives in `teamStatuses`.
    private static func categoryRank(_ wire: String) -> Int {
        guard let category = IssueStatusCategory.from(wire),
              let rank = IssueStatusCategory.displayOrder.firstIndex(of: category)
        else { return IssueStatusCategory.displayOrder.count }
        return rank
    }

    /// The category a builtin enum value belongs to (the seed mapping).
    public static func categoryOf(_ status: IssueStatus) -> IssueStatusCategory {
        guard let index = DomainContract.issueStatusDefaultKeys.firstIndex(of: status.rawValue),
              index < DomainContract.issueStatusDefaultCategories.count,
              let category = IssueStatusCategory.from(DomainContract.issueStatusDefaultCategories[index])
        else { return .backlog }
        return category
    }

    // MARK: Resolution

    /// The resolution chain: (a) the team row with `id == statusId`,
    /// (b) else the team row whose `builtinKey` matches the anchor,
    /// (c) else a constructed builtin default. Never fails.
    ///
    /// UNKNOWN ANCHOR (cross-platform rule): a `status` enum value from a
    /// NEWER server normalizes to `.backlog` via `IssueStatus.from` BEFORE the
    /// row lookup, so the issue joins the team's REAL Backlog row/group; only
    /// with no synced rows at all does it land on the constructed default.
    public static func resolve(
        statusId: String?, anchor: String?, team: [ResolvedIssueStatus]
    ) -> ResolvedIssueStatus {
        if let statusId, let row = team.first(where: { $0.rowId == statusId }) { return row }
        let anchorStatus = IssueStatus.from(anchor)
        if let row = team.first(where: { $0.builtinKey == anchorStatus }) { return row }
        return builtinDefault(for: anchorStatus)
    }

    /// Convenience over a synced issue row.
    public static func resolve(_ issue: IssueEntity, team: [ResolvedIssueStatus]) -> ResolvedIssueStatus {
        resolve(statusId: issue.statusId, anchor: issue.status, team: team)
    }
}
