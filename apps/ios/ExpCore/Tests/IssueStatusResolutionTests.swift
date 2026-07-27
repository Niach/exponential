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

    func testCategoryValuesAndOrdersMatchGeneratedContract() {
        XCTAssertEqual(
            IssueStatusCategory.allCases.map(\.rawValue),
            DomainContract.issueStatusCategoryValues
        )
        XCTAssertEqual(
            IssueStatusCategory.displayOrder.map(\.rawValue),
            DomainContract.issueStatusCategoryDisplayOrder
        )
        XCTAssertEqual(
            IssueStatusCategory.settingsOrder.map(\.rawValue),
            DomainContract.issueStatusCategorySettingsOrder
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
            ["in_progress", "in_review", "todo", "backlog", "done", "cancelled", "duplicate"]
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
            ["c", "b", "d", "e", "z", "a"]
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

    // A category value this build doesn't know must not drop the row; it
    // renders through the builtin anchor's category instead.
    func testUnknownCategoryFallsBackToTheAnchorsCategory() {
        let rows = [IssueStatusEntity(
            id: "x", teamId: "team-1", category: "triaging", name: "Weird",
            color: nil, sortOrder: 1, builtinKey: IssueStatus.inProgress.rawValue,
            createdAt: "2026-01-01 00:00:00+00", updatedAt: "2026-01-01 00:00:00+00"
        )]
        let team = IssueStatusResolver.teamStatuses(rows)
        XCTAssertEqual(team.first?.category, .started)
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
