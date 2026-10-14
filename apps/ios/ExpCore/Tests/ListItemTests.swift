import Foundation
import XCTest
@testable import ExpCore

// EXP-1248: the list item ×4 — geometry and the PR node states, replayed from
// `list-item.json` (desktop `domain::list_item`, Android `ListItemTest`). Same
// test names.
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
        struct PrNode: Decodable {
            let name: String
            let state: String
            let ring: String
            let filled: Bool
        }
        let geometry: Geometry
        let prNodes: [PrNode]
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
