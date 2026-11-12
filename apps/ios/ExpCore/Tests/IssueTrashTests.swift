import Foundation
import XCTest

@testable import ExpCore

// #EXP-1797: the issue trash replays the ×4 fixture `issue-lifecycle.json` →
// `trash` (web `issue-lifecycle.test.ts`, desktop and Android mirror it):
// the copy, the caption countdown, the pills per role and the sweep's rule.
final class IssueTrashTests: XCTestCase {
    private func trash() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/issue-lifecycle.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        let root = try XCTUnwrap(json as? [String: Any])
        return try XCTUnwrap(root["trash"] as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try trash()[key] as? [[String: Any]], key)
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func date(_ raw: Any?) throws -> Date {
        try XCTUnwrap(IssueTrash.parse(raw as? String), "\(String(describing: raw))")
    }

    func testTheCopyAndRetentionMatchTheFixture() throws {
        let trash = try trash()
        let copy = try XCTUnwrap(trash["copy"] as? [String: String])
        XCTAssertEqual(IssueTrash.Copy.band, copy["band"])
        XCTAssertEqual(IssueTrash.Copy.row, copy["row"])
        XCTAssertEqual(IssueTrash.Copy.restore, copy["restore"])
        XCTAssertEqual(IssueTrash.Copy.purge, copy["purge"])
        XCTAssertEqual(IssueTrash.retentionDays, trash["retentionDays"] as? Int)
        let access = try XCTUnwrap(trash["access"] as? [String: String])
        XCTAssertEqual(IssueTrash.purgeRole, access["purge"])
    }

    func testCaptionCases() throws {
        for testCase in try cases("captionCases") {
            let name = testCase["name"] as? String ?? "?"
            let input = try XCTUnwrap(testCase["input"] as? [String: Any], name)
            let purgeAt = input["purgeAt"] as? String
            XCTAssertEqual(
                IssueTrash.caption(purgeAt: purgeAt, now: try date(input["now"])),
                testCase["expected"] as? String,
                name
            )
        }
    }

    func testActionCases() throws {
        for testCase in try cases("actionCases") {
            let name = testCase["name"] as? String ?? "?"
            let input = try XCTUnwrap(testCase["input"] as? [String: Any], name)
            let role = try XCTUnwrap(input["role"] as? String, name)
            XCTAssertEqual(
                IssueTrash.actions(role: role).map(\.rawValue),
                testCase["expected"] as? [String],
                name
            )
        }
    }

    func testPurgeDueCases() throws {
        for testCase in try cases("purgeDueCases") {
            let name = testCase["name"] as? String ?? "?"
            let input = try XCTUnwrap(testCase["input"] as? [String: Any], name)
            let deletedAt = input["deletedAt"] as? String
            XCTAssertEqual(
                IssueTrash.isPurgeDue(deletedAt: deletedAt, now: try date(input["now"])),
                testCase["expected"] as? Bool,
                name
            )
        }
    }

    func testRowText() {
        XCTAssertEqual(IssueTrash.rowText(identifier: "EXP-12", title: "Fix the {title} bug"), "EXP-12 Fix the {title} bug")
    }

    func testTrashedIssueDecodes() throws {
        let json = #"""
        {"id":"i1","identifier":"EXP-12","title":"Fix","boardId":"b1","deletedAt":"2026-11-12T12:00:00.000Z","deletedBy":null,"deletedByAgentId":null,"purgeAt":"2026-12-12T12:00:00.000Z"}
        """#
        let issue = try JSONDecoder().decode(TrashedIssue.self, from: Data(json.utf8))
        XCTAssertEqual(issue.identifier, "EXP-12")
        XCTAssertNil(issue.deletedBy)
        XCTAssertEqual(issue.purgeAt, "2026-12-12T12:00:00.000Z")
    }
}
