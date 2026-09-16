import Foundation
import XCTest
@testable import ExpCore

// EXP-829/EXP-909: the Devices page's per-device LOGIN rules, locked against
// the same fixtures and the same test names as web `lib/agent-usage.test.ts`
// (`deviceLoginRows` / `sortDeviceLogins` / `loginLabel`) and desktop
// `ui/src/usage_bar.rs`. One row per login under its machine, the active
// login first, attention-first ordering, the refresh floor.
final class AgentAccountsRowsTests: XCTestCase {
    private func row(
        deviceId: String,
        agent: String,
        deviceLabel: String? = nil,
        mine: Bool = true,
        online: Bool = true,
        profileId: String = "system",
        profileLabel: String = "Default",
        active: Bool = true,
        signedIn: Bool = true,
        email: String? = nil,
        plan: String? = nil,
        usage: AgentUsage? = nil,
        checkedAt: String? = nil,
        health: AgentAccountHealth = .unknown
    ) -> AgentProfileUsageRow {
        AgentProfileUsageRow(
            key: "\(deviceId):\(agent):\(profileId)",
            deviceId: deviceId,
            deviceLabel: deviceLabel ?? deviceId,
            mine: mine,
            online: online,
            agent: agent,
            profileId: profileId,
            profileLabel: profileLabel,
            active: active,
            signedIn: signedIn,
            email: email,
            plan: plan,
            usage: usage,
            checkedAt: checkedAt,
            health: health
        )
    }

    private func usage(_ fetchedAt: String, _ percent: Double, stale: Bool = false, key: String = "weekly") -> AgentUsage {
        AgentUsage(
            fetchedAt: fetchedAt,
            stale: stale,
            windows: [AgentUsageWindow(key: key, label: "Week", percent: percent, resetsAt: nil)]
        )
    }

    private func usageJson(_ fetchedAt: String, _ key: String, _ percent: Int) -> String {
        #"{"fetchedAt":"\#(fetchedAt)","stale":false,"windows":[{"key":"\#(key)","label":"Week","percent":\#(percent)}]}"#
    }

    // MARK: - One device's logins (EXP-909)

    // The Devices page lists a machine's logins under its row: claude before
    // codex (contract order), and inside an agent the machine's ACTIVE login
    // leads — this list answers "what is this machine running", and the broken
    // sibling below it still wears its badge. Then attention, then the labels.
    func testDeviceLoginsLeadWithTheActiveLoginInContractAgentOrder() {
        let rows = [
            row(deviceId: "d1", agent: "codex", profileId: "codex-a", profileLabel: "Work"),
            row(
                deviceId: "d1", agent: "claude", profileId: "p2", profileLabel: "Second",
                active: false, signedIn: false, health: .signedOut
            ),
            row(
                deviceId: "d1", agent: "claude", profileId: "p3", profileLabel: "Third",
                active: false
            ),
            row(deviceId: "d1", agent: "claude", profileId: "system", profileLabel: "Default"),
        ]
        XCTAssertEqual(
            AgentAccountsRows.sortDeviceLogins(rows).map(\.profileId),
            ["system", "p2", "p3", "codex-a"]
        )
    }

    // `deviceLoginRows` is the same derivation `profileRows` runs, entered per
    // COMPOSED device row — one entry point, one rule, already ordered.
    func testDeviceLoginRowsDeriveFromTheComposedRow() throws {
        let device = SteerDevice(
            deviceId: "d1",
            deviceLabel: "Studio",
            lastSeenAt: "2026-08-28T09:59:00Z",
            agentAccounts: try XCTUnwrap(AgentUsagePresentation.parseAccounts("""
                {"claude":{"signedIn":true,"email":"dev@acme.test","plan":"max","profiles":[
                {"id":"p2","label":"Second","signedIn":true,"active":false,"email":"two@acme.test"},
                {"id":"system","label":"Default","signedIn":true,"active":true,
                 "email":"dev@acme.test","plan":"max"}]}}
                """)),
            agentUsage: [:],
            agentUsageAt: "2026-08-28T09:59:00Z"
        )
        let rows = AgentAccountsRows.deviceLoginRows(device)
        XCTAssertEqual(rows.map(\.profileId), ["system", "p2"])
        XCTAssertEqual(rows.map(\.key), ["d1:claude:system", "d1:claude:p2"])
        XCTAssertTrue(rows.allSatisfy { $0.mine && $0.online })
        XCTAssertTrue(AgentAccountsRows.deviceLoginRows(
            SteerDevice(deviceId: "d2", deviceLabel: "Bare")
        ).isEmpty)
    }

    // The login row's title says WHO, never how it is doing: the brand mark
    // names the agent, the device row above names the machine, and the badge
    // says whether the login still works.
    func testTheLoginLabelIsTheIdentityNeverTheStatus() {
        XCTAssertEqual(
            AgentAccountsRows.loginLabel(
                row(deviceId: "d1", agent: "claude", email: "dev@acme.test", plan: "max")
            ),
            "dev@acme.test"
        )
        // No email: the bare plan (an agent that names a provider).
        XCTAssertEqual(
            AgentAccountsRows.loginLabel(row(deviceId: "d1", agent: "zed", plan: "anthropic (oauth)")),
            "anthropic (oauth)"
        )
        // Neither: the profile's own label — and a SIGNED-OUT login still
        // reads as itself, with the badge carrying the state.
        let out = row(
            deviceId: "d1", agent: "claude", profileId: "p2", profileLabel: "Second",
            signedIn: false, health: .signedOut
        )
        XCTAssertEqual(AgentAccountsRows.loginLabel(out), "Second")
        XCTAssertEqual(AgentAccountsRows.healthBadge(out), "Signed out")
        XCTAssertNil(AgentAccountsRows.healthBadge(row(deviceId: "d1", agent: "claude", health: .ok)))
    }

    // MARK: - Rows off the devices shape

    // A machine that reports no PROFILES (an older build, or a single-login
    // install) still gets exactly one row per agent: the ambient `system`
    // profile, labelled "Default", carrying the top-level account and the
    // pre-profile `agentUsage` slot. An agent that reported ONLY usage — no
    // account at all — still gets its row, signed out, dated by the row's
    // `agent_usage_at`.
    func testAgentProfileUsageRowsFallBackToTheSystemProfile() throws {
        let device = DeviceEntity(
            id: "row-1",
            userId: "me",
            deviceId: "dev-1",
            label: "Studio",
            agentAccounts: #"{"claude":{"signedIn":true,"email":"dev@acme.test","plan":"max","checkedAt":"2026-08-28T11:00:00.000Z"}}"#,
            agentUsage: """
                {"claude":\(usageJson("2026-08-28T11:55:00.000Z", "session", 42)),
                 "codex":\(usageJson("2026-08-28T11:55:00.000Z", "weekly", 8))}
                """,
            agentUsageAt: "2026-08-28T11:30:00.000Z",
            lastSeenAt: "2026-08-28T11:59:00.000Z"
        )
        let rows = AgentAccountsRows.sortDeviceLogins(
            AgentAccountsRows.profileRows(devices: [device], currentUserId: "me", isOnline: { _ in true })
        )
        // Contract agent order: claude, then the codex row that reported
        // numbers but no account at all.
        XCTAssertEqual(rows.map(\.key), ["dev-1:claude:system", "dev-1:codex:system"])

        let claude = rows[0]
        XCTAssertEqual(claude.agent, "claude")
        XCTAssertEqual(claude.profileId, AgentAccountsRows.systemProfileId)
        XCTAssertEqual(claude.profileLabel, "Default")
        XCTAssertTrue(claude.active, "the ambient login is always the active one")
        XCTAssertTrue(claude.mine)
        XCTAssertTrue(claude.online)
        XCTAssertTrue(claude.signedIn)
        XCTAssertEqual(claude.deviceLabel, "Studio")
        XCTAssertEqual(claude.email, "dev@acme.test")
        XCTAssertEqual(claude.plan, "max")
        XCTAssertEqual(AgentAccountsRows.peakPercent(claude.usage), 42)
        // The account's own probe stamp wins over the row's usage stamp.
        XCTAssertEqual(claude.checkedAt, "2026-08-28T11:00:00.000Z")

        let codex = rows[1]
        XCTAssertFalse(codex.signedIn)
        XCTAssertNil(codex.email)
        XCTAssertEqual(AgentAccountsRows.peakPercent(codex.usage), 8)
        // No account to date it: the device's `agent_usage_at` is the fallback.
        XCTAssertEqual(codex.checkedAt, "2026-08-28T11:30:00.000Z")

        // A teammate's shared machine is never "mine", and the online-ness is
        // the caller's to decide.
        let theirs = DeviceEntity(
            id: "row-2",
            userId: "someone-else",
            deviceId: "dev-2",
            label: "Server",
            agentAccounts: #"{"claude":{"signedIn":true,"checkedAt":""}}"#,
            sharedTeamIds: ["team-1"]
        )
        let shared = AgentAccountsRows.profileRows(devices: [theirs], currentUserId: "me", isOnline: { _ in false })
        XCTAssertEqual(shared.count, 1)
        XCTAssertFalse(shared[0].mine)
        XCTAssertFalse(shared[0].online)
        // An empty `checkedAt` is nothing to say, never an "as of " with no date.
        XCTAssertNil(shared[0].checkedAt)

        // A machine that reported nothing at all contributes no rows.
        let quiet = DeviceEntity(id: "row-3", userId: "me", deviceId: "dev-3", label: "Quiet")
        XCTAssertTrue(AgentAccountsRows.profileRows(devices: [quiet], currentUserId: "me", isOnline: { _ in true }).isEmpty)
    }

    // EXP-747 B5: with profiles the section renders ONE row each — the
    // profile's own label/identity/usage — and only the ACTIVE profile falls
    // back to the pre-profile `agentUsage` slot (those numbers are its, not
    // the others').
    func testAgentProfileUsageRowsReadEveryProfile() throws {
        let device = DeviceEntity(
            id: "row-1",
            userId: "me",
            deviceId: "dev-1",
            label: "Studio",
            agentAccounts: """
                {"claude":{"signedIn":true,"email":"work@acme.test","checkedAt":"2026-08-28T11:00:00.000Z",
                 "profiles":[
                   {"id":"system","label":"","signedIn":true,"email":"work@acme.test","plan":"max","active":true,
                    "checkedAt":"2026-08-28T11:10:00.000Z"},
                   {"id":"0a1b2c3d","label":"Personal","signedIn":true,"email":"home@acme.test","active":false,
                    "usage":\(usageJson("2026-08-28T11:20:00.000Z", "weekly", 61))},
                   {"id":"9f9f9f9f","signedIn":false,"active":false}
                 ]}}
                """,
            agentUsage: #"{"claude":\#(usageJson("2026-08-28T11:55:00.000Z", "session", 42))}"#,
            lastSeenAt: "2026-08-28T11:59:00.000Z"
        )
        let rows = AgentAccountsRows.profileRows(devices: [device], currentUserId: "me", isOnline: { _ in true })
        XCTAssertEqual(rows.map(\.key), ["dev-1:claude:system", "dev-1:claude:0a1b2c3d", "dev-1:claude:9f9f9f9f"])

        let system = rows[0]
        XCTAssertEqual(system.profileLabel, "Default", "an empty label on the system profile reads Default")
        XCTAssertTrue(system.active)
        XCTAssertEqual(system.plan, "max")
        // The active profile without numbers of its own reads the old slot.
        XCTAssertEqual(AgentAccountsRows.peakPercent(system.usage), 42)
        XCTAssertEqual(system.checkedAt, "2026-08-28T11:10:00.000Z")

        let personal = rows[1]
        XCTAssertEqual(personal.profileLabel, "Personal")
        XCTAssertFalse(personal.active)
        XCTAssertEqual(personal.email, "home@acme.test")
        XCTAssertEqual(AgentAccountsRows.peakPercent(personal.usage), 61)
        // No probe stamp of its own: the account's.
        XCTAssertEqual(personal.checkedAt, "2026-08-28T11:00:00.000Z")

        let old = rows[2]
        XCTAssertFalse(old.signedIn)
        // An inactive profile never inherits the ambient slot's numbers.
        XCTAssertNil(old.usage)
        // A profile with no label and a non-system id is named by its id.
        XCTAssertEqual(old.profileLabel, "9f9f9f9f")
    }

    func testPeakPercentIsTheFullestWindow() {
        XCTAssertEqual(AgentAccountsRows.peakPercent(nil), 0)
        XCTAssertEqual(AgentAccountsRows.peakPercent(AgentUsage(windows: [])), 0)
        let report = AgentUsage(windows: [
            AgentUsageWindow(key: "session", label: "5h", percent: 12),
            AgentUsageWindow(key: "weekly", label: "Week", percent: 78),
            AgentUsageWindow(key: "credits", label: "Credits", percent: nil),
        ])
        XCTAssertEqual(AgentAccountsRows.peakPercent(report), 78)
    }

    // The floor: a fetch younger than five minutes names the next allowed
    // time; an older one (or none at all) allows a refresh right now; a
    // stamp from the future counts as just fetched.
    func testARefreshInsideTheFloorIsRefused() throws {
        let now = try XCTUnwrap(WireTimestamps.parse("2026-08-28T12:00:00.000Z"))
        XCTAssertNil(AgentAccountsRows.refreshAllowedAt(nil, now: now))
        XCTAssertNil(AgentAccountsRows.refreshAllowedAt(AgentUsage(fetchedAt: nil), now: now))
        XCTAssertNil(AgentAccountsRows.refreshAllowedAt(AgentUsage(fetchedAt: "garbage"), now: now))
        XCTAssertNil(AgentAccountsRows.refreshAllowedAt(usage("2026-08-28T11:54:59.000Z", 1), now: now))
        XCTAssertEqual(
            AgentAccountsRows.refreshAllowedAt(usage("2026-08-28T11:58:00.000Z", 1), now: now),
            WireTimestamps.parse("2026-08-28T12:03:00.000Z")
        )
        XCTAssertEqual(
            AgentAccountsRows.refreshAllowedAt(usage("2026-08-28T12:01:00.000Z", 1), now: now),
            WireTimestamps.parse("2026-08-28T12:06:00.000Z")
        )
    }

    // MARK: - One group per account (EXP-817)

    // MARK: - The section's layout rules

    // MARK: - EXP-849: a retired agent id never renders

    // A desktop below the version floor keeps heart-beating `pi` in every one
    // of these columns; nothing this build draws may name an agent it has no
    // label, glyph or launcher for — not a row, not a tab, not a chip.
    func testAnAgentOutsideTheContractIsNeverARow() throws {
        let stale = DeviceEntity(
            id: "row-9",
            userId: "me",
            deviceId: "old-box",
            label: "Old box",
            agentAccounts: #"{"pi":{"signedIn":true,"email":"pi@acme.test"},"claude":{"signedIn":true,"email":"a@acme.test"}}"#,
            agentUsage: "{\"pi\":\(usageJson("2026-08-28T11:55:00.000Z", "weekly", 99))}",
            lastSeenAt: "2026-08-28T11:59:00.000Z"
        )
        let rows = AgentAccountsRows.profileRows(
            devices: [stale], currentUserId: "me", isOnline: { _ in true }
        )
        XCTAssertEqual(rows.map(\.key), ["old-box:claude:system"])
        // The machine row draws the same filtered set of logins.
        XCTAssertEqual(
            AgentAccountsRows.deviceRows(rows, deviceId: "old-box").map(\.agent), ["claude"]
        )
    }

    // The same drop, one layer down: the synced row → SteerDevice mapping every
    // picker and machine row reads. NULL `agents` still means "an older sender
    // that runs claude"; only KNOWN ids survive the filter.
    func testRetiredAgentIdsDropOutOfTheDeviceMapping() throws {
        let stale = DeviceEntity(
            id: "row-9",
            userId: "me",
            deviceId: "old-box",
            label: "Old box",
            agents: #"["claude","pi"]"#,
            unauthedAgents: #"["pi"]"#,
            acpAgents: #"["claude","pi"]"#,
            launchDefaults: #"{"defaultAgent":"pi","agents":{"pi":{"model":"pi-1"},"claude":{"model":"opus"}}}"#,
            agentAccounts: #"{"pi":{"signedIn":true},"claude":{"signedIn":true}}"#,
            agentUsage: #"{"pi":{"fetchedAt":"2026-08-28T11:00:00Z","windows":[]}}"#,
            lastSeenAt: "2026-08-28T11:59:00.000Z"
        )
        let device = SteerDevice(entity: stale, currentUserId: "me")
        XCTAssertEqual(device.agents, ["claude"])
        XCTAssertEqual(device.unauthedAgentIds, [])
        XCTAssertEqual(device.acpAgentIds, ["claude"])
        XCTAssertNil(device.launchDefaults?.defaultAgent)
        XCTAssertEqual(device.launchDefaults?.agents?.keys.sorted(), ["claude"])
        XCTAssertEqual(device.agentAccounts?.keys.sorted(), ["claude"])
        XCTAssertEqual(device.agentUsage?.keys.sorted(), [])
    }
    // MARK: - EXP-862: the chip menu

    // Three states, three menus (web `MachineAccountChip`, Android and the
    // desktop chips): a signed-out or expired login offers ONLY "Sign in", a
    // healthy login the machine is not using offers "Set as default" plus
    // "Remove account", and the machine's current login offers the removal
    // alone.
    func testTheChipMenuOffersOneMenuPerState() throws {
        func chip(
            _ signedIn: Bool,
            _ active: Bool,
            _ health: AgentAccountHealth,
            profileId: String = "work"
        ) -> AgentProfileUsageRow {
            AgentProfileUsageRow(
                key: "dev:claude:\(profileId)",
                deviceId: "dev",
                deviceLabel: "dev",
                mine: true,
                online: true,
                agent: "claude",
                profileId: profileId,
                profileLabel: "Work",
                active: active,
                signedIn: signedIn,
                email: nil,
                plan: nil,
                usage: nil,
                checkedAt: nil,
                health: health
            )
        }
        let signedOut = chip(false, true, .signedOut)
        XCTAssertTrue(AgentAccountsRows.chipSignsIn(signedOut))
        XCTAssertFalse(AgentAccountsRows.chipSetsDefault(signedOut, canSwitchAccount: true))
        XCTAssertFalse(AgentAccountsRows.canRemoveAccount(
            signedOut, canAgentLogin: true, canRemoveAccount: true
        ))

        let expired = chip(true, false, .needsRelogin)
        XCTAssertTrue(AgentAccountsRows.chipSignsIn(expired))
        XCTAssertFalse(AgentAccountsRows.chipSetsDefault(expired, canSwitchAccount: true))
        XCTAssertFalse(AgentAccountsRows.canRemoveAccount(
            expired, canAgentLogin: true, canRemoveAccount: true
        ))

        let current = chip(true, true, .ok)
        XCTAssertFalse(AgentAccountsRows.chipSignsIn(current))
        XCTAssertFalse(AgentAccountsRows.chipSetsDefault(current, canSwitchAccount: true))
        XCTAssertTrue(AgentAccountsRows.canRemoveAccount(
            current, canAgentLogin: true, canRemoveAccount: true
        ))

        let other = chip(true, false, .ok)
        XCTAssertFalse(AgentAccountsRows.chipSignsIn(other))
        XCTAssertTrue(AgentAccountsRows.chipSetsDefault(other, canSwitchAccount: true))
        XCTAssertTrue(AgentAccountsRows.canRemoveAccount(
            other, canAgentLogin: true, canRemoveAccount: true
        ))

        // The ambient login is the agent CLI's own: never removable.
        let ambient = chip(true, false, .ok, profileId: AgentAccountsRows.systemProfileId)
        XCTAssertFalse(AgentAccountsRows.canRemoveAccount(
            ambient, canAgentLogin: true, canRemoveAccount: true
        ))
    }

    // "Set as default" is a CAPABILITY, not just a state: `agent_profile_use`
    // shipped in desktop/CLI 0.14.38 and the server answers
    // PRECONDITION_FAILED below it, so a machine without the `account-switch`
    // cap is never offered it.
    func testSetAsDefaultNeedsTheAccountSwitchCap() throws {
        let other = AgentProfileUsageRow(
            key: "dev:claude:work",
            deviceId: "dev",
            deviceLabel: "dev",
            mine: true,
            online: true,
            agent: "claude",
            profileId: "work",
            profileLabel: "Work",
            active: false,
            signedIn: true,
            email: "dev@acme.test",
            plan: nil,
            usage: nil,
            checkedAt: nil,
            health: .ok
        )
        XCTAssertFalse(AgentAccountsRows.chipSetsDefault(other, canSwitchAccount: false))
        XCTAssertTrue(AgentAccountsRows.chipSetsDefault(other, canSwitchAccount: true))
    }

    // EXP-862: the removal is capped too, and refused for the same three
    // reasons the server gives — same sentences, so a raced downgrade reads
    // the same thing twice.
    func testRemoveAccountIsCappedAndExplained() throws {
        let row = AgentProfileUsageRow(
            key: "dev:claude:work",
            deviceId: "dev",
            deviceLabel: "dev",
            mine: true,
            online: true,
            agent: "claude",
            profileId: "work",
            profileLabel: "Work",
            active: false,
            signedIn: true,
            email: "dev@acme.test",
            plan: nil,
            usage: nil,
            checkedAt: nil,
            health: .ok
        )
        XCTAssertNil(AgentAccountsRows.removeAccountBlockReason(
            row, canAgentLogin: true, canRemoveAccount: true
        ))
        XCTAssertEqual(
            AgentAccountsRows.removeAccountBlockReason(
                row, canAgentLogin: true, canRemoveAccount: false
            ),
            AgentAccountsRows.removeAccountOldApp
        )
        XCTAssertEqual(
            AgentAccountsRows.removeAccountBlockReason(
                row, canAgentLogin: false, canRemoveAccount: true
            ),
            AgentAccountsRows.removeAccountOldApp
        )
        XCTAssertEqual(AgentAccountsRows.removeCap, "account-remove")
        XCTAssertEqual(
            AgentAccountsRows.removeAccountConfirmCopy(
                account: "work@example.com", device: "Mac mini"
            ),
            "Delete work@example.com on Mac mini? The login is removed from this device only; the account itself is untouched."
        )
    }

    // The cap the rule reads is the one `SteerDevice.canSwitchAccount` looks
    // for — locked so a rename cannot silently disable the switch everywhere.
    func testTheSwitchCapIsTheDeviceCap() throws {
        XCTAssertEqual(AgentAccountsRows.switchCap, "account-switch")
        let withCap = SteerDevice(
            deviceId: "dev",
            deviceLabel: "dev",
            caps: [AgentAccountsRows.switchCap]
        )
        XCTAssertTrue(withCap.canSwitchAccount)
        XCTAssertFalse(SteerDevice(deviceId: "dev", deviceLabel: "dev", caps: ["agent-login"]).canSwitchAccount)
        let remover = SteerDevice(
            deviceId: "dev",
            deviceLabel: "dev",
            caps: [AgentAccountsRows.removeCap]
        )
        XCTAssertTrue(remover.canRemoveAccount)
        XCTAssertFalse(withCap.canRemoveAccount)
    }

    // MARK: - Adding a login (EXP-862)

    private func account(
        signedIn: Bool? = nil,
        email: String? = nil,
        profiles: [AgentAccountProfile]? = nil
    ) -> AgentAccount {
        AgentAccount(signedIn: signedIn, email: email, profiles: profiles)
    }

    // Web `addAccountLoginTarget`, name for name: the ambient config dir only
    // while nothing lives in it. Sending NEITHER field is what the device
    // reads as its ambient login, so a taken one must always name a new
    // profile instead of signing a second account on top of the first.
    func testAddAccountLoginTargetUsesTheAmbientLoginWhileItIsSignedOut() {
        XCTAssertEqual(
            AgentAccountsRows.addAccountLoginTarget(nil, label: "X"),
            AgentAccountsRows.AddLoginTarget(profileId: "system", newProfileLabel: nil)
        )
        XCTAssertEqual(
            AgentAccountsRows.addAccountLoginTarget(account(signedIn: false), label: "X"),
            AgentAccountsRows.AddLoginTarget(profileId: "system", newProfileLabel: nil)
        )
        // A machine whose ACTIVE login is a profile, with the ambient dir
        // still free: the ambient one takes it, top-level flag or not.
        XCTAssertEqual(
            AgentAccountsRows.addAccountLoginTarget(
                account(
                    signedIn: true,
                    profiles: [
                        AgentAccountProfile(id: "system", signedIn: false),
                        AgentAccountProfile(id: "p1", active: true, signedIn: true),
                    ]
                ),
                label: "X"
            ),
            AgentAccountsRows.AddLoginTarget(profileId: "system", newProfileLabel: nil)
        )
    }

    func testAddAccountLoginTargetCreatesANewProfileOnceTheAmbientLoginIsTaken() {
        let taken = account(signedIn: true, email: "a@b.c")
        XCTAssertEqual(
            AgentAccountsRows.addAccountLoginTarget(taken, label: "  a@b.c "),
            AgentAccountsRows.AddLoginTarget(profileId: nil, newProfileLabel: "a@b.c")
        )
        XCTAssertEqual(
            AgentAccountsRows.addAccountLoginTarget(
                taken, label: String(repeating: "x", count: 80)
            ).newProfileLabel?.count,
            64
        )
    }

    private func accountWithProfiles(_ labels: [String]) -> AgentAccount {
        account(
            signedIn: true,
            profiles: [AgentAccountProfile(id: "system", label: "Default", signedIn: true)]
                + labels.enumerated().map { i, label in
                    AgentAccountProfile(id: "p\(i + 1)", label: label, signedIn: true)
                }
        )
    }

    // `nextProfileLabel` picks the smallest free number ≥ 2 (web/Android).
    func testNextProfileLabelStartsAtTwoWithOnlyTheAmbientLogin() {
        XCTAssertEqual(
            AgentAccountsRows.nextProfileLabel(nil, agentLabel: "Claude Code"),
            "Claude Code account 2"
        )
        XCTAssertEqual(
            AgentAccountsRows.nextProfileLabel(accountWithProfiles([]), agentLabel: "Codex"),
            "Codex account 2"
        )
    }

    func testNextProfileLabelSkipsTheNumbersStillInUse() {
        XCTAssertEqual(
            AgentAccountsRows.nextProfileLabel(
                accountWithProfiles(["Codex account 2", "Codex account 3"]),
                agentLabel: "Codex"
            ),
            "Codex account 4"
        )
    }

    // "account 2" was removed: counting would say "account 3", which is
    // already a usable profile, and the sign-in sheet would land at open.
    func testNextProfileLabelReusesAGapInsteadOfReissuingATakenLabel() {
        XCTAssertEqual(
            AgentAccountsRows.nextProfileLabel(
                accountWithProfiles(["Claude Code account 3"]),
                agentLabel: "Claude Code"
            ),
            "Claude Code account 2"
        )
    }

    func testNextProfileLabelMatchesLabelsExactly() {
        XCTAssertEqual(
            AgentAccountsRows.nextProfileLabel(
                accountWithProfiles(["claude code account 2", "Codex account 2"]),
                agentLabel: "Claude Code"
            ),
            "Claude Code account 2"
        )
    }

    // The sign-in sheet closes on the TRANSITION into "landed", so a new
    // profile must not read as landed off the ambient flag that was already
    // true when the flow started (web `agentLoginLanded`).
    func testANewProfileLandsOnItsOwnLabelNotTheAmbientFlag() {
        let before = account(
            signedIn: true,
            email: "one@acme.test",
            profiles: [AgentAccountProfile(id: "system", signedIn: true)]
        )
        XCTAssertFalse(AgentAccountsRows.loginLanded(
            account: before, profileId: nil, newProfileLabel: "two@acme.test"
        ))
        let after = account(
            signedIn: true,
            email: "one@acme.test",
            profiles: [
                AgentAccountProfile(id: "system", signedIn: true),
                AgentAccountProfile(id: "p2", label: "two@acme.test", signedIn: true),
            ]
        )
        XCTAssertTrue(AgentAccountsRows.loginLanded(
            account: after, profileId: nil, newProfileLabel: " two@acme.test "
        ))
        // A new profile that is still signing in, or one the agent refused the
        // moment it was made, has not landed.
        let pending = account(
            signedIn: true,
            profiles: [
                AgentAccountProfile(id: "system", signedIn: true),
                AgentAccountProfile(id: "p2", label: "two@acme.test", signedIn: false),
            ]
        )
        XCTAssertFalse(AgentAccountsRows.loginLanded(
            account: pending, profileId: nil, newProfileLabel: "two@acme.test"
        ))
        let refused = account(
            signedIn: true,
            profiles: [
                AgentAccountProfile(id: "system", signedIn: true),
                AgentAccountProfile(
                    id: "p2",
                    label: "two@acme.test",
                    signedIn: true,
                    health: "needs_relogin"
                ),
            ]
        )
        XCTAssertFalse(AgentAccountsRows.loginLanded(
            account: refused, profileId: nil, newProfileLabel: "two@acme.test"
        ))
    }

    // The other two paths: an existing profile watches its OWN state, the
    // ambient login its `system` entry (never the top-level flag a signed-in
    // PROFILE also sets).
    func testAnExistingLoginLandsOnItsOwnState() {
        let entry = account(
            signedIn: true,
            profiles: [
                AgentAccountProfile(id: "system", signedIn: false),
                AgentAccountProfile(id: "p1", signedIn: true),
            ]
        )
        XCTAssertTrue(AgentAccountsRows.loginLanded(
            account: entry, profileId: "p1", newProfileLabel: nil
        ))
        XCTAssertFalse(AgentAccountsRows.loginLanded(
            account: entry, profileId: "p2", newProfileLabel: nil
        ))
        XCTAssertFalse(AgentAccountsRows.loginLanded(
            account: entry, profileId: "system", newProfileLabel: nil
        ))
        XCTAssertTrue(AgentAccountsRows.loginLanded(
            account: account(signedIn: true), profileId: "system", newProfileLabel: nil
        ))
        XCTAssertFalse(AgentAccountsRows.loginLanded(
            account: nil, profileId: nil, newProfileLabel: nil
        ))
    }
}
