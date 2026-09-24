import XCTest
@testable import ExpCore

// EXP-1029: the machine's WORKFLOW model pair, resolved against the DEFAULT
// agent's vocabulary. A model id belongs to one agent, so a stored value only
// counts while that agent is the default — anything else reads as the agent's
// contract default rather than reaching a CLI that cannot run it.
final class DeviceWorkflowSettingsTests: XCTestCase {

    func testAStoredPairOfTheAgentsOwnModelsWins() {
        let resolved = DeviceWorkflowSettings.resolve(
            agent: "claude",
            stored: DeviceWorkflowDefaults(model: "sonnet", strongModel: "opus")
        )
        XCTAssertEqual(resolved.model, "sonnet")
        XCTAssertEqual(resolved.strongModel, "opus")
    }

    func testAForeignAgentsModelFallsBackToTheAgentsDefault() {
        // codex ids stored while claude became the default (and vice versa).
        let claude = DeviceWorkflowSettings.resolve(
            agent: "claude",
            stored: DeviceWorkflowDefaults(model: "gpt-5.6-sol", strongModel: "gpt-5.6-luna")
        )
        XCTAssertEqual(claude.model, DomainContract.workflowLaunchClaudeModel)
        XCTAssertEqual(claude.strongModel, DomainContract.workflowLaunchClaudeStrongModel)

        let codex = DeviceWorkflowSettings.resolve(
            agent: "codex",
            stored: DeviceWorkflowDefaults(model: "opus", strongModel: "fable")
        )
        XCTAssertEqual(codex.model, DomainContract.workflowLaunchCodexModel)
        XCTAssertEqual(codex.strongModel, DomainContract.workflowLaunchCodexStrongModel)
    }

    func testAnAbsentOrBlankPairFallsBack() {
        let absent = DeviceWorkflowSettings.resolve(agent: "claude", stored: nil)
        XCTAssertEqual(absent.model, "opus")
        XCTAssertEqual(absent.strongModel, "fable")

        // A machine that stored one half only keeps that half.
        let half = DeviceWorkflowSettings.resolve(
            agent: "claude",
            stored: DeviceWorkflowDefaults(model: "", strongModel: "sonnet")
        )
        XCTAssertEqual(half.model, DomainContract.deviceAgentDefaultsWorkflowModel)
        XCTAssertEqual(half.strongModel, "sonnet")
    }

    func testTheDefaultsAndTheVocabularyArePerAgent() {
        XCTAssertEqual(
            DeviceWorkflowSettings.modelValues(for: "claude"), DomainContract.codingModelValues
        )
        XCTAssertEqual(
            DeviceWorkflowSettings.modelValues(for: "codex"), DomainContract.codexModelValues
        )
        // No "CLI default" blank on either rung: a workflow always names a
        // concrete model.
        XCTAssertFalse(DeviceWorkflowSettings.modelValues(for: "codex").contains(""))

        let codex = DeviceWorkflowSettings.defaults(for: "codex")
        XCTAssertEqual(codex.model, "gpt-5.6-sol")
        XCTAssertEqual(codex.strongModel, "gpt-5.6-luna")
        // An agent this build has no workflow vocabulary for reads as the
        // device-defaults pair rather than as blanks.
        let unknown = DeviceWorkflowSettings.defaults(for: "future-agent")
        XCTAssertEqual(unknown.model, DomainContract.deviceAgentDefaultsWorkflowModel)
        XCTAssertEqual(unknown.strongModel, DomainContract.deviceAgentDefaultsWorkflowStrongModel)
    }
}
