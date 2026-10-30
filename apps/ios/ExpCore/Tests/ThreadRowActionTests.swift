import Foundation
import XCTest
@testable import ExpCore

// #EXP-1464: what a conversation's session rows do, locked ×4 against
// `packages/domain-contract/fixtures/thread-row-action.json`; every case
// compares the WHOLE expected object.
final class ThreadRowActionTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/thread-row-action.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func string(_ value: Any?) -> String? {
        value as? String
    }

    func testCopyAndTestIds() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["copy"] as? [String: String], [
            "openChat": ThreadRowAction.Copy.openChat,
            "answered": ThreadRowAction.Copy.answered,
        ])
        XCTAssertEqual(fixture["testIds"] as? [String: String], [
            "openChat": ThreadRowAction.TestIds.openChat,
            "expanded": ThreadRowAction.TestIds.expanded,
            "event": ThreadRowAction.TestIds.event,
        ])
    }

    func testReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let expected = try XCTUnwrap(testCase["expected"] as? [String: Any])
            let state = try string(input["state"]).map { raw in
                try XCTUnwrap(SessionState(rawValue: raw), "\(name): unknown state \(raw)")
            }
            let result = ThreadRowAction.threadRowAction(ThreadRowAction.Input(
                type: try XCTUnwrap(string(input["type"])),
                sessionId: string(input["sessionId"]),
                rootId: try XCTUnwrap(string(input["rootId"])),
                state: state,
                blocked: try XCTUnwrap(input["blocked"] as? Bool),
                inline: try XCTUnwrap(input["inline"] as? Bool),
                answered: try XCTUnwrap(input["answered"] as? Bool)
            ))
            let want = ThreadRowAction.Result(
                action: try XCTUnwrap(ThreadRowAction.Action(rawValue: try XCTUnwrap(string(expected["action"])))),
                sessionId: string(expected["sessionId"]),
                chevron: try XCTUnwrap(expected["chevron"] as? Bool),
                kindWord: string(expected["kindWord"]),
                tone: string(expected["tone"])
            )
            XCTAssertEqual(Set(expected.keys), ["action", "sessionId", "chevron", "kindWord", "tone"], name)
            XCTAssertEqual(result, want, name)
        }
    }

    func testReplaysEveryWaitingCase() throws {
        let cases = try XCTUnwrap(try fixture()["waitingCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let messages = try XCTUnwrap(testCase["messages"] as? [[String: Any]]).map { row in
                TreeMessage(
                    id: try XCTUnwrap(string(row["id"])),
                    sessionId: try XCTUnwrap(string(row["sessionId"])),
                    authorKind: "agent",
                    authorId: nil,
                    body: "",
                    card: (row["card"] as? [String: Any]).map(TreeMessageCard.init(object:)),
                    answeredAt: string(row["answeredAt"]),
                    consumedAt: nil,
                    createdAt: try XCTUnwrap(string(row["createdAt"]))
                )
            }
            XCTAssertEqual(
                ThreadRowAction.sessionWaitingCard(
                    sessionId: try XCTUnwrap(string(testCase["sessionId"])),
                    messages: messages
                ),
                string(testCase["expected"]),
                name
            )
        }
    }

    func testWaitingCardInline() {
        let question = ThreadRow.question(ThreadQuestionRow(
            id: "q", sessionId: "s1", question: "Ship?", options: [], custom: true, multi: false,
            surface: nil, answer: nil, answers: nil, surfaceData: nil, answered: false
        ))
        XCTAssertTrue(ThreadRowAction.waitingCardInline([question], sessionId: "s1"))
        XCTAssertFalse(ThreadRowAction.waitingCardInline([question], sessionId: "s2"))
        let answered = ThreadRow.question(ThreadQuestionRow(
            id: "q", sessionId: "s1", question: "Ship?", options: [], custom: true, multi: false,
            surface: nil, answer: "Yes", answers: nil, surfaceData: nil, answered: true
        ))
        XCTAssertFalse(ThreadRowAction.waitingCardInline([answered], sessionId: "s1"))
    }
}
