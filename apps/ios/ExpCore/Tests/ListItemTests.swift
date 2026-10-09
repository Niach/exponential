import Foundation
import XCTest
@testable import ExpCore

// EXP-1248: the list item ×4 — geometry, the big session row's caption and
// the PR node states, replayed from `list-item.json` (web
// `session-row-caption.test.ts`, desktop `domain::list_item` /
// `domain::session_row`, Android `ListItemTest`). Same test names.
final class ListItemTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Geometry: Decodable {
            let base: Double
            let indent: Double
            let mark: Double
            let gap: Double
            let small: Double
            let big: Double
            let prRow: Double
            let prRowPhone: Double
            let prNode: Double
            let rail: Double
        }
        struct Elapsed: Decodable {
            let name: String
            let ms: Double
            let expected: String
        }
        struct Expected: Decodable {
            let text: String
            let tone: String
        }
        struct Caption: Decodable {
            let name: String
            let ended: Bool
            let paused: Bool
            let state: String
            let device: String?
            let startedAt: String?
            let updatedAt: String?
            let endedAt: String?
            let blockedLabel: String?
            let now: String
            let expected: Expected
        }
        struct PrNode: Decodable {
            let name: String
            let state: String
            let ring: String
            let filled: Bool
        }
        let geometry: Geometry
        let elapsed: [Elapsed]
        let captions: [Caption]
        let prNodes: [PrNode]
    }

    private struct DisplayFixture: Decodable {
        struct Case: Decodable {
            let name: String
            let status: String
            let state: String
            let statusTone: String
        }
        let cases: [Case]
    }

    private func load<T: Decodable>(_ name: String) throws -> T {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/\(name)")
        return try JSONDecoder().decode(T.self, from: try Data(contentsOf: url))
    }

    func testTheGeometryMatchesTheFixture() throws {
        let geometry = try (load("list-item.json") as Fixture).geometry
        XCTAssertEqual(Double(ListItem.base), geometry.base)
        XCTAssertEqual(Double(ListItem.indent), geometry.indent)
        XCTAssertEqual(Double(ListItem.mark), geometry.mark)
        XCTAssertEqual(Double(ListItem.gap), geometry.gap)
        XCTAssertEqual(Double(ListItem.small), geometry.small)
        XCTAssertEqual(Double(ListItem.big), geometry.big)
        XCTAssertEqual(Double(ListItem.prRow), geometry.prRow)
        XCTAssertEqual(Double(ListItem.prRowPhone), geometry.prRowPhone)
        XCTAssertEqual(Double(ListItem.prNode), geometry.prNode)
        XCTAssertEqual(Double(ListItem.rail), geometry.rail)
        XCTAssertEqual(Double(ListItem.leadX(depth: 2)), geometry.base + 2 * geometry.indent)
    }

    func testListElapsedMatchesTheFixture() throws {
        let cases = try (load("list-item.json") as Fixture).elapsed
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            XCTAssertEqual(SessionRowCaption.listElapsed(ms: testCase.ms), testCase.expected, testCase.name)
        }
    }

    func testSessionRowCaptionMatchesTheFixture() throws {
        let cases = try (load("list-item.json") as Fixture).captions
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let state = try XCTUnwrap(CodingSessionDisplayState(rawValue: testCase.state), testCase.name)
            let now = try XCTUnwrap(WireTimestamps.parse(testCase.now), testCase.name)
            let caption = SessionRowCaption.sessionRowCaption(
                ended: testCase.ended,
                paused: testCase.paused,
                state: state,
                device: testCase.device,
                startedAt: testCase.startedAt,
                updatedAt: testCase.updatedAt,
                endedAt: testCase.endedAt,
                blockedLabel: testCase.blockedLabel,
                now: now
            )
            XCTAssertEqual(caption.text, testCase.expected.text, testCase.name)
            XCTAssertEqual(caption.tone.rawValue, testCase.expected.tone, testCase.name)
        }
    }

    func testPaintsEveryLiveStateInSessionDisplayJsonsStatusTone() throws {
        let display: DisplayFixture = try load("session-display.json")
        for testCase in display.cases where testCase.status != "ended" {
            let state = try XCTUnwrap(CodingSessionDisplayState(rawValue: testCase.state), testCase.name)
            let caption = SessionRowCaption.sessionRowCaption(
                ended: false, paused: false, state: state, device: "mint",
                startedAt: nil, updatedAt: nil, endedAt: nil, now: Date(timeIntervalSince1970: 0)
            )
            XCTAssertEqual(caption.tone.rawValue, testCase.statusTone, testCase.name)
        }
    }

    func testPrNodeStatesMatchTheFixture() throws {
        let cases = try (load("list-item.json") as Fixture).prNodes
        XCTAssertEqual(cases.count, PrNodeState.allCases.count)
        for testCase in cases {
            let state = try XCTUnwrap(PrNodeState(rawValue: testCase.state), testCase.name)
            XCTAssertEqual(state.ring.rawValue, testCase.ring, testCase.name)
            XCTAssertEqual(state.filled, testCase.filled, testCase.name)
        }
    }
}
