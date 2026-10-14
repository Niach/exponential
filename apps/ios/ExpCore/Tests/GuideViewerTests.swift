import Foundation
import XCTest
@testable import ExpCore

// EXP-1283: `GuideViewer.state` replays fixtures/session-results.json
// `viewer.cases` (TS `guide-viewer.test.ts`), comparing the WHOLE value.
final class GuideViewerTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/session-results.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testGuideViewerReplaysEveryCase() throws {
        let viewer = try XCTUnwrap(try fixture()["viewer"] as? [String: Any])
        let cases = try XCTUnwrap(viewer["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let guide = (testCase["guide"] as? [Any])?.compactMap { $0 as? [String: Any] } ?? []
            let state = GuideViewer.state(
                records: guide,
                attachmentId: try XCTUnwrap(testCase["attachmentId"] as? String)
            )
            let actual: Any = state.map { state -> [String: Any] in
                [
                    "index": state.index,
                    "count": state.count,
                    "topic": state.topic,
                    "label": state.label,
                    "text": state.text ?? NSNull(),
                    "prevId": state.prevId ?? NSNull(),
                    "nextId": state.nextId ?? NSNull(),
                ]
            } ?? NSNull()
            let expected = (testCase["expected"] ?? NSNull()) as AnyObject
            XCTAssertTrue((actual as AnyObject).isEqual(expected), "\(name)\n actual: \(actual)\n expected: \(expected)")
        }
    }
}
