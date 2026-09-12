import Foundation
import XCTest
@testable import ExpCore

// EXP-849: account HEALTH — the device-written field beside `signedIn`, and
// the derivation every client applies when a machine reports none. Same test
// names as web `lib/agent-usage.test.ts` and Android
// `AgentAccountHealthTest.kt`: a reported value wins, an absent one derives
// from `signedIn`, an unreadable one never reads as healthier than the row,
// and the worst of several is what a device row badges.
final class AgentAccountHealthTests: XCTestCase {
    func testAReportedHealthWins() {
        XCTAssertEqual(
            AgentAccountHealth.resolve("needs_relogin", signedIn: true), .needsRelogin
        )
        XCTAssertEqual(AgentAccountHealth.resolve("ok", signedIn: false), .ok)
        XCTAssertEqual(AgentAccountHealth.resolve("signed_out", signedIn: nil), .signedOut)
        XCTAssertEqual(AgentAccountHealth.resolve("unknown", signedIn: true), .unknown)
        // Padding is wire noise; CASE is not — the wire values are lowercase
        // by contract, so anything else came from somewhere else (web and
        // Android compare the trimmed token as sent too).
        XCTAssertEqual(
            AgentAccountHealth.resolve("  needs_relogin ", signedIn: nil), .needsRelogin
        )
        XCTAssertEqual(AgentAccountHealth.resolve("NEEDS_RELOGIN", signedIn: true), .unknown)
    }

    func testAnAbsentHealthDerivesFromSignedIn() {
        XCTAssertEqual(AgentAccountHealth.resolve(nil, signedIn: true), .ok)
        XCTAssertEqual(AgentAccountHealth.resolve(nil, signedIn: false), .signedOut)
        // A nil `signedIn` is not a signed-in claim: web's
        // `derivedAgentHealth(signedIn === true)` reads it as signed out, and
        // so does this.
        XCTAssertEqual(AgentAccountHealth.resolve(nil, signedIn: nil), .signedOut)
        XCTAssertEqual(AgentAccountHealth.resolve("", signedIn: true), .ok)
        XCTAssertEqual(AgentAccountHealth.derived(signedIn: nil), .signedOut)
    }

    // A newer device naming a state this build has no word for claims NOTHING
    // (`unknown`) — never `ok`, and never a raw token on screen.
    func testAnUnknownValueDegradesToUnknown() {
        XCTAssertEqual(AgentAccountHealth.parse("expired_soon"), .unknown)
        XCTAssertEqual(AgentAccountHealth.resolve("expired_soon", signedIn: false), .unknown)
        XCTAssertEqual(AgentAccountHealth.resolve("expired_soon", signedIn: true), .unknown)
        XCTAssertEqual(AgentAccountHealth.parse(nil), .unknown)
    }

    func testAnAbsentAccountIsUnknownNotSignedOut() {
        XCTAssertEqual(AgentAccountHealth.of(nil), .unknown)
        XCTAssertEqual(
            AgentAccountHealth.of(AgentAccount(signedIn: false)), .signedOut
        )
        XCTAssertEqual(
            AgentAccountHealth.of(
                AgentAccount(signedIn: true, email: "dev@acme.test", health: "needs_relogin")
            ),
            .needsRelogin
        )
        XCTAssertEqual(
            AgentAccountHealth.of(AgentAccountProfile(id: "p1", signedIn: true)), .ok
        )
    }

    // The badge order: something to fix first, then an absent login, then
    // ignorance, then healthy.
    func testTheWorstHealthWins() {
        XCTAssertEqual(AgentAccountHealth.worst([.ok, .signedOut, .needsRelogin]), .needsRelogin)
        XCTAssertEqual(AgentAccountHealth.worst([.ok, .unknown, .signedOut]), .signedOut)
        XCTAssertEqual(AgentAccountHealth.worst([.ok, .unknown]), .unknown)
        XCTAssertEqual(AgentAccountHealth.worst([.ok, .ok]), .ok)
        // Nil for an empty set — nothing was reported, so nothing is claimed
        // (web `worstHealth`, Android `worst`).
        XCTAssertNil(AgentAccountHealth.worst([]))
    }

    // "Needs re-login" is NOT "Signed out" on any client, and the two quiet
    // states wear no badge at all.
    func testBadgeCopyIsDistinct() {
        XCTAssertEqual(AgentAccountHealth.needsRelogin.badgeLabel, "Needs re-login")
        XCTAssertEqual(AgentAccountHealth.signedOut.badgeLabel, "Signed out")
        XCTAssertNil(AgentAccountHealth.ok.badgeLabel)
        XCTAssertNil(AgentAccountHealth.unknown.badgeLabel)
        XCTAssertTrue(AgentAccountHealth.needsRelogin.needsAttention)
        XCTAssertTrue(AgentAccountHealth.signedOut.needsAttention)
        XCTAssertFalse(AgentAccountHealth.unknown.needsAttention)
        XCTAssertFalse(AgentAccountHealth.ok.needsAttention)
    }

    // The wire values are the contract's four, byte-identical ×4.
    func testWireValues() {
        XCTAssertEqual(
            AgentAccountHealth.allCases.map(\.rawValue),
            ["ok", "needs_relogin", "signed_out", "unknown"]
        )
    }

    // MARK: - Off the devices shape

    // The rows the Accounts/Devices surfaces draw carry the DERIVED health:
    // the profile's own report, the top-level account's for a pre-profile
    // machine, and `unknown` for an agent the machine only reported usage for.
    func testProfileRowsCarryHealth() {
        let device = DeviceEntity(
            id: "row-1",
            userId: "me",
            deviceId: "dev-1",
            label: "Studio",
            agentAccounts: #"""
            {"claude":{"signedIn":true,"email":"dev@acme.test","health":"needs_relogin",
              "profiles":[{"id":"system","label":"Default","active":true,"signedIn":true,
                           "email":"dev@acme.test","health":"needs_relogin"},
                          {"id":"work","label":"Work","signedIn":true,
                           "email":"work@acme.test","health":"ok"}]},
             "codex":{"signedIn":false}}
            """#,
            agentUsage: nil,
            agentUsageAt: nil,
            lastSeenAt: "2026-09-12T11:59:00.000Z"
        )
        let rows = AgentAccountsRows.profileRows(
            devices: [device], currentUserId: "me", isOnline: { _ in true }
        )
        let byKey = Dictionary(rows.map { ($0.key, $0) }, uniquingKeysWith: { a, _ in a })
        XCTAssertEqual(byKey["dev-1:claude:system"]?.health, .needsRelogin)
        XCTAssertEqual(byKey["dev-1:claude:work"]?.health, .ok)
        XCTAssertEqual(byKey["dev-1:codex:system"]?.health, .signedOut)

        // The machine badges the worst of them.
        XCTAssertEqual(AgentAccountsRows.deviceHealth(rows, deviceId: "dev-1"), .needsRelogin)
        XCTAssertEqual(AgentAccountsRows.deviceHealth(rows, deviceId: "dev-2"), .unknown)

        // A broken login leads the machine's chips, and a group folded over
        // several machines wears the worst health.
        XCTAssertEqual(
            AgentAccountsRows.deviceRows(rows, deviceId: "dev-1").first?.health, .needsRelogin
        )
        let groups = AgentAccountsRows.accountGroups(rows) { _ in false }
        XCTAssertEqual(groups.first { $0.email == "dev@acme.test" }?.health, .needsRelogin)
        XCTAssertEqual(groups.first { $0.email == "work@acme.test" }?.health, .ok)
    }

    // A signed-IN account the agent refused sorts with the signed-out ones:
    // both have something to do.
    func testNeedsReloginSortsWithAttention() {
        XCTAssertEqual(
            AgentAccountsRows.attentionRank(signedIn: true, usage: nil, health: .needsRelogin), 0
        )
        XCTAssertEqual(
            AgentAccountsRows.attentionRank(signedIn: true, usage: nil, health: .ok), 2
        )
        // The pre-EXP-849 two-argument call keeps its exact ordering.
        XCTAssertEqual(AgentAccountsRows.attentionRank(signedIn: false, usage: nil), 0)
        XCTAssertEqual(AgentAccountsRows.attentionRank(signedIn: true, usage: nil), 2)
    }
}
