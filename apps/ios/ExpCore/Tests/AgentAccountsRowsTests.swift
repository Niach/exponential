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
        profileId: String = "p1",
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
    // sibling below it still wears its badge. Then attention, then the order
    // the machine sent them in (a login has no name to sort by).
    func testDeviceLoginsLeadWithTheActiveLoginInContractAgentOrder() {
        let rows = [
            row(deviceId: "d1", agent: "codex", profileId: "codex-a"),
            row(deviceId: "d1", agent: "claude", profileId: "p4", active: false),
            row(
                deviceId: "d1", agent: "claude", profileId: "p2",
                active: false, signedIn: false, health: .signedOut
            ),
            row(deviceId: "d1", agent: "claude", profileId: "p3", active: false),
            row(deviceId: "d1", agent: "claude", profileId: "p1"),
        ]
        XCTAssertEqual(
            AgentAccountsRows.sortDeviceLogins(rows).map(\.profileId),
            ["p1", "p2", "p4", "p3", "codex-a"]
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
                {"id":"p2","signedIn":true,"active":false,"email":"two@acme.test"},
                {"id":"p1","signedIn":true,"active":true,
                 "email":"dev@acme.test","plan":"max"}]}}
                """)),
            agentUsage: [:],
            agentUsageAt: "2026-08-28T09:59:00Z"
        )
        let rows = AgentAccountsRows.deviceLoginRows(device)
        XCTAssertEqual(rows.map(\.profileId), ["p1", "p2"])
        XCTAssertEqual(rows.map(\.key), ["d1:claude:p1", "d1:claude:p2"])
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
        // Neither: "No email" (EXP-1013) — the badge carries the state.
        let out = row(
            deviceId: "d1", agent: "claude", profileId: "p2",
            signedIn: false, health: .signedOut
        )
        XCTAssertEqual(AgentAccountsRows.loginLabel(out), "No email")
        XCTAssertEqual(AgentAccountsRows.healthBadge(out), "Signed out")
        XCTAssertNil(AgentAccountsRows.healthBadge(row(deviceId: "d1", agent: "claude", health: .ok)))
    }

    // MARK: - Rows off the devices shape

    // No ambient login is ever used, so nothing is synthesized: a machine
    // whose agent reports NO profiles (only top-level fields, or only usage)
    // has no row for it — never a `system` / "Default" row.
    func testAnAgentWithoutProfilesHasNoRows() throws {
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
        XCTAssertTrue(
            AgentAccountsRows.profileRows(devices: [device], currentUserId: "me", isOnline: { _ in true }).isEmpty
        )

        // A teammate's shared machine is never "mine", and the online-ness is
        // the caller's to decide.
        let theirs = DeviceEntity(
            id: "row-2",
            userId: "someone-else",
            deviceId: "dev-2",
            label: "Server",
            agentAccounts: #"{"claude":{"signedIn":true,"profiles":[{"id":"p1","signedIn":true,"checkedAt":""}]}}"#,
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
    // profile's own identity/usage, in the order the machine sent them — and
    // only the ACTIVE profile falls back to the pre-profile `agentUsage` slot
    // (those numbers are its, not the others'). A legacy `label` is ignored.
    func testAgentProfileUsageRowsReadEveryProfile() throws {
        let device = DeviceEntity(
            id: "row-1",
            userId: "me",
            deviceId: "dev-1",
            label: "Studio",
            agentAccounts: """
                {"claude":{"signedIn":true,"email":"work@acme.test","checkedAt":"2026-08-28T11:00:00.000Z",
                 "profiles":[
                   {"id":"5e5e5e5e","label":"Default","signedIn":true,"email":"work@acme.test","plan":"max","active":true,
                    "checkedAt":"2026-08-28T11:10:00.000Z","lastLoginAt":"2026-08-28T10:00:00.000Z"},
                   {"id":"0a1b2c3d","label":"Personal","signedIn":true,"email":"home@acme.test","active":false,
                    "usage":\(usageJson("2026-08-28T11:20:00.000Z", "weekly", 61))},
                   {"id":"9f9f9f9f","signedIn":false,"active":false}
                 ]}}
                """,
            agentUsage: #"{"claude":\#(usageJson("2026-08-28T11:55:00.000Z", "session", 42))}"#,
            lastSeenAt: "2026-08-28T11:59:00.000Z"
        )
        let rows = AgentAccountsRows.profileRows(devices: [device], currentUserId: "me", isOnline: { _ in true })
        XCTAssertEqual(rows.map(\.key), ["dev-1:claude:5e5e5e5e", "dev-1:claude:0a1b2c3d", "dev-1:claude:9f9f9f9f"])

        let work = rows[0]
        XCTAssertEqual(AgentAccountsRows.loginLabel(work), "work@acme.test", "named by its email, never a label")
        XCTAssertTrue(work.active)
        XCTAssertEqual(work.plan, "max")
        // The active profile without numbers of its own reads the old slot.
        XCTAssertEqual(AgentAccountsRows.peakPercent(work.usage), 42)
        XCTAssertEqual(work.checkedAt, "2026-08-28T11:10:00.000Z")

        let personal = rows[1]
        XCTAssertEqual(AgentAccountsRows.loginLabel(personal), "home@acme.test")
        XCTAssertFalse(personal.active)
        XCTAssertEqual(personal.email, "home@acme.test")
        XCTAssertEqual(AgentAccountsRows.peakPercent(personal.usage), 61)
        // No probe stamp of its own: the account's.
        XCTAssertEqual(personal.checkedAt, "2026-08-28T11:00:00.000Z")

        let old = rows[2]
        XCTAssertFalse(old.signedIn)
        // An inactive profile never inherits the active slot's numbers.
        XCTAssertNil(old.usage)
        // Never signed in, no email: "No email", never its id.
        XCTAssertEqual(AgentAccountsRows.loginLabel(old), "No email")
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
            agentAccounts: #"{"pi":{"signedIn":true,"profiles":[{"id":"p9","signedIn":true}]},"claude":{"signedIn":true,"profiles":[{"id":"p1","signedIn":true,"email":"a@acme.test"}]}}"#,
            agentUsage: "{\"pi\":\(usageJson("2026-08-28T11:55:00.000Z", "weekly", 99))}",
            lastSeenAt: "2026-08-28T11:59:00.000Z"
        )
        let rows = AgentAccountsRows.profileRows(
            devices: [stale], currentUserId: "me", isOnline: { _ in true }
        )
        XCTAssertEqual(rows.map(\.key), ["old-box:claude:p1"])
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

    // Two states, two menus (web `MachineAccountChip`, Android and the
    // desktop chips): a signed-out or expired login offers "Sign in", a
    // healthy login — the machine's last used one or not — the removal alone
    // (EXP-1158: no "make it the default" entry; a start takes the last used
    // login).
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
                active: active,
                signedIn: signedIn,
                email: nil,
                plan: nil,
                usage: nil,
                checkedAt: nil,
                health: health
            )
        }
        // EXP-944: a signed-out or expired NAMED login keeps its Sign in AND
        // offers Remove account — the removal deletes a profile dir, and the
        // credential's state never decided whether that is possible.
        let signedOut = chip(false, true, .signedOut)
        XCTAssertTrue(AgentAccountsRows.chipSignsIn(signedOut))
        XCTAssertTrue(AgentAccountsRows.canRemoveAccount(
            signedOut, canAgentLogin: true, canRemoveAccount: true
        ))

        let expired = chip(true, false, .needsRelogin)
        XCTAssertTrue(AgentAccountsRows.chipSignsIn(expired))
        XCTAssertTrue(AgentAccountsRows.canRemoveAccount(
            expired, canAgentLogin: true, canRemoveAccount: true
        ))

        let current = chip(true, true, .ok)
        XCTAssertFalse(AgentAccountsRows.chipSignsIn(current))
        XCTAssertTrue(AgentAccountsRows.canRemoveAccount(
            current, canAgentLogin: true, canRemoveAccount: true
        ))

        let other = chip(true, false, .ok)
        XCTAssertFalse(AgentAccountsRows.chipSignsIn(other))
        XCTAssertTrue(AgentAccountsRows.canRemoveAccount(
            other, canAgentLogin: true, canRemoveAccount: true
        ))
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

    // EXP-1137: a build with the sign-out body offers "Sign out" on every
    // signed-in login; the fixed order ×4 is sign in, sign out, remove (web
    // `accountChipActions`, desktop `chip_actions`, Android `chipActions`).
    func testSignOutRidesTheSignOutCap() throws {
        func chip(
            _ signedIn: Bool,
            _ active: Bool,
            _ health: AgentAccountHealth,
            profileId: String = "work"
        ) -> AgentProfileUsageRow {
            AgentProfileUsageRow(
                key: "dev:codex:\(profileId)",
                deviceId: "dev",
                deviceLabel: "mint",
                mine: true,
                online: true,
                agent: "codex",
                profileId: profileId,
                active: active,
                signedIn: signedIn,
                email: "me@example.com",
                plan: nil,
                usage: nil,
                checkedAt: nil,
                health: health
            )
        }
        XCTAssertEqual(AgentAccountsRows.signOutCap, "account-sign-out")

        let live = chip(true, true, .ok)
        XCTAssertTrue(AgentAccountsRows.chipSignsOut(
            live, canAgentLogin: true, canSignOutAccount: true
        ))
        // ...and not on an EXP-862 build, with the server's sentence.
        XCTAssertEqual(
            AgentAccountsRows.signOutBlockReason(
                live, canAgentLogin: true, canSignOutAccount: false
            ),
            AgentAccountsRows.signOutOldApp
        )
        // A signed-out login: the sign-in and the removal, nothing to sign
        // out of.
        let dead = chip(false, true, .signedOut)
        XCTAssertTrue(AgentAccountsRows.chipSignsIn(dead))
        XCTAssertFalse(AgentAccountsRows.chipSignsOut(
            dead, canAgentLogin: true, canSignOutAccount: true
        ))
        XCTAssertEqual(
            AgentAccountsRows.signOutBlockReason(
                dead, canAgentLogin: true, canSignOutAccount: true
            ),
            "That login is already signed out there."
        )
        XCTAssertTrue(AgentAccountsRows.canRemoveAccount(
            dead, canAgentLogin: true, canRemoveAccount: true
        ))
        // A revoked credential still signs out: that is how it leaves.
        XCTAssertTrue(AgentAccountsRows.chipSignsOut(
            chip(true, false, .needsRelogin), canAgentLogin: true, canSignOutAccount: true
        ))
        // The sign-out cap never removes a profile, and `agent-login` is
        // required for everything.
        XCTAssertFalse(AgentAccountsRows.canRemoveAccount(
            chip(true, false, .ok), canAgentLogin: true, canRemoveAccount: false
        ))
        XCTAssertFalse(AgentAccountsRows.chipSignsOut(
            chip(true, false, .ok), canAgentLogin: false, canSignOutAccount: true
        ))

        // The device cap, and the pinned sentence.
        let signer = SteerDevice(deviceId: "dev", deviceLabel: "dev", caps: [AgentAccountsRows.signOutCap])
        XCTAssertTrue(signer.canSignOutAccount)
        XCTAssertFalse(SteerDevice(deviceId: "dev", deviceLabel: "dev", caps: ["account-remove"]).canSignOutAccount)
        XCTAssertEqual(
            AgentAccountsRows.signOutConfirmCopy(account: "me@example.com", device: "mint"),
            "Sign me@example.com out on mint? The login stays listed so it can sign in again; the account itself is untouched."
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
        XCTAssertEqual(AgentAccountsRows.importCap, "agent-import")
        XCTAssertTrue(
            SteerDevice(deviceId: "dev", deviceLabel: "dev", caps: [AgentAccountsRows.importCap]).canImportAgent
        )
        XCTAssertFalse(withCap.canImportAgent)
    }

    // MARK: - Signing in: the landing by email

    private func profile(
        _ id: String,
        email: String? = nil,
        lastLoginAt: String? = nil
    ) -> AgentAccountProfile {
        AgentAccountProfile(id: id, signedIn: true, email: email, lastLoginAt: lastLoginAt)
    }

    private func account(_ profiles: [AgentAccountProfile]) -> AgentAccount {
        AgentAccount(signedIn: true, profiles: profiles)
    }

    // The baseline maps every reported profile to its `lastLoginAt` ("" when
    // it has none): presence counts as much as the stamp.
    func testTheBaselineRecordsEveryProfile() {
        XCTAssertEqual(AgentAccountsRows.loginBaseline(nil), [:])
        XCTAssertEqual(
            AgentAccountsRows.loginBaseline(account([
                profile("a", lastLoginAt: "2026-10-01T10:00:00Z"),
                profile("b"),
            ])),
            ["a": "2026-10-01T10:00:00Z", "b": ""]
        )
    }

    // Nothing moved: no landing, so the sheet stays open.
    func testNoLandingWhileNothingCommitted() {
        let before = account([
            profile("a", email: "a@acme.test", lastLoginAt: "2026-10-01T10:00:00Z"),
            profile("c", email: "c@acme.test"),
        ])
        let baseline = AgentAccountsRows.loginBaseline(before)
        XCTAssertNil(AgentAccountsRows.loginLanding(
            account: before, baseline: baseline, intendedProfileId: "c"
        ))
        XCTAssertNil(AgentAccountsRows.loginLanding(
            account: nil, baseline: baseline, intendedProfileId: nil
        ))
    }

    // Add account with a NEW email: a new profile id carrying a stamp lands,
    // and it is not a duplicate.
    func testANewEmailLandsOnANewProfile() {
        let baseline = AgentAccountsRows.loginBaseline(account([
            profile("a", email: "a@acme.test", lastLoginAt: "2026-10-01T10:00:00Z"),
        ]))
        let after = account([
            profile("a", email: "a@acme.test", lastLoginAt: "2026-10-01T10:00:00Z"),
            profile("n", email: "new@acme.test", lastLoginAt: "2026-10-10T12:00:00Z"),
        ])
        XCTAssertEqual(
            AgentAccountsRows.loginLanding(account: after, baseline: baseline, intendedProfileId: nil),
            AgentAccountsRows.LoginLanding(profileId: "n", email: "new@acme.test", duplicate: false)
        )
    }

    // Re-signing the tapped login refreshes IT: no warning.
    func testReSigningTheIntendedLoginIsNoDuplicate() {
        let baseline = AgentAccountsRows.loginBaseline(account([
            profile("c", email: "c@acme.test"),
        ]))
        let after = account([
            profile("c", email: "c@acme.test", lastLoginAt: "2026-10-10T12:00:00Z"),
        ])
        XCTAssertEqual(
            AgentAccountsRows.loginLanding(account: after, baseline: baseline, intendedProfileId: "c"),
            AgentAccountsRows.LoginLanding(profileId: "c", email: "c@acme.test", duplicate: false)
        )
    }

    // The contract's example: A, B, C with C expired. Sign in on C, but the
    // browser signs in as A: A is refreshed, B and C untouched, and the
    // person is warned. Never A, B, A.
    func testSigningInAsAKnownEmailRefreshesItAndWarns() {
        let before = account([
            profile("a", email: "a@acme.test", lastLoginAt: "2026-10-01T10:00:00Z"),
            profile("b", email: "b@acme.test", lastLoginAt: "2026-10-02T10:00:00Z"),
            profile("c", email: "c@acme.test", lastLoginAt: "2026-09-01T10:00:00Z"),
        ])
        let baseline = AgentAccountsRows.loginBaseline(before)
        let after = account([
            profile("a", email: "a@acme.test", lastLoginAt: "2026-10-10T12:00:00Z"),
            profile("b", email: "b@acme.test", lastLoginAt: "2026-10-02T10:00:00Z"),
            profile("c", email: "c@acme.test", lastLoginAt: "2026-09-01T10:00:00Z"),
        ])
        let landing = AgentAccountsRows.loginLanding(
            account: after, baseline: baseline, intendedProfileId: "c"
        )
        XCTAssertEqual(
            landing,
            AgentAccountsRows.LoginLanding(profileId: "a", email: "a@acme.test", duplicate: true)
        )
        XCTAssertEqual(
            landing.map(AgentAccountsRows.alreadyAddedToast),
            "a@acme.test was already added. Refreshed it."
        )
        // Add account (no intended profile) onto a known email: a duplicate too.
        XCTAssertEqual(
            AgentAccountsRows.loginLanding(account: after, baseline: baseline, intendedProfileId: nil)?
                .duplicate,
            true
        )
        // An existing profile with no stamp before is still an EXISTING one.
        let unstamped = AgentAccountsRows.loginBaseline(account([profile("a", email: "a@acme.test")]))
        XCTAssertEqual(
            AgentAccountsRows.loginLanding(
                account: account([profile("a", email: "a@acme.test", lastLoginAt: "2026-10-10T12:00:00Z")]),
                baseline: unstamped,
                intendedProfileId: nil
            )?.duplicate,
            true
        )
    }
}
