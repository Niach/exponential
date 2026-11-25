import Foundation
import XCTest
@testable import ExpCore

// #EXP-2124 / #EXP-2109: packages/domain-contract/fixtures/sidebar-menus.json,
// the rows a phone draws.
final class SidebarMenusTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        var url = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { url.deleteLastPathComponent() }
        url.appendPathComponent("packages/domain-contract/fixtures/sidebar-menus.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    private func replay(_ key: String, _ run: (SidebarMenus.Kind, SidebarMenus.Input) -> [SidebarMenus.Action]) throws {
        var replayed = 0
        for c in try XCTUnwrap(try fixture()[key] as? [[String: Any]]) {
            guard let kind = SidebarMenus.Kind(rawValue: try XCTUnwrap(c["kind"] as? String)) else { continue }
            let flags = c["input"] as? [String: Any]
            let input = SidebarMenus.Input(owner: flags?["owner"] as? Bool ?? false, mine: flags?["mine"] as? Bool ?? false)
            XCTAssertEqual(run(kind, input).map(\.rawValue), c["expected"] as? [String], "\(c["name"] ?? "")")
            replayed += 1
        }
        XCTAssertGreaterThan(replayed, 5)
    }

    func testMenus() throws {
        try replay("menuCases") { SidebarMenus.menu($0, $1) }
        try replay("phoneCases") { SidebarMenus.phoneMenu($0, $1) }
        // Every phone case is a kind this app draws.
        for c in try XCTUnwrap(try fixture()["phoneCases"] as? [[String: Any]]) {
            XCTAssertNotNil(SidebarMenus.Kind(rawValue: try XCTUnwrap(c["kind"] as? String)), "\(c["name"] ?? "")")
        }
    }

    /// The phones' Devices list has no device page: its verbs
    /// without a destination are left out.
    func testDeviceMenusOnThePhones() {
        XCTAssertEqual(
            SidebarMenus.deviceMenu(mine: true), [.rename, .copyLink, .separator, .settings]
        )
        XCTAssertEqual(SidebarMenus.deviceMenu(mine: false), [.copyLink])
    }

    func testCopyAndIcons() throws {
        let fixture = try fixture()
        let copy = try XCTUnwrap(fixture["copy"] as? [String: String])
        let icons = try XCTUnwrap(fixture["icons"] as? [String: String])
        for action in SidebarMenus.Action.allCases where action != .separator {
            XCTAssertEqual(SidebarMenus.copy[action], copy[action.rawValue])
            XCTAssertEqual(SidebarMenus.icons[action], icons[action.rawValue])
        }
        XCTAssertEqual(SidebarMenus.topLevel, copy["top-level"])
        XCTAssertEqual(SidebarMenus.linkCopied, copy["linkCopied"])
        let newCopy = try XCTUnwrap(fixture["newCopy"] as? [String: String])
        let newIcons = try XCTUnwrap(fixture["newIcons"] as? [String: String])
        for kind in SidebarMenus.NewKind.allCases {
            XCTAssertEqual(SidebarMenus.newCopy[kind], newCopy[kind.rawValue])
            XCTAssertEqual(SidebarMenus.newIcons[kind], newIcons[kind.rawValue])
        }
    }

    func testNewKinds() throws {
        for c in try XCTUnwrap(try fixture()["newCases"] as? [[String: Any]]) {
            let got = SidebarMenus.newKinds(owner: try XCTUnwrap(c["owner"] as? Bool)).map(\.rawValue)
            XCTAssertEqual(got, c["expected"] as? [String])
        }
    }

    func testMoveTargets() throws {
        for c in try XCTUnwrap(try fixture()["moveCases"] as? [[String: Any]]) {
            let rows = try XCTUnwrap(c["rows"] as? [[String: Any]]).map { row in
                SidebarFolders.Row(
                    kind: try XCTUnwrap(SidebarFolders.Kind(rawValue: try XCTUnwrap(row["kind"] as? String))),
                    id: try XCTUnwrap(row["id"] as? String), depth: try XCTUnwrap(row["depth"] as? Int),
                    group: row["kind"] as? String == "folder", name: row["name"] as? String, open: true, count: 0
                )
            }
            let got = SidebarMenus.moveTargets(rows, id: try XCTUnwrap(c["id"] as? String))
            let expected = try XCTUnwrap(c["expected"] as? [String: Any])
            XCTAssertEqual(got.topLevel, expected["topLevel"] as? String)
            XCTAssertEqual(got.folders.map(\.id), (expected["folders"] as? [[String: Any]])?.compactMap { $0["id"] as? String })
        }
    }

    /// Move to folder › New folder (the TS reference's cases).
    func testMoveToNewFolder() {
        let top = SidebarFolders.moveToNewFolder(items: ["a", "b", "c"], entries: [], id: "b", newFolderId: "f")
        XCTAssertEqual(top?.createdFolderId, "f")
        XCTAssertEqual(
            top.map { SidebarFolders.tree(items: ["a", "b", "c"], entries: SidebarFolders.apply($0, to: [])) },
            [.item(id: "a"), .folder(id: "f", name: "New folder", children: ["b"]), .item(id: "c")]
        )
        let entries: [SidebarFolders.Entry] = [
            .init(id: "g", kind: .folder, targetId: nil, parentId: nil, name: "G", sortOrder: 1),
            .init(id: "ea", kind: .item, targetId: "a", parentId: "g", name: nil, sortOrder: 1),
        ]
        let emptied = SidebarFolders.moveToNewFolder(items: ["a", "b"], entries: entries, id: "a", newFolderId: "f")
        XCTAssertEqual(emptied?.deletes, ["g"])
        XCTAssertNil(SidebarFolders.moveToNewFolder(items: ["a"], entries: [], id: "x", newFolderId: "f"))
    }
}
