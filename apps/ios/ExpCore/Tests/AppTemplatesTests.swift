import Foundation
import XCTest
@testable import ExpCore

// VAPP-234: locks the gallery, its copy and test ids to
// packages/domain-contract/fixtures/app-templates.json (desktop
// `domain::app_templates`, Android `AppTemplatesTest`).
final class AppTemplatesTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/app-templates.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testTheListCopyAndTestIdsMatchTheFixture() throws {
        let fixture = try fixture()
        let templates = try XCTUnwrap(fixture["templates"] as? [[String: String]]).map {
            AppTemplates.Template(id: $0["id"] ?? "", name: $0["name"] ?? "", icon: $0["icon"] ?? "", color: $0["color"] ?? "", description: $0["description"] ?? "")
        }
        XCTAssertEqual(templates, AppTemplates.templates)
        let copy = try XCTUnwrap(fixture["copy"] as? [String: String])
        XCTAssertEqual(copy["title"], AppTemplates.Copy.title)
        XCTAssertEqual(copy["fromTemplate"], AppTemplates.Copy.fromTemplate)
        XCTAssertEqual(copy["includeRecords"], AppTemplates.Copy.includeRecords)
        XCTAssertEqual(copy["note"], AppTemplates.Copy.note)
        let ids = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(ids["row"], AppTemplates.TestIds.row)
        XCTAssertEqual(ids["records"], AppTemplates.TestIds.records)
        XCTAssertEqual(ids["fromTemplate"], AppTemplates.TestIds.fromTemplate)
    }
}
