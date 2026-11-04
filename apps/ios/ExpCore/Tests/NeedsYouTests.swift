import Foundation
import XCTest
@testable import ExpCore

// #FEED-191 Needs you, locked ×4 against
// `packages/domain-contract/fixtures/needs-you.json`: every case, the WHOLE
// expected array. Read through `#filePath`: the unit-test bundle carries no
// repo resources.
final class NeedsYouTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/needs-you.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func json(_ row: NeedsYou.Row) -> [String: Any] {
        [
            "messageId": row.messageId,
            "sessionId": row.sessionId,
            "rootId": row.rootId,
            "kind": row.kind.rawValue,
            "opens": row.opens.rawValue,
            "title": row.title,
            "chat": row.chat,
            "meta": row.meta,
            "age": row.age,
            "createdAt": row.createdAt,
        ]
    }

    func testConstantsMatchTheFixture() throws {
        let f = try fixture()
        XCTAssertEqual(f["route"] as? String, NeedsYou.route)
        XCTAssertEqual(f["copy"] as? [String: String], ["entry": NeedsYou.Copy.entry, "empty": NeedsYou.Copy.empty])
        XCTAssertEqual(f["prefix"] as? [String: String], ["question": NeedsYou.Prefix.question])
        XCTAssertEqual(f["testIds"] as? [String: String], [
            "entry": NeedsYou.TestIds.entry, "row": NeedsYou.TestIds.row, "list": NeedsYou.TestIds.list,
        ])
    }

    func testCases() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let sessions = try XCTUnwrap(input["sessions"] as? [[String: Any]], name).map { raw in
                NeedsYou.SessionInput(
                    id: raw["id"] as? String ?? "",
                    rootId: raw["rootId"] as? String ?? "",
                    name: raw["name"] as? String ?? "",
                    status: raw["status"] as? String ?? "",
                    agentId: raw["agentId"] as? String,
                    userId: raw["userId"] as? String
                )
            }
            let messages = try XCTUnwrap(input["messages"] as? [[String: Any]], name).map { raw in
                NeedsYou.MessageInput(
                    id: raw["id"] as? String ?? "",
                    sessionId: raw["sessionId"] as? String ?? "",
                    authorSessionId: raw["authorSessionId"] as? String,
                    body: raw["body"] as? String ?? "",
                    createdAt: raw["createdAt"] as? String ?? "",
                    answeredAt: raw["answeredAt"] as? String,
                    card: (raw["card"] as? [String: Any]).map(TreeMessageCard.init(object:))
                )
            }
            let actual = NeedsYou.rows(
                now: try XCTUnwrap((input["now"] as? NSNumber)?.doubleValue, name),
                me: input["me"] as? String,
                sessions: sessions,
                messages: messages
            ).map(json)
            let expected = try XCTUnwrap(entry["expected"] as? [Any], name)
            XCTAssertTrue(
                NSArray(array: actual).isEqual(to: expected),
                "\(name)\nactual: \(actual)\nexpected: \(expected)"
            )
        }
    }
}
