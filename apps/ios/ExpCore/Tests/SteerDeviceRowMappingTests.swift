import Foundation
import XCTest
@testable import ExpCore

// EXP-403: every `SteerDevice` comes off a synced devices row
// (`SteerDevice(entity:)`, DeviceRows) — the durable per-machine state. These
// tests feed the Electric wire form (snake_case, jsonb as real JSON) through
// `DeviceEntity` and the mapping, so the row decode and the presentation
// reads are pinned together. EXP-432's sharing fields and EXP-437's
// `launchDefaults` (the machine's own coding defaults) are optional columns.

private let now = ISO8601DateFormatter().date(from: "2026-08-03T10:00:30Z")!
/// Inside the online window at `now`.
private let fresh = "2026-08-03T10:00:00.000Z"
/// Long outside it.
private let stale = "2026-08-01T10:00:00.000Z"

final class SteerDeviceRowMappingTests: XCTestCase {
    /// One devices row → the `SteerDevice` every surface renders, as seen by
    /// the caller `me`. `fields` is the row body after the `id`/`user_id`.
    private func mapRow(
        _ fields: String, userId: String = "me", ownerName: String? = nil
    ) throws -> SteerDevice {
        let json = #"{"id":"row-1","user_id":"\#(userId)",\#(fields)}"#
        let entity = try JSONDecoder().decode(DeviceEntity.self, from: Data(json.utf8))
        return SteerDevice(entity: entity, now: now, currentUserId: "me", ownerName: ownerName)
    }

    func testMapsRegistryRow() throws {
        let device = try mapRow("""
        "device_id":"d1","label":"build-box","kind":"server",
        "platform":"linux","agents":["claude"],"caps":["actions"],
        "last_seen_at":"\(stale)","version":"0.8.52",
        "update_requested_at":"\(stale)","active_sessions":"1"
        """)
        XCTAssertEqual(device.deviceId, "d1")
        XCTAssertEqual(device.deviceLabel, "build-box")
        XCTAssertEqual(device.rowId, "row-1")
        XCTAssertFalse(device.isOnline)
        XCTAssertTrue(device.isServer)
        XCTAssertTrue(device.isRegistered)
        XCTAssertEqual(device.version, "0.8.52")
        XCTAssertTrue(device.updateRequested == true)
        // EXP-411: an update parked behind a live session.
        XCTAssertTrue(device.updateBlocked == true)
        XCTAssertEqual(device.lastSeenAt, stale)
    }

    /// A row seen inside the window reads online; a desktop row is no server.
    func testMapsFreshDesktopRowAsOnline() throws {
        let device = try mapRow("""
        "device_id":"d2","label":"macbook","kind":"desktop",
        "agents":["claude","codex"],"caps":[],"last_seen_at":"\(fresh)"
        """)
        XCTAssertTrue(device.isOnline)
        XCTAssertFalse(device.isServer)
        XCTAssertFalse(device.updateRequested == true)
        XCTAssertTrue(device.updateBlocked == false)
    }

    /// EXP-420: `devices.latestVersions` is its own two-field result now — the
    /// machines themselves arrive over the devices shape, so nothing carries
    /// the hint as an envelope field any more. Either channel may be null:
    /// the instance simply doesn't advertise one.
    func testDecodesLatestVersionsResult() throws {
        let versions = try JSONDecoder().decode(
            LatestVersions.self,
            from: Data(#"{"desktop":"1.2.3","cli":null}"#.utf8)
        )
        XCTAssertEqual(versions.desktop, "1.2.3")
        XCTAssertNil(versions.cli)
    }

    /// EXP-420: the Update affordance may only claim a newer version exists
    /// with evidence — unknown/unparsable on either side compares false, and
    /// the compare is numeric, not lexicographic.
    func testUpdateAvailableComparesNumericallyAndFailsClosed() throws {
        let device = try mapRow("""
        "device_id":"d1","label":"box","kind":"server",
        "last_seen_at":"\(fresh)","version":"0.9.9"
        """)
        XCTAssertTrue(device.updateAvailable(latest: "0.10.0"))
        XCTAssertFalse(device.updateAvailable(latest: "0.9.9"))
        XCTAssertFalse(device.updateAvailable(latest: "0.9.8"))
        XCTAssertFalse(device.updateAvailable(latest: nil))
        XCTAssertFalse(device.updateAvailable(latest: "junk"))
    }

    /// EXP-409: `agents` means RUNNABLE (installed AND signed in), so the
    /// absent-vs-empty distinction is load-bearing — a NULL column is an
    /// older claude-only sender, an EMPTY one a machine that can run nothing
    /// right now. `unauthed_agents` carries the signed-out installs.
    func testMapsRunnableAndSignedOutAgents() throws {
        let signedOut = try mapRow("""
        "device_id":"d4","label":"macbook","agents":[],
        "unauthed_agents":["claude"],"caps":[],"last_seen_at":"\(fresh)"
        """)
        XCTAssertEqual(signedOut.agentIds, [])
        XCTAssertFalse(signedOut.hasRunnableAgent)
        XCTAssertTrue(signedOut.needsAgentSignIn)
        XCTAssertEqual(signedOut.unauthedAgentIds, ["claude"])

        let partly = try mapRow("""
        "device_id":"d5","label":"mini","agents":["codex"],
        "unauthed_agents":["claude","bogus"],"caps":[],"last_seen_at":"\(fresh)"
        """)
        XCTAssertEqual(partly.agentIds, ["codex"])
        XCTAssertTrue(partly.hasRunnableAgent)
        XCTAssertFalse(partly.needsAgentSignIn)
        // Values outside the contract never reach the UI copy.
        XCTAssertEqual(partly.unauthedAgentIds, ["claude"])

        // A NULL `agents` keeps the claude-only fallback; a NULL
        // `unauthed_agents` is simply nothing signed out.
        let legacy = try mapRow("""
        "device_id":"d6","label":"old","caps":[],"last_seen_at":"\(fresh)"
        """)
        XCTAssertEqual(legacy.agentIds, ["claude"])
        XCTAssertTrue(legacy.hasRunnableAgent)
        XCTAssertFalse(legacy.needsAgentSignIn)
        XCTAssertEqual(legacy.unauthedAgentIds, [])
    }

    /// An OFFLINE machine is never the sign-in case — it already reads as
    /// offline, and the row keeps its last-seen caption.
    func testOfflineMachineIsNotTheSignInCase() throws {
        let device = try mapRow("""
        "device_id":"d7","label":"box","agents":[],
        "unauthed_agents":["claude"],"caps":[],"last_seen_at":"\(stale)"
        """)
        XCTAssertFalse(device.isOnline)
        XCTAssertFalse(device.needsAgentSignIn)
    }

    /// Nullable columns arrive as JSON nulls and map to nil / empty, never
    /// a dropped row.
    func testMapsExplicitNullColumns() throws {
        let device = try mapRow("""
        "device_id":"d3","label":"old-desktop","kind":"desktop",
        "platform":null,"agents":["claude"],"caps":[],"last_seen_at":null,
        "version":null,"update_requested_at":null,"shared_team_ids":null
        """)
        XCTAssertFalse(device.isOnline)  // no heartbeat = fail-closed offline
        XCTAssertNil(device.platform)
        XCTAssertNil(device.lastSeenAt)
        XCTAssertNil(device.version)
        XCTAssertFalse(device.updateRequested == true)
        XCTAssertEqual(device.sharedTeamIds, [])
        XCTAssertTrue(device.isMine)
    }

    /// EXP-432/FEED-33: teammates' shared servers ride along after the
    /// caller's own rows. `owner` is the whole tell — own rows never
    /// carry it — while `sharedTeamIds` rides own rows too, so it must never
    /// be read as "somebody else's machine". The Postgres text literal and
    /// the stored JSON array both decode.
    func testMapsTeamSharedDevice() throws {
        let teamId = "11111111-1111-1111-1111-111111111111"
        let otherTeam = "22222222-2222-2222-2222-222222222222"
        let own = try mapRow("""
        "device_id":"own","label":"my-box","kind":"server",
        "agents":["claude"],"caps":["actions"],"last_seen_at":"\(fresh)",
        "shared_team_ids":["\(teamId)","\(otherTeam)"]
        """)
        XCTAssertTrue(own.isMine)  // shared out, but still the caller's machine
        XCTAssertNil(own.owner)
        XCTAssertEqual(own.sharedTeamIds, [teamId, otherTeam])

        let shared = try mapRow("""
        "device_id":"mate","label":"buildbox","kind":"server",
        "agents":["claude"],"caps":["actions"],"last_seen_at":"\(fresh)",
        "shared_team_ids":"{\(teamId)}"
        """, userId: "u2", ownerName: "Danny")
        XCTAssertFalse(shared.isMine)
        XCTAssertEqual(shared.owner?.id, "u2")
        XCTAssertEqual(shared.owner?.name, "Danny")
        XCTAssertEqual(shared.sharedTeamIds, [teamId])
        // A teammate's shared server is a start target like any other.
        XCTAssertTrue(shared.isOnline)
        XCTAssertTrue(shared.hasRunnableAgent)
    }

    /// EXP-484: the remote sign-in cap is read independently of the others —
    /// a machine that advertises none simply offers no Login control.
    func testMapsTheAgentLoginCap() throws {
        let modern = try mapRow("""
        "device_id":"d20","label":"macbook","agents":["claude"],
        "caps":["actions","agent-login"],"last_seen_at":"\(fresh)"
        """)
        XCTAssertTrue(modern.canAgentLogin)

        let bare = try mapRow("""
        "device_id":"d22","label":"bare","agents":["claude"],
        "caps":[],"last_seen_at":"\(fresh)"
        """)
        XCTAssertFalse(bare.canAgentLogin)
    }

    /// EXP-437: a machine reports its own per-agent coding defaults so a
    /// remote Start-coding sheet can open on THEM. `model`/`effort` always ride
    /// because `""` is a real choice ("CLI default / omit the flag"), while
    /// the booleans are stored only when true — an entry with none of them
    /// is simply all-false, not a decode failure.
    func testMapsLaunchDefaults() throws {
        let device = try mapRow("""
        "device_id":"d9","label":"macbook","agents":["claude","codex"],
        "caps":["actions"],"last_seen_at":"\(fresh)",
        "launch_defaults":{"defaultAgent":"codex","agents":{
        "claude":{"model":"fable","effort":"","planMode":true},
        "codex":{"model":"","effort":"high"}}}
        """)
        XCTAssertEqual(device.defaultLaunchAgent, "codex")

        let claude = try XCTUnwrap(device.agentDefaults(for: "claude"))
        XCTAssertEqual(claude.model, "fable")
        XCTAssertEqual(claude.effort, "")  // blank = CLI default, NOT absent
        XCTAssertEqual(claude.planMode, true)
        XCTAssertNil(claude.ultracode)     // absent boolean = false

        let codex = try XCTUnwrap(device.agentDefaults(for: "codex"))
        XCTAssertEqual(codex.model, "")
        XCTAssertEqual(codex.effort, "high")
        XCTAssertNil(codex.planMode)

        // The map covers RUNNABLE agents only — nothing to seed for the rest.
        XCTAssertNil(device.agentDefaults(for: "zed"))
    }

    /// EXP-773 deleted the "Start in terminal" preference. A stored
    /// `launch_defaults` may still carry `startInTerminal`, and the decoder
    /// must keep ignoring it — the defaults would otherwise fail to parse and
    /// every picker would lose them.
    func testIgnoresTheRetiredTerminalDefault() throws {
        let device = try mapRow("""
        "device_id":"d10","label":"macbook","agents":["claude"],
        "caps":["actions"],"last_seen_at":"\(fresh)",
        "launch_defaults":{"defaultAgent":"claude","startInTerminal":true}
        """)
        XCTAssertEqual(device.launchDefaults?.defaultAgent, "claude")
    }

    /// EXP-749: `acp_agents` names which runnable agents the machine's ACP
    /// engine can drive. Every build above the desktop floor reports it, so a
    /// NULL set is a stale row and reads as EMPTY: nothing starts there.
    /// EXP-773 removed the PTY fallback, so a miss is unstartable rather than
    /// a terminal run.
    func testAgentNotReadyOnlyWhenAcpAgentsAreKnown() throws {
        let reported = try mapRow("""
        "device_id":"d12","label":"macbook","agents":["claude","codex","zed"],
        "acp_agents":["claude","codex","bogus"],"caps":["acp"],"last_seen_at":"\(fresh)"
        """)
        // Values outside the contract never reach the UI copy.
        XCTAssertEqual(reported.acpAgentIds, ["claude", "codex"])
        XCTAssertFalse(reported.agentNotReady("claude"))
        XCTAssertTrue(reported.agentNotReady("zed"))

        // A row that never reported reads as EMPTY, like a reported empty one.
        let unreported = try mapRow("""
        "device_id":"d13","label":"old-box","agents":["claude","codex"],
        "caps":[],"last_seen_at":"\(fresh)"
        """)
        XCTAssertEqual(unreported.acpAgentIds, [])
        XCTAssertTrue(unreported.agentNotReady("claude"))
        XCTAssertTrue(unreported.agentNotReady("codex"))

        // Reported EMPTY is knowledge, not ignorance: nothing runs over ACP
        // there, so no agent can start.
        let none = try mapRow("""
        "device_id":"d14","label":"stale-box","agents":["claude"],
        "acp_agents":[],"caps":["acp"],"last_seen_at":"\(fresh)"
        """)
        XCTAssertEqual(none.acpAgentIds, [])
        XCTAssertTrue(none.agentNotReady("claude"))
    }

    /// A stored default naming an agent the machine cannot run right now
    /// (signed out, uninstalled) must not preselect it, or the sheet offers an
    /// agent the launcher would refuse.
    func testDefaultLaunchAgentClampsToRunnableAgents() throws {
        let device = try mapRow("""
        "device_id":"d10","label":"mini","agents":["claude"],
        "caps":[],"last_seen_at":"\(fresh)",
        "launch_defaults":{"defaultAgent":"codex","agents":{
        "claude":{"model":"opus","effort":"max","ultracode":true,"skipPermissions":true}}}
        """)
        XCTAssertNil(device.defaultLaunchAgent)
        let claude = try XCTUnwrap(device.agentDefaults(for: "claude"))
        XCTAssertEqual(claude.ultracode, true)
        // EXP-690 removed the skip-permissions option; a stored row may still
        // carry the key and the defaults must decode straight past it.
        XCTAssertEqual(claude.model, "opus")
    }

    /// A machine with no stored defaults: every accessor reads as "nothing
    /// advertised" so the sheet falls back to contract defaults.
    func testMapsWithoutLaunchDefaults() throws {
        let device = try mapRow("""
        "device_id":"d11","label":"old","agents":["claude"],
        "caps":[],"last_seen_at":"\(fresh)"
        """)
        XCTAssertNil(device.launchDefaults)
        XCTAssertNil(device.defaultLaunchAgent)
        XCTAssertNil(device.agentDefaults(for: "claude"))
    }

    // MARK: - editableAgentIds (EXP-1042)

    /// The device-settings defaults editor covers every agent the machine
    /// NAMES, not just the runnable ones — web's `editorAgents` unions five
    /// sources and the sheet has to match it, because an agent known only
    /// from `agentAccounts` is exactly where a stored `defaultAgent` comes
    /// from: drop it and the sheet re-anchors the default and clears the
    /// stored account, then autosaves the loss back.
    func testEditableAgentsUnionsAccountsAndUsageToo() throws {
        let accountsOnly = try mapRow("""
        "device_id":"d20","label":"quiet-box","agents":[],
        "unauthed_agents":[],"caps":[],"last_seen_at":"\(stale)",
        "agent_accounts":{"codex":{"signedIn":true,"email":"a@b.c"}}
        """)
        XCTAssertEqual(accountsOnly.agentIds, [])
        XCTAssertTrue(accountsOnly.editableAgentIds.contains("codex"))

        let usageOnly = try mapRow("""
        "device_id":"d21","label":"usage-box","agents":[],"caps":[],
        "agent_usage":{"codex":{"fetchedAt":"2026-09-24T10:00:00.000Z"}}
        """)
        XCTAssertTrue(usageOnly.editableAgentIds.contains("codex"))
    }

    /// The other three sources still ride, in contract order, and an agent
    /// outside the contract never becomes a tab.
    func testEditableAgentsUnionsTheReportedAndStoredSources() throws {
        let device = try mapRow("""
        "device_id":"d22","label":"box","agents":["codex"],
        "unauthed_agents":["claude","ghost"],"caps":[],"last_seen_at":"\(fresh)",
        "launch_defaults":{"defaultAgent":"codex","agents":{"claude":{"model":"opus"}}}
        """)
        XCTAssertEqual(device.editableAgentIds, DomainContract.codingAgentValues)
        XCTAssertFalse(device.editableAgentIds.contains("ghost"))
    }

    /// A machine that names NOTHING at all keeps its defaults editable: every
    /// contract agent gets its tab of per-agent defaults.
    func testAMachineThatNamesNothingOffersEveryContractAgent() throws {
        let device = try mapRow("""
        "device_id":"d23","label":"silent","agents":[],
        "unauthed_agents":[],"caps":[],"last_seen_at":"\(stale)"
        """)
        XCTAssertEqual(device.editableAgentIds, DomainContract.codingAgentValues)
    }

    /// A row without the sharing column still maps, and reads as the
    /// caller's own — never as somebody else's, which would hide the
    /// rename/remove actions on a perfectly ordinary machine.
    func testRowsMapWithoutTheSharingColumn() throws {
        let device = try mapRow("""
        "device_id":"d8","label":"box","kind":"server",
        "agents":["claude"],"caps":[],"last_seen_at":"\(fresh)"
        """)
        XCTAssertEqual(device.sharedTeamIds, [])
        XCTAssertNil(device.owner)
        XCTAssertTrue(device.isMine)
    }
}
