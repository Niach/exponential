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
        let faceDots: [FaceDotsCase]
    }

    private struct FaceDotsCase: Decodable {
        let name: String
        let faces: [String]
        let runLive: Bool
        let needsInput: Bool
        let prOpen: Bool
        let dots: [String: String]
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
            "faceDot": DetailChrome.faceDot,
            "faceDotGap": DetailChrome.faceDotGap,
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

    // MARK: - Face dots

    func testEveryFaceDotsCaseHasATest() throws {
        let named: Set<String> = [
            "no run and no pull request: no dots",
            "a live run dots the Run tab",
            "a run waiting on a person dots the Run tab amber",
            "an ended run carries no dot, even if it still says needs input",
            "an open pull request dots Results",
            "an open pull request with no Results face dots Changes",
            "a live run with an open pull request dots both",
            "an open pull request with neither face shows no dot",
            "an issue-less run dots its Run tab too",
        ]
        XCTAssertEqual(Set(try fixture().faceDots.map(\.name)), named)
    }

    private func assertFaceDots(_ name: String) throws {
        let testCase = try XCTUnwrap(try fixture().faceDots.first { $0.name == name }, name)
        let faces = try testCase.faces.map { try XCTUnwrap(WorkFaceKind(rawValue: $0), $0) }
        let dots = DetailChrome.faceDots(
            faces: faces,
            runLive: testCase.runLive,
            needsInput: testCase.needsInput,
            prOpen: testCase.prOpen
        )
        let wire = Dictionary(uniqueKeysWithValues: dots.map { ($0.key.rawValue, $0.value.rawValue) })
        XCTAssertEqual(wire, testCase.dots, name)
    }

    func testNoRunAndNoPullRequestNoDots() throws {
        try assertFaceDots("no run and no pull request: no dots")
    }

    func testALiveRunDotsTheRunTab() throws {
        try assertFaceDots("a live run dots the Run tab")
    }

    func testARunWaitingOnAPersonDotsTheRunTabAmber() throws {
        try assertFaceDots("a run waiting on a person dots the Run tab amber")
    }

    func testAnEndedRunCarriesNoDotEvenIfItStillSaysNeedsInput() throws {
        try assertFaceDots("an ended run carries no dot, even if it still says needs input")
    }

    func testAnOpenPullRequestDotsResults() throws {
        try assertFaceDots("an open pull request dots Results")
    }

    func testAnOpenPullRequestWithNoResultsFaceDotsChanges() throws {
        try assertFaceDots("an open pull request with no Results face dots Changes")
    }

    func testALiveRunWithAnOpenPullRequestDotsBoth() throws {
        try assertFaceDots("a live run with an open pull request dots both")
    }

    func testAnOpenPullRequestWithNeitherFaceShowsNoDot() throws {
        try assertFaceDots("an open pull request with neither face shows no dot")
    }

    func testAnIssueLessRunDotsItsRunTabToo() throws {
        try assertFaceDots("an issue-less run dots its Run tab too")
    }
}
