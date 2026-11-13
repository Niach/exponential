import Foundation
import XCTest
@testable import ExpCore

// #FEED-232 nav badges, locked ×4 against
// `packages/domain-contract/fixtures/nav-badges.json`: every case list
// (labelCases, chatsCases, rowCases, appIconCases) plus the max. Read through
// `#filePath`: the unit-test bundle carries no repo resources.
final class NavBadgesTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/nav-badges.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]], key)
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func value(_ object: Any?) throws -> NavBadges.Value {
        let dict = try XCTUnwrap(object as? [String: Any])
        return NavBadges.Value(
            count: try XCTUnwrap(dict["count"] as? Int),
            tone: try XCTUnwrap(NavBadges.Tone(rawValue: try XCTUnwrap(dict["tone"] as? String)))
        )
    }

    func testMax() throws {
        XCTAssertEqual(try fixture()["max"] as? Int, NavBadges.max)
    }

    func testLabelCases() throws {
        for c in try cases("labelCases") {
            let name = c["name"] as? String ?? ""
            XCTAssertEqual(NavBadges.countBadgeLabel(try XCTUnwrap(c["count"] as? Int)), c["expected"] as? String, name)
        }
    }

    func testChatsCases() throws {
        for c in try cases("chatsCases") {
            let name = c["name"] as? String ?? ""
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let groups = try ((input["groups"] as? [[String: Any]]) ?? []).map { group in
                ChatsListGroup(
                    // The fixture names the running band `working` (its title).
                    key: try XCTUnwrap(ChatsListGroup.Key(rawValue: (group["key"] as? String) == "working" ? "running" : (group["key"] as? String) ?? "")),
                    title: try XCTUnwrap(group["title"] as? String),
                    rows: try ((group["rows"] as? [[String: Any]]) ?? []).map { row in
                        ChatsListRow(
                            sessionId: try XCTUnwrap(row["sessionId"] as? String),
                            teamId: try XCTUnwrap(row["teamId"] as? String),
                            depth: try XCTUnwrap(row["depth"] as? Int),
                            parentId: row["parentId"] as? String,
                            rootId: row["rootId"] as? String,
                            title: try XCTUnwrap(row["title"] as? String),
                            busy: try XCTUnwrap(row["busy"] as? Bool),
                            needsYou: try XCTUnwrap(row["needsYou"] as? Bool),
                            unread: try XCTUnwrap(row["unread"] as? Bool),
                            activityAt: try XCTUnwrap(row["activityAt"] as? String),
                            deviceId: row["deviceId"] as? String
                        )
                    }
                )
            }
            let reads = try ((input["reads"] as? [[String: Any]]) ?? []).map {
                TreeSessionRead(
                    sessionId: try XCTUnwrap($0["sessionId"] as? String),
                    readAt: try XCTUnwrap($0["readAt"] as? String),
                    archivedAt: $0["archivedAt"] as? String
                )
            }
            XCTAssertEqual(NavBadges.chatsBadge(groups, reads: reads), try value(c["expected"]), name)
        }
    }

    func testRowCases() throws {
        for c in try cases("rowCases") {
            let name = c["name"] as? String ?? ""
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = NavBadges.rowBadge(
                unreadCount: try XCTUnwrap(input["unreadCount"] as? Int),
                needsYou: try XCTUnwrap(input["needsYou"] as? Bool)
            )
            XCTAssertEqual(got, try value(c["expected"]), name)
        }
    }

    func testAppIconCases() throws {
        for c in try cases("appIconCases") {
            let name = c["name"] as? String ?? ""
            let teams = try ((c["teams"] as? [[String: Any]]) ?? []).map { try XCTUnwrap($0["needsYou"] as? Int) }
            XCTAssertEqual(NavBadges.appIconBadge(teams), c["expected"] as? Int, name)
        }
    }
}
