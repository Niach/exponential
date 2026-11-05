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
        XCTAssertEqual(f["private"] as? [String: String], ["plan": NeedsYou.Private.plan, "question": NeedsYou.Private.question])
        XCTAssertEqual(f["testIds"] as? [String: String], [
            "entry": NeedsYou.TestIds.entry, "row": NeedsYou.TestIds.row, "list": NeedsYou.TestIds.list,
        ])
    }

    func testCases() throws {
        let f = try fixture()
        let cases = try XCTUnwrap(f["cases"] as? [[String: Any]])
        let prCases = try XCTUnwrap(f["prCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        XCTAssertFalse(prCases.isEmpty)
        for entry in cases + prCases {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let sessions = try XCTUnwrap(input["sessions"] as? [[String: Any]], name).map { raw in
                NeedsYou.SessionInput(
                    id: raw["id"] as? String ?? "",
                    rootId: raw["rootId"] as? String ?? "",
                    name: raw["name"] as? String ?? "",
                    status: raw["status"] as? String ?? "",
                    agentId: raw["agentId"] as? String,
                    userId: raw["userId"] as? String,
                    busy: raw["busy"] as? Bool ?? false
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
            let pullRequests = (input["pullRequests"] as? [[String: Any]] ?? []).map { raw in
                NeedsYou.PullRequestInput(
                    id: raw["id"] as? String ?? "",
                    rootSessionId: raw["rootSessionId"] as? String,
                    number: raw["number"] as? Int ?? -1,
                    title: raw["title"] as? String ?? "",
                    state: raw["state"] as? String ?? "",
                    checksState: raw["checksState"] as? String,
                    basePullRequestId: raw["basePullRequestId"] as? String,
                    createdAt: raw["createdAt"] as? String ?? ""
                )
            }
            let actual = NeedsYou.rows(
                now: try XCTUnwrap((input["now"] as? NSNumber)?.doubleValue, name),
                me: input["me"] as? String,
                sessions: sessions,
                messages: messages,
                pullRequests: pullRequests
            ).map(json)
            let expected = try XCTUnwrap(entry["expected"] as? [Any], name)
            XCTAssertTrue(
                NSArray(array: actual).isEqual(to: expected),
                "\(name)\nactual: \(actual)\nexpected: \(expected)"
            )
        }
    }

    /// A digest's `ask` card keeps its origin's ids through the decode (chats.ts
    /// `digestMessage`), so the fold can name the origin's card.
    func testADigestKeepsAnAsksOriginIds() throws {
        let raw = """
        {"first": null, "preview": null, "waiting": {"id": "mirror", "authorKind": "agent",
         "authorSessionId": "c1", "body": "Asks: Ship it?",
         "card": {"kind": "ask", "sessionId": "c1", "messageId": "q2"},
         "answeredAt": null, "createdAt": "2027-01-15T07:55:00.000Z"},
         "agentAt": null, "agentTimes": []}
        """
        let kept = Chats.digestMessages(sessionId: "r1", digest: raw, opener: nil)
        let mirror = try XCTUnwrap(kept.first)
        XCTAssertEqual(kept.count, 1)
        XCTAssertEqual(mirror.card?.sessionId, "c1")
        XCTAssertEqual(mirror.card?.messageId, "q2")
        let session = { (id: String) in
            NeedsYou.SessionInput(id: id, rootId: "r1", name: id, status: "idle", agentId: nil, userId: "me")
        }
        let rows = NeedsYou.rows(
            now: 1_800_000_000_000,
            me: "me",
            sessions: [session("r1"), session("c1")],
            messages: [
                NeedsYou.MessageInput(
                    id: mirror.id, sessionId: mirror.sessionId, authorSessionId: mirror.authorSessionId,
                    body: mirror.body, createdAt: mirror.createdAt, answeredAt: mirror.answeredAt, card: mirror.card
                ),
            ]
        )
        XCTAssertEqual(rows.map(\.messageId), ["q2"])
        XCTAssertEqual(rows.map(\.sessionId), ["c1"])
    }
}
