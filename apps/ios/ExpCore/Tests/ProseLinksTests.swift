import Foundation
import XCTest
@testable import ExpCore

// #EXP-1674: replayed ×4 against fixtures/prose-links.json (web
// `prose-links.test.ts`, desktop `domain::prose_links`, Android `ProseLinksTest`).
final class ProseLinksTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
        let url = root.appendingPathComponent("packages/domain-contract/fixtures/prose-links.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    }

    func testEveryCaseRewritesTheSame() throws {
        let json = try fixture()
        let origin = try XCTUnwrap(json["origin"] as? String)
        let team = try XCTUnwrap(json["team"] as? String)
        let synced = try XCTUnwrap(json["synced"] as? [String: String])
        let cases = try XCTUnwrap(json["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let text = try XCTUnwrap(testCase["text"] as? String)
            let out = ProseLinks.chipBareAppLinks(text, origin: origin, team: team) { kind, id in
                synced["\(kind.rawValue):\(id)"]
            }
            XCTAssertEqual(out, testCase["out"] as? String, name)
        }
    }

    func testEveryChipKindCaseReadsTheSame() throws {
        let cases = try XCTUnwrap(try fixture()["chipKindCases"] as? [[String: Any]])
        for testCase in cases {
            let href = try XCTUnwrap(testCase["href"] as? String)
            XCTAssertEqual(
                ProseLinks.chipKind(ofHref: href)?.rawValue, testCase["kind"] as? String,
                testCase["name"] as? String ?? ""
            )
        }
    }
}
