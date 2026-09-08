import Foundation
import XCTest
@testable import ExpCore

// EXP-785: the collapsed tool-group caption, locked ×4 (web
// `tool-group-summary.test.ts`, Android `ToolGroupSummaryTest`, desktop
// `steer::tool_group_summary`) against the ONE contract fixture — same cases,
// same test names.
final class ToolGroupSummaryTests: XCTestCase {
    private struct FixtureCase {
        let name: String
        let calls: [ToolCallSummary]
        let expected: String
    }

    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func cases() throws -> [FixtureCase] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/tool-group-summary.json")
        let data = try Data(contentsOf: url)
        let raw = try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [[String: Any]])
        return try raw.map { object in
            let calls = try XCTUnwrap(object["calls"] as? [[String: Any]]).map { call in
                ToolCallSummary(
                    kind: call["kind"] as? String ?? "",
                    detail: call["detail"] as? String,
                    failed: call["failed"] as? Bool ?? false
                )
            }
            return FixtureCase(
                name: try XCTUnwrap(object["name"] as? String),
                calls: calls,
                expected: try XCTUnwrap(object["expected"] as? String)
            )
        }
    }

    func testEveryFixtureCaseRendersByteExact() throws {
        let cases = try cases()
        XCTAssertGreaterThanOrEqual(cases.count, 12)
        for fixture in cases {
            XCTAssertEqual(ToolGroupSummary.summarize(fixture.calls), fixture.expected, fixture.name)
        }
    }

    func testTheFixtureCoversEverySegment() throws {
        let expectations = try cases().map(\.expected)
        func covers(_ needle: String) -> Bool { expectations.contains { $0.contains(needle) } }
        XCTAssertTrue(covers("No tool calls"))
        XCTAssertTrue(covers("Used 1 tool"))
        XCTAssertTrue(covers("Used 3 tools"))
        for first in ["Ran ", "Edited ", "Read ", "Searched ", "Fetched "] {
            XCTAssertTrue(expectations.contains { $0.hasPrefix(first) }, first)
        }
        for segment in [
            "1 command", "2 commands", "1 file", "2 files", "1 time", "2 times",
            "1 page", "2 pages", "1 other tool", "2 other tools", "1 failed", "2 failed",
        ] {
            XCTAssertTrue(covers(segment), segment)
        }
        XCTAssertEqual(ToolGroupSummary.separator, " \u{00B7} ")
        XCTAssertTrue(covers(ToolGroupSummary.separator))
    }
}
