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
}
