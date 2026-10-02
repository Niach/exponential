import Foundation
import XCTest
@testable import ExpCore

// EXP-1162: the detail chrome, locked ×4 (web `detail-chrome.test.ts`,
// Android `DetailChromeTest`, desktop `domain::detail_chrome`) against the ONE
// contract fixture — same cases, same test names.
final class DetailChromeTests: XCTestCase {
    private struct Fixture: Decodable {
        let constants: [String: Double]
        let collapse: [CollapseCase]
    }

    private struct CollapseCase: Decodable {
        let name: String
        let hasTitleRow: Bool
        let titleBottom: Double?
        let headerBottom: Double
        let collapsed: Bool
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/detail-chrome.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testConstants() throws {
        let mirrored: [String: Double] = [
            "collapseMs": DetailChrome.collapseMs,
            "collapseRise": DetailChrome.collapseRise,
            "edgeTop": DetailChrome.edgeTop,
            "edgeBottom": DetailChrome.edgeBottom,
            "edgeBlur": DetailChrome.edgeBlur,
            "scrim": DetailChrome.scrim,
        ]
        XCTAssertEqual(mirrored, try fixture().constants)
    }

    /// Every fixture case is named by exactly one test below.
    func testEveryCaseHasATest() throws {
        let named: Set<String> = [
            "the title row sits below the header",
            "one point of the title still shows",
            "the title's bottom edge meets the header's: it breaks",
            "the title scrolled far away",
            "a face with no title row is always collapsed",
            "a title row not measured yet stays expanded",
        ]
        XCTAssertEqual(Set(try fixture().collapse.map(\.name)), named)
    }

    private func assertCase(_ name: String) throws {
        let testCase = try XCTUnwrap(try fixture().collapse.first { $0.name == name }, name)
        XCTAssertEqual(
            DetailChrome.isTitleCollapsed(
                hasTitleRow: testCase.hasTitleRow,
                titleBottom: testCase.titleBottom,
                headerBottom: testCase.headerBottom
            ),
            testCase.collapsed,
            name
        )
    }

    func testTheTitleRowSitsBelowTheHeader() throws {
        try assertCase("the title row sits below the header")
    }

    func testOnePointOfTheTitleStillShows() throws {
        try assertCase("one point of the title still shows")
    }

    func testTheTitlesBottomEdgeMeetsTheHeadersItBreaks() throws {
        try assertCase("the title's bottom edge meets the header's: it breaks")
    }

    func testTheTitleScrolledFarAway() throws {
        try assertCase("the title scrolled far away")
    }

    func testAFaceWithNoTitleRowIsAlwaysCollapsed() throws {
        try assertCase("a face with no title row is always collapsed")
    }

    func testATitleRowNotMeasuredYetStaysExpanded() throws {
        try assertCase("a title row not measured yet stays expanded")
    }
}
