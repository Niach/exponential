import Foundation
import XCTest
@testable import ExpCore

// EXP-1170: the New issue page's copy + autosave debounce, locked ×4 (web
// `issue-draft-page.test.ts`, Android `IssueDraftPageTest`, desktop
// `domain::issue_draft`) against the ONE contract fixture.
final class IssueDraftPageTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Autosave: Decodable {
            let debounceMs: Double
        }

        let copy: [String: String]
        let autosave: Autosave
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/issue-draft.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testCopy() throws {
        let mirrored: [String: String] = [
            "header": IssueDraftPage.header,
            "titlePlaceholder": IssueDraftPage.titlePlaceholder,
            "descriptionPlaceholder": IssueDraftPage.descriptionPlaceholder,
            "create": IssueDraftPage.create,
            "discard": IssueDraftPage.discard,
            "untitled": IssueDraftPage.untitled,
        ]
        XCTAssertEqual(mirrored, try fixture().copy)
    }

    func testAutosaveDebounce() throws {
        XCTAssertEqual(IssueDraftPage.autosaveDebounceMs, try fixture().autosave.debounceMs)
    }
}
