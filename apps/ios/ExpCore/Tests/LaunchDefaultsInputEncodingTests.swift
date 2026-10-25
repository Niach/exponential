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
        XCTAssertNil(claude.index(forKey: "threadModel"))
    }

    /// 2026-10-15: the dispatcher root's per-agent `threadModel` rides like
    /// `subagentModel` (only when set) and decodes off the synced row; every
    /// model is explicit, the contract names the defaults.
    func testThreadModelRidesPerAgentAndDecodes() throws {
        let object = try json(DeviceLaunchDefaultsInput(agents: [
            "claude": AgentLaunchDefaultsInput(model: "fable", subagentModel: "opus", threadModel: "haiku"),
            "codex": AgentLaunchDefaultsInput(model: "gpt-5.6-sol", threadModel: "gpt-5.6-luna"),
        ]))
        let agents = try XCTUnwrap(object["agents"] as? [String: Any])
        let claude = try XCTUnwrap(agents["claude"] as? [String: Any])
        XCTAssertEqual(claude["threadModel"] as? String, "haiku")
        XCTAssertEqual(claude["subagentModel"] as? String, "opus")
        let codex = try XCTUnwrap(agents["codex"] as? [String: Any])
        XCTAssertEqual(codex["threadModel"] as? String, "gpt-5.6-luna")

        let row = try JSONDecoder().decode(
            DeviceLaunchDefaults.self,
            from: Data(#"{"agents":{"claude":{"model":"opus","threadModel":"sonnet"},"codex":{"model":"gpt-5.6-sol"}}}"#.utf8)
        )
        XCTAssertEqual(row.agents?["claude"]?.threadModel, "sonnet")
        XCTAssertNil(row.agents?["codex"]?.threadModel)

        XCTAssertEqual(DomainContract.codingModelValues, ["fable", "opus", "sonnet", "haiku"])
        XCTAssertEqual(LaunchVocabulary.modelValues(for: "claude"), DomainContract.codingModelValues)
        XCTAssertEqual(LaunchVocabulary.modelValues(for: "codex"), DomainContract.codexModelValues)
        XCTAssertFalse(LaunchVocabulary.modelValues(for: "codex").contains(LaunchVocabulary.cliDefault))
        XCTAssertEqual(LaunchVocabulary.defaultThreadModel(for: "claude"), "haiku")
        XCTAssertEqual(LaunchVocabulary.defaultThreadModel(for: "codex"), "gpt-5.6-luna")
        XCTAssertEqual(LaunchVocabulary.defaultModel(for: "codex"), "gpt-5.6-sol")
        XCTAssertEqual(DomainContract.deviceAgentDefaultsSubagentModel, "opus")
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

    /// EXP-1236: `computerUseModel` rides beside the switch as a TOP-LEVEL
    /// string: encoded when set, absent when nil (the server keeps the stored
    /// alias), decoded leniently (absent, null or a non-string = nil, which
    /// the sheet seeds as the contract default).
    func testComputerUseModelRidesBesideTheSwitchAndIsOmittedWhenNil() throws {
        let picked = try json(DeviceLaunchDefaultsInput(
            agents: [:], computerUse: true, computerUseModel: "sonnet"
        ))
        XCTAssertEqual(picked["computerUseModel"] as? String, "sonnet")
        XCTAssertEqual(Set(picked.keys), ["agents", "computerUse", "computerUseModel"])
        let unset = try json(DeviceLaunchDefaultsInput(agents: [:], computerUse: true))
        XCTAssertNil(unset.index(forKey: "computerUseModel"))

        let decode = { (raw: String) throws -> DeviceLaunchDefaults in
            try JSONDecoder().decode(DeviceLaunchDefaults.self, from: Data(raw.utf8))
        }
        let row = try decode(#"{"computerUse":true,"computerUseModel":"opus","agents":{}}"#)
        XCTAssertEqual(row.computerUseModel, "opus")
        XCTAssertEqual(row.computerUse, true)
        XCTAssertNil(try decode(#"{"computerUse":true}"#).computerUseModel)
        XCTAssertNil(try decode(#"{"computerUseModel":null}"#).computerUseModel)
        let odd = try decode(#"{"defaultAgent":"codex","computerUseModel":7}"#)
        XCTAssertNil(odd.computerUseModel)
        XCTAssertEqual(odd.defaultAgent, "codex")

        // The contract names the default and it is one of the pickable aliases.
        XCTAssertEqual(DomainContract.deviceComputerUseDefaultsModel, "haiku")
        XCTAssertTrue(
            DomainContract.codingModelValues.contains(DomainContract.deviceComputerUseDefaultsModel)
        )
        XCTAssertEqual(LaunchVocabulary.seedComputerUseModel(nil), "haiku")
        XCTAssertEqual(LaunchVocabulary.seedComputerUseModel("opus"), "opus")
        XCTAssertEqual(LaunchVocabulary.seedComputerUseModel(""), "haiku")
    }

    /// The synced devices row (stringified jsonb) keeps the key through the
    /// retired-agent filter that rebuilds the decoded defaults.
    func testSyncedDeviceRowKeepsComputerUse() throws {
        let row = DeviceEntity(
            id: "row-1",
            userId: "me",
            deviceId: "box",
            label: "Box",
            launchDefaults: #"{"defaultAgent":"claude","computerUse":true,"computerUseModel":"fable"}"#,
            lastSeenAt: "2026-10-05T11:59:00.000Z"
        )
        let device = SteerDevice(entity: row, currentUserId: "me")
        XCTAssertEqual(device.launchDefaults?.computerUse, true)
        // EXP-1236: the model alias survives the same rebuild.
        XCTAssertEqual(device.launchDefaults?.computerUseModel, "fable")
        XCTAssertEqual(device.launchDefaults?.defaultAgent, "claude")
    }

    /// NEW-53: `voice` rides beside `computerUse`; nil writes no key (the
    /// server carries the stored one forward), an assistant-less voice writes
    /// only the switch.
    func testVoiceRidesAsATopLevelObject() throws {
        let object = try json(DeviceLaunchDefaultsInput(
            agents: [:],
            computerUse: false,
            voice: VoiceLaunchDefaults(assistantAgentId: "a-1", speakReplies: true)
        ))
        let voice = try XCTUnwrap(object["voice"] as? [String: Any])
        XCTAssertEqual(voice["assistantAgentId"] as? String, "a-1")
        XCTAssertEqual(voice["speakReplies"] as? Bool, true)

        let none = try json(DeviceLaunchDefaultsInput(voice: VoiceLaunchDefaults(speakReplies: false)))
        let bare = try XCTUnwrap(none["voice"] as? [String: Any])
        XCTAssertEqual(Set(bare.keys), ["speakReplies"])

        XCTAssertNil(try json(DeviceLaunchDefaultsInput(agents: [:])).index(forKey: "voice"))
    }
}
