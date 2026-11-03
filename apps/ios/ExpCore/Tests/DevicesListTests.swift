import Foundation
import XCTest
@testable import ExpCore

// A-16 / EXP-1560: the Devices page card, replayed ×4 against `devices-list.json`.
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
        var copy = try XCTUnwrap(try fixture()["copy"] as? [String: Any])
        let status = try XCTUnwrap(copy.removeValue(forKey: "status") as? [String: String])
        XCTAssertEqual(status, [
            "needs_relogin": DevicesList.Copy.status(.needsRelogin),
            "signed_out": DevicesList.Copy.status(.signedOut),
            "limit": DevicesList.Copy.status(.limit),
        ])
        let declared: [String: String] = [
            "mine": DevicesList.Copy.mine,
            "team": DevicesList.Copy.team,
            "offline": DevicesList.Copy.offline,
            "noAccount": DevicesList.Copy.noAccount,
            "checking": DevicesList.Copy.checking,
            "addAccount": DevicesList.Copy.addAccount,
            "updateAvailable": DevicesList.Copy.updateAvailable,
            "updateQueued": DevicesList.Copy.updateQueued,
            "needsRelogin": DevicesList.Copy.needsRelogin,
            "signedOut": DevicesList.Copy.signedOut,
            "update": DevicesList.Copy.update,
            "signIn": DevicesList.Copy.signIn,
            "signInTitle": DevicesList.Copy.signInTitle,
            "signInWaiting": DevicesList.Copy.signInWaiting,
            "limitReached": DevicesList.Copy.limitReached,
            "deviceActions": DevicesList.Copy.deviceActions,
            "settings": DevicesList.Copy.settings,
            "removeFromTeam": DevicesList.Copy.removeFromTeam,
            "moreAccounts": DevicesList.Copy.moreAccounts,
            "longLivedRefused": DevicesList.Copy.longLivedRefused,
            "persistLogin": DevicesList.Copy.persistLogin,
            "renewPersistedLogin": DevicesList.Copy.renewPersistedLogin,
            "persistNote": DevicesList.Copy.persistNote,
            "persistedBrowser": DevicesList.Copy.persistedBrowser,
            "persistedToken": DevicesList.Copy.persistedToken,
        ]
        XCTAssertEqual(declared, try XCTUnwrap(copy as? [String: String]))
    }

    /// EXP-1560: the card grid's geometry, and the column formula.
    func testCardMatchesTheFixture() throws {
        let card = try XCTUnwrap(try fixture()["card"] as? [String: Any])
        XCTAssertEqual((card["minWidth"] as? NSNumber)?.doubleValue, Double(DevicesList.Card.minWidth))
        XCTAssertEqual((card["gap"] as? NSNumber)?.doubleValue, Double(DevicesList.Card.gap))
        XCTAssertEqual((card["padding"] as? NSNumber)?.doubleValue, Double(DevicesList.Card.padding))
        XCTAssertEqual((card["iconSize"] as? NSNumber)?.doubleValue, Double(DevicesList.Card.iconSize))
        XCTAssertEqual((card["markSize"] as? NSNumber)?.doubleValue, Double(DevicesList.Card.markSize))
        XCTAssertEqual(card["maxAccountIcons"] as? Int, DevicesList.Card.maxAccountIcons)
        // A 390pt phone's section (16pt page padding each side) = 1 column.
        XCTAssertEqual(DevicesList.columns(width: 358), 1)
        XCTAssertEqual(DevicesList.columns(width: 1100), 4)
        XCTAssertEqual(DevicesList.columns(width: 0), 1)
    }

    /// EXP-1626: no renewal reminder, so no reminder window.
    func testNoRenewalReminderWindow() throws {
        XCTAssertNil(try fixture()["keepSignedIn"])
    }

    /// EXP-1560: `accountIcons` — five fit, a sixth folds behind `⋯`.
    func testAccountIconsReplay() throws {
        let cases = try XCTUnwrap(try fixture()["accountIcons"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any], name)
            let expect = try XCTUnwrap(testCase["expect"] as? [String: Any], name)
            let got = DevicesList.accountIcons(
                count: try XCTUnwrap(input["count"] as? Int, name),
                expanded: input["expanded"] as? Bool ?? false
            )
            XCTAssertEqual(got.visible, expect["visible"] as? Int, name)
            XCTAssertEqual(got.more, expect["more"] as? Bool, name)
        }
    }

    /// EXP-1560: `limitHit` reads FRESH windows only.
    func testLimitHitNeedsAFreshFullWindow() {
        let now = Date()
        let iso = ISO8601DateFormatter()
        let fresh = iso.string(from: now.addingTimeInterval(-60))
        let old = iso.string(from: now.addingTimeInterval(-3600))
        let full = [AgentUsageWindow(key: "session", label: "5h", percent: 100, resetsAt: nil)]
        let half = [AgentUsageWindow(key: "session", label: "5h", percent: 50, resetsAt: nil)]
        XCTAssertTrue(DevicesList.limitHit(AgentUsage(fetchedAt: fresh, windows: full), now: now))
        XCTAssertFalse(DevicesList.limitHit(AgentUsage(fetchedAt: fresh, windows: half), now: now))
        XCTAssertFalse(DevicesList.limitHit(AgentUsage(fetchedAt: old, windows: full), now: now))
        XCTAssertFalse(DevicesList.limitHit(AgentUsage(fetchedAt: fresh, stale: true, windows: full), now: now))
        XCTAssertFalse(DevicesList.limitHit(nil, now: now))
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
                    health: AgentAccountHealth.parse(login["health"] as? String),
                    limitHit: login["limitHit"] as? Bool ?? false,
                    tokenExpiresAt: login["tokenExpiresAt"] as? String
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
                logins: logins,
                now: try XCTUnwrap(DevicesList.parseIso(try XCTUnwrap(input["now"] as? String, name)), name)
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
                XCTAssertEqual(got.status?.rawValue, want["status"] as? String, name)
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
