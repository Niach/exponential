import ExpCore
@testable import ExpUI
import SwiftUI
import XCTest

// #EXP-1795: the MCP picker rows lead with the catalog brand mark (untinted),
// the shell for a stdio server, else the plug.
final class McpServerPickerTests: XCTestCase {
    func testEveryCatalogMarkHasABrandAsset() {
        for mark in DomainContract.mcpCatalogMarks {
            XCTAssertEqual(AppIcons.brandAssetName(mark), "brand-\(mark)", mark)
        }
    }

    func testRowsLeadWithBrandShellOrPlug() {
        let items = McpServerPicker<EmptyView>.items([
            McpServerRow(id: "a", name: "Linear", url: "https://mcp.linear.app/mcp", connection: .init(status: "connected")),
            McpServerRow(id: "b", name: "GitLab", url: "https://gitlab.acme.dev/api/v4/mcp", connection: .init(status: "missing")),
            McpServerRow(id: "c", name: "Local", command: "npx srv", connection: .init(status: "not_needed")),
            McpServerRow(id: "d", name: "Docs", url: "https://docs.acme.dev/mcp", connection: .init(status: "connected")),
        ])
        let marks: [PickerRowMark] = items.map { $0.mark }
        XCTAssertEqual(marks, [
            .brand("linear"), .brand("gitlab"), .glyph(AppIcons.sessionShell, nil), .glyph(AppIcons.uiMcp, nil),
        ])
        XCTAssertEqual(items.map { $0.description }, ["mcp.linear.app", "Connect first", "npx srv", "docs.acme.dev"])
        XCTAssertEqual(items.map { $0.disabled }, [false, true, false, false])
    }
}
