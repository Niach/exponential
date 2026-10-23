import Foundation
import XCTest

@testable import ExpCore

// The session composer footer's model pill reads the engine's `model` option.
final class SessionComposerModelTests: XCTestCase {
    func testReadsTheSessionModelOffTheConfigOption() {
        XCTAssertNil(AgentSessionConfig(options: []).modelValue)
        XCTAssertNil(AgentSessionConfig(options: [
            AgentConfigOption(id: "model", label: "Model", value: ""),
        ]).modelValue)
        XCTAssertEqual(
            AgentSessionConfig(options: [
                AgentConfigOption(id: "model", label: "Model", value: "opus"),
            ]).modelValue,
            "opus"
        )
    }

    /// Web `fallbackModel` ×4: live, else the row, else the device's
    /// session model, else the contract default (NEW-8: no dispatcher rung).
    func testComposerModelFallsBackLikeWeb() {
        let device = DeviceLaunchDefaults(agents: [
            "claude": AgentLaunchDefaults(model: "opus", threadModel: "sonnet"),
        ])
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: "fable", rowModel: "opus", cli: "claude", launchDefaults: device
        ), "fable")
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: nil, rowModel: "sonnet", cli: "claude", launchDefaults: device
        ), "sonnet")
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: nil, rowModel: nil, cli: "claude", launchDefaults: device
        ), "opus")
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: nil, rowModel: nil, cli: nil, launchDefaults: nil
        ), LaunchVocabulary.defaultModel(for: "claude"))
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: nil, rowModel: "", cli: "codex", launchDefaults: nil
        ), DomainContract.deviceAgentDefaultsCodexModel)
    }
}
