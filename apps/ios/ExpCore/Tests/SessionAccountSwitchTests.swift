import Foundation
import XCTest
@testable import ExpCore

// EXP-849: the usage wall's account switch — the rules and copy hand-mirrored
// ×4 (web `lib/session-account-switch.ts`, desktop
// `crates/ui/src/account_switch.rs`, Android `domain/SessionAccountSwitch.kt`).
final class SessionAccountSwitchTests: XCTestCase {
    private func accounts() -> [String: AgentAccount] {
        [
            "claude": AgentAccount(
                signedIn: true,
                email: "me@x.test",
                profiles: [
                    AgentAccountProfile(id: "me", active: true, signedIn: true, email: "me@x.test", health: "ok"),
                    AgentAccountProfile(id: "work", signedIn: true, email: "work@x.test", health: "ok"),
                    AgentAccountProfile(id: "stale", signedIn: true, email: "stale@x.test", health: "needs_relogin"),
                    AgentAccountProfile(id: "gone", signedIn: false, email: "gone@x.test"),
                ]
            ),
            "codex": AgentAccount(
                signedIn: true,
                profiles: [AgentAccountProfile(id: "cx", active: true, signedIn: true, email: "cx@x.test")]
            ),
        ]
    }

    private func options(cli: String? = "claude", account: String? = "me") -> [SessionAccountSwitch.Option] {
        SessionAccountSwitch.options(
            accounts: accounts(),
            usage: nil,
            launchDefaults: DeviceLaunchDefaults(defaultAgent: "codex"),
            cli: cli,
            account: account
        )
    }

    func testOptionsAreTheSessionAgentsSignedInLoginsInComposerOrder() {
        // `sortDeviceLogins` puts the repairable login before the healthy ones.
        XCTAssertEqual(options().map(\.account.id), ["me", "stale", "work"])
        XCTAssertEqual(options(cli: "codex").map(\.account.id), ["cx"])
        // No cli on the row: the device's default agent.
        XCTAssertEqual(options(cli: nil).map(\.account.id), ["cx"])
        XCTAssertEqual(
            SessionAccountSwitch.options(accounts: nil, usage: nil, launchDefaults: nil, cli: "claude", account: nil),
            []
        )
    }

    func testTheCurrentAccountIsTheSessionsSyncedAccountNeverAGuess() {
        XCTAssertEqual(options().filter(\.current).map(\.account.id), ["me"])
        XCTAssertEqual(options(account: "work").filter(\.current).map(\.account.id), ["work"])
        XCTAssertTrue(options(account: nil).filter(\.current).isEmpty)
        XCTAssertTrue(options(account: "").filter(\.current).isEmpty)
    }

    func testEveryLoginCarriesItsOwnRefusal() {
        let byId = Dictionary(uniqueKeysWithValues: options().map { ($0.account.id, $0.blockedReason) })
        XCTAssertEqual(byId["me"], .some(SessionAccountSwitch.reasonAlready))
        XCTAssertEqual(byId["work"], .some(nil))
        XCTAssertEqual(byId["stale"], .some(SessionAccountSwitch.reasonNeedsRelogin))
        let signedOut = AccountOption(id: "x", agent: "claude", email: "x@x.test", isLastUsed: false, health: .signedOut)
        XCTAssertEqual(
            SessionAccountSwitch.optionBlockedReason(signedOut, currentAccount: nil),
            SessionAccountSwitch.reasonSignedOut
        )
    }

    func testTheRunLevelRefusalOrderIsAgentThenMachineThenTurn() {
        typealias S = SessionAccountSwitch
        XCTAssertEqual(S.blockedReason(agent: "codex", deviceOnline: false, busy: true), S.reasonAgent)
        XCTAssertEqual(S.blockedReason(agent: nil, deviceOnline: true, busy: false), S.reasonAgent)
        XCTAssertEqual(S.blockedReason(agent: "claude", deviceOnline: false, busy: true), S.reasonOffline)
        XCTAssertEqual(S.blockedReason(agent: "claude", deviceOnline: true, busy: true), S.reasonBusy)
        XCTAssertNil(S.blockedReason(agent: "claude", deviceOnline: true, busy: false))
    }

    func testOnlyWhoeverMaySteerTheSessionSwitchesIt() {
        XCTAssertTrue(SessionAccountSwitch.canSteer(shared: true, userId: "a", currentUserId: "b"))
        XCTAssertTrue(SessionAccountSwitch.canSteer(shared: false, userId: "a", currentUserId: "a"))
        XCTAssertFalse(SessionAccountSwitch.canSteer(shared: false, userId: "a", currentUserId: "b"))
        XCTAssertFalse(SessionAccountSwitch.canSteer(shared: false, userId: "a", currentUserId: nil))
    }

    func testASwitchSendsOnlySessionIdAndTheProfileVerbatim() throws {
        let option = try XCTUnwrap(options().first { $0.account.id == "work" }).account
        let input = SessionAccountSwitch.sendInput(sessionId: "s-1", option: option)
        XCTAssertEqual(input, .switchAccount(sessionId: "s-1", account: "work"))
        let data = try JSONEncoder().encode(input)
        let wire = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(wire.keys.sorted(), ["account", "sessionId"])
    }

    /// The copy is byte-identical ×4: every sentence appears verbatim in the
    /// desktop and Android sources (and web's, when it is checked out).
    func testTheCopyIsByteIdenticalAcrossClients() throws {
        let root = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // ExpCore/Tests/
            .deletingLastPathComponent() // ExpCore/
            .deletingLastPathComponent() // apps/ios/
            .deletingLastPathComponent() // apps/
            .deletingLastPathComponent() // the repo root
        let sources = [
            "apps/desktop/crates/ui/src/account_switch.rs",
            "apps/android/app/src/main/java/com/exponential/app/domain/SessionAccountSwitch.kt",
            "apps/web/src/lib/session-account-switch.ts",
        ]
        let copy = [
            SessionAccountSwitch.wallSwitchLabel,
            SessionAccountSwitch.switchLabel,
            SessionAccountSwitch.costNote,
            SessionAccountSwitch.reasonAgent,
            SessionAccountSwitch.reasonOffline,
            SessionAccountSwitch.reasonBusy,
            SessionAccountSwitch.reasonSignedOut,
            SessionAccountSwitch.reasonNeedsRelogin,
        ]
        for path in sources {
            let url = root.appendingPathComponent(path)
            guard FileManager.default.fileExists(atPath: url.path) else { continue }
            let text = try String(contentsOf: url, encoding: .utf8)
            for sentence in copy {
                XCTAssertTrue(text.contains("\"\(sentence)\"") || text.contains("`\(sentence)`"), "\(path): \(sentence)")
            }
        }
    }
}
