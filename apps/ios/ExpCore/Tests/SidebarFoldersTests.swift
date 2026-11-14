import Foundation
import XCTest
@testable import ExpCore

// #EXP-1832: sidebar folders ×4, replayed against
// packages/domain-contract/fixtures/sidebar-folders.json (TS
// `sidebar-folders.test.ts`, desktop `domain::sidebar_folders`, Android
// `SidebarFoldersTest`): every list, comparing the WHOLE expected value as
// parsed JSON.
final class SidebarFoldersTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/sidebar-folders.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]], key)
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func entries(_ raw: Any?) throws -> [SidebarFolders.Entry] {
        try XCTUnwrap(raw as? [[String: Any]]).map { row in
            SidebarFolders.Entry(
                id: try XCTUnwrap(row["id"] as? String),
                kind: try XCTUnwrap(SidebarFolders.Kind(rawValue: try XCTUnwrap(row["kind"] as? String))),
                targetId: row["targetId"] as? String,
                parentId: row["parentId"] as? String,
                name: row["name"] as? String,
                sortOrder: try XCTUnwrap((row["sortOrder"] as? NSNumber)?.doubleValue)
            )
        }
    }

    private func json(_ value: String?) -> Any { value.map { $0 as Any } ?? NSNull() }

    private func json(_ node: SidebarFolders.Node) -> [String: Any] {
        switch node {
        case let .item(id): return ["kind": "item", "id": id]
        case let .folder(id, name, children): return ["kind": "folder", "id": id, "name": name, "children": children]
        }
    }

    private func json(_ writes: SidebarFolders.Writes?) -> Any {
        guard let writes else { return NSNull() }
        return [
            "upserts": writes.upserts.map { w -> [String: Any] in
                ["id": json(w.id), "kind": w.kind.rawValue, "targetId": json(w.targetId), "parentId": json(w.parentId), "name": json(w.name), "sortOrder": w.sortOrder]
            },
            "deletes": writes.deletes,
            "createdFolderId": json(writes.createdFolderId),
        ] as [String: Any]
    }

    private func assertSame(_ actual: Any, _ expected: Any?, _ name: String, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(NSArray(object: actual), NSArray(object: expected ?? NSNull()), name, file: file, line: line)
    }

    func testConstantsMatchTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["maxDepth"] as? Int, SidebarFolders.maxDepth)
        XCTAssertEqual(fixture["defaultFolderName"] as? String, SidebarFolders.defaultFolderName)
    }

    func testTree() throws {
        for item in try cases("tree") {
            let name = try XCTUnwrap(item["name"] as? String)
            let tree = SidebarFolders.tree(items: try XCTUnwrap(item["items"] as? [String]), entries: try entries(item["entries"]))
            assertSame(tree.map(json), item["expected"], name)
        }
    }

    func testMove() throws {
        for item in try cases("move") {
            let name = try XCTUnwrap(item["name"] as? String)
            let raw = try XCTUnwrap(item["move"] as? [String: Any], name)
            let move = SidebarFolders.Move(
                id: try XCTUnwrap(raw["id"] as? String),
                before: raw["before"] as? String,
                after: raw["after"] as? String,
                into: raw["into"] as? String
            )
            let writes = SidebarFolders.move(
                items: try XCTUnwrap(item["items"] as? [String]),
                entries: try entries(item["entries"]),
                move: move,
                newFolderId: try XCTUnwrap(item["newFolderId"] as? String)
            )
            assertSame(json(writes), item["expected"], name)
        }
    }

    func testUngroup() throws {
        for item in try cases("ungroup") {
            let name = try XCTUnwrap(item["name"] as? String)
            let writes = SidebarFolders.ungroup(
                items: try XCTUnwrap(item["items"] as? [String]),
                entries: try entries(item["entries"]),
                folderId: try XCTUnwrap(item["folderId"] as? String)
            )
            assertSame(json(writes), item["expected"], name)
        }
    }

    func testRows() throws {
        for item in try cases("rows") {
            let name = try XCTUnwrap(item["name"] as? String)
            let closed = Set(try XCTUnwrap(item["closed"] as? [String]))
            let tree = SidebarFolders.tree(items: try XCTUnwrap(item["items"] as? [String]), entries: try entries(item["entries"]))
            let rows = SidebarFolders.rows(tree) { !closed.contains($0) }.map { row -> [String: Any] in
                ["kind": row.kind.rawValue, "id": row.id, "depth": row.depth, "group": row.group, "name": json(row.name), "open": row.open, "count": row.count]
            }
            assertSame(rows, item["expected"], name)
        }
    }

    func testFolderNames() throws {
        for item in try cases("folderNames") {
            let input = try XCTUnwrap(item["input"] as? String)
            XCTAssertEqual(SidebarFolders.folderName(input), item["expected"] as? String, input)
        }
    }

    func testOpenKeys() throws {
        for item in try cases("openKeys") {
            let key = SidebarFolders.openKey(teamId: try XCTUnwrap(item["teamId"] as? String), folderId: try XCTUnwrap(item["folderId"] as? String))
            XCTAssertEqual(key, item["expected"] as? String)
        }
    }

    func testNoStoredFlagIsOpen() throws {
        let defaults = try XCTUnwrap(UserDefaults(suiteName: "SidebarFoldersTests"))
        defaults.removePersistentDomain(forName: "SidebarFoldersTests")
        XCTAssertTrue(SidebarFolders.isOpen(teamId: "t", folderId: "f", defaults: defaults))
        SidebarFolders.setOpen(false, teamId: "t", folderId: "f", defaults: defaults)
        XCTAssertFalse(SidebarFolders.isOpen(teamId: "t", folderId: "f", defaults: defaults))
    }
}
