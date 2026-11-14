import Foundation
import XCTest
@testable import ExpCore

// #EXP-1830: the home screen widgets, locked ×3 (TS `home-widgets.test.ts`,
// Android `HomeWidgetsTest`) against `home-widgets.json`, the WHOLE object.
final class HomeWidgetsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/home-widgets.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let cases = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        return cases
    }

    func testConstantsMatchTheFixture() throws {
        let json = try fixture()
        XCTAssertEqual(json["kinds"] as? [String], HomeWidgets.Kind.allCases.map(\.rawValue))
        let copy = try XCTUnwrap(json["copy"] as? [String: String])
        typealias C = HomeWidgets.Copy
        XCTAssertEqual(copy, [
            "myIssues": C.myIssues, "needsYou": C.needsYou, "emptyIssues": C.emptyIssues,
            "emptyNeedsYou": C.emptyNeedsYou, "signedOut": C.signedOut, "sealedRow": C.sealedRow,
            "pickBoard": C.pickBoard, "pickTeams": C.pickTeams, "boardGallery": C.boardGallery,
            "boardGalleryDescription": C.boardGalleryDescription,
            "myIssuesGalleryDescription": C.myIssuesGalleryDescription,
            "needsYouGalleryDescription": C.needsYouGalleryDescription,
        ])
        typealias I = HomeWidgets.Icons
        XCTAssertEqual(json["icons"] as? [String: String], [
            "myIssues": I.myIssues, "needsYou": I.needsYou, "plan": I.plan, "question": I.question, "pr": I.pr,
        ])
        XCTAssertEqual(json["openCategories"] as? [String], HomeWidgets.openCategories)
        XCTAssertEqual(json["categoryOrder"] as? [String], HomeWidgets.categoryOrder)
        XCTAssertEqual(json["capacity"] as? [String: Int], [
            "small": HomeWidgets.Capacity.small, "medium": HomeWidgets.Capacity.medium,
            "large": HomeWidgets.Capacity.large,
        ])
        XCTAssertEqual(json["look"] as? [String: Double], [
            "padding": HomeWidgets.Look.padding, "headerGap": HomeWidgets.Look.headerGap,
            "rowHeight": HomeWidgets.Look.rowHeight, "glyph": HomeWidgets.Look.glyph,
        ])
        XCTAssertEqual(HomeWidgets.priorityOrder, IssuePriority.displayOrder.map(\.rawValue))
    }

    func testAgeMatchesResultsTab() {
        let now = 1_000_000_000.0
        for minutes in [0.0, 0.5, 1, 59, 60, 61, 1_439, 1_440, 10_000] {
            let created = now - minutes * 60_000
            XCTAssertEqual(HomeWidgets.age(created, now: now), ResultsTab.age(created, now: now), "\(minutes)")
        }
    }

    private func statuses(_ raw: Any?) throws -> [HomeWidgets.Status] {
        try XCTUnwrap(raw as? [[String: Any]]).map {
            HomeWidgets.Status(
                id: try XCTUnwrap($0["id"] as? String),
                category: try XCTUnwrap($0["category"] as? String),
                color: try XCTUnwrap($0["color"] as? String),
                sortOrder: try XCTUnwrap($0["sortOrder"] as? Double)
            )
        }
    }

    private func issues(_ raw: Any?) throws -> [HomeWidgets.Issue] {
        try XCTUnwrap(raw as? [[String: Any]]).map {
            HomeWidgets.Issue(
                id: try XCTUnwrap($0["id"] as? String),
                identifier: try XCTUnwrap($0["identifier"] as? String),
                title: try XCTUnwrap($0["title"] as? String),
                statusId: try XCTUnwrap($0["statusId"] as? String),
                boardId: try XCTUnwrap($0["boardId"] as? String),
                teamId: try XCTUnwrap($0["teamId"] as? String),
                priority: try XCTUnwrap($0["priority"] as? String),
                sortOrder: try XCTUnwrap($0["sortOrder"] as? Double),
                assigneeId: $0["assigneeId"] as? String,
                updatedAt: try XCTUnwrap($0["updatedAt"] as? String)
            )
        }
    }

    private func issueList(_ raw: Any?) throws -> HomeWidgets.List<HomeWidgets.IssueRow> {
        let expected = try XCTUnwrap(raw as? [String: Any])
        let rows = try XCTUnwrap(expected["rows"] as? [[String: String]]).map {
            HomeWidgets.IssueRow(
                id: $0["id"] ?? "", identifier: $0["identifier"] ?? "", title: $0["title"] ?? "",
                statusCategory: $0["statusCategory"] ?? "", statusColor: $0["statusColor"] ?? ""
            )
        }
        return HomeWidgets.List(count: try XCTUnwrap(expected["count"] as? Int), rows: rows)
    }

    func testReplaysEveryBoardCase() throws {
        for testCase in try cases("boardCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let list = HomeWidgets.boardWidget(
                boardId: try XCTUnwrap(input["boardId"] as? String),
                statuses: try statuses(input["statuses"]),
                issues: try issues(input["issues"]),
                max: try XCTUnwrap(input["max"] as? Int)
            )
            XCTAssertEqual(list, try issueList(testCase["expected"]), name)
        }
    }

    func testReplaysEveryMyIssuesCase() throws {
        for testCase in try cases("myIssuesCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let list = HomeWidgets.myIssuesWidget(
                me: try XCTUnwrap(input["me"] as? String),
                teamIds: try XCTUnwrap(input["teamIds"] as? [String]),
                statuses: try statuses(input["statuses"]),
                issues: try issues(input["issues"]),
                max: try XCTUnwrap(input["max"] as? Int)
            )
            XCTAssertEqual(list, try issueList(testCase["expected"]), name)
        }
    }

    func testReplaysEveryNeedsYouCase() throws {
        for testCase in try cases("needsYouCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let rows = try XCTUnwrap(input["rows"] as? [[String: String]]).map {
                HomeWidgets.NeedsYouInput(
                    messageId: $0["messageId"] ?? "", sessionId: $0["sessionId"] ?? "", rootId: $0["rootId"] ?? "",
                    kind: $0["kind"] ?? "", opens: $0["opens"] ?? "", title: $0["title"] ?? "", age: $0["age"] ?? ""
                )
            }
            let list = HomeWidgets.needsYouWidget(
                rows: rows,
                sealedRootIds: try XCTUnwrap(input["sealedRootIds"] as? [String]),
                max: try XCTUnwrap(input["max"] as? Int)
            )
            let expected = try XCTUnwrap(testCase["expected"] as? [String: Any])
            let expectedRows = try XCTUnwrap(expected["rows"] as? [[String: String]]).map {
                HomeWidgets.NeedsYouRow(
                    messageId: $0["messageId"] ?? "", sessionId: $0["sessionId"] ?? "", rootId: $0["rootId"] ?? "",
                    opens: $0["opens"] ?? "", icon: $0["icon"] ?? "", title: $0["title"] ?? "", age: $0["age"] ?? ""
                )
            }
            XCTAssertEqual(
                list, HomeWidgets.List(count: try XCTUnwrap(expected["count"] as? Int), rows: expectedRows), name
            )
        }
    }

    /// PRIVACY: the app seals before it writes the app group snapshot.
    func testSealedItemsNeverCarryCardText() {
        func item(_ rootId: String, _ title: String) -> HomeWidgets.Snapshot.NeedsYouItem {
            HomeWidgets.Snapshot.NeedsYouItem(
                row: HomeWidgets.NeedsYouInput(
                    messageId: "m-\(rootId)", sessionId: rootId, rootId: rootId, kind: "plan", opens: "plan",
                    title: title, age: ""
                ),
                createdAtMs: 1
            )
        }
        let sealed = HomeWidgets.sealed(
            [item("open", "Ship the widgets"), item("secret", "Rotate the keys")], sealedRootIds: ["secret"]
        )
        XCTAssertEqual(sealed.map(\.row.title), ["Ship the widgets", HomeWidgets.Copy.sealedRow])
        XCTAssertEqual(sealed.map(\.row.messageId), ["m-open", "m-secret"])
    }
}
