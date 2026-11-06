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

    /// #EXP-1710 (merge-train.json NEEDS YOU): `auto` mode drops the ready
    /// rows, `approval` keeps them; a stuck PR is ONE pr row with its
    /// actions in either mode; a person's Hold is none.
    func testMergeTrainRows() throws {
        let now = try XCTUnwrap(WireTimestamps.parse("2027-01-15T10:00:00.000Z")).timeIntervalSince1970 * 1000
        let sessions = [
            NeedsYou.SessionInput(id: "r1", rootId: "r1", name: "Palette", status: "idle", agentId: nil, userId: "me"),
        ]
        func pr(_ id: String, _ number: Int, mergeState: String?, hold: String? = nil, offline: String? = nil) -> NeedsYou.PullRequestInput {
            NeedsYou.PullRequestInput(
                id: id, rootSessionId: "r1", number: number, title: "PR \(number)", state: "open",
                checksState: "success", basePullRequestId: nil, createdAt: "2027-01-15T07:00:00.000Z",
                mergeState: mergeState, mergeWait: mergeState == "checks" ? "approval" : nil, mergeHold: hold,
                deviceOfflineSince: offline
            )
        }
        let prs = [
            pr("ready", 10, mergeState: "checks"),
            pr("stuck", 12, mergeState: "held", hold: "bounces"),
            pr("person", 13, mergeState: "held", hold: "person"),
            pr("offline", 14, mergeState: "conflict", offline: "2027-01-15T07:30:00.000Z"),
            pr("queued", 15, mergeState: "queued"),
        ]
        let auto = NeedsYou.rows(
            now: now, me: "me", sessions: sessions, messages: [], pullRequests: prs, mergeMode: "auto", owner: true
        )
        XCTAssertEqual(auto.map(\.messageId).sorted(), ["offline", "stuck"])
        let stuck = try XCTUnwrap(auto.first { $0.messageId == "stuck" })
        XCTAssertEqual(stuck.title, "Auto-merge stuck on #12")
        XCTAssertEqual(stuck.meta, "3 bounces")
        XCTAssertEqual(stuck.kind, .pr)
        XCTAssertEqual(stuck.opens, .results)
        XCTAssertEqual(stuck.actions, [.retry, .mergeNow, .closePr])
        XCTAssertEqual(auto.first { $0.messageId == "offline" }?.meta, "Device offline")
        let approval = NeedsYou.rows(
            now: now, me: "me", sessions: sessions, messages: [], pullRequests: prs, mergeMode: "approval", owner: false
        )
        XCTAssertEqual(approval.map(\.messageId).sorted(), ["offline", "ready", "stuck"])
        XCTAssertEqual(approval.first { $0.messageId == "stuck" }?.actions, [.retry, .closePr])
        XCTAssertEqual(approval.first { $0.messageId == "ready" }?.actions, [])
    }

}
