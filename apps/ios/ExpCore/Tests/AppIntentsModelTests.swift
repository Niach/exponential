import Foundation
import XCTest
@testable import ExpCore

// #EXP-1640: the App Intents model, replayed off `app-intents.json` (TS
// `app-intents.test.ts`, Android `AppIntentsTest`, same case names).
final class AppIntentsModelTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/app-intents.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let all = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(all.isEmpty, key)
        return all
    }

    func testCopyAndPhrasesAreTheFixtures() throws {
        let f = try fixture()
        let copy = try XCTUnwrap(f["copy"] as? [String: String])
        typealias C = AppIntentsModel.Copy
        XCTAssertEqual(copy, [
            "created": C.created, "askTitle": C.askTitle, "noBoard": C.noBoard, "noAgent": C.noAgent,
            "sent": C.sent, "signedOut": C.signedOut, "createIssueTitle": C.createIssueTitle,
            "askAgentTitle": C.askAgentTitle,
        ])
        let phrases = try XCTUnwrap(f["phrases"] as? [String: [String]])
        XCTAssertEqual(phrases["createIssue"], AppIntentsModel.Phrases.createIssue)
        XCTAssertEqual(phrases["askAgent"], AppIntentsModel.Phrases.askAgent)
        // Every iOS phrase names the app.
        for phrase in AppIntentsModel.Phrases.createIssue + AppIntentsModel.Phrases.askAgent {
            XCTAssertTrue(phrase.contains("${applicationName}"), phrase)
        }
        XCTAssertEqual(f["titleMax"] as? Int, AppIntentsModel.titleMax)
        XCTAssertEqual(C.created(identifier: "APP-7"), "Created APP-7")
        XCTAssertEqual(C.noAgent(name: "Designer"), "No agent called Designer")
        XCTAssertEqual(C.sent(agent: "Assistant"), "Sent to Assistant")
    }

    func testTitleCases() throws {
        for c in try cases("titleCases") {
            XCTAssertEqual(AppIntentsModel.issueTitle(c["input"] as! String), c["expected"] as? String, c["name"] as? String ?? "")
        }
    }

    func testBoardCases() throws {
        for c in try cases("boardCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let boards = try XCTUnwrap(input["boards"] as? [[String: Any]]).map {
                AppIntentsModel.Board(id: $0["id"] as! String, sortOrder: ($0["sortOrder"] as! NSNumber).doubleValue)
            }
            let got = AppIntentsModel.board(
                boards: boards, lastBoardId: input["lastBoardId"] as? String, pickedId: input["pickedId"] as? String
            )
            XCTAssertEqual(got, c["expected"] as? String, c["name"] as? String ?? "")
        }
    }

    func testAgentCases() throws {
        for c in try cases("agentCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let agents = try XCTUnwrap(input["agents"] as? [[String: Any]]).map {
                AppIntentsModel.Agent(
                    id: $0["id"] as! String, name: $0["name"] as! String,
                    sortOrder: ($0["sortOrder"] as! NSNumber).doubleValue,
                    role: $0["role"] as? String, ownerId: $0["ownerId"] as? String
                )
            }
            let got = AppIntentsModel.matchAgent(agents: agents, me: input["me"] as? String, spoken: input["spoken"] as! String)
            XCTAssertEqual(got, c["expected"] as? String, c["name"] as? String ?? "")
        }
    }
}
