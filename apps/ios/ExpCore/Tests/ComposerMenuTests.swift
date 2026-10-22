import Foundation
import XCTest
@testable import ExpCore

// EXP-1249: the composer's "+" menu, locked ×4 against
// `composer-menu.json` — same case names as web `composer-menu.test.ts`.
final class ComposerMenuTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/composer-menu.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func wire(_ layout: [ComposerMenu.Entry]) -> [String] {
        layout.map { entry in
            switch entry {
            case let .row(row): row.id.rawValue
            case .separator: "-"
            }
        }
    }

    func testComposerMenuLayoutMatchesTheFixtureCases() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertEqual(cases.count, 5)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let c = try XCTUnwrap(testCase["conditions"] as? [String: Bool], name)
            let layout = ComposerMenu.composerMenuLayout(ComposerMenu.Conditions(
                subagentModel: c["subagentModel"] ?? false,
                ultracode: c["ultracode"] ?? false,
                mcp: c["mcp"] ?? false,
                computerUse: c["computerUse"] ?? false,
                agents: c["agents"] ?? false
            ))
            XCTAssertEqual(wire(layout), testCase["expected"] as? [String], name)
        }
    }

    func testTheRowsCopyGlyphsAndKindsAreTheFixtures() throws {
        let rows = try XCTUnwrap(try fixture()["rows"] as? [[String: Any]])
            .filter { $0["kind"] as? String != "separator" }
        XCTAssertEqual(ComposerMenu.allRows.count, rows.count)
        for (row, want) in zip(ComposerMenu.allRows, rows) {
            XCTAssertEqual(row.id.rawValue, want["id"] as? String)
            XCTAssertEqual(row.kind.rawValue, want["kind"] as? String)
            XCTAssertEqual(row.label, want["label"] as? String)
            XCTAssertEqual(row.codexLabel, want["codexLabel"] as? String)
            XCTAssertEqual(row.icon, want["icon"] as? String)
        }
        XCTAssertEqual(ComposerMenu.allRows.first { $0.id == .effort }?.label(codex: true), "Reasoning")
    }

    func testNamesTheContractsComputerUsePayloadKeyAndCap() throws {
        let block = try XCTUnwrap(try fixture()["computerUse"] as? [String: Any])
        XCTAssertEqual(ComposerMenu.computerUsePayloadKey, block["payloadKey"] as? String)
        XCTAssertEqual(ComposerMenu.computerUseRunCap, block["cap"] as? String)
        XCTAssertTrue(DomainContract.codingSessionLaunchKeys.contains(ComposerMenu.computerUsePayloadKey))
    }

    func testSpellsTheRowTestIdsFromTheFixtureTemplate() throws {
        let ids = try XCTUnwrap(try fixture()["testIds"] as? [String: String])
        XCTAssertEqual(ComposerMenu.plusTestId, ids["plus"])
        XCTAssertEqual(ComposerMenu.menuTestId, ids["menu"])
        XCTAssertEqual(ComposerMenu.implementSubmitTestId, ids["implementSubmit"])
        XCTAssertEqual(ComposerMenu.submenuSearchTestId, ids["submenuSearch"])
        // EXP-1253/EXP-1270: the suggestion rows and the watermark are gone.
        XCTAssertNil(ids["suggestion"])
        XCTAssertNil(ids["brandMark"])
        for id in ComposerMenu.RowId.allCases {
            XCTAssertEqual(
                ComposerMenu.rowTestId(id),
                ids["row"]?.replacingOccurrences(of: "{id}", with: id.rawValue)
            )
        }
        XCTAssertEqual(ComposerMenu.plusLabel, try fixture()["plusLabel"] as? String)
        XCTAssertNil(try fixture()["suggestions"])
        XCTAssertEqual(ComposerMenu.submenuRowHeight, (try fixture()["submenuRowHeight"] as? NSNumber)?.doubleValue)
        XCTAssertEqual(ComposerMenu.submenuMaxRows, try fixture()["submenuMaxRows"] as? Int)
    }

    func testImplementButtonLabel() throws {
        let cases = try XCTUnwrap(try fixture()["implementLabels"] as? [[String: Any]])
        for testCase in cases {
            let count = try XCTUnwrap(testCase["count"] as? Int)
            XCTAssertEqual(ComposerMenu.implementButtonLabel(count), testCase["expected"] as? String)
        }
    }

    func testMcpServerRowsReadinessAndPreselect() throws {
        let rows = try JSONDecoder().decode([McpServerRow].self, from: Data("""
            [{"id":"a","name":"Linear","url":"https://mcp.linear.app/sse","enabledByDefault":true,"connection":{"status":"connected"}},
             {"id":"b","name":"Sentry","enabledByDefault":true,"connection":{"status":"expired"}},
             {"id":"c","name":"Local","command":"npx srv","connection":{"status":"not_needed"}}]
            """.utf8))
        XCTAssertNil(McpServers.notReadyLabel(rows[0]))
        XCTAssertEqual(McpServers.notReadyLabel(rows[1]), "Reconnect first")
        XCTAssertEqual(McpServers.notReadyLabel(McpServerRow(id: "d", name: "x", connection: .init(status: "missing"))), "Connect first")
        XCTAssertEqual(McpServers.preselect(rows, saved: nil), ["a"])
        XCTAssertEqual(McpServers.preselect(rows, saved: ["b", "c"]), ["c"])
        XCTAssertEqual(McpServers.location(rows[0]), "mcp.linear.app")
        XCTAssertEqual(McpServers.location(rows[2]), "npx srv")
        XCTAssertNil(McpServers.pickedValue([]))
        XCTAssertEqual(McpServers.pickedValue(["a", "c"]), "2")
    }
}
