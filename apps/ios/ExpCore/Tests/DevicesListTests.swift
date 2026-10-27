import Foundation
import XCTest
@testable import ExpCore

// A-16: the Devices page row, replayed ×4 against `devices-list.json`.
final class DevicesListTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/devices-list.json")
        return try XCTUnwrap(
            try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any]
        )
    }

    func testCopyMatchesTheFixture() throws {
        let copy = try XCTUnwrap(try fixture()["copy"] as? [String: String])
        let declared: [String: String] = [
            "mine": DevicesList.Copy.mine,
            "team": DevicesList.Copy.team,
            "offline": DevicesList.Copy.offline,
            "noAccount": DevicesList.Copy.noAccount,
            "checking": DevicesList.Copy.checking,
            "addAccount": DevicesList.Copy.addAccount,
            "attention": DevicesList.Copy.attention,
            "updateAvailable": DevicesList.Copy.updateAvailable,
            "updateQueued": DevicesList.Copy.updateQueued,
            "needsRelogin": DevicesList.Copy.needsRelogin,
            "signedOut": DevicesList.Copy.signedOut,
            "update": DevicesList.Copy.update,
            "signIn": DevicesList.Copy.signIn,
        ]
        XCTAssertEqual(declared, copy)
    }

    func testEveryCaseReplays() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any], name)
            let expect = try XCTUnwrap(testCase["expect"] as? [String: Any], name)
            let logins = try XCTUnwrap(input["logins"] as? [[String: Any]], name).map { login in
                DevicesList.Login(
                    agent: login["agent"] as? String ?? "",
                    profileId: login["profileId"] as? String ?? "",
                    label: login["label"] as? String ?? "",
                    health: AgentAccountHealth.parse(login["health"] as? String)
                )
            }
            let row = DevicesList.row(DevicesList.Input(
                online: input["online"] as? Bool ?? false,
                mine: input["mine"] as? Bool ?? false,
                server: input["server"] as? Bool ?? false,
                registered: input["registered"] as? Bool ?? false,
                outdated: input["outdated"] as? Bool ?? false,
                latest: input["latest"] as? String,
                updateQueued: input["updateQueued"] as? Bool ?? false,
                accountsReported: input["accountsReported"] as? Bool ?? false,
                logins: logins
            ))
            XCTAssertEqual(row.group.rawValue, expect["group"] as? String, name)
            XCTAssertEqual(row.noAccount, expect["noAccount"] as? Bool, name)
            let items = try XCTUnwrap(expect["attention"] as? [[String: Any]], name)
            XCTAssertEqual(row.attention.count, items.count, name)
            for (got, want) in zip(row.attention, items) {
                XCTAssertEqual(got.kind.rawValue, want["kind"] as? String, name)
                XCTAssertEqual(got.text, want["text"] as? String, name)
                XCTAssertEqual(got.action?.rawValue, want["action"] as? String, name)
                XCTAssertEqual(got.agent, want["agent"] as? String, name)
                XCTAssertEqual(got.profileId, want["profileId"] as? String, name)
            }
        }
    }

    /// The old IDE bug: an agent installed but never signed in (no profile)
    /// badges nothing — the device-level health folds profiles only.
    func testAnAgentWithoutProfilesBadgesNothing() {
        XCTAssertEqual(AgentAccountsRows.deviceHealth([], deviceId: "mint"), .unknown)
        XCTAssertNil(AgentAccountsRows.deviceHealth([], deviceId: "mint").badgeLabel)
    }
}
