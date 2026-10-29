import Foundation
import XCTest
@testable import ExpCore

// S-3: the New-chat composer's agent row, locked ×4 (TS
// `composer-agent-row.test.ts`, desktop `domain::composer_agent_row`,
// Android `ComposerAgentRowTest`) against `composer-home.json` `agentRow`.
final class ComposerAgentRowTests: XCTestCase {
    private func block() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/composer-home.json")
        let json = try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
        return try XCTUnwrap(json["agentRow"] as? [String: Any])
    }

    func testReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try block()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let agents = try XCTUnwrap(testCase["agents"] as? [[String: Any]]).map { row in
                AgentLike(
                    id: try XCTUnwrap(row["id"] as? String),
                    teamId: "t",
                    name: try XCTUnwrap(row["name"] as? String),
                    icon: nil,
                    color: nil,
                    sortOrder: try XCTUnwrap((row["sortOrder"] as? NSNumber)?.doubleValue),
                    role: row["role"] as? String,
                    ownerId: row["ownerId"] as? String
                )
            }
            let state = ComposerAgentRow.composerAgentRow(
                agents: agents, me: testCase["me"] as? String, picked: testCase["picked"] as? String
            )
            let expected = try XCTUnwrap(testCase["expected"] as? [String: Any])
            XCTAssertEqual(state.options, expected["options"] as? [String], name)
            XCTAssertEqual(state.selected, expected["selected"] as? String, name)
            XCTAssertEqual(state.label, expected["label"] as? String, name)
            XCTAssertEqual(state.agents.map(\.id) + [ComposerAgentRow.temporaryPick], state.options, name)
            XCTAssertEqual(state.agent?.id, state.selected == ComposerAgentRow.temporaryPick ? nil : state.selected, name)
            XCTAssertEqual(state.isTemporary, state.selected == ComposerAgentRow.temporaryPick, name)
        }
    }

    func testReplaysEverySeedCase() throws {
        let cases = try XCTUnwrap(try block()["seedCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let seed = try XCTUnwrap(testCase["seed"] as? [String: Any])
            let pick = ComposerAgentRow.seedPick(
                issueIds: seed["issueIds"] as? [String] ?? [],
                promptId: seed["promptId"] as? String,
                deviceId: seed["deviceId"] as? String
            )
            XCTAssertEqual(pick, testCase["expected"] as? String, name)
        }
    }

    func testReplaysEverySubjectCase() throws {
        let cases = try XCTUnwrap(try block()["subjectCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let subject: AgentComposerPrompt.Subject?
            switch testCase["subject"] as? String {
            case "issues": subject = .issues(count: 1)
            case "prompt": subject = .prompt
            case nil: subject = nil
            case let other?: return XCTFail("\(name): unknown subject \(other)")
            }
            let pick = ComposerAgentRow.pickForSubject(subject, current: testCase["current"] as? String)
            XCTAssertEqual(pick, testCase["expected"] as? String, name)
        }
    }

    func testConstantsCopyAndTestIdsMatchTheFixture() throws {
        let block = try block()
        XCTAssertEqual(block["temporaryId"] as? String, ComposerAgentRow.temporaryPick)
        XCTAssertEqual(block["temporaryIcon"] as? String, ComposerAgentRow.temporaryIcon)
        XCTAssertEqual((block["size"] as? NSNumber)?.doubleValue, ComposerAgentRow.size)
        XCTAssertEqual((block["ringWidth"] as? NSNumber)?.doubleValue, ComposerAgentRow.ringWidth)
        XCTAssertEqual((block["ringGap"] as? NSNumber)?.doubleValue, ComposerAgentRow.ringGap)
        XCTAssertEqual((block["gap"] as? NSNumber)?.doubleValue, ComposerAgentRow.gap)
        XCTAssertEqual((block["labelToRow"] as? NSNumber)?.doubleValue, ComposerAgentRow.labelToRow)
        XCTAssertEqual((block["rowToCard"] as? NSNumber)?.doubleValue, ComposerAgentRow.rowToCard)
        XCTAssertEqual(block["copy"] as? [String: String], [
            "to": ComposerAgentRow.Copy.to, "temporary": ComposerAgentRow.Copy.temporary,
        ])
        let ids = try XCTUnwrap(block["testIds"] as? [String: String])
        XCTAssertEqual(ids["row"], ComposerAgentRow.TestIds.row)
        XCTAssertEqual(ids["label"], ComposerAgentRow.TestIds.label)
        XCTAssertEqual(ids["option"]?.replacingOccurrences(of: "{id}", with: "x"), ComposerAgentRow.TestIds.option("x"))
    }
}
