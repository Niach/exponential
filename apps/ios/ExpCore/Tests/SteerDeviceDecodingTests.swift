import Foundation
import XCTest
@testable import ExpCore

// EXP-403: registry rows (the durable per-machine state, synced through the
// devices shape) and bare relay-presence rows decode into ONE `SteerDevice`,
// mirroring apps/web/src/lib/steer-devices.ts. The registry
// fields are all optional, so a presence row — which carries none — must read
// as ONLINE: treating an absent `online` as offline would hide every start
// affordance the moment an older server answers. EXP-432 adds the two sharing
// fields on the same terms — both optional, both absent from an older server,
// as does EXP-437's `launchDefaults` (the machine's own coding defaults).

/// The rows always arrive wrapped in a `{ devices }` object, so the fixtures
/// decode through one here.
private struct Envelope: Decodable {
    let devices: [SteerDevice]
}

final class SteerDeviceDecodingTests: XCTestCase {
    private func decode(_ json: String) throws -> Envelope {
        try JSONDecoder().decode(Envelope.self, from: Data(json.utf8))
    }

    func testDecodesRegistryRow() throws {
        let result = try decode("""
        {"devices":[{"deviceId":"d1","deviceLabel":"build-box","kind":"server",
        "platform":"linux","agents":["claude"],"caps":["actions"],"online":false,
        "lastSeenAt":"2026-08-03T10:00:00.000Z","registered":true,"version":"0.8.52",
        "updateRequested":true,"updateBlocked":true}]}
        """)
        let device = try XCTUnwrap(result.devices.first)
        XCTAssertEqual(device.deviceId, "d1")
        XCTAssertEqual(device.deviceLabel, "build-box")
        XCTAssertFalse(device.isOnline)
        XCTAssertTrue(device.isServer)
        XCTAssertTrue(device.isRegistered)
        XCTAssertEqual(device.version, "0.8.52")
        XCTAssertTrue(device.updateRequested == true)
        XCTAssertTrue(device.updateBlocked == true)
        XCTAssertEqual(device.lastSeenAt, "2026-08-03T10:00:00.000Z")
        XCTAssertTrue(device.canRunActions)
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
        let result = try decode("""
        {"devices":[{"deviceId":"d1","deviceLabel":"box","kind":"server",
        "online":true,"registered":true,"version":"0.9.9"}]}
        """)
        let device = try XCTUnwrap(result.devices.first)
        XCTAssertTrue(device.updateAvailable(latest: "0.10.0"))
        XCTAssertFalse(device.updateAvailable(latest: "0.9.9"))
        XCTAssertFalse(device.updateAvailable(latest: "0.9.8"))
        XCTAssertFalse(device.updateAvailable(latest: nil))
        XCTAssertFalse(device.updateAvailable(latest: "junk"))
    }

    func testDecodesPresenceRowAsOnlineDesktop() throws {
        let result = try decode("""
        {"devices":[{"deviceId":"d2","deviceLabel":"macbook","connectedAt":1754212345678,
        "agents":["claude","codex"],"caps":[]}]}
        """)
        let device = try XCTUnwrap(result.devices.first)
        XCTAssertTrue(device.isOnline)       // absent `online` = alive by construction
        XCTAssertFalse(device.isServer)      // absent `kind` = the desktop app
        XCTAssertFalse(device.isRegistered)  // nothing to rename or remove
        XCTAssertNil(device.version)
        XCTAssertNil(device.lastSeenAt)
        XCTAssertNil(device.updateBlocked)   // pre-EXP-411 server: no field
    }

    /// EXP-409: `agents` means RUNNABLE (installed AND signed in), so the
    /// absent-vs-empty distinction is load-bearing — an ABSENT list is a
    /// pre-EXP-201 claude-only sender, an EMPTY one a machine that can run
    /// nothing right now. `unauthedAgents` carries the signed-out installs.
    func testDecodesRunnableAndSignedOutAgents() throws {
        let result = try decode("""
        {"devices":[{"deviceId":"d4","deviceLabel":"macbook","agents":[],
        "unauthedAgents":["claude"],"caps":[],"online":true},
        {"deviceId":"d5","deviceLabel":"mini","agents":["codex"],
        "unauthedAgents":["claude","bogus"],"caps":[],"online":true},
        {"deviceId":"d6","deviceLabel":"old","caps":[],"online":true}]}
        """)
        let signedOut = try XCTUnwrap(result.devices.first)
        XCTAssertEqual(signedOut.agentIds, [])
        XCTAssertFalse(signedOut.hasRunnableAgent)
        XCTAssertTrue(signedOut.needsAgentSignIn)
        XCTAssertEqual(signedOut.unauthedAgentIds, ["claude"])

        let partly = result.devices[1]
        XCTAssertEqual(partly.agentIds, ["codex"])
        XCTAssertTrue(partly.hasRunnableAgent)
        XCTAssertFalse(partly.needsAgentSignIn)
        // Values outside the contract never reach the UI copy.
        XCTAssertEqual(partly.unauthedAgentIds, ["claude"])

        // Absent `agents` keeps the legacy claude-only fallback; absent
        // `unauthedAgents` is simply nothing signed out.
        let legacy = result.devices[2]
        XCTAssertEqual(legacy.agentIds, ["claude"])
        XCTAssertTrue(legacy.hasRunnableAgent)
        XCTAssertFalse(legacy.needsAgentSignIn)
        XCTAssertEqual(legacy.unauthedAgentIds, [])
    }

    /// An OFFLINE machine is never the sign-in case — it already reads as
    /// offline, and the row keeps its last-seen caption.
    func testOfflineMachineIsNotTheSignInCase() throws {
        let result = try decode("""
        {"devices":[{"deviceId":"d7","deviceLabel":"box","agents":[],
        "unauthedAgents":["claude"],"caps":[],"online":false,
        "lastSeenAt":"2026-08-03T10:00:00.000Z"}]}
        """)
        let device = try XCTUnwrap(result.devices.first)
        XCTAssertFalse(device.isOnline)
        XCTAssertFalse(device.needsAgentSignIn)
    }

    func testDecodesExplicitNullRegistryFields() throws {
        // A relay-connected device that never registered: present keys, JSON
        // nulls.
        let result = try decode("""
        {"devices":[{"deviceId":"d3","deviceLabel":"old-desktop","kind":"desktop",
        "platform":null,"agents":["claude"],"caps":[],"online":true,"lastSeenAt":null,
        "registered":false,"version":null,"updateRequested":false,"sharedTeamId":null}]}
        """)
        let device = try XCTUnwrap(result.devices.first)
        XCTAssertTrue(device.isOnline)
        XCTAssertNil(device.platform)
        XCTAssertNil(device.lastSeenAt)
        XCTAssertNil(device.version)
        XCTAssertFalse(device.isRegistered)
        XCTAssertNil(device.sharedTeamId)
        XCTAssertTrue(device.isMine)
    }

    /// EXP-432: teammates' shared servers ride along after the caller's own
    /// rows. `owner` is the whole tell — own rows never
    /// carry it — while `sharedTeamId` rides own rows too, so it must never be
    /// read as "somebody else's machine".
    func testDecodesTeamSharedDevice() throws {
        let teamId = "11111111-1111-1111-1111-111111111111"
        let result = try decode("""
        {"devices":[{"deviceId":"own","deviceLabel":"my-box","kind":"server",
        "agents":["claude"],"caps":["actions"],"online":true,"registered":true,
        "sharedTeamId":"\(teamId)"},
        {"deviceId":"mate","deviceLabel":"buildbox","kind":"server",
        "agents":["claude"],"caps":["actions"],"online":true,"registered":true,
        "sharedTeamId":"\(teamId)","owner":{"id":"u2","name":"Danny"}}]}
        """)
        let own = try XCTUnwrap(result.devices.first)
        XCTAssertTrue(own.isMine)  // shared out, but still the caller's machine
        XCTAssertNil(own.owner)
        XCTAssertEqual(own.sharedTeamId, teamId)

        let shared = result.devices[1]
        XCTAssertFalse(shared.isMine)
        XCTAssertEqual(shared.owner?.id, "u2")
        XCTAssertEqual(shared.owner?.name, "Danny")
        XCTAssertEqual(shared.sharedTeamId, teamId)
        // A teammate's shared server is a start target like any other.
        XCTAssertTrue(shared.isOnline)
        XCTAssertTrue(shared.hasRunnableAgent)
        XCTAssertTrue(shared.canRunActions)
    }

    /// EXP-437: a machine advertises its own per-agent coding defaults so a
    /// remote Start-coding sheet can open on THEM. `model`/`effort` always ride
    /// the wire because `""` is a real choice ("CLI default / omit the flag"),
    /// while the booleans are sent only when true — an entry with none of them
    /// is simply all-false, not a decode failure.
    func testDecodesLaunchDefaults() throws {
        let result = try decode("""
        {"devices":[{"deviceId":"d9","deviceLabel":"macbook","agents":["claude","codex"],
        "caps":["actions"],"online":true,
        "launchDefaults":{"defaultAgent":"codex","agents":{
        "claude":{"model":"fable","effort":"","planMode":true},
        "codex":{"model":"","effort":"high"}}}}]}
        """)
        let device = try XCTUnwrap(result.devices.first)
        XCTAssertEqual(device.defaultLaunchAgent, "codex")

        let claude = try XCTUnwrap(device.agentDefaults(for: "claude"))
        XCTAssertEqual(claude.model, "fable")
        XCTAssertEqual(claude.effort, "")  // blank = CLI default, NOT absent
        XCTAssertEqual(claude.planMode, true)
        XCTAssertNil(claude.ultracode)     // absent boolean = false
        XCTAssertNil(claude.skipPermissions)

        let codex = try XCTUnwrap(device.agentDefaults(for: "codex"))
        XCTAssertEqual(codex.model, "")
        XCTAssertEqual(codex.effort, "high")
        XCTAssertNil(codex.planMode)

        // The map covers RUNNABLE agents only — nothing to seed for the rest.
        XCTAssertNil(device.agentDefaults(for: "pi"))
    }

    /// The advertised default agent is only ever a SUGGESTION: a machine that
    /// names one it can't run right now (signed out since it last saved its
    /// settings) must not preselect it, or the sheet offers an agent the
    /// launcher would refuse.
    func testDefaultLaunchAgentClampsToRunnableAgents() throws {
        let result = try decode("""
        {"devices":[{"deviceId":"d10","deviceLabel":"mini","agents":["claude"],
        "caps":[],"online":true,
        "launchDefaults":{"defaultAgent":"codex","agents":{
        "claude":{"model":"opus","effort":"max","ultracode":true,"skipPermissions":true}}}}]}
        """)
        let device = try XCTUnwrap(result.devices.first)
        XCTAssertNil(device.defaultLaunchAgent)
        let claude = try XCTUnwrap(device.agentDefaults(for: "claude"))
        XCTAssertEqual(claude.ultracode, true)
        XCTAssertEqual(claude.skipPermissions, true)
    }

    /// An older desktop advertises no defaults at all. Every accessor must read
    /// as "nothing advertised" so the sheet falls back to contract defaults —
    /// never as a decode error that would drop the machine from the picker.
    func testDecodesWithoutLaunchDefaults() throws {
        let result = try decode("""
        {"devices":[{"deviceId":"d11","deviceLabel":"old","agents":["claude"],
        "caps":[],"online":true}]}
        """)
        let device = try XCTUnwrap(result.devices.first)
        XCTAssertNil(device.launchDefaults)
        XCTAssertNil(device.defaultLaunchAgent)
        XCTAssertNil(device.agentDefaults(for: "claude"))
    }

    /// A pre-EXP-432 server omits BOTH sharing fields. Every row must still
    /// decode, and read as the caller's own — never as somebody else's, which
    /// would hide the rename/remove actions on a perfectly ordinary machine.
    func testLegacyRowsDecodeWithoutSharingFields() throws {
        let result = try decode("""
        {"devices":[{"deviceId":"d8","deviceLabel":"box","kind":"server",
        "agents":["claude"],"caps":[],"online":true,"registered":true}]}
        """)
        let device = try XCTUnwrap(result.devices.first)
        XCTAssertNil(device.sharedTeamId)
        XCTAssertNil(device.owner)
        XCTAssertTrue(device.isMine)
    }
}
