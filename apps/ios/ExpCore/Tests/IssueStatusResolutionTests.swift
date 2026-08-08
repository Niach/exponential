import Foundation
import XCTest
@testable import ExpCore

// EXP-314 — the custom-status resolution contract. Every assertion here has a
// byte-identical twin on web, desktop and Android: the started-clock table, the
// resolve fallback chain, the category orders and the constructed builtin
// defaults are cross-platform contracts with no shared code behind them.
final class IssueStatusResolutionTests: XCTestCase {

    // MARK: - Fixtures

    private func row(
        id: String,
        category: IssueStatusCategory,
        name: String,
        color: String? = nil,
        sortOrder: Double = 1,
        builtinKey: IssueStatus? = nil,
        createdAt: String = "2026-01-01 00:00:00+00"
    ) -> IssueStatusEntity {
        IssueStatusEntity(
            id: id,
            teamId: "team-1",
            category: category.rawValue,
            name: name,
            color: color,
            sortOrder: sortOrder,
            builtinKey: builtinKey?.rawValue,
            createdAt: createdAt,
            updatedAt: createdAt
        )
    }

    private func issue(
        id: String,
        number: Int? = nil,
        status: String = "todo",
        statusId: String? = nil,
        priority: IssuePriority = .none,
        updatedAt: String = "2026-07-01 10:00:00+00"
    ) -> IssueEntity {
        IssueEntity(
            id: id, boardId: "b1", number: number, identifier: nil, title: id,
            description: nil, status: status, statusId: statusId,
            priority: priority.rawValue,
            assigneeId: nil, creatorId: nil, source: nil, dueDate: nil,
            sortOrder: nil, completedAt: nil, duplicateOfId: nil,
            prUrl: nil, prNumber: nil, prState: nil, branch: nil, prMergedAt: nil,
            createdAt: "2026-06-01 10:00:00+00", updatedAt: updatedAt
        )
    }

    /// The 7 locked builtin rows exactly as a team is seeded.
    private func seededTeam() -> [IssueStatusEntity] {
        DomainContract.issueStatusDefaultKeys.indices.map { index in
            let key = DomainContract.issueStatusDefaultKeys[index]
            return row(
                id: "row-\(key)",
                category: IssueStatusCategory(rawValue: DomainContract.issueStatusDefaultCategories[index])!,
                name: DomainContract.issueStatusDefaultNames[index],
                sortOrder: Double(DomainContract.issueStatusDefaultSortOrders[index]),
                builtinKey: IssueStatus(rawValue: key)
            )
        }
    }

    // MARK: - Category orders

    func testCategoryValuesAndOrderMatchGeneratedContract() {
        XCTAssertEqual(
            IssueStatusCategory.allCases.map(\.rawValue),
            DomainContract.issueStatusCategoryValues
        )
        // EXP-448: ONE order — list groups, pickers and the settings sections.
        XCTAssertEqual(
            IssueStatusCategory.displayOrder,
            [.backlog, .unstarted, .started, .completed, .cancelled, .duplicate]
        )
        XCTAssertEqual(
            IssueStatusCategory.displayOrder.map(\.rawValue),
            DomainContract.issueStatusCategoryDisplayOrder
        )
    }

    func testUnknownCategoryDecodesToNil() {
        XCTAssertEqual(IssueStatusCategory.from("started"), .started)
        XCTAssertNil(IssueStatusCategory.from("triaging"))
        XCTAssertNil(IssueStatusCategory.from(nil))
    }

    // MARK: - Started clock table (the cross-platform literal contract)

    func testStartedClockTableForOneAndTwoStartedStatuses() {
        XCTAssertEqual(IssueStatusResolver.startedClockIconName(index: 0, count: 1), "progress-2-4")
        XCTAssertEqual(IssueStatusResolver.startedClockIconName(index: 0, count: 2), "progress-2-4")
        XCTAssertEqual(IssueStatusResolver.startedClockIconName(index: 1, count: 2), "progress-3-4")
    }

    func testStartedClockTableForThreeStartedStatuses() {
        XCTAssertEqual(
            (0..<3).map { IssueStatusResolver.startedClockIconName(index: $0, count: 3) },
            ["progress-1-4", "progress-2-4", "progress-3-4"]
        )
    }

    func testStartedClockTableForFourOrMoreStartedStatuses() {
        XCTAssertEqual(
            (0..<4).map { IssueStatusResolver.startedClockIconName(index: $0, count: 4) },
            ["progress-1-5", "progress-2-5", "progress-3-5", "progress-4-5"]
        )
        // Transiently >4 (racing owners) clamps into the 5-slice table.
        XCTAssertEqual(
            (0..<5).map { IssueStatusResolver.startedClockIconName(index: $0, count: 5) },
            ["progress-1-5", "progress-2-5", "progress-3-5", "progress-4-5", "progress-4-5"]
        )
    }

    func testClockIndexIsClampedIntoRange() {
        XCTAssertEqual(IssueStatusResolver.startedClockIconName(index: -3, count: 3), "progress-1-4")
        XCTAssertEqual(IssueStatusResolver.startedClockIconName(index: 99, count: 2), "progress-3-4")
    }

    func testNonStartedCategoryGlyphs() {
        XCTAssertEqual(IssueStatusResolver.iconName(category: .backlog), "circle-dashed")
        XCTAssertEqual(IssueStatusResolver.iconName(category: .unstarted), "circle")
        XCTAssertEqual(IssueStatusResolver.iconName(category: .completed), "circle-check")
        XCTAssertEqual(IssueStatusResolver.iconName(category: .cancelled), "circle-x")
        XCTAssertEqual(IssueStatusResolver.iconName(category: .duplicate), "copy")
    }

    // MARK: - Constructed builtin defaults

    func testBuiltinDefaultsMirrorTheGeneratedContract() {
        XCTAssertEqual(
            IssueStatusResolver.builtinDefaults.map { $0.builtinKey?.rawValue },
            DomainContract.issueStatusDefaultKeys
        )
        XCTAssertEqual(
            IssueStatusResolver.builtinDefaults.map(\.name),
            DomainContract.issueStatusDefaultNames
        )
        XCTAssertEqual(
            IssueStatusResolver.builtinDefaults.map { $0.category.rawValue },
            DomainContract.issueStatusDefaultCategories
        )
        XCTAssertEqual(
            IssueStatusResolver.builtinDefaults.map { $0.colorHex },
            DomainContract.issueStatusDefaultColors
        )
        // Group keys are the synthetic form — no row exists behind them.
        XCTAssertEqual(
            IssueStatusResolver.builtinDefaults.map(\.id),
            DomainContract.issueStatusDefaultKeys.map { "builtin:\($0)" }
        )
        XCTAssertTrue(IssueStatusResolver.builtinDefaults.allSatisfy { $0.rowId == nil })
    }

    // The two seeded started builtins are the N=2 clock pair — the exact
    // glyphs the icon registry re-pointed `status-in-progress` /
    // `status-in-review` to (EXP-273 + EXP-314).
    func testBuiltinDefaultGlyphs() {
        XCTAssertEqual(IssueStatusResolver.builtinDefault(for: .backlog).iconName, "circle-dashed")
        XCTAssertEqual(IssueStatusResolver.builtinDefault(for: .todo).iconName, "circle")
        XCTAssertEqual(IssueStatusResolver.builtinDefault(for: .inProgress).iconName, "progress-2-4")
        XCTAssertEqual(IssueStatusResolver.builtinDefault(for: .inReview).iconName, "progress-3-4")
        XCTAssertEqual(IssueStatusResolver.builtinDefault(for: .done).iconName, "circle-check")
        XCTAssertEqual(IssueStatusResolver.builtinDefault(for: .cancelled).iconName, "circle-x")
        XCTAssertEqual(IssueStatusResolver.builtinDefault(for: .duplicate).iconName, "copy")
    }

    func testFallbackTeamIsInCategoryDisplayOrder() {
        XCTAssertEqual(
            IssueStatusResolver.builtinFallbackTeam.map { $0.builtinKey?.rawValue },
            ["backlog", "todo", "in_progress", "in_review", "done", "cancelled", "duplicate"]
        )
    }

    func testCategoryAnchorMap() {
        XCTAssertEqual(IssueStatusResolver.anchor(for: .backlog), .backlog)
        XCTAssertEqual(IssueStatusResolver.anchor(for: .unstarted), .todo)
        XCTAssertEqual(IssueStatusResolver.anchor(for: .started), .inProgress)
        XCTAssertEqual(IssueStatusResolver.anchor(for: .completed), .done)
        XCTAssertEqual(IssueStatusResolver.anchor(for: .cancelled), .cancelled)
        XCTAssertEqual(IssueStatusResolver.anchor(for: .duplicate), .duplicate)
    }

    // MARK: - Team ordering

    func testTeamStatusesOrderIsCategoryThenSortOrderThenCreatedAtThenId() {
        let rows = [
            row(id: "z", category: .backlog, name: "Icebox", sortOrder: 2),
            row(id: "a", category: .completed, name: "Shipped", sortOrder: 1),
            row(id: "b", category: .started, name: "Coding", sortOrder: 2),
            row(id: "c", category: .started, name: "Designing", sortOrder: 1),
            // Same category + sortOrder: createdAt breaks the tie, then id.
            row(id: "e", category: .unstarted, name: "Later", sortOrder: 1, createdAt: "2026-02-02 00:00:00+00"),
            row(id: "d", category: .unstarted, name: "Ready", sortOrder: 1, createdAt: "2026-01-05 00:00:00+00"),
        ]
        XCTAssertEqual(
            IssueStatusResolver.teamStatuses(rows).map(\.id),
            ["z", "d", "e", "c", "b", "a"]
        )
    }

    func testStartedGlyphsFollowPositionInTeamOrder() {
        let rows = [
            row(id: "s3", category: .started, name: "Review", sortOrder: 3),
            row(id: "s1", category: .started, name: "Design", sortOrder: 1),
            row(id: "s2", category: .started, name: "Build", sortOrder: 2),
            row(id: "b", category: .backlog, name: "Backlog", sortOrder: 1, builtinKey: .backlog),
        ]
        let team = IssueStatusResolver.teamStatuses(rows)
        XCTAssertEqual(
            team.filter { $0.category == .started }.map(\.iconName),
            ["progress-1-4", "progress-2-4", "progress-3-4"]
        )
    }

    // MARK: - UNKNOWN CATEGORY (one degradation for sorting AND rendering)

    private func unknownCategoryRow(id: String, builtinKey: IssueStatus? = nil) -> IssueStatusEntity {
        IssueStatusEntity(
            id: id, teamId: "team-1", category: "triaging", name: "Weird",
            color: nil, sortOrder: 1, builtinKey: builtinKey?.rawValue,
            createdAt: "2026-01-01 00:00:00+00", updatedAt: "2026-01-01 00:00:00+00"
        )
    }

    // A category value this build doesn't know must not drop the row: it
    // degrades to `backlog` — dashed-circle glyph, backlog color, backlog
    // (= ACTIVE) in-group sort branch — for EVERY consumer at once.
    func testUnknownCategoryRendersTheBacklogTreatment() {
        let team = IssueStatusResolver.teamStatuses([unknownCategoryRow(id: "x")])
        XCTAssertEqual(team.first?.category, .backlog)
        XCTAssertEqual(team.first?.iconName, "circle-dashed")
        XCTAssertEqual(team.first?.anchor, .backlog)
    }

    // Even a row that carries a builtin key uses the SAME degradation as the
    // sort does — rendering must never disagree with `categoryRank`.
    func testUnknownCategoryIgnoresTheAnchorsCategory() {
        let team = IssueStatusResolver.teamStatuses([unknownCategoryRow(id: "x", builtinKey: .inProgress)])
        XCTAssertEqual(team.first?.category, .backlog)
        XCTAssertEqual(team.first?.iconName, "circle-dashed")
        // The builtin key survives (writes still reference it); only the
        // rendered/sorted category degrades.
        XCTAssertEqual(team.first?.builtinKey, .inProgress)
    }

    // …and it sorts LAST, after duplicate — the sort half of the same rule.
    func testUnknownCategorySortsLast() {
        let rows = seededTeam() + [unknownCategoryRow(id: "x", builtinKey: .inProgress)]
        XCTAssertEqual(IssueStatusResolver.teamStatuses(rows).last?.id, "x")
        // Not merged into the backlog group it renders like.
        XCTAssertEqual(
            IssueStatusResolver.teamStatuses(rows).map(\.id).firstIndex(of: "row-duplicate"),
            IssueStatusResolver.teamStatuses(rows).count - 2
        )
    }

    // The degraded category is what the in-group sorter switches on, so an
    // unknown category takes the ACTIVE branch (overdue → priority → due →
    // number), never the terminal recency branch.
    func testUnknownCategoryUsesTheActiveSortBranch() throws {
        let team = IssueStatusResolver.teamStatuses([unknownCategoryRow(id: "x")])
        let category = try XCTUnwrap(team.first).category
        let urgent = issue(id: "i-urgent", number: 1, priority: .urgent, updatedAt: "2026-01-01 00:00:00+00")
        let low = issue(id: "i-low", number: 2, priority: .low, updatedAt: "2026-09-09 00:00:00+00")
        // Active branch ⇒ priority wins. (The cancelled/duplicate branch would
        // have put the more recently updated `i-low` first.)
        XCTAssertEqual(
            IssueSorting.sorted([low, urgent], category: category, today: "2026-06-01").map(\.id),
            ["i-urgent", "i-low"]
        )
    }

    // MARK: - Resolution chain

    func testResolvePrefersTheRowIdMatch() {
        let team = IssueStatusResolver.teamStatuses(
            seededTeam() + [row(id: "custom-1", category: .started, name: "Coding", color: "#6366f1", sortOrder: 5)]
        )
        let resolved = IssueStatusResolver.resolve(statusId: "custom-1", anchor: "in_progress", team: team)
        XCTAssertEqual(resolved.id, "custom-1")
        XCTAssertEqual(resolved.rowId, "custom-1")
        XCTAssertEqual(resolved.name, "Coding")
        XCTAssertNil(resolved.builtinKey)
        XCTAssertEqual(resolved.colorHex, "#6366f1")
        // A custom started status still anchors to in_progress for writes.
        XCTAssertEqual(resolved.anchor, .inProgress)
    }

    func testResolveFallsBackToTheAnchorRowWhenStatusIdIsMissingOrStale() {
        let team = IssueStatusResolver.teamStatuses(seededTeam())
        XCTAssertEqual(
            IssueStatusResolver.resolve(statusId: nil, anchor: "in_review", team: team).id,
            "row-in_review"
        )
        // A deleted/unsynced row id degrades to the anchor, never to nothing.
        XCTAssertEqual(
            IssueStatusResolver.resolve(statusId: "gone", anchor: "done", team: team).id,
            "row-done"
        )
    }

    func testResolveFallsBackToTheConstructedDefaultWithNoTeamRows() {
        let resolved = IssueStatusResolver.resolve(statusId: "anything", anchor: "cancelled", team: [])
        XCTAssertEqual(resolved.id, "builtin:cancelled")
        XCTAssertNil(resolved.rowId)
        XCTAssertEqual(resolved.builtinKey, .cancelled)
        XCTAssertEqual(resolved.iconName, "circle-x")
    }

    // Unknown / missing anchor lands on backlog, exactly like IssueStatus.from.
    func testResolveWithUnknownAnchorLandsOnBacklog() {
        XCTAssertEqual(
            IssueStatusResolver.resolve(statusId: nil, anchor: "triaged", team: []).id,
            "builtin:backlog"
        )
        XCTAssertEqual(
            IssueStatusResolver.resolve(statusId: nil, anchor: nil, team: []).id,
            "builtin:backlog"
        )
    }

    // UNKNOWN ANCHOR (cross-platform rule): the normalization to backlog
    // happens BEFORE the team-row lookup, so a forward-compat status from a
    // newer server joins the team's REAL Backlog row/group — it must never
    // spawn a second, constructed backlog group next to it. Only with NO
    // synced rows at all does it degrade to `builtin:backlog` (above).
    func testResolveWithUnknownAnchorJoinsTheTeamsRealBacklogRow() {
        let team = IssueStatusResolver.teamStatuses(seededTeam())
        let resolved = IssueStatusResolver.resolve(
            issue(id: "i1", status: "triaged"), team: team
        )
        XCTAssertEqual(resolved.id, "row-backlog")
        XCTAssertEqual(resolved.rowId, "row-backlog")
        XCTAssertEqual(resolved.builtinKey, .backlog)
        // A stale/unknown statusId does not change the anchoring.
        XCTAssertEqual(
            IssueStatusResolver.resolve(
                issue(id: "i2", status: "triaged", statusId: "gone"), team: team
            ).id,
            "row-backlog"
        )
    }

    // MARK: - FILTER TOKENS (survive the fallback→synced re-key)

    // A `builtin:<key>` group key stored while the statuses shape was still
    // syncing must keep matching the SYNCED row it re-keys into.
    func testBuiltinFilterTokenMatchesTheSyncedRowItRekeysInto() {
        let team = IssueStatusResolver.teamStatuses(seededTeam())
        let backlog = team.first { $0.builtinKey == .backlog }!
        XCTAssertTrue(statusMatchesFilterToken(backlog, token: "builtin:backlog"))
        XCTAssertTrue(statusMatchesFilterToken(backlog, token: "row-backlog"))
        // …and only that row.
        let done = team.first { $0.builtinKey == .done }!
        XCTAssertFalse(statusMatchesFilterToken(done, token: "builtin:backlog"))
        XCTAssertFalse(statusMatchesFilterToken(done, token: "row-backlog"))
        // A CUSTOM row (no builtin key) never answers to a builtin token.
        let custom = IssueStatusResolver.teamStatuses(
            [row(id: "custom-1", category: .started, name: "Coding")]
        )[0]
        XCTAssertFalse(statusMatchesFilterToken(custom, token: "builtin:in_progress"))
        XCTAssertTrue(statusMatchesFilterToken(custom, token: "custom-1"))
        // A constructed fallback row still matches its own synthetic key.
        let fallbackBacklog = IssueStatusResolver.builtinDefault(for: .backlog)
        XCTAssertTrue(statusMatchesFilterToken(fallbackBacklog, token: "builtin:backlog"))
        XCTAssertFalse(statusMatchesFilterToken(fallbackBacklog, token: "row-backlog"))
    }

    func testStatusFilterMatchingSurvivesTheSnapshotLandingMidSession() {
        // Filter picked pre-sync, off the constructed fallback rows…
        var filters = IssueFilters()
        filters.toggleStatus(IssueStatusResolver.builtinDefault(for: .backlog))
        XCTAssertEqual(filters.statusIds, ["builtin:backlog"])

        // …then the shape lands and the issue resolves to the real row.
        let team = IssueStatusResolver.teamStatuses(seededTeam())
        let backlogIssue = IssueStatusResolver.resolve(issue(id: "i1", status: "backlog"), team: team)
        let doneIssue = IssueStatusResolver.resolve(issue(id: "i2", status: "done"), team: team)
        XCTAssertEqual(backlogIssue.id, "row-backlog")
        XCTAssertTrue(matchesFilters(status: backlogIssue, priority: .none, issueLabelIds: [], filters: filters))
        XCTAssertFalse(matchesFilters(status: doneIssue, priority: .none, issueLabelIds: [], filters: filters))

        // Un-checking the SYNCED row clears the stale token instead of adding
        // a second one for the same group.
        filters.toggleStatus(backlogIssue)
        XCTAssertTrue(filters.statusIds.isEmpty)
    }

    func testStatusFilterTogglesRealRowsByRowId() {
        let team = IssueStatusResolver.teamStatuses(seededTeam())
        let inReview = team.first { $0.builtinKey == .inReview }!
        var filters = IssueFilters()
        filters.toggleStatus(inReview)
        XCTAssertEqual(filters.statusIds, ["row-in_review"])
        XCTAssertTrue(filters.selectsStatus(inReview))
        XCTAssertFalse(filters.selectsStatus(team.first { $0.builtinKey == .todo }!))
        filters.toggleStatus(inReview)
        XCTAssertTrue(filters.statusIds.isEmpty)
    }

    // MARK: - Wire tolerance

    func testEntityDecodesElectricWireRow() throws {
        let json = """
        {"id":"s1","team_id":"t1","category":"started","name":"Coding",
         "color":"#6366f1","sort_order":"2.5","builtin_key":null,
         "created_at":"2026-01-01 00:00:00+00","updated_at":"2026-01-01 00:00:00+00",
         "some_future_column":"ignored"}
        """
        let entity = try JSONDecoder().decode(IssueStatusEntity.self, from: Data(json.utf8))
        XCTAssertEqual(entity.sortOrder, 2.5)
        XCTAssertNil(entity.builtinKey)
        XCTAssertEqual(entity.color, "#6366f1")
    }

    func testIssueRowDecodesWithAndWithoutStatusId() throws {
        let base = """
        {"id":"i1","board_id":"b1","number":"1","identifier":"EXP-1","title":"T",
         "status":"todo","priority":"none","sort_order":"1",
         "created_at":"2026-01-01 00:00:00+00","updated_at":"2026-01-01 00:00:00+00"
        """
        let withId = try JSONDecoder().decode(
            IssueEntity.self, from: Data((base + ",\"status_id\":\"s9\"}").utf8)
        )
        XCTAssertEqual(withId.statusId, "s9")
        let without = try JSONDecoder().decode(IssueEntity.self, from: Data((base + "}").utf8))
        XCTAssertNil(without.statusId)
    }
}
