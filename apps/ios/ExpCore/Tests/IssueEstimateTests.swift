import Foundation
import XCTest
@testable import ExpCore

// EXP-630: the estimate helpers, locked ×4 against
// `domain-contract/fixtures/issue-estimate.json` — same cases, same test
// names on the web (`issue-estimate.test.ts`), Android (IssueEstimateTest) and
// the desktop (domain::issue_estimate).
final class IssueEstimateTests: XCTestCase {
    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources. `JSONSerialization` rather
    /// than Decodable: the phrase payloads mix null, number and string.
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/issue-estimate.json")
        let object = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(object as? [String: Any])
    }

    private func rows(_ key: String) throws -> [[String: Any]] {
        let rows = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(rows.isEmpty, "the issue-estimate fixture's \(key) must not be empty")
        return rows
    }

    private func optionalInt(_ row: [String: Any], _ key: String) -> Int? {
        guard let value = row[key], !(value is NSNull) else { return nil }
        return value as? Int
    }

    func testLaddersAndTshirtLabelsMatchTheFixture() throws {
        let fixture = try fixture()
        let scales = try XCTUnwrap(fixture["scales"] as? [String: [Int]])
        XCTAssertEqual(IssueEstimate.scales, scales)
        XCTAssertEqual(IssueEstimate.tshirtLabels, try XCTUnwrap(fixture["tshirtLabels"] as? [String]))
        XCTAssertEqual(IssueEstimate.noEstimate, try XCTUnwrap(fixture["noEstimate"] as? String))
        // Every ladder is a known contract scale, and `none` has none.
        for key in scales.keys {
            XCTAssertTrue(DomainContract.issueEstimationValues.contains(key), key)
        }
        XCTAssertEqual(IssueEstimate.scale(DomainContract.issueEstimationNone), [])
        XCTAssertFalse(IssueEstimate.isEnabled(DomainContract.issueEstimationNone))
        XCTAssertFalse(IssueEstimate.isEnabled(nil))
        XCTAssertTrue(IssueEstimate.isEnabled(DomainContract.issueEstimationTshirt))
    }

    func testLabel() throws {
        for row in try rows("labels") {
            let name = try XCTUnwrap(row["name"] as? String)
            let scale = try XCTUnwrap(row["scale"] as? String)
            let value = optionalInt(row, "value")
            XCTAssertEqual(estimateLabel(value, scale: scale), row["label"] as? String, "label: \(name)")
            if let value {
                XCTAssertEqual(estimateShortLabel(value, scale: scale), row["short"] as? String, "label: \(name)")
            } else {
                XCTAssertTrue(row["short"] is NSNull, "label: \(name)")
            }
        }
    }

    func testPicker() throws {
        for row in try rows("pickers") {
            let name = try XCTUnwrap(row["name"] as? String)
            let scale = try XCTUnwrap(row["scale"] as? String)
            let expected = try XCTUnwrap(row["values"] as? [Int])
            XCTAssertEqual(
                estimatePickerValues(current: optionalInt(row, "current"), scale: scale),
                expected,
                "picker: \(name)"
            )
        }
    }

    func testPhrase() throws {
        for row in try rows("phrases") {
            let name = try XCTUnwrap(row["name"] as? String)
            let scale = try XCTUnwrap(row["scale"] as? String)
            let payload = row["payload"] as? [String: Any]
            XCTAssertEqual(estimateEventPhrase(payload: payload, scale: scale), row["phrase"] as? String, "phrase: \(name)")
        }
    }

    // The timeline hands the helper a payload it parsed from the stored JSON
    // string; a nil payload reads as cleared, like the web's `payload?.to`.
    func testPhraseWithoutPayloadReadsAsCleared() {
        XCTAssertEqual(estimateEventPhrase(payload: nil, scale: "linear"), "removed the estimate")
        XCTAssertEqual(estimateEventPhrase(payload: ["to": NSNull()], scale: "linear"), "removed the estimate")
    }
}
