import Foundation
import XCTest
@testable import ExpCore

// EXP-1537: what a send leaves in THE composer, locked ×4 against
// `packages/domain-contract/fixtures/composer-send.json` — every case's
// WHOLE result. Read through `#filePath`: the unit-test bundle carries no
// repo resources.
final class ComposerSendTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/composer-send.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func content(_ raw: Any?, _ name: String) throws -> ComposerSend.Content {
        let object = try XCTUnwrap(raw as? [String: Any], name)
        return ComposerSend.Content(
            text: try XCTUnwrap(object["text"] as? String, name),
            attachments: try XCTUnwrap(object["attachments"] as? [String], name)
        )
    }

    func testEveryFixtureCaseReplays() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let outcome = try XCTUnwrap(
                (input["outcome"] as? String).flatMap(ComposerSend.Outcome.init(rawValue:)), name
            )
            let sent = try content(input["sent"], name)
            let current = try content(input["current"], name)
            XCTAssertEqual(
                ComposerSend.afterSend(outcome: outcome, sent: sent, current: current),
                try content(entry["expected"], name),
                name
            )
            // The text half alone agrees with the whole on every sent case.
            if outcome == .sent {
                XCTAssertEqual(
                    ComposerSend.textAfterSend(sent: sent.text, current: current.text),
                    try content(entry["expected"], name).text,
                    name
                )
            }
        }
    }
}
