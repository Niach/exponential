import ExpCore
@testable import ExpUI
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
        XCTAssertEqual(items.map(\.mark), [
            .brand("linear"), .brand("gitlab"), .glyph(AppIcons.sessionShell, nil), .glyph(AppIcons.uiMcp, nil),
        ])
        XCTAssertEqual(items.map(\.description), ["mcp.linear.app", "Connect first", "npx srv", "docs.acme.dev"])
        XCTAssertEqual(items.map(\.disabled), [false, true, false, false])
    }
}
