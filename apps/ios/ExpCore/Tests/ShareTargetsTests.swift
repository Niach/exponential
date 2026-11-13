import Foundation
import XCTest
@testable import ExpCore

// EXP-1635: sharing into Exponential, locked ×3 (TS `share-targets.test.ts`,
// Android `ShareTargetsTest`) against `share-targets.json`.
final class ShareTargetsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/share-targets.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    private func agents(_ input: [String: Any]) throws -> [AgentLike] {
        try XCTUnwrap(input["agents"] as? [[String: Any]]).map { row in
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
    }

    func testConstantsMatchTheFixture() throws {
        let json = try fixture()
        let copy = try XCTUnwrap(json["copy"] as? [String: String])
        XCTAssertEqual(copy["title"], ShareTargets.Copy.title)
        XCTAssertEqual(copy["newIssue"], ShareTargets.Copy.newIssue)
        XCTAssertEqual(copy["newChat"], ShareTargets.Copy.newChat)
        XCTAssertEqual(copy["newChatSubtitle"], ShareTargets.Copy.newChatSubtitle)
        XCTAssertEqual(copy["agents"], ShareTargets.Copy.agents)
        XCTAssertEqual(copy["imagesOne"], ShareTargets.Copy.imagesOne)
        XCTAssertEqual(copy["imagesMany"], ShareTargets.Copy.imagesMany)
        let ids = try XCTUnwrap(json["ids"] as? [String: String])
        XCTAssertEqual(ids["newIssue"], ShareTargets.Ids.newIssue)
        XCTAssertEqual(ids["newChat"], ShareTargets.Ids.newChat)
        let icons = try XCTUnwrap(json["icons"] as? [String: String])
        XCTAssertEqual(icons["newIssue"], ShareTargets.Icons.newIssue)
        XCTAssertEqual(icons["newChat"], ShareTargets.Icons.newChat)
        let testIds = try XCTUnwrap(json["testIds"] as? [String: String])
        XCTAssertEqual(testIds["page"], ShareTargets.TestIds.page)
        XCTAssertEqual(testIds["preview"], ShareTargets.TestIds.preview)
        XCTAssertEqual(testIds["row"], ShareTargets.TestIds.row("{id}"))
        let direct = try XCTUnwrap(json["directShare"] as? [String: Any])
        XCTAssertEqual(direct["agentPrefix"] as? String, ShareTargets.DirectShare.agentPrefix)
        XCTAssertEqual(direct["chatPrefix"] as? String, ShareTargets.DirectShare.chatPrefix)
        XCTAssertEqual(direct["maxEntries"] as? Int, ShareTargets.DirectShare.maxEntries)
    }

    func testReplaysEveryTargetCase() throws {
        let cases = try XCTUnwrap(try fixture()["targetCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let expected = try XCTUnwrap(testCase["expected"] as? [String: Any])
            let result = ShareTargets.shareTargets(agents: try agents(input), me: input["me"] as? String)
            XCTAssertEqual(result.rows, expected["rows"] as? [String], name)
            XCTAssertEqual(result.agents.map(\.id), expected["agents"] as? [String], name)
        }
    }

    func testReplaysEveryTextCase() throws {
        let cases = try XCTUnwrap(try fixture()["textCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let text = ShareTargets.shareChatText(
                text: input["text"] as? String,
                subject: input["subject"] as? String,
                url: input["url"] as? String,
                hasImages: try XCTUnwrap(input["hasImages"] as? Bool)
            )
            XCTAssertEqual(text, testCase["expected"] as? String, name)
        }
    }

    func testReplaysEveryDirectShareCase() throws {
        let cases = try XCTUnwrap(try fixture()["directShareCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let chats = try XCTUnwrap(input["chats"] as? [[String: Any]]).map { row in
                ShareTargets.Chat(
                    id: try XCTUnwrap(row["id"] as? String),
                    title: try XCTUnwrap(row["title"] as? String),
                    updatedAt: try XCTUnwrap(row["updatedAt"] as? String),
                    agentId: row["agentId"] as? String,
                    sealed: row["sealed"] as? Bool ?? false
                )
            }
            let entries = ShareTargets.directShareEntries(
                agents: try agents(input), chats: chats, me: input["me"] as? String
            )
            let expected = try XCTUnwrap(testCase["expected"] as? [[String: String]])
            XCTAssertEqual(entries.map(\.id), expected.map { $0["id"] ?? "" }, name)
            XCTAssertEqual(entries.map(\.title), expected.map { $0["title"] ?? "" }, name)
        }
    }

    func testReplaysEveryParseCase() throws {
        let cases = try XCTUnwrap(try fixture()["parseCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let parsed = ShareTargets.parseDirectShareId(try XCTUnwrap(testCase["input"] as? String))
            guard let expected = testCase["expected"] as? [String: String] else {
                XCTAssertNil(parsed, name)
                continue
            }
            let id = try XCTUnwrap(expected["id"])
            XCTAssertEqual(parsed, expected["kind"] == "agent" ? .agent(id) : .chat(id), name)
        }
    }

    func testReplaysEveryIssueBoardCase() throws {
        let cases = try XCTUnwrap(try fixture()["issueBoardCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let boards = try XCTUnwrap(input["boards"] as? [[String: String]])
            let result = ShareTargets.shareIssueBoards(
                teamId: input["teamId"] as? String,
                boards: boards,
                lastBoardId: input["lastBoardId"] as? String,
                id: { $0["id"] ?? "" },
                team: { $0["teamId"] ?? "" }
            )
            let expected = try XCTUnwrap(testCase["expected"] as? [String: Any])
            XCTAssertEqual(result.boards.map { $0["id"] ?? "" }, try XCTUnwrap(expected["boards"] as? [String]), name)
            XCTAssertEqual(result.board?["id"], expected["board"] as? String, name)
        }
    }
}
