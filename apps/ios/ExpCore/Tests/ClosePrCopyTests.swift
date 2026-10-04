import Foundation
import XCTest

@testable import ExpCore

// EXP-1154: the Close PR copy is the ×4 fixture `close-pr.json`, byte for
// byte (web `close-pr-dialog.test.tsx`, desktop, Android mirror it).
final class ClosePrCopyTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/close-pr.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testTheCloseCopyMatchesTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(ClosePrCopy.menuItem, fixture["menuItem"] as? String)
        XCTAssertEqual(ClosePrCopy.title, fixture["title"] as? String)
        XCTAssertEqual(ClosePrCopy.body, fixture["body"] as? String)
        XCTAssertEqual(ClosePrCopy.batchLineTemplate, fixture["batchLine"] as? String)
        XCTAssertEqual(ClosePrCopy.confirm, fixture["confirm"] as? String)
    }

    func testTheBatchLineJoinsOnlyWhenOtherIssuesShareThePr() {
        XCTAssertEqual(ClosePrCopy.message(otherLinkedIssues: 0), ClosePrCopy.body)
        XCTAssertEqual(
            ClosePrCopy.message(otherLinkedIssues: 2),
            "\(ClosePrCopy.body) It also closes the pull request for 2 linked issues."
        )
    }
}
