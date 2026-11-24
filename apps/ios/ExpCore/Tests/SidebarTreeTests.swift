import Foundation
import XCTest
@testable import ExpCore

// #EXP-2109: the phones' walk of the sidebar tree, replayed against
// packages/domain-contract/fixtures/sidebar-tree.json (TS
// `sidebar-tree.test.ts`, Android `SidebarTreeTest`): `teams`, `teamItems`
// and `phone`, the WHOLE expected value.
final class SidebarTreeTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/sidebar-tree.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    private func cases(_ raw: Any?) throws -> [[String: Any]] {
        let list = try XCTUnwrap(raw as? [[String: Any]])
        XCTAssertFalse(list.isEmpty)
        return list
    }

    private func string(_ row: [String: Any], _ key: String) throws -> String {
        try XCTUnwrap(row[key] as? String, key)
    }

    private func sort(_ row: [String: Any]) throws -> Double {
        try XCTUnwrap((row["sortOrder"] as? NSNumber)?.doubleValue)
    }

    private func entries(_ raw: Any?) throws -> [SidebarFolders.Entry] {
        try XCTUnwrap(raw as? [[String: Any]]).map { row in
            SidebarFolders.Entry(
                id: try string(row, "id"),
                kind: try XCTUnwrap(SidebarFolders.Kind(rawValue: try string(row, "kind"))),
                targetId: row["targetId"] as? String,
                parentId: row["parentId"] as? String,
                name: row["name"] as? String,
                sortOrder: try sort(row)
            )
        }
    }

    private func items(_ raw: Any?) throws -> [SidebarTree.TeamItem] {
        try XCTUnwrap(raw as? [[String: Any]]).map { row in
            SidebarTree.TeamItem(
                id: try string(row, "id"),
                kind: try XCTUnwrap(SidebarTree.ItemKind(rawValue: try string(row, "kind")))
            )
        }
    }

    func testCopyAndTeamsMenu() throws {
        let fixture = try fixture()
        let copy = try XCTUnwrap(fixture["copy"] as? [String: String])
        typealias C = SidebarTree.Copy
        for (key, value) in [
            "teams": C.teams, "newTeam": C.newTeam, "joinTeam": C.joinTeam, "new": C.new,
            "board": C.newBoard, "agent": C.newAgent, "app": C.newApp, "settings": C.settings,
        ] {
            XCTAssertEqual(copy[key], value, key)
        }
        XCTAssertEqual(fixture["teamsMenu"] as? [String], SidebarTree.teamsMenu.map(\.rawValue))
    }

    func testTeams() throws {
        for c in try cases(try fixture()["teams"]) {
            let name = try string(c, "name")
            let teams = try XCTUnwrap(c["teams"] as? [[String: Any]]).map {
                (id: try string($0, "id"), name: try string($0, "name"), kind: try string($0, "kind"))
            }
            XCTAssertEqual(SidebarTree.teams(teams), c["expected"] as? [String], name)
        }
    }

    func testTeamItems() throws {
        for c in try cases(try fixture()["teamItems"]) {
            let name = try string(c, "name")
            let got = SidebarTree.teamItems(
                teamId: try string(c, "teamId"),
                boards: try XCTUnwrap(c["boards"] as? [[String: Any]]).map {
                    (id: try string($0, "id"), sortOrder: try sort($0))
                },
                agents: try XCTUnwrap(c["agents"] as? [[String: Any]]).map {
                    SidebarTree.Agent(id: try string($0, "id"), sortOrder: try sort($0), role: try string($0, "role"))
                },
                apps: try XCTUnwrap(c["apps"] as? [[String: Any]]).map {
                    (id: try string($0, "id"), name: try string($0, "name"))
                },
                sessions: try XCTUnwrap(c["sessions"] as? [[String: Any]]).map {
                    SidebarTree.Session(
                        id: try string($0, "id"), teamId: try string($0, "teamId"), parentId: $0["parentId"] as? String
                    )
                },
                entries: try entries(c["entries"])
            )
            let expected = try items(c["expected"])
            XCTAssertEqual(got, expected, name)
        }
    }

    func testPhoneTeamMenu() throws {
        let phone = try XCTUnwrap(try fixture()["phone"] as? [String: Any])
        for c in try cases(phone["teamMenu"]) {
            let name = try string(c, "name")
            let got = SidebarTree.phoneTeamMenu(owner: try XCTUnwrap(c["owner"] as? Bool)).map { entry -> [String: Any] in
                ["id": entry.id.rawValue, "children": entry.children.map(\.rawValue)]
            }
            let expected = try XCTUnwrap(c["expected"] as? NSArray)
            XCTAssertEqual(got as NSArray, expected, name)
        }
    }

    func testPhonePage() throws {
        let phone = try XCTUnwrap(try fixture()["phone"] as? [String: Any])
        for c in try cases(phone["page"]) {
            let name = try string(c, "name")
            let got = SidebarTree.phoneTeamPage(
                items: try items(c["items"]), entries: try entries(c["entries"]), folderId: c["folderId"] as? String
            )
            let json: Any = got.map { rows in
                rows.map { row -> [String: Any] in
                    ["kind": row.kind.rawValue, "id": row.id, "name": row.name.map { $0 as Any } ?? NSNull()]
                } as NSArray
            } ?? NSNull()
            XCTAssertEqual(json as? NSObject, c["expected"] as? NSObject, name)
        }
    }
}
