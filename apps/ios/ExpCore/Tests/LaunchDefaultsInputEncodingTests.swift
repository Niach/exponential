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

    func testPickedDefaultAccountRidesAsItself() throws {
        let object = try json(DeviceLaunchDefaultsInput(
            defaultAgent: "claude",
            defaultAccount: "0a1b2c3d"
        ))
        XCTAssertEqual(object["defaultAccount"] as? String, "0a1b2c3d")
        XCTAssertNil(object.index(forKey: "agents"))
    }

    func testNoDefaultAgentWritesNoAccountKey() throws {
        // The account is one of the default agent's logins: alone it names
        // nothing, so there is nothing to clear either.
        let object = try json(DeviceLaunchDefaultsInput(agents: [:]))
        XCTAssertNil(object.index(forKey: "defaultAgent"))
        XCTAssertNil(object.index(forKey: "defaultAccount"))
    }
}
