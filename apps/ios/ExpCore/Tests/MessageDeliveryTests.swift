import Foundation
import XCTest
@testable import ExpCore

// #FEED-237: a person's own message, WhatsApp-style, locked ×4 against
// `packages/domain-contract/fixtures/message-delivery.json` (`stateCases`,
// `outboxCases`, `actionCases`, `handoffCases`, the copy, the marks and the testIds).
final class MessageDeliveryTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/message-delivery.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testCopyMarksAndTestIds() throws {
        let fixture = try fixture()
        let copy = try XCTUnwrap(fixture["copy"] as? [String: [String: String]])
        let marks = try XCTUnwrap(fixture["marks"] as? [String: [String: String]])
        XCTAssertEqual(Set(try XCTUnwrap(copy["status"]).keys), Set(MessageDelivery.State.allCases.map(\.rawValue)))
        for state in MessageDelivery.State.allCases {
            XCTAssertEqual(copy["status"]?[state.rawValue], MessageDelivery.Copy.status(state))
            let mark = MessageDelivery.mark(state)
            XCTAssertEqual(marks[state.rawValue], ["icon": mark.icon, "tone": mark.tone.rawValue], state.rawValue)
        }
        let actions: [MessageDelivery.Action] = [.retry, .delete, .cancel]
        XCTAssertEqual(copy["menu"], Dictionary(uniqueKeysWithValues: actions.map { ($0.rawValue, MessageDelivery.Copy.menu($0)) }))
        XCTAssertEqual(fixture["markSize"] as? Double, MessageDelivery.markSize)
        XCTAssertEqual(fixture["matchWindowMs"] as? Double, MessageDelivery.matchWindowMs)
        XCTAssertEqual(fixture["handoffMaxMs"] as? Double, MessageDelivery.handoffMaxMs)
        XCTAssertEqual(fixture["testIds"] as? [String: String], [
            "status": MessageDelivery.TestIds.status,
            "menu": MessageDelivery.TestIds.menu,
        ])
    }

    func testReplaysEveryStateCase() throws {
        let cases = try XCTUnwrap(try fixture()["stateCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let local = try (input["local"] as? String).map { raw in
                try XCTUnwrap(MessageDelivery.LocalState(rawValue: raw), name)
            }
            let result = MessageDelivery.deliveryState(MessageDelivery.StateInput(
                local: local,
                stored: try XCTUnwrap(input["stored"] as? Bool),
                deliveredAt: input["deliveredAt"] as? String,
                consumedAt: input["consumedAt"] as? String,
                echoed: try XCTUnwrap(input["echoed"] as? Bool)
            ))
            XCTAssertEqual(result.rawValue, testCase["expected"] as? String, name)
        }
    }

    func testReplaysEveryOutboxCase() throws {
        let cases = try XCTUnwrap(try fixture()["outboxCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let outbox = try XCTUnwrap(input["outbox"] as? [[String: Any]]).map { entry in
                MessageDelivery.OutboxEntry(
                    clientId: try XCTUnwrap(entry["clientId"] as? String),
                    text: try XCTUnwrap(entry["text"] as? String),
                    createdAt: try XCTUnwrap(entry["createdAt"] as? String),
                    state: try XCTUnwrap(MessageDelivery.LocalState(rawValue: try XCTUnwrap(entry["state"] as? String))),
                    messageId: entry["messageId"] as? String
                )
            }
            let rows = try XCTUnwrap(input["rows"] as? [[String: Any]]).map { row in
                MessageDelivery.OutboxRow(
                    id: try XCTUnwrap(row["id"] as? String),
                    authorKind: try XCTUnwrap(row["authorKind"] as? String),
                    authorId: row["authorId"] as? String,
                    body: try XCTUnwrap(row["body"] as? String),
                    createdAt: try XCTUnwrap(row["createdAt"] as? String)
                )
            }
            let expected = try XCTUnwrap(testCase["expected"] as? [String: [String]])
            let result = MessageDelivery.outboxMerge(
                viewerId: try XCTUnwrap(input["viewerId"] as? String), outbox: outbox, rows: rows
            )
            XCTAssertEqual(Set(expected.keys), ["drop", "keep"], name)
            XCTAssertEqual(result, MessageDelivery.Merge(drop: expected["drop"] ?? [], keep: expected["keep"] ?? []), name)
        }
    }

    func testReplaysEveryActionCase() throws {
        let cases = try XCTUnwrap(try fixture()["actionCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let state = try XCTUnwrap(MessageDelivery.State(rawValue: try XCTUnwrap(input["state"] as? String)), name)
            let result = MessageDelivery.bubbleActions(state: state, stored: try XCTUnwrap(input["stored"] as? Bool))
            XCTAssertEqual(result.map(\.rawValue), testCase["expected"] as? [String], name)
        }
    }

    func testReplaysEveryHandoffCase() throws {
        let cases = try XCTUnwrap(try fixture()["handoffCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let rows = try XCTUnwrap(input["rows"] as? [[String: Any]]).map { row in
                MessageDelivery.HandoffRow(
                    id: try XCTUnwrap(row["id"] as? String),
                    authorKind: try XCTUnwrap(row["authorKind"] as? String),
                    authorId: row["authorId"] as? String,
                    body: try XCTUnwrap(row["body"] as? String),
                    createdAt: try XCTUnwrap(row["createdAt"] as? String),
                    consumedAt: row["consumedAt"] as? String,
                    hasCard: row["card"].map { !($0 is NSNull) } ?? false
                )
            }
            let echoes = try XCTUnwrap(input["echoes"] as? [[String: Any]]).map { echo in
                MessageDelivery.HandoffEcho(
                    text: try XCTUnwrap(echo["text"] as? String),
                    at: (echo["at"] as? NSNumber)?.doubleValue
                )
            }
            let result = MessageDelivery.ownHandoff(
                viewerId: try XCTUnwrap(input["viewerId"] as? String),
                now: try XCTUnwrap(input["now"] as? String),
                feedEmpty: try XCTUnwrap(input["feedEmpty"] as? Bool),
                rows: rows,
                echoes: echoes
            )
            XCTAssertEqual(result, testCase["expected"] as? [String], name)
        }
    }
}
