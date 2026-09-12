import Foundation
import XCTest
@testable import ExpCore

// EXP-829: the Devices page's Accounts section rules, locked against the same
// fixtures and the same test names as web `lib/agent-usage.test.ts`
// (`accountUsageGroups`) and desktop `ui/src/usage_bar.rs` /
// `accounts_section.rs`. One row per ACCOUNT, the freshest machine's numbers,
// chips online-first, attention-first ordering, the refresh floor.
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
        checkedAt: String? = nil
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
            checkedAt: checkedAt
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
        let rows = AgentAccountsRows.sortAttentionFirst(
            AgentAccountsRows.profileRows(devices: [device], currentUserId: "me", isOnline: { _ in true })
        )
        // Signed-out rows lead: codex reported numbers but no account.
        XCTAssertEqual(rows.map(\.key), ["dev-1:codex:system", "dev-1:claude:system"])

        let claude = rows[1]
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

        let codex = rows[0]
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

    func testAttentionFirstLeadsWithTheSignedOutRows() {
        let rows = AgentAccountsRows.sortAttentionFirst([
            row(deviceId: "b", agent: "claude", email: "low@acme.test", usage: usage("2026-08-28T11:00:00.000Z", 10)),
            row(deviceId: "a", agent: "codex", email: "hot@acme.test", usage: usage("2026-08-28T11:00:00.000Z", 96)),
            row(deviceId: "a", agent: "claude", signedIn: false),
            row(deviceId: "a", agent: "claude", profileId: "work", email: "mid@acme.test", usage: usage("2026-08-28T11:00:00.000Z", 60)),
        ])
        XCTAssertEqual(rows.map(\.key), [
            "a:claude:system",
            "a:codex:system",
            "a:claude:work",
            "b:claude:system",
        ])
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

    func testMergesTheSameEmailAcrossMachinesFreshestReportFirst() throws {
        let groups = AgentAccountsRows.accountGroups(
            [
                row(
                    deviceId: "server", agent: "claude", online: false,
                    email: "Dev@Acme.test", plan: "max",
                    usage: usage("2026-08-26T10:00:00.000Z", 69),
                    checkedAt: "2026-08-27T10:00:00.000Z"
                ),
                row(
                    deviceId: "macbook", agent: "claude",
                    email: "dev@acme.test",
                    usage: usage("2026-08-28T11:00:00.000Z", 75),
                    checkedAt: "2026-08-28T11:00:00.000Z"
                ),
                row(
                    deviceId: "mint", agent: "claude",
                    email: "other@acme.test",
                    usage: usage("2026-08-28T11:30:00.000Z", 46)
                ),
            ],
            canRefresh: { _ in false }
        )
        XCTAssertEqual(groups.map(\.key), ["claude:dev@acme.test", "claude:other@acme.test"])
        let shared = try XCTUnwrap(groups.first)
        // The chips: online machines lead.
        XCTAssertEqual(shared.rows.map(\.deviceId), ["macbook", "server"])
        // The numbers are the FRESHEST member's, the plan the first one named.
        XCTAssertEqual(shared.usage?.windows?.first?.percent, 75)
        XCTAssertEqual(shared.plan, "max")
        XCTAssertEqual(shared.checkedAt, "2026-08-28T11:00:00.000Z")
        XCTAssertNil(shared.refreshTarget)
    }

    func testKeepsEmailLessAndSignedOutRowsApart() throws {
        let groups = AgentAccountsRows.accountGroups(
            [
                row(deviceId: "a", agent: "zed", plan: "openai-codex (oauth)"),
                row(deviceId: "b", agent: "zed", plan: "openai-codex (oauth)"),
                row(deviceId: "a", agent: "claude", signedIn: false, email: "x@y.z"),
                row(deviceId: "b", agent: "claude", signedIn: false),
            ],
            canRefresh: { _ in false }
        )
        XCTAssertEqual(groups.map(\.key), [
            "zed:a:system",
            "zed:b:system",
            "claude:a:system",
            "claude:b:system",
        ])
        XCTAssertFalse(groups[2].signedIn)
    }

    func testPrefersANonStaleReportOnATieAndAReportWithWindowsOverNone() throws {
        let groups = AgentAccountsRows.accountGroups(
            [
                row(deviceId: "a", agent: "claude", email: "dev@acme.test", usage: usage("2026-08-28T11:00:00.000Z", 10, stale: true)),
                row(deviceId: "b", agent: "claude", email: "dev@acme.test", usage: usage("2026-08-28T11:00:00.000Z", 20)),
                row(deviceId: "c", agent: "claude", email: "dev@acme.test"),
            ],
            canRefresh: { _ in false }
        )
        XCTAssertEqual(groups.count, 1)
        XCTAssertEqual(groups[0].usage?.windows?.first?.percent, 20)

        // Same stamp, both current: the one with windows beats the empty one.
        let empty = AgentUsage(fetchedAt: "2026-08-28T11:00:00.000Z", stale: false, windows: [])
        let withWindows = AgentAccountsRows.accountGroups(
            [
                row(deviceId: "a", agent: "claude", email: "dev@acme.test", usage: empty),
                row(deviceId: "b", agent: "claude", email: "dev@acme.test", usage: usage("2026-08-28T11:00:00.000Z", 5)),
            ],
            canRefresh: { _ in false }
        )
        XCTAssertEqual(withWindows[0].usage?.windows?.first?.percent, 5)
    }

    func testTargetsTheRefreshAtTheEligibleMemberWithTheFreshestNumbers() throws {
        let groups = AgentAccountsRows.accountGroups(
            [
                row(deviceId: "stale-but-capable", agent: "claude", email: "dev@acme.test", usage: usage("2026-08-26T10:00:00.000Z", 69)),
                row(deviceId: "fresh-and-capable", agent: "claude", email: "dev@acme.test", usage: usage("2026-08-28T11:00:00.000Z", 75)),
                row(deviceId: "freshest-but-not-mine", agent: "claude", mine: false, email: "dev@acme.test", usage: usage("2026-08-28T11:30:00.000Z", 75)),
            ],
            canRefresh: { $0.mine }
        )
        XCTAssertEqual(groups[0].refreshTarget?.deviceId, "fresh-and-capable")
        // The group's own numbers still come from the freshest member of all.
        XCTAssertEqual(groups[0].usage?.fetchedAt, "2026-08-28T11:30:00.000Z")
    }

    func testOrdersGroupsAttentionFirst() throws {
        let groups = AgentAccountsRows.sortGroupsAttentionFirst(
            AgentAccountsRows.accountGroups(
                [
                    row(deviceId: "a", agent: "claude", email: "low@acme.test", usage: usage("2026-08-28T11:00:00.000Z", 10)),
                    row(deviceId: "a", agent: "codex", email: "hot@acme.test", usage: usage("2026-08-28T11:00:00.000Z", 96)),
                    row(deviceId: "b", agent: "claude", signedIn: false),
                    row(deviceId: "a", agent: "claude", profileId: "work", email: "mid@acme.test", usage: usage("2026-08-28T11:00:00.000Z", 60)),
                ],
                canRefresh: { _ in false }
            )
        )
        XCTAssertEqual(groups.map(\.key), [
            "claude:b:system",
            "codex:hot@acme.test",
            "claude:mid@acme.test",
            "claude:low@acme.test",
        ])
    }

    func testFoldsTheSyncedDeviceRowsEndToEnd() throws {
        // Two machines, one login: the section shows ONE row with two chips.
        let devices = ["macbook", "server"].map { id in
            DeviceEntity(
                id: "row-\(id)",
                userId: "me",
                deviceId: id,
                label: id,
                agentAccounts: #"{"claude":{"signedIn":true,"email":"dev@acme.test","plan":"max","checkedAt":"2026-08-28T11:00:00.000Z"}}"#,
                agentUsage: #"{"claude":\#(usageJson("2026-08-28T11:00:00.000Z", "weekly", 75))}"#,
                lastSeenAt: "2026-08-28T11:59:00.000Z"
            )
        }
        let rows = AgentAccountsRows.profileRows(devices: devices, currentUserId: "me", isOnline: { _ in true })
        let groups = AgentAccountsRows.accountGroups(rows, canRefresh: { _ in true })
        XCTAssertEqual(groups.count, 1)
        XCTAssertEqual(groups[0].rows.count, 2)
        XCTAssertEqual(groups[0].refreshTarget?.deviceId, "macbook")
    }

    // MARK: - The section's layout rules

    private func group(_ agent: String, _ key: String) -> AgentAccountUsageGroup {
        AgentAccountUsageGroup(
            key: key, agent: agent, signedIn: true, email: nil, plan: nil,
            rows: [row(deviceId: "dev-1", agent: agent)],
            usage: nil, checkedAt: nil, refreshTarget: nil
        )
    }

    // The agent headings follow the CONTRACT order, never the group order or
    // the alphabet; an agent this build has no name for still gets its own
    // section, after the known ones.
    func testSectionsFollowTheContractAgentOrder() {
        let sections = AgentAccountsRows.sections([
            group("aider", "aider:a"),
            group("zed", "zed:a"),
            group("codex", "codex:a"),
            group("claude", "claude:a"),
            group("codex", "codex:b"),
        ])
        XCTAssertEqual(
            sections.map { ($0.agent, $0.groups.count) }.map { "\($0.0):\($0.1)" },
            ["claude:1", "codex:2", "aider:1", "zed:1"]
        )
        XCTAssertEqual(sections[1].groups.map(\.key), ["codex:a", "codex:b"])
    }

    // The chip names the machine, and the profile only when it is not the
    // ambient login; the row title is the login, or that there is none.
    func testChipsNameTheMachineAndANamedProfile() {
        XCTAssertEqual(AgentAccountsRows.chipLabel(row(deviceId: "dev-1", agent: "claude", deviceLabel: "Studio")), "Studio")
        XCTAssertEqual(
            AgentAccountsRows.chipLabel(row(deviceId: "dev-1", agent: "claude", deviceLabel: "Studio", profileId: "0a1b2c3d", profileLabel: "Personal")),
            "Studio · Personal"
        )
        XCTAssertEqual(
            AgentAccountsRows.chipLabel(row(deviceId: "dev-1", agent: "claude", deviceLabel: "", profileId: "0a1b2c3d", profileLabel: "Personal")),
            "dev-1 · Personal"
        )

        // EXP-862: the title names the ACCOUNT, never its sign-in state — a
        // signed-out login is said once, by its chip's badge. A group with
        // nothing to name itself by falls back to its profile's label (the
        // ambient login's is "Default"), which is also the last resort.
        let anonymous = group("claude", "claude:a")
        XCTAssertEqual(AgentAccountsRows.groupCaption(anonymous), "Default")
        let empty = AgentAccountUsageGroup(
            key: "claude:a", agent: "claude", signedIn: true, email: nil, plan: nil,
            rows: [], usage: nil, checkedAt: nil, refreshTarget: nil
        )
        XCTAssertEqual(
            AgentAccountsRows.groupCaption(empty), AgentAccountsRows.systemProfileLabel
        )
        let signedOut = AgentAccountUsageGroup(
            key: "claude:a", agent: "claude", signedIn: false, email: "x@y.z", plan: "max",
            rows: [], usage: nil, checkedAt: nil, refreshTarget: nil
        )
        XCTAssertEqual(AgentAccountsRows.groupCaption(signedOut), "x@y.z")
        let labelled = AgentAccountUsageGroup(
            key: "claude:dev-1:work", agent: "claude", signedIn: false, email: nil, plan: nil,
            rows: [row(deviceId: "dev-1", agent: "claude", profileId: "work", profileLabel: "Work")],
            usage: nil, checkedAt: nil, refreshTarget: nil
        )
        XCTAssertEqual(AgentAccountsRows.groupCaption(labelled), "Work")
        let named = AgentAccountUsageGroup(
            key: "claude:dev@acme.test", agent: "claude", signedIn: true, email: "dev@acme.test", plan: "max",
            rows: [], usage: nil, checkedAt: nil, refreshTarget: nil
        )
        XCTAssertEqual(AgentAccountsRows.groupCaption(named), "dev@acme.test")
        let provider = AgentAccountUsageGroup(
            key: "zed:a:system", agent: "zed", signedIn: true, email: nil, plan: "anthropic (oauth)",
            rows: [], usage: nil, checkedAt: nil, refreshTarget: nil
        )
        XCTAssertEqual(AgentAccountsRows.groupCaption(provider), "anthropic (oauth)")
    }
    // MARK: - EXP-849: a retired agent id never renders

    // A desktop below the version floor keeps heart-beating `pi` in every one
    // of these columns; nothing this build draws may name an agent it has no
    // label, glyph or launcher for — not a row, not a tab, not a chip.
    func testAnAgentOutsideTheContractIsNeverARowOrASection() throws {
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
        let sections = AgentAccountsRows.sections(
            AgentAccountsRows.accountGroups(rows, canRefresh: { _ in false })
        )
        XCTAssertEqual(sections.map(\.agent), ["claude"])
        // The machine row draws the same filtered set of chips.
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
}
