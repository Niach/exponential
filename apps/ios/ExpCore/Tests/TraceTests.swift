import Foundation
import XCTest
@testable import ExpCore

// EXP-1324: THE Trace models, locked ×4 against
// `packages/domain-contract/fixtures/trace.json` — every case's WHOLE
// expected object. Read through `#filePath`: the unit-test bundle carries no
// repo resources.
final class TraceTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/trace.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ group: String) throws -> [[String: Any]] {
        let all = try XCTUnwrap(try fixture()["cases"] as? [String: Any])
        return try XCTUnwrap(all[group] as? [[String: Any]])
    }

    private func int(_ raw: Any?) -> Int? { (raw as? NSNumber)?.intValue }

    private func line(_ raw: Any?) throws -> TraceLineModel? {
        guard let raw = raw as? [String: Any] else { return nil }
        return TraceLineModel(
            icon: try XCTUnwrap(raw["icon"] as? String),
            title: try XCTUnwrap(raw["title"] as? String),
            description: raw["description"] as? String,
            status: (raw["status"] as? String).flatMap(TraceStatus.init(rawValue:))
        )
    }

    func testTool() throws {
        let all = try cases("tool")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = Trace.tool(
                toolKind: try XCTUnwrap(input["toolKind"] as? String),
                name: try XCTUnwrap(input["name"] as? String),
                detail: input["detail"] as? String,
                settled: try XCTUnwrap(input["settled"] as? Bool),
                failed: try XCTUnwrap(input["failed"] as? Bool)
            )
            XCTAssertEqual(got, try line(c["expected"]), "\(c["name"] ?? "")")
        }
    }

    func testReasoning() throws {
        for c in try cases("reasoning") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = Trace.reasoning(
                running: try XCTUnwrap(input["running"] as? Bool),
                durationMs: int(input["durationMs"])
            )
            XCTAssertEqual(got, try line(c["expected"]), "\(c["name"] ?? "")")
        }
    }

    func testSubagent() throws {
        for c in try cases("subagent") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = Trace.subagent(
                label: input["label"] as? String,
                running: try XCTUnwrap(input["running"] as? Bool),
                failed: try XCTUnwrap(input["failed"] as? Bool),
                durationMs: int(input["durationMs"])
            )
            XCTAssertEqual(got, try line(c["expected"]), "\(c["name"] ?? "")")
        }
    }

    func testTests() throws {
        let all = try cases("tests")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let name = "\(c["name"] ?? "")"
            let expected = try XCTUnwrap(c["expected"] as? [String: Any])
            let results = Trace.parseTestResults(c["output"] as? String)
            if let raw = expected["results"] as? [String: Any] {
                XCTAssertEqual(results, TestResults(
                    passed: int(raw["passed"]) ?? -1,
                    failed: int(raw["failed"]) ?? -1,
                    total: int(raw["total"]) ?? -1,
                    suite: raw["suite"] as? String
                ), name)
            } else {
                XCTAssertNil(results, name)
            }
            XCTAssertEqual(Trace.failedTests(results), try line(expected["line"]), name)
        }
    }
}
