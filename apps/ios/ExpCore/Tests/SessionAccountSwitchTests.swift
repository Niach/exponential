import Foundation
import XCTest
@testable import ExpCore

// EXP-849 phase 3: the mid-session account switch gate — claude only, the
// owner only, idle only, and only onto a login the machine can actually use.
// Same fixture and the same test names as Android
// `SessionAccountSwitchTest.kt` and web `agent-account-switch.test.ts`; the
// refusal sentences are locked ×4.
final class SessionAccountSwitchTests: XCTestCase {
    private let accounts: [String: AgentAccount] = [
        "claude": AgentAccount(
            signedIn: true,
            email: "me@acme.test",
            plan: "max",
            profiles: [
                AgentAccountProfile(
                    id: "me",
                    active: true,
                    signedIn: true,
                    email: "me@acme.test",
                    plan: "max",
                    health: "ok"
                ),
                AgentAccountProfile(
                    id: "work",
                    signedIn: true,
                    email: "work@acme.test",
                    health: "ok"
                ),
                AgentAccountProfile(id: "stale", signedIn: true, health: "needs_relogin"),
                AgentAccountProfile(id: "empty", signedIn: false),
            ],
            health: "ok"
        ),
        "codex": AgentAccount(signedIn: true, email: "cx@acme.test"),
    ]

    private func option(_ id: String) throws -> SessionAccountOption {
        try XCTUnwrap(
            SessionAccountSwitch.options(accounts: accounts, agent: "claude")
                .first { $0.profileId == id }
        )
    }

    private func refusal(
        _ id: String,
        agent: String? = "claude",
        mine: Bool = true,
        sessionEnded: Bool = false,
        deviceOnline: Bool = true,
        canResume: Bool = true,
        canSwitchAccount: Bool = true,
        turnState: AgentTurnState = .ended,
        currentAccount: String? = nil
    ) throws -> String? {
        let option = try option(id)
        return SessionAccountSwitch.refusal(
            option: option,
            agent: agent,
            mine: mine,
            sessionEnded: sessionEnded,
            deviceOnline: deviceOnline,
            canResume: canResume,
            canSwitchAccount: canSwitchAccount,
            turnState: turnState,
            currentAccount: currentAccount
        )
    }

    func testOptionsListEveryProfileTheMachineReported() throws {
        let options = SessionAccountSwitch.options(accounts: accounts, agent: "claude")
        XCTAssertEqual(options.map(\.profileId), ["me", "work", "stale", "empty"])
        XCTAssertEqual(options[0].caption, "me@acme.test")
        XCTAssertEqual(options[1].caption, "work@acme.test")
        // EXP-1013: a login that names nobody reads "No email".
        XCTAssertEqual(options[3].caption, "No email")
        XCTAssertEqual(options[2].health, .needsRelogin)
        XCTAssertTrue(options[0].active)
        XCTAssertFalse(options[1].active)
    }

    // No ambient login is ever used: an agent reporting only top-level
    // fields (no profiles) offers nothing — never a synthesized `system` row.
    func testAnAgentWithoutProfilesOffersNothing() {
        XCTAssertTrue(SessionAccountSwitch.options(accounts: accounts, agent: "codex").isEmpty)
        // Nothing reported for the agent = nothing to switch between.
        XCTAssertTrue(SessionAccountSwitch.options(accounts: accounts, agent: "gemini").isEmpty)
        XCTAssertTrue(SessionAccountSwitch.options(accounts: nil, agent: "claude").isEmpty)
        XCTAssertTrue(SessionAccountSwitch.options(accounts: accounts, agent: nil).isEmpty)
        XCTAssertTrue(SessionAccountSwitch.options(accounts: accounts, agent: "").isEmpty)
    }

    func testAnIdleClaudeRunOwnedByMeMaySwitch() throws {
        XCTAssertNil(try refusal("work"))
        // The machine's own active login is still a valid continuation target —
        // only a run KNOWN to be on it is refused.
        XCTAssertNil(try refusal("me"))
        XCTAssertEqual(
            try refusal("me", currentAccount: "me"),
            SessionAccountSwitch.reasonAlready
        )
        // A legacy `system` run account is unknown, never a row's.
        XCTAssertNil(try refusal("me", currentAccount: "system"))
        XCTAssertFalse(
            SessionAccountSwitch.options(accounts: accounts, agent: "claude", currentAccount: "system")
                .contains(where: \.current)
        )
    }

    func testCodexNeverSwitchesMidRun() throws {
        XCTAssertEqual(try refusal("work", agent: "codex"), SessionAccountSwitch.reasonAgent)
        XCTAssertEqual(try refusal("work", agent: nil), SessionAccountSwitch.reasonAgent)
        XCTAssertFalse(SessionAccountSwitch.supports(agent: "codex"))
        XCTAssertTrue(SessionAccountSwitch.supports(agent: "claude"))
    }

    func testASwitchWaitsForTheTurnAndForTheMachine() throws {
        XCTAssertEqual(
            try refusal("work", turnState: .started), SessionAccountSwitch.reasonBusy
        )
        XCTAssertEqual(
            try refusal("work", deviceOnline: false), SessionAccountSwitch.reasonOffline
        )
        XCTAssertEqual(try refusal("work", canResume: false), SessionAccountSwitch.reasonNoCap)
        // EXP-849: a machine that resumes but ignores `account` on a live run
        // would switch nothing at all, so it gets the same "update it" line.
        XCTAssertEqual(
            try refusal("work", canSwitchAccount: false), SessionAccountSwitch.reasonNoCap
        )
        XCTAssertEqual(try refusal("work", sessionEnded: true), SessionAccountSwitch.reasonEnded)
        XCTAssertEqual(try refusal("work", mine: false), SessionAccountSwitch.reasonNotMine)
    }

    func testAnUnusableAccountIsNamedNeverSilentlyOffered() throws {
        XCTAssertEqual(try refusal("stale"), SessionAccountSwitch.reasonNeedsRelogin)
        XCTAssertEqual(try refusal("empty"), SessionAccountSwitch.reasonSignedOut)
    }

    func testASwitchNamesTheAccountItTargets() throws {
        // The PRESENCE of `account` is what makes the server accept a resume of
        // a LIVE run, so the profile is always named.
        XCTAssertEqual(SessionAccountSwitch.wireAccount(try option("me")), "me")
        XCTAssertEqual(SessionAccountSwitch.wireAccount(try option("work")), "work")
    }

    // The copy every client prints around a switch and the run it produced.
    func testSwitchCopy() {
        XCTAssertEqual(SessionAccountSwitch.sectionTitle, "Accounts")
        XCTAssertEqual(SessionAccountSwitch.switchLabel, "Switch to this account")
        XCTAssertEqual(SessionAccountSwitch.wallSwitchLabel, "Switch account")
        XCTAssertEqual(
            SessionAccountSwitch.costNote,
            "The run continues under the other account. Re-reading the transcript once costs tokens."
        )
    }
}
