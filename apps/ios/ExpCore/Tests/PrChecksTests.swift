import Foundation
import XCTest
@testable import ExpCore

// #EXP-1611: the CI UI ×4, replayed against
// packages/domain-contract/fixtures/pr-checks.json (TS `pr-checks.test.ts`,
// desktop `domain::pr_checks`, Android `PrChecksTest`): `mergePill`,
// `formatDuration`, `checksList`, `checksSummary`, `failingBody`, plus the
// copy and test ids.
final class PrChecksTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/pr-checks.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func string(_ value: Any?) -> String? { value as? String }

    private func checks(_ raw: Any?) throws -> [PrChecks.Check] {
        try XCTUnwrap(raw as? [[String: Any]]).map { row in
            PrChecks.Check(
                name: try XCTUnwrap(row["name"] as? String),
                status: try XCTUnwrap(row["status"] as? String),
                conclusion: string(row["conclusion"]),
                startedAt: string(row["startedAt"]),
                completedAt: string(row["completedAt"]),
                detailsUrl: string(row["detailsUrl"])
            )
        }
    }

    private func date(_ raw: Any?) throws -> Date {
        try XCTUnwrap(WireTimestamps.parse(try XCTUnwrap(raw as? String)))
    }

    private func optional(_ value: String?) -> Any { value ?? NSNull() }

    func testCopyAndTestIds() throws {
        let fixture = try fixture()
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(copy["checks"] as? String, PrChecks.Copy.checks)
        XCTAssertEqual(copy["mergeFailing"] as? String, PrChecks.Copy.mergeFailing)
        let summary = try XCTUnwrap(copy["summary"] as? [String: String])
        XCTAssertEqual(summary["failing"], PrChecks.Copy.summaryFailing)
        XCTAssertEqual(summary["running"], PrChecks.Copy.summaryRunning)
        XCTAssertEqual(summary["passed"], PrChecks.Copy.summaryPassed)
        let testIds = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(testIds["checksMenu"], PrChecks.TestIds.checksMenu)
        XCTAssertEqual(testIds["checkRow"], PrChecks.TestIds.checkRow)
    }

    func testMergePillReplaysEveryCase() throws {
        for item in try cases("mergePill") {
            let input = try XCTUnwrap(item["input"] as? [String: Any])
            let pill = PrChecks.mergePill(state: string(input["state"]), stack: try XCTUnwrap(input["stack"] as? Bool))
            let actual: [String: Any] = [
                "label": pill.label, "glyph": pill.glyph.rawValue,
                "primary": pill.primary, "confirm": pill.confirm.rawValue,
            ]
            XCTAssertEqual(actual as NSDictionary, try XCTUnwrap(item["expected"] as? NSDictionary), "\(input)")
        }
    }

    func testFormatDurationReplaysEveryCase() throws {
        for item in try cases("formatDuration") {
            let ms = try XCTUnwrap(item["ms"] as? Double)
            XCTAssertEqual(PrChecks.formatDuration(ms: ms), item["expected"] as? String, "\(ms)")
        }
    }

    func testChecksListReplaysEveryCase() throws {
        for item in try cases("checksList") {
            let name = try XCTUnwrap(item["name"] as? String)
            let rows = PrChecks.checksList(try checks(item["checks"]), now: try date(item["now"]))
            let actual: [[String: Any]] = rows.map {
                [
                    "name": $0.name, "glyph": $0.glyph.rawValue, "tone": $0.tone.rawValue,
                    "duration": optional($0.duration), "url": optional($0.url),
                ]
            }
            XCTAssertEqual(actual as NSArray, try XCTUnwrap(item["expected"] as? NSArray), name)
        }
    }

    func testChecksSummaryReplaysEveryCase() throws {
        for item in try cases("checksSummary") {
            let name = try XCTUnwrap(item["name"] as? String)
            let summary = PrChecks.checksSummary(try checks(item["checks"]))
            if item["expected"] is NSNull {
                XCTAssertNil(summary, name)
                continue
            }
            let actual: [String: Any]? = summary.map {
                ["glyph": $0.glyph.rawValue, "tone": $0.tone.rawValue, "value": $0.value]
            }
            XCTAssertEqual(actual as NSDictionary?, item["expected"] as? NSDictionary, name)
        }
    }

    func testFailingBodyReplaysEveryCase() throws {
        for item in try cases("failingBody") {
            XCTAssertEqual(PrChecks.failingBody(try checks(item["checks"])), item["expected"] as? String)
        }
    }

    func testSpecimenListsEveryRow() throws {
        let specimen = try XCTUnwrap(try fixture()["specimen"] as? [String: Any])
        let rows = PrChecks.checksList(try checks(specimen["checks"]), now: try date(specimen["now"]))
        XCTAssertEqual(rows.map(\.name), ["gate", "e2e", "android", "lint", "docs"])
    }

    func testFailingPromptFillsTheTitle() {
        let plain = Prompts.MergeFailingPr.copy(number: 42, stack: false, body: "Failing: gate.")
        XCTAssertEqual(plain.title, "Merge PR #42 with failing checks?")
        XCTAssertEqual(plain.body, "Failing: gate.")
        XCTAssertEqual(plain.focus, "cancel")
        XCTAssertEqual(
            Prompts.MergeFailingPr.copy(number: 42, stack: true, body: "Failing: gate.").title,
            "Merge the stack with failing checks?"
        )
    }

    func testEntityDecodesTheWire() throws {
        let wire = Data("""
            {"id":"c1","team_id":"t","pull_request_id":"pr1","head_sha":"abc","name":"gate",
             "status":"completed","conclusion":"failure","required":"t",
             "started_at":"2026-11-01 12:00:00+00","completed_at":"2026-11-01 12:04:12+00",
             "details_url":"https://x","created_at":"x","updated_at":"x"}
            """.utf8)
        let row = try JSONDecoder().decode(PullRequestCheckEntity.self, from: wire)
        XCTAssertTrue(row.required)
        let check = PrChecks.Check(row)
        XCTAssertEqual(PrChecks.checkDuration(check, now: Date()), "4m 12s")
        XCTAssertEqual(PrChecks.aggregate([check]), DomainContract.prChecksStateFailure)
    }
}
