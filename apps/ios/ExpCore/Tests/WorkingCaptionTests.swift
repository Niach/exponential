import Foundation
import XCTest
@testable import ExpCore

// EXP-1501: the working row's caption, locked ×4 against
// `packages/domain-contract/fixtures/working-caption.json` — every case's
// WHOLE string. Read through `#filePath`: the unit-test bundle carries no
// repo resources.
final class WorkingCaptionTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/working-caption.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func int(_ raw: Any?) -> Int? { (raw as? NSNumber)?.intValue }

    func testTheCopyMatchesTheFixture() throws {
        let copy = try XCTUnwrap(try fixture()["copy"] as? [String: String])
        XCTAssertEqual(copy["fallback"], AgentFeed.workingFallbackLabel)
        XCTAssertEqual(copy["writing"], AgentFeed.workingWritingLabel)
        XCTAssertEqual(copy["writingBlank"], AgentFeed.workingWritingBlankLabel)
    }

    func testEveryFixtureCaseReplays() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let now = try XCTUnwrap(int(input["now"]), name)
            var writing: WorkingWriting?
            if let raw = input["writing"] as? [String: Any] {
                writing = WorkingWriting(
                    tool: try XCTUnwrap(raw["tool"] as? String, name),
                    startedAt: try XCTUnwrap(int(raw["startedAt"]), name)
                )
            }
            XCTAssertEqual(
                AgentFeed.workingCaption(
                    startedAt: int(input["startedAt"]),
                    tokens: int(input["tokens"]),
                    now: Date(timeIntervalSince1970: Double(now) / 1000),
                    writing: writing
                ),
                entry["expected"] as? String,
                name
            )
        }
    }

    func testTheWritingToolLabel() {
        XCTAssertEqual(AgentFeed.writingToolLabel("mcp__exponential__exponential_show"), "exponential_show")
        XCTAssertEqual(AgentFeed.writingToolLabel(" Write\n"), "Write")
        XCTAssertEqual(AgentFeed.writingToolLabel("Multi  word\ttool"), "Multi word tool")
        // No second `__`: nothing to drop.
        XCTAssertEqual(AgentFeed.writingToolLabel("mcp__lonely"), "mcp__lonely")
        XCTAssertNil(AgentFeed.writingToolLabel("  "))
        XCTAssertNil(AgentFeed.writingToolLabel("mcp__server__"))
    }
}
