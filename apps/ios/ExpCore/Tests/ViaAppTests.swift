import Foundation
import XCTest
@testable import ExpCore

// #VAPP-229: the "via <app>" caption, locked ×4 (web via-app.ts, desktop
// `domain::via_app`, Android `ViaAppTest`) against the ONE contract fixture.
final class ViaAppTests: XCTestCase {
    private struct Fixture: Decodable {
        let copy: Copy
        let captionCases: [CaptionCase]
        let appendCases: [AppendCase]
    }

    private struct Copy: Decodable {
        let viaApp: String
        let separator: String
    }

    private struct CaptionCase: Decodable {
        let name: String
        let viaAppName: String?
        let text: String?
    }

    private struct AppendCase: Decodable {
        let name: String
        let text: String
        let viaAppName: String?
        let expected: String
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/via-app.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testCopy() throws {
        let copy = try fixture().copy
        XCTAssertEqual(ViaApp.captionTemplate, copy.viaApp)
        XCTAssertEqual(ViaApp.separator, copy.separator)
    }

    func testCaptionCases() throws {
        let cases = try fixture().captionCases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            XCTAssertEqual(viaAppCaption(c.viaAppName), c.text, c.name)
        }
    }

    func testAppendCases() throws {
        let cases = try fixture().appendCases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            XCTAssertEqual(withViaApp(c.text, c.viaAppName), c.expected, c.name)
        }
    }
}
