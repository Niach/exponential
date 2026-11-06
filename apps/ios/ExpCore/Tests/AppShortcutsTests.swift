import Foundation
import XCTest
@testable import ExpCore

// #EXP-1638: the app icon's long-press menu, locked ×3 (TS
// `app-shortcuts.test.ts`, Android `AppShortcutsTest`) against
// `app-shortcuts.json`.
final class AppShortcutsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/app-shortcuts.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    func testConstantsMatchTheFixture() throws {
        let json = try fixture()
        let copy = try XCTUnwrap(json["copy"] as? [String: String])
        XCTAssertEqual(copy["newIssue"], AppShortcuts.Copy.newIssue)
        XCTAssertEqual(copy["newChat"], AppShortcuts.Copy.newChat)
        XCTAssertEqual(copy["search"], AppShortcuts.Copy.search)
        let icons = try XCTUnwrap(json["icons"] as? [String: String])
        XCTAssertEqual(icons["newIssue"], AppShortcuts.Icons.newIssue)
        XCTAssertEqual(icons["newChat"], AppShortcuts.Icons.newChat)
        XCTAssertEqual(icons["search"], AppShortcuts.Icons.search)
        XCTAssertEqual(icons["chat"], AppShortcuts.Icons.chat)
        XCTAssertEqual(json["statics"] as? [String], AppShortcuts.statics)
        XCTAssertEqual(json["recentChatPrefix"] as? String, AppShortcuts.recentChatPrefix)
        XCTAssertEqual(json["maxEntries"] as? Int, AppShortcuts.maxEntries)
    }

    func testReplaysEveryListCase() throws {
        let cases = try XCTUnwrap(try fixture()["listCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let chats = try XCTUnwrap(input["chats"] as? [[String: Any]]).map { row in
                AppShortcuts.Chat(
                    id: try XCTUnwrap(row["id"] as? String),
                    title: try XCTUnwrap(row["title"] as? String),
                    updatedAt: try XCTUnwrap(row["updatedAt"] as? String),
                    agentId: row["agentId"] as? String,
                    sealed: row["sealed"] as? Bool ?? false
                )
            }
            let expected = try XCTUnwrap(testCase["expected"] as? [[String: String]]).map {
                AppShortcuts.Entry(id: $0["id"] ?? "", title: $0["title"] ?? "", icon: $0["icon"] ?? "")
            }
            XCTAssertEqual(AppShortcuts.appShortcuts(chats: chats), expected, name)
        }
    }

    func testReplaysEveryParseCase() throws {
        let cases = try XCTUnwrap(try fixture()["parseCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let parsed = AppShortcuts.parseAppShortcut(try XCTUnwrap(testCase["input"] as? String))
            guard let expected = testCase["expected"] as? [String: String] else {
                XCTAssertNil(parsed, name)
                continue
            }
            let target: AppShortcuts.Target? = switch expected["kind"] {
            case "new-issue": .newIssue
            case "new-chat": .newChat
            case "search": .search
            case "chat": .chat(try XCTUnwrap(expected["id"]))
            default: nil
            }
            XCTAssertEqual(parsed, try XCTUnwrap(target, name), name)
        }
    }
}
