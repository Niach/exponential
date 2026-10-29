import Foundation
import XCTest
@testable import ExpCore

// NEW-83: the issue's Files list, locked ×4 against
// `domain-contract/fixtures/issue-files.json` — same cases, same names on the
// web (`issueFileRows`), desktop (ui::issue_files) and Android.
final class IssueFilesTests: XCTestCase {
    private struct Row {
        let id: String
        let contentType: String
        let asFile: Bool
        let commentId: String?
    }

    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func cases() throws -> [[String: Any]] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/issue-files.json")
        let object = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        let fixture = try XCTUnwrap(object as? [String: Any])
        let cases = try XCTUnwrap(fixture["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty, "the issue-files fixture's cases must not be empty")
        return cases
    }

    func testIssueFileRowsMatchesTheFixture() throws {
        for testCase in try cases() {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let texts = try XCTUnwrap(testCase["texts"] as? [Any]).map { $0 as? String }
            let rows = try XCTUnwrap(testCase["rows"] as? [[String: Any]]).map { row in
                Row(
                    id: row["id"] as? String ?? "",
                    contentType: row["contentType"] as? String ?? "",
                    asFile: row["asFile"] as? Bool ?? false,
                    commentId: row["commentId"] as? String
                )
            }
            let expected = try XCTUnwrap(testCase["files"] as? [String], name)
            let listed = AttachmentFiles.issueFileRows(
                rows,
                texts: texts,
                id: \.id,
                contentType: \.contentType,
                asFile: \.asFile,
                commentId: \.commentId
            ).map(\.id)
            XCTAssertEqual(listed, expected, name)
        }
    }
}
