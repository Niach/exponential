import XCTest
@testable import ExpCore

// The `devices.setLaunchDefaults` wire payload. Absent vs null is load-bearing
// for ONE key: the server reads an ABSENT `defaultAccount` as "an older client
// that never sends it, keep the stored pin" and an explicit NULL as the clear
// (back to the agent's ambient login), so the encoder is pinned rather than
// left to Codable's `encodeIfPresent`.
final class LaunchDefaultsInputEncodingTests: XCTestCase {
    private func json(_ value: some Encodable) throws -> [String: Any] {
        let data = try JSONEncoder().encode(value)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func testUnsetDefaultAccountBesideAnAgentEncodesAnExplicitNull() throws {
        let object = try json(DeviceLaunchDefaultsInput(
            defaultAgent: "claude",
            agents: ["claude": AgentLaunchDefaultsInput(model: "opus")]
        ))
        XCTAssertEqual(object["defaultAgent"] as? String, "claude")
        XCTAssertNotNil(object.index(forKey: "defaultAccount"))
        XCTAssertTrue(object["defaultAccount"] is NSNull)
        // Every other field still rides only when set.
        let claude = try XCTUnwrap((object["agents"] as? [String: Any])?["claude"] as? [String: Any])
        XCTAssertEqual(claude["model"] as? String, "opus")
        XCTAssertNil(claude.index(forKey: "effort"))
        XCTAssertNil(claude.index(forKey: "subagentModel"))
    }

    /// EXP-1042: the account picker offers one AMBIENT option per login-less
    /// agent, and its id is the `system` sentinel — a picker word, never a
    /// profile id. The server clamp takes any non-empty string, so the input
    /// itself folds the sentinel into the clear: picking it writes an
    /// explicit null, exactly like web, not the literal "system".
    func testTheAmbientSentinelEncodesAsAnExplicitNull() throws {
        let object = try json(DeviceLaunchDefaultsInput(
            defaultAgent: "codex",
            defaultAccount: AgentAccountsRows.systemProfileId,
            agents: ["codex": AgentLaunchDefaultsInput(model: "gpt-5-codex")]
        ))
        XCTAssertEqual(object["defaultAgent"] as? String, "codex")
        XCTAssertNotNil(object.index(forKey: "defaultAccount"))
        XCTAssertTrue(object["defaultAccount"] is NSNull)
        XCTAssertNil(object["defaultAccount"] as? String)
    }

    /// And so does the blank the sheet's draft holds while nothing is stored.
    func testABlankDefaultAccountEncodesAsAnExplicitNull() throws {
        let object = try json(DeviceLaunchDefaultsInput(
            defaultAgent: "claude", defaultAccount: ""
        ))
        XCTAssertTrue(object["defaultAccount"] is NSNull)
    }

    func testPickedDefaultAccountRidesAsItself() throws {
        let object = try json(DeviceLaunchDefaultsInput(
            defaultAgent: "claude",
            defaultAccount: "0a1b2c3d"
        ))
        XCTAssertEqual(object["defaultAccount"] as? String, "0a1b2c3d")
        XCTAssertNil(object.index(forKey: "agents"))
    }

    /// EXP-1029: a whole-object save replaces the stored defaults, so an
    /// editor of the workflow pair has to send it on EVERY write.
    func testTheWorkflowPairRidesThePayload() throws {
        let object = try json(DeviceLaunchDefaultsInput(
            defaultAgent: "claude",
            defaultAccount: "0a1b2c3d",
            workflow: DeviceWorkflowDefaultsInput(model: "opus", strongModel: "fable")
        ))
        let workflow = try XCTUnwrap(object["workflow"] as? [String: Any])
        XCTAssertEqual(workflow["model"] as? String, "opus")
        XCTAssertEqual(workflow["strongModel"] as? String, "fable")
    }

    /// And a sender that knows nothing about it writes no key at all — an
    /// absent `workflow` leaves the stored pair alone.
    func testAnAbsentWorkflowPairWritesNoKey() throws {
        let object = try json(DeviceLaunchDefaultsInput(
            defaultAgent: "claude",
            defaultAccount: "0a1b2c3d"
        ))
        XCTAssertNil(object.index(forKey: "workflow"))
    }

    func testNoDefaultAgentWritesNoAccountKey() throws {
        // The account is one of the default agent's logins: alone it names
        // nothing, so there is nothing to clear either.
        let object = try json(DeviceLaunchDefaultsInput(agents: [:]))
        XCTAssertNil(object.index(forKey: "defaultAgent"))
        XCTAssertNil(object.index(forKey: "defaultAccount"))
    }
}
