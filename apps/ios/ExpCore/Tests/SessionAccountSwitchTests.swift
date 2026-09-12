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
                    id: "system",
                    active: true,
                    signedIn: true,
                    email: "me@acme.test",
                    plan: "max",
                    health: "ok"
                ),
                AgentAccountProfile(
                    id: "work",
                    label: "Work",
                    signedIn: true,
                    email: "work@acme.test",
                    health: "ok"
                ),
                AgentAccountProfile(
                    id: "stale", label: "Old", signedIn: true, health: "needs_relogin"
                ),
                AgentAccountProfile(id: "empty", label: "Spare", signedIn: false),
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
            turnState: turnState,
            currentAccount: currentAccount
        )
    }

    func testOptionsListEveryProfileTheMachineReported() throws {
        let options = SessionAccountSwitch.options(accounts: accounts, agent: "claude")
        XCTAssertEqual(options.map(\.profileId), ["system", "work", "stale", "empty"])
        XCTAssertEqual(options[0].label, "Default")
        XCTAssertEqual(options[1].label, "Work")
        XCTAssertEqual(options[1].caption, "work@acme.test")
        // The label is the last resort when a login names nobody.
        XCTAssertEqual(options[3].caption, "Spare")
        XCTAssertEqual(options[2].health, .needsRelogin)
        XCTAssertTrue(options[0].active)
        XCTAssertFalse(options[1].active)
    }

    func testAPreProfileMachineOffersItsOneAmbientAccount() {
        let options = SessionAccountSwitch.options(accounts: accounts, agent: "codex")
        XCTAssertEqual(options.count, 1)
        XCTAssertEqual(options[0].profileId, "system")
        XCTAssertEqual(options[0].caption, "cx@acme.test")
        XCTAssertEqual(options[0].health, .ok)
        XCTAssertTrue(options[0].active)
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
        XCTAssertNil(try refusal("system"))
        XCTAssertEqual(
            try refusal("system", currentAccount: "system"),
            SessionAccountSwitch.reasonAlready
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
        XCTAssertEqual(try refusal("work", sessionEnded: true), SessionAccountSwitch.reasonEnded)
        XCTAssertEqual(try refusal("work", mine: false), SessionAccountSwitch.reasonNotMine)
    }

    func testAnUnusableAccountIsNamedNeverSilentlyOffered() throws {
        XCTAssertEqual(try refusal("stale"), SessionAccountSwitch.reasonNeedsRelogin)
        XCTAssertEqual(try refusal("empty"), SessionAccountSwitch.reasonSignedOut)
    }

    func testTheAmbientLoginIsNeverNamedOnTheWire() throws {
        let ambient = try option("system")
        let work = try option("work")
        XCTAssertNil(SessionAccountSwitch.wireAccount(ambient))
        XCTAssertEqual(SessionAccountSwitch.wireAccount(work), "work")
    }

    // The copy every client prints around a switch and the run it produced.
    func testSwitchCopy() {
        XCTAssertEqual(SessionAccountSwitch.sectionTitle, "Accounts")
        XCTAssertEqual(SessionAccountSwitch.switchLabel, "Switch to this account")
        XCTAssertEqual(SessionAccountSwitch.wallSwitchLabel, "Switch account")
        XCTAssertEqual(
            SessionAccountSwitch.costNote,
            "Switching continues this run under the other account. The agent re-reads the "
                + "transcript once, which costs tokens."
        )
        XCTAssertEqual(SessionAccountSwitch.continuationNote, "Continues an earlier run")
        XCTAssertEqual(
            SessionAccountSwitch.continuationCostNote,
            "The agent re-read the transcript once to pick it up — a one-time cost."
        )
    }
}
