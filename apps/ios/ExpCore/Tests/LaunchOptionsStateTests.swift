import Foundation
import XCTest
@testable import ExpCore

/// EXP-1277: the Agent page composer's launch options are SENDABLE from the
/// first render — claude's model is never blank, whichever path built or
/// reseeded the state. A blank rendered an empty Model chip and the server
/// refused the start ("Unknown claude model").
@MainActor
final class LaunchOptionsStateTests: XCTestCase {
    private func device(
        id: String,
        agents: [String] = ["claude"],
        claudeEmail: String? = "work@x.test",
        claudeModel: String? = nil,
        defaultAgent: String? = nil
    ) -> SteerDevice {
        var accounts: [String: AgentAccount] = [:]
        if let claudeEmail {
            accounts["claude"] = AgentAccount(
                signedIn: true,
                email: claudeEmail,
                profiles: [
                    AgentAccountProfile(id: "\(id)-p", active: true, signedIn: true, email: claudeEmail, health: "ok"),
                ]
            )
        }
        return SteerDevice(
            deviceId: id,
            deviceLabel: id,
            agents: agents,
            agentAccounts: accounts.isEmpty ? nil : accounts,
            launchDefaults: DeviceLaunchDefaults(
                defaultAgent: defaultAgent,
                agents: claudeModel.map { ["claude": AgentLaunchDefaults(model: $0)] }
            )
        )
    }

    func testFreshStateIsASendableClaudeLaunch() {
        let state = LaunchOptionsState()
        XCTAssertEqual(state.agent, "claude")
        XCTAssertEqual(state.model, DomainContract.codingModelValues.first)
        XCTAssertEqual(state.buildOptions().model, "fable")
    }

    func testSeedWithoutADeviceKeepsAValidModel() {
        let state = LaunchOptionsState()
        state.seed(from: nil)
        XCTAssertEqual(state.model, "fable")
        XCTAssertEqual(state.buildOptions().model, "fable")
    }

    func testSeedTakesTheMachinesAdvertisedModel() {
        let state = LaunchOptionsState()
        state.seed(from: device(id: "mac", claudeModel: "opus"))
        XCTAssertEqual(state.model, "opus")
        // An advertised value outside today's contract falls back, never
        // blank (another login, so nothing carries over from the first box).
        state.seed(from: device(id: "other", claudeEmail: "else@x.test", claudeModel: "future-model"))
        XCTAssertEqual(state.model, "fable")
    }

    func testCarryOverKeepsTheLoginAndClampsTheModel() {
        let state = LaunchOptionsState()
        let mac = device(id: "mac")
        state.seed(from: mac)
        state.selectAccount(state.accountOptions(on: mac)[0], device: mac)
        state.model = "opus"
        // Same login on the next machine (EXP-1278): the pick carries over.
        let laptop = device(id: "laptop", claudeModel: "sonnet")
        state.seed(from: laptop)
        XCTAssertEqual(state.account, "laptop-p")
        XCTAssertEqual(state.model, "opus")
        // A blank that slipped in (an unseeded state) is repaired on the way.
        state.model = ""
        state.seed(from: device(id: "third"))
        XCTAssertEqual(state.model, "fable")
    }

    func testSameAgentAccountPickNeverBlanksTheModel() {
        let state = LaunchOptionsState()
        let mac = device(id: "mac")
        // The agent is already claude, so `selectAgent` early-returns.
        state.selectAccount(state.accountOptions(on: mac)[0], device: mac)
        XCTAssertEqual(state.model, "fable")
        XCTAssertEqual(state.buildOptions().model, "fable")
    }

    func testWireNeverCarriesABlankClaudeModel() {
        let state = LaunchOptionsState()
        state.model = ""
        XCTAssertEqual(state.buildOptions().model, "fable")
        state.model = "not-a-model"
        XCTAssertEqual(state.buildOptions().model, "fable")
    }

    func testCodexSeedsItsExplicitContractDefault() {
        let state = LaunchOptionsState()
        // No login reported: the per-agent fallback rows, codex last used.
        let box = device(id: "box", agents: ["claude", "codex"], claudeEmail: nil, defaultAgent: "codex")
        state.seed(from: box)
        XCTAssertEqual(state.agent, "codex")
        XCTAssertEqual(state.model, DomainContract.deviceAgentDefaultsCodexModel)
        XCTAssertEqual(state.buildOptions().model, "gpt-5.6-sol")
        // A blank (a pre-explicit "CLI default") never rides the wire.
        state.model = ""
        XCTAssertEqual(state.buildOptions().model, "gpt-5.6-sol")
    }

    func testReconcileSeedsOnlyOnAnotherMachine() {
        let state = LaunchOptionsState()
        let mac = device(id: "mac", claudeModel: "opus")
        state.reconcile(device: mac)
        XCTAssertEqual(state.model, "opus")
        state.model = "sonnet"
        // The same machine again keeps the person's edit.
        state.reconcile(device: mac)
        XCTAssertEqual(state.model, "sonnet")
    }

    // EXP-1291: the options in `sessions.send` form — the agent rides as
    // `cli`, the claude-only toggle stays claude-only, nothing picked = no
    // MCP list, and an untouched computer-use flip stays off the wire.
    func testBuildOptionsIsTheSendForm() {
        let state = LaunchOptionsState()
        state.ultracode = true
        let claude = state.buildOptions()
        XCTAssertEqual(claude.cli, "claude")
        // S-3: plan mode left the launch keys.
        XCTAssertNil(claude.planMode)
        XCTAssertEqual(claude.ultracode, true)
        XCTAssertNil(claude.mcpServerIds)
        XCTAssertNil(claude.computerUse)
        XCTAssertNil(claude.account)

        state.selectAgent("codex", device: device(id: "mac", agents: ["claude", "codex"]))
        state.mcpServerIds = ["m-1"]
        state.computerUse = false
        let codex = state.buildOptions()
        XCTAssertEqual(codex.cli, "codex")
        XCTAssertNil(codex.ultracode)
        XCTAssertEqual(codex.mcpServerIds, ["m-1"])
        XCTAssertEqual(codex.computerUse, false)
    }
}
