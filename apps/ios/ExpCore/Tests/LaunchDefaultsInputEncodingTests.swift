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
            agents: ["claude": AgentLaunchDefaultsInput(model: "opus")],
            workflow: DeviceWorkflowDefaultsInput(model: "opus", strongModel: "fable")
        ))
        XCTAssertNil(object.index(forKey: "defaultAgent"))
        // Nor any account key: the two edited blocks are the whole payload.
        XCTAssertEqual(Set(object.keys), ["agents", "workflow"])
        // Every other field still rides only when set.
        let claude = try XCTUnwrap((object["agents"] as? [String: Any])?["claude"] as? [String: Any])
        XCTAssertEqual(claude["model"] as? String, "opus")
        XCTAssertNil(claude.index(forKey: "effort"))
        XCTAssertNil(claude.index(forKey: "subagentModel"))
    }

    /// EXP-1029: a whole-object save replaces the stored defaults, so an
    /// editor of the workflow pair has to send it on EVERY write.
    func testTheWorkflowPairRidesThePayload() throws {
        let object = try json(DeviceLaunchDefaultsInput(
            workflow: DeviceWorkflowDefaultsInput(model: "opus", strongModel: "fable")
        ))
        let workflow = try XCTUnwrap(object["workflow"] as? [String: Any])
        XCTAssertEqual(workflow["model"] as? String, "opus")
        XCTAssertEqual(workflow["strongModel"] as? String, "fable")
    }

    /// And a sender that knows nothing about it writes no key at all — an
    /// absent `workflow` leaves the stored pair alone.
    func testAnAbsentWorkflowPairWritesNoKey() throws {
        let object = try json(DeviceLaunchDefaultsInput(agents: [:]))
        XCTAssertNil(object.index(forKey: "workflow"))
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
}
