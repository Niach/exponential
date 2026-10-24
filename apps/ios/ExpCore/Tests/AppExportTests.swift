import Foundation
import XCTest
@testable import ExpCore

// VAPP-121: app export ×4, replayed against
// packages/domain-contract/fixtures/app-export.json (TS `app-export.test.ts`,
// desktop `domain::app_export`, Android `AppExportTest`): every
// `exportCases` entry compared as parsed JSON (key order free), plus the
// format, suffix, limits, copy and the capture text.
final class AppExportTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/app-export.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testFormatLimitsAndCopyMatchTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["format"] as? String, AppExport.format)
        XCTAssertEqual(fixture["fileSuffix"] as? String, AppExport.fileSuffix)
        XCTAssertEqual(fixture["mime"] as? String, AppExport.mime)
        let limits = try XCTUnwrap(fixture["limits"] as? [String: Int])
        XCTAssertEqual(limits["importMaxBytes"], AppExport.Limits.importMaxBytes)
        XCTAssertEqual(limits["recordsMax"], AppExport.Limits.recordsMax)
        XCTAssertEqual(limits["issuesShown"], AppExport.Limits.issuesShown)
        let copy = try XCTUnwrap(fixture["copy"] as? [String: String])
        let mine: [String: String] = [
            "export": AppExport.Copy.export,
            "exportTitle": AppExport.Copy.exportTitle,
            "includeRecords": AppExport.Copy.includeRecords,
            "import": AppExport.Copy.import,
            "importTitle": AppExport.Copy.importTitle,
            "importPlaceholder": AppExport.Copy.importPlaceholder,
            "chooseFile": AppExport.Copy.chooseFile,
            "importAction": AppExport.Copy.importAction,
            "importNote": AppExport.Copy.importNote,
            "moreIssues": AppExport.Copy.moreIssues,
        ]
        XCTAssertEqual(copy, mine)
        XCTAssertEqual(AppExport.Copy.exportTitle("Working hours"), "Export Working hours")
        XCTAssertEqual(AppExport.Copy.moreIssues(3), "and 3 more")
        let capture = try XCTUnwrap(fixture["capture"] as? [String: Any])
        XCTAssertEqual(capture["importText"] as? String, #"{"format":"exponential-app/1","app":{"name":"Todos"}}"#)
    }

    func testExportCases() throws {
        let cases = try XCTUnwrap(try fixture()["exportCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for item in cases {
            let name = try XCTUnwrap(item["name"] as? String)
            let input = try XCTUnwrap(item["input"] as? [String: Any], name)
            let expected = try XCTUnwrap(item["expected"] as? [String: Any], name)
            let app = try XCTUnwrap(input["app"] as? [String: Any], name)
            let records = try (input["records"] as? [[String: Any]] ?? []).map { row in
                AppExport.Record(
                    id: try XCTUnwrap(row["id"] as? String),
                    kind: try XCTUnwrap(row["kind"] as? String),
                    data: try XCTUnwrap(CardJSON(any: row["data"])),
                    createdAt: try XCTUnwrap(row["createdAt"] as? String)
                )
            }
            let file = AppExport.build(
                name: try XCTUnwrap(app["name"] as? String),
                icon: app["icon"] as? String,
                color: app["color"] as? String,
                description: app["description"] as? String,
                package: try XCTUnwrap(CardJSON(any: input["package"])),
                records: records,
                includeRecords: input["includeRecords"] as? Bool ?? false
            )
            XCTAssertEqual(AppExport.fileName(slug: try XCTUnwrap(app["slug"] as? String)), expected["fileName"] as? String, name)
            XCTAssertEqual(file, try XCTUnwrap(CardJSON(any: expected["file"])), name)
            // The written text parses back to the same file.
            let reparsed = try JSONSerialization.jsonObject(with: Data(AppExport.text(file).utf8))
            XCTAssertEqual(CardJSON(any: reparsed), file, name)
            XCTAssertTrue(AppExport.text(file).hasSuffix("}\n"), name)
        }
    }

    func testVisibleIssuesCapAtTheLimit() {
        let issues = (0..<11).map { ImportIssue(path: "/records/\($0)", message: "must be {kind, data}") }
        let visible = AppExport.visibleIssues(issues)
        XCTAssertEqual(visible.shown.count, AppExport.Limits.issuesShown)
        XCTAssertEqual(visible.more, "and 3 more")
        XCTAssertNil(AppExport.visibleIssues(Array(issues.prefix(2))).more)
        XCTAssertEqual(ImportIssue(path: "/package", message: "is required").line, "/package: is required")
    }
}
