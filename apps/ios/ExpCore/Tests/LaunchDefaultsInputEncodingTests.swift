import XCTest
@testable import ExpCore

// The `devices.setLaunchDefaults` wire payload. EXP-1158: it carries neither
// `defaultAgent` nor an account — the last used agent is the DEVICE's to
// write, and the server carries the stored one forward over a save that omits
// it. Every field rides only when set.
final class LaunchDefaultsInputEncodingTests: XCTestCase {
    private func json(_ value: some Encodable) throws -> [String: Any] {
        let data = try JSONEncoder().encode(value)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func testTheInputNeverEncodesTheLastUsedAgentNorAnAccount() throws {
        let object = try json(DeviceLaunchDefaultsInput(
            agents: ["claude": AgentLaunchDefaultsInput(model: "opus")]
        ))
        XCTAssertNil(object.index(forKey: "defaultAgent"))
        // Nor any account key: the edited agents block is the whole payload.
        XCTAssertEqual(Set(object.keys), ["agents"])
        // Every other field still rides only when set.
        let claude = try XCTUnwrap((object["agents"] as? [String: Any])?["claude"] as? [String: Any])
        XCTAssertEqual(claude["model"] as? String, "opus")
        XCTAssertNil(claude.index(forKey: "effort"))
        XCTAssertNil(claude.index(forKey: "subagentModel"))
    }

    /// EXP-1082 §6: the desktop-owned auto-rotate toggle is ECHOED, never
    /// invented. A stored value rides back verbatim on the whole-object save
    /// and an absent one writes no key, so the server's carry-forward compat
    /// can retire without this client clearing the toggle.
    func testAutoRotateAccountsIsEchoedVerbatimAndAbsentWritesNoKey() throws {
        let echoed = try json(DeviceLaunchDefaultsInput(
            agents: [
                "claude": AgentLaunchDefaultsInput(model: "opus", autoRotateAccounts: false),
                "codex": AgentLaunchDefaultsInput(model: "gpt-5-codex"),
            ]
        ))
        let agents = try XCTUnwrap(echoed["agents"] as? [String: Any])
        let claude = try XCTUnwrap(agents["claude"] as? [String: Any])
        XCTAssertEqual(claude["autoRotateAccounts"] as? Bool, false)
        let codex = try XCTUnwrap(agents["codex"] as? [String: Any])
        XCTAssertNil(codex.index(forKey: "autoRotateAccounts"))

        // And the synced row's value survives the decode it is echoed from.
        let row = try JSONDecoder().decode(
            AgentLaunchDefaults.self,
            from: Data(#"{"model":"opus","autoRotateAccounts":true}"#.utf8)
        )
        XCTAssertEqual(row.autoRotateAccounts, true)
        let bare = try JSONDecoder().decode(
            AgentLaunchDefaults.self, from: Data(#"{"model":"opus"}"#.utf8)
        )
        XCTAssertNil(bare.autoRotateAccounts)
    }

    /// EXP-1196: the device-level `computerUse` switch is a TOP-LEVEL key
    /// beside `agents`: encoded when set (false included), absent when nil,
    /// and decoded leniently (absent, null or a non-boolean = nil = off).
    func testComputerUseRoundTripsAsATopLevelKey() throws {
        let on = try json(DeviceLaunchDefaultsInput(
            agents: ["claude": AgentLaunchDefaultsInput(model: "opus")],
            computerUse: true
        ))
        XCTAssertEqual(on["computerUse"] as? Bool, true)
        XCTAssertEqual(Set(on.keys), ["agents", "computerUse"])
        let off = try json(DeviceLaunchDefaultsInput(agents: [:], computerUse: false))
        XCTAssertEqual(off["computerUse"] as? Bool, false)
        let unset = try json(DeviceLaunchDefaultsInput(agents: [:]))
        XCTAssertNil(unset.index(forKey: "computerUse"))

        let decode = { (raw: String) throws -> DeviceLaunchDefaults in
            try JSONDecoder().decode(DeviceLaunchDefaults.self, from: Data(raw.utf8))
        }
        let row = try decode(#"{"defaultAgent":"claude","computerUse":true,"agents":{"claude":{"model":"opus"}}}"#)
        XCTAssertEqual(row.computerUse, true)
        XCTAssertEqual(row.defaultAgent, "claude")
        XCTAssertEqual(row.agents?["claude"]?.model, "opus")
        XCTAssertEqual(try decode(#"{"computerUse":false}"#).computerUse, false)
        XCTAssertNil(try decode(#"{"defaultAgent":"codex"}"#).computerUse)
        XCTAssertNil(try decode(#"{"computerUse":null}"#).computerUse)
        // A shape this build does not expect degrades the key, not the row.
        let odd = try decode(#"{"defaultAgent":"codex","computerUse":"yes"}"#)
        XCTAssertNil(odd.computerUse)
        XCTAssertEqual(odd.defaultAgent, "codex")
    }

    /// The synced devices row (stringified jsonb) keeps the key through the
    /// retired-agent filter that rebuilds the decoded defaults.
    func testSyncedDeviceRowKeepsComputerUse() throws {
        let row = DeviceEntity(
            id: "row-1",
            userId: "me",
            deviceId: "box",
            label: "Box",
            launchDefaults: #"{"defaultAgent":"claude","computerUse":true}"#,
            lastSeenAt: "2026-10-05T11:59:00.000Z"
        )
        let device = SteerDevice(entity: row, currentUserId: "me")
        XCTAssertEqual(device.launchDefaults?.computerUse, true)
        XCTAssertEqual(device.launchDefaults?.defaultAgent, "claude")
    }
}
