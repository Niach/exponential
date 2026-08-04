import Foundation
import XCTest
@testable import ExpCore

// EXP-403: `devices.list` (the durable registry merged with relay presence)
// and `steer.myDevices` (presence only) decode into ONE `SteerDevice`,
// mirroring apps/web/src/lib/steer-devices.ts. The registry fields are all
// optional, so a presence row — which carries none of them — must still read
// as ONLINE: treating an absent `online` as offline would hide every start
// affordance the moment an older server answers.
final class SteerDeviceDecodingTests: XCTestCase {
    private func decode(_ json: String) throws -> SteerDevicesResult {
        try JSONDecoder().decode(SteerDevicesResult.self, from: Data(json.utf8))
    }

    func testDecodesRegistryRow() throws {
        let result = try decode("""
        {"devices":[{"deviceId":"d1","deviceLabel":"build-box","kind":"server",
        "platform":"linux","agents":["claude"],"caps":["actions"],"online":false,
        "lastSeenAt":"2026-08-03T10:00:00.000Z","registered":true,"version":"0.8.52",
        "updateRequested":true}],"latestVersions":{"desktop":"1.2.3","cli":"0.8.53"}}
        """)
        let device = try XCTUnwrap(result.devices.first)
        XCTAssertEqual(device.deviceId, "d1")
        XCTAssertEqual(device.deviceLabel, "build-box")
        XCTAssertFalse(device.isOnline)
        XCTAssertTrue(device.isServer)
        XCTAssertTrue(device.isRegistered)
        XCTAssertEqual(device.version, "0.8.52")
        XCTAssertTrue(device.updateRequested == true)
        XCTAssertEqual(device.lastSeenAt, "2026-08-03T10:00:00.000Z")
        XCTAssertTrue(device.canRunActions)
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
        // The entry `devices.list` appends for a relay-connected device that
        // never registered: present keys, JSON nulls.
        let result = try decode("""
        {"devices":[{"deviceId":"d3","deviceLabel":"old-desktop","kind":"desktop",
        "platform":null,"agents":["claude"],"caps":[],"online":true,"lastSeenAt":null,
        "registered":false,"version":null,"updateRequested":false}]}
        """)
        let device = try XCTUnwrap(result.devices.first)
        XCTAssertTrue(device.isOnline)
        XCTAssertNil(device.platform)
        XCTAssertNil(device.lastSeenAt)
        XCTAssertNil(device.version)
        XCTAssertFalse(device.isRegistered)
    }
}
