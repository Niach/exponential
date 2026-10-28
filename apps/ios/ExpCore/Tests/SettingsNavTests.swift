import Foundation
import XCTest
@testable import ExpCore

// #EXP-1381: the Team page's settings rows, locked ×4 (TS
// `settings-nav.test.ts`, Android `SettingsNavTest`) against
// fixtures/settings-nav.json: `rows` (id, label, icon, group, visible,
// phone, count), `phoneRows` and the test ids equal the generated constants.
final class SettingsNavTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/settings-nav.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testRowsMatchTheFixture() throws {
        let rows = try XCTUnwrap(try fixture()["rows"] as? [[String: Any]])
        XCTAssertFalse(rows.isEmpty)
        XCTAssertEqual(rows.count, SettingsNav.rows.count)
        for (expected, got) in zip(rows, SettingsNav.rows) {
            let id = try XCTUnwrap(expected["id"] as? String)
            XCTAssertEqual(got.id, id)
            XCTAssertEqual(got.label, expected["label"] as? String, id)
            XCTAssertEqual(got.icon, expected["icon"] as? String, id)
            XCTAssertEqual(got.group, expected["group"] as? String, id)
            XCTAssertEqual(got.visible.rawValue, expected["visible"] as? String, id)
            XCTAssertEqual(got.phone, expected["phone"] as? Bool, id)
            XCTAssertEqual(got.count?.rawValue, expected["count"] as? String, id)
            XCTAssertNotNil(SettingsNav.Id(rawValue: id), id)
        }
        XCTAssertEqual(SettingsNav.Id.allCases.map(\.rawValue), rows.compactMap { $0["id"] as? String })
    }

    func testPhoneRowsMatchTheFixture() throws {
        let phoneRows = try XCTUnwrap(try fixture()["phoneRows"] as? [String])
        XCTAssertEqual(SettingsNav.phoneRows.map(\.rawValue), phoneRows)
        // `phoneRows` = exactly the rows flagged `phone`, in row order.
        XCTAssertEqual(SettingsNav.rows.filter(\.phone).map(\.id), phoneRows)
        for id in SettingsNav.phoneRows {
            XCTAssertEqual(SettingsNav.row(id).id, id.rawValue)
        }
    }

    func testVisibleCasesMatchTheFixture() throws {
        let cases = try XCTUnwrap(try fixture()["visibleCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"] as? String ?? "?"
            let isOwner = try XCTUnwrap(c["role"] as? String, name) == DomainContract.teamRoleOwner
            let cloud = try XCTUnwrap(c["cloud"] as? Bool, name)
            let visible = SettingsNav.rows
                .filter { SettingsNav.isVisible($0, isOwner: isOwner, cloud: cloud) }
                .map(\.id)
            XCTAssertEqual(visible, c["expected"] as? [String], name)
            XCTAssertEqual(
                SettingsNav.visiblePhoneRows(isOwner: isOwner, cloud: cloud).map(\.rawValue),
                c["expectedPhone"] as? [String],
                name
            )
        }
    }

    func testTestIdsMatchTheFixture() throws {
        let ids = try XCTUnwrap(try fixture()["testIds"] as? [String: String])
        XCTAssertEqual(SettingsNav.rowTestId, ids["row"])
        XCTAssertEqual(SettingsNav.pageTestId, ids["page"])
    }
    // MARK: - S-3 `nav`

    private func nav() throws -> [String: Any] {
        try XCTUnwrap(try fixture()["nav"] as? [String: Any])
    }

    func testNavGroupsPagesAndEntriesMatchTheFixture() throws {
        let nav = try nav()
        let groups = try XCTUnwrap(nav["groups"] as? [[String: Any]])
        XCTAssertEqual(groups.count, SettingsNav.groups.count)
        for (want, group) in zip(groups, SettingsNav.groups) {
            XCTAssertEqual(want["id"] as? String, group.id)
            XCTAssertEqual(want["label"] as? String, group.label, group.id)
            XCTAssertEqual(want["platforms"] as? [String], group.platforms?.map(\.rawValue), group.id)
            XCTAssertEqual(want["items"] as? [String], group.items, group.id)
        }
        XCTAssertEqual(nav["children"] as? [String: [String]], SettingsNav.children)
        let pages = try XCTUnwrap(nav["pages"] as? [String: [String: Any]])
        XCTAssertEqual(Set(pages.keys), Set(SettingsNav.pages.keys))
        for (id, want) in pages {
            let page = try XCTUnwrap(SettingsNav.pages[id], id)
            XCTAssertEqual(want["label"] as? String, page.label, id)
            XCTAssertEqual(want["icon"] as? String, page.icon, id)
            XCTAssertEqual(want["visible"] as? String, page.visible.rawValue, id)
            XCTAssertEqual(want["platforms"] as? [String], page.platforms?.map(\.rawValue), id)
        }
        let rowPlatforms = try XCTUnwrap(nav["rowPlatforms"] as? [String: [String]])
        XCTAssertEqual(rowPlatforms, SettingsNav.rowPlatforms.mapValues { $0.map(\.rawValue) })
        let newEntries = try XCTUnwrap(nav["newEntries"] as? [String: String])
        XCTAssertEqual(newEntries, SettingsNav.newEntries.mapValues(\.rawValue))
        // Every page item resolves to a row or a page; children too.
        for item in SettingsNav.groups.flatMap(\.items) + SettingsNav.children.values.flatMap({ $0 }) where !item.hasPrefix("@") {
            XCTAssertNotNil(SettingsNav.page(item), item)
        }
        XCTAssertEqual(SettingsNav.page("billing")?.platforms, [.web])
        XCTAssertEqual(SettingsNav.page("labels")?.label, "Labels")
    }

    func testNavCasesMatchTheFixture() throws {
        let cases = try XCTUnwrap(try nav()["navCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"] as? String ?? "?"
            let platform = try XCTUnwrap(SettingsNav.Platform(rawValue: try XCTUnwrap(c["platform"] as? String)), name)
            let isOwner = try XCTUnwrap(c["role"] as? String, name) == DomainContract.teamRoleOwner
            let cloud = try XCTUnwrap(c["cloud"] as? Bool, name)
            let got = SettingsNav.visibleGroups(platform: platform, isOwner: isOwner, cloud: cloud)
            var byId: [String: [String]] = [:]
            for group in got { byId[group.id] = group.items }
            XCTAssertEqual(byId, c["expected"] as? [String: [String]], name)
        }
    }

    /// The iOS nav (the phones' Settings): no This device group, no Import,
    /// no Plan & Billing; About stays; Voice is gone from Personal.
    func testIosNavForAnOwnerAndAMember() {
        let owner = SettingsNav.visibleGroups(platform: .ios, isOwner: true)
        XCTAssertEqual(owner.map(\.id), ["team", "boards", "agents", "apps", "personal"])
        XCTAssertEqual(owner.first { $0.id == "team" }?.items, [
            "general", "members", "issues", "repositories", "prompts", "widget", "storage",
        ])
        XCTAssertEqual(owner.first { $0.id == "agents" }?.items, ["@assistant", "@agents", "@new-agent", "mcp-servers"])
        XCTAssertEqual(owner.first { $0.id == "personal" }?.items, ["account", "notifications", "security", "about"])
        let member = SettingsNav.visibleGroups(platform: .ios, isOwner: false)
        XCTAssertEqual(member.first { $0.id == "boards" }?.items, ["@boards"])
        XCTAssertEqual(member.first { $0.id == "agents" }?.items, ["@assistant", "@agents", "mcp-servers"])
    }
}
