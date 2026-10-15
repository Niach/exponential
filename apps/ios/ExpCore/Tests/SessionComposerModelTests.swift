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

    /// Web `fallbackModel` ×4: live, else the row, else (a dispatcher) the
    /// device's thread model, else the device/contract session model.
    func testComposerModelFallsBackLikeWeb() {
        let device = DeviceLaunchDefaults(agents: [
            "claude": AgentLaunchDefaults(model: "opus", threadModel: "sonnet"),
        ])
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: "fable", rowModel: "opus", cli: "claude", dispatch: true, launchDefaults: device
        ), "fable")
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: nil, rowModel: "opus", cli: "claude", dispatch: true, launchDefaults: device
        ), "opus")
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: nil, rowModel: nil, cli: "claude", dispatch: true, launchDefaults: device
        ), "sonnet")
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: nil, rowModel: nil, cli: nil, dispatch: true, launchDefaults: nil
        ), "haiku")
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: nil, rowModel: nil, cli: "codex", dispatch: true, launchDefaults: device
        ), DomainContract.deviceAgentDefaultsCodexThreadModel)
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: nil, rowModel: nil, cli: "claude", dispatch: false, launchDefaults: device
        ), "opus")
        XCTAssertEqual(LaunchVocabulary.composerModel(
            live: nil, rowModel: "", cli: "codex", dispatch: false, launchDefaults: nil
        ), DomainContract.deviceAgentDefaultsCodexModel)
    }
}
