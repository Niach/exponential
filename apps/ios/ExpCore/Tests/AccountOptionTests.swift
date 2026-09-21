import Foundation
import XCTest
@testable import ExpCore

// EXP-872/EXP-988: the contract tests for `AccountOptions.flatten`, locked
// against the SAME fixture and the SAME test names as web
// `lib/accounts/account-option.test.ts`, desktop `account_option.rs` and
// Android `AccountOptionTest.kt`. Change a rule here, change it in all four.
final class AccountOptionTests: XCTestCase {
    /// The fixture ×4: two claude logins (work = active, home = a dead
    /// credential), one codex login, and a `system` codex profile that is
    /// signed out. The default agent is codex.
    private func fixtureAccounts() -> [String: AgentAccount] {
        [
            "claude": AgentAccount(
                signedIn: true,
                email: "work@x.test",
                profiles: [
                    AgentAccountProfile(
                        id: "work",
                        label: "Work laptop",
                        active: true,
                        signedIn: true,
                        email: "work@x.test",
                        usage: AgentUsage(
                            fetchedAt: "2026-09-19T10:00:00Z",
                            windows: [
                                AgentUsageWindow(key: "session", label: "5h", percent: 40),
                                AgentUsageWindow(key: "weekly", label: "Week", percent: 85),
                                AgentUsageWindow(key: "model:opus", label: "Opus", percent: 10),
                            ]
                        ),
                        health: "ok"
                    ),
                    AgentAccountProfile(
                        id: "home",
                        label: "Default",
                        signedIn: true,
                        email: "home@x.test",
                        health: "needs_relogin"
                    ),
                ]
            ),
            "codex": AgentAccount(
                signedIn: true,
                profiles: [
                    AgentAccountProfile(
                        id: "main",
                        label: "Main",
                        active: true,
                        signedIn: true,
                        email: "codex@x.test",
                        usage: AgentUsage(windows: [
                            AgentUsageWindow(key: "session", label: "5h", percent: 5),
                            AgentUsageWindow(key: "weekly", label: "Week", percent: 50),
                        ])
                    ),
                    AgentAccountProfile(id: "system", active: false, signedIn: false),
                ]
            ),
            // A retired agent still sitting in a synced row (EXP-849).
            "pi": AgentAccount(signedIn: true, email: "pi@x.test"),
        ]
    }

    private func fixture(
        defaults: DeviceLaunchDefaults? = DeviceLaunchDefaults(defaultAgent: "codex")
    ) -> [AccountOption] {
        AccountOptions.flatten(
            accounts: fixtureAccounts(), usage: nil, launchDefaults: defaults
        )
    }

    func testYieldsOneOptionPerSignedInLoginAcrossBothAgents() {
        XCTAssertEqual(
            fixture().map(\.key), ["codex:main", "claude:work", "claude:home"]
        )
    }

    func testLabelsEveryOptionByEmailNeverByProfileNameAndNeverDefault() {
        // A profile { id: 'work', label: 'Work laptop', email: 'a@x.test' }
        // yields email 'a@x.test'; no option's email is 'Work laptop' or
        // contains the word 'default'.
        let options = fixture()
        XCTAssertEqual(
            options.map(\.email), ["codex@x.test", "work@x.test", "home@x.test"]
        )
        for option in options {
            XCTAssertNotEqual(option.email, "Work laptop")
            XCTAssertFalse(option.email.lowercased().contains("default"))
        }
    }

    func testPutsTheDeviceDefaultFirstAndMarksExactlyOneOption() {
        // defaultAgent = 'codex' with an active codex login → that login is
        // options[0] and the only `isDeviceDefault`.
        let options = fixture()
        XCTAssertEqual(options.first?.agent, "codex")
        XCTAssertEqual(options.first?.id, "main")
        XCTAssertEqual(options.first?.isDeviceDefault, true)
        XCTAssertEqual(options.filter(\.isDeviceDefault).count, 1)
        XCTAssertEqual(AccountOptions.defaultOption(options), options.first)
    }

    func testPrefersTheStoredDefaultAccountOfTheDefaultAgent() {
        // EXP-872: "default agent" became "default account" — the device
        // stores the profile id, and it wins over the agent's ACTIVE login.
        let options = fixture(
            defaults: DeviceLaunchDefaults(defaultAgent: "claude", defaultAccount: "home")
        )
        XCTAssertEqual(options.first?.agent, "claude")
        XCTAssertEqual(options.first?.id, "home")
        XCTAssertEqual(options.first?.isDeviceDefault, true)
        XCTAssertEqual(options.filter(\.isDeviceDefault).count, 1)
        // A stored profile the device no longer reports falls back to the
        // active login.
        let gone = fixture(
            defaults: DeviceLaunchDefaults(defaultAgent: "claude", defaultAccount: "retired")
        )
        XCTAssertEqual(gone.first?.agent, "claude")
        XCTAssertEqual(gone.first?.id, "work")
    }

    func testFallsBackToTheFirstContractAgentsActiveLoginWhenNoDefaultAgentIsSet() {
        let options = fixture(defaults: nil)
        XCTAssertEqual(options.first?.agent, "claude")
        XCTAssertEqual(options.first?.id, "work")
        XCTAssertEqual(options.first?.isDeviceDefault, true)
        XCTAssertEqual(options.filter(\.isDeviceDefault).count, 1)
        // A default agent with no active login falls back the same way.
        let stale = fixture(defaults: DeviceLaunchDefaults(defaultAgent: "pi"))
        XCTAssertEqual(stale.first?.agent, "claude")
        XCTAssertEqual(stale.first?.id, "work")
    }

    func testCarriesTheAgentOnTheOptionSoAPickImpliesIt() {
        let options = fixture()
        XCTAssertEqual(options.first { $0.id == "main" }?.agent, "codex")
        XCTAssertEqual(options.first { $0.id == "home" }?.agent, "claude")
        XCTAssertEqual(options.first { $0.id == "home" }?.health, .needsRelogin)
        XCTAssertEqual(options.first?.key, "codex:main")
    }

    func testDerivesLimitsAs0To1FractionsFromTheSessionWeeklyAndFirstModelWindows() {
        // windows [{session, 40}, {weekly, 85}, {model:opus, 'Opus', 10}] →
        // limits { fiveHour: 0.4, week: 0.85, model: { 'Opus', 0.1 } }.
        let options = fixture()
        XCTAssertEqual(
            options.first { $0.id == "work" }?.limits,
            AccountLimits(
                fiveHour: 0.4,
                week: 0.85,
                model: AccountLimits.ModelLimit(label: "Opus", used: 0.1)
            )
        )
        // Codex reports no per-model window: no `model` at all.
        XCTAssertEqual(
            options.first { $0.id == "main" }?.limits,
            AccountLimits(fiveHour: 0.05, week: 0.5)
        )
    }

    func testOmitsLimitsForALoginWithNoUsageReport() {
        XCTAssertNil(fixture().first { $0.id == "home" }?.limits)
    }

    func testShowsThePlanForALoginTheDeviceReportsWithoutAnAddress() {
        let options = AccountOptions.flatten(
            accounts: [
                "claude": AgentAccount(
                    signedIn: true,
                    profiles: [
                        AgentAccountProfile(
                            id: "p1", label: "Plan only", active: true,
                            signedIn: true, plan: "Max"
                        ),
                        AgentAccountProfile(id: "p2", label: "Bare", signedIn: true),
                    ]
                )
            ],
            usage: nil,
            launchDefaults: nil
        )
        XCTAssertEqual(options.map(\.email), ["Max", "No email"])
    }

    func testYieldsTheAmbientSystemLoginForADeviceThatReportsNoProfiles() {
        let options = AccountOptions.flatten(
            accounts: ["claude": AgentAccount(signedIn: true, email: "solo@x.test")],
            usage: [
                "claude": AgentUsage(windows: [
                    AgentUsageWindow(key: "session", label: "5h", percent: 20)
                ])
            ],
            launchDefaults: nil
        )
        XCTAssertEqual(options, [
            AccountOption(
                id: "system",
                agent: "claude",
                email: "solo@x.test",
                isDeviceDefault: true,
                health: .ok,
                limits: AccountLimits(fiveHour: 0.2, week: 0)
            )
        ])
    }

    func testSkipsSignedOutLoginsAndRetiredAgents() {
        let options = fixture()
        XCTAssertFalse(options.contains { $0.id == "system" })
        XCTAssertFalse(options.contains { $0.agent == "pi" })
        XCTAssertEqual(
            AccountOptions.flatten(accounts: nil, usage: nil, launchDefaults: nil), []
        )
        XCTAssertNil(AccountOptions.defaultOption([]))
    }
}
