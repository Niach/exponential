import Foundation
import XCTest
@testable import ExpCore

// EXP-825: `steer.startSession`'s three subject payloads gain an optional
// `prompt` (and, EXP-792, an optional `account`). Absent vs present is
// load-bearing: the server treats a missing `prompt` as "no instructions",
// REQUIRES one for the Chat / Create action builtins, and refuses it on a
// resume — so the encoders are pinned rather than left to Codable's defaults.
final class SteerStartInputEncodingTests: XCTestCase {
    private func json(_ value: some Encodable) throws -> [String: Any] {
        let data = try JSONEncoder().encode(value)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    // MARK: - Single issue

    func testIssueStartOmitsPromptAndAccountWhenUnset() throws {
        let object = try json(StartSessionInput(
            issueId: "i-1", deviceId: "d-1", agent: "claude", model: "opus",
            subagentModel: nil, effort: "", ultracode: false, planMode: true,
            resume: nil, account: nil, prompt: nil, stack: nil
        ))
        XCTAssertEqual(object["issueId"] as? String, "i-1")
        XCTAssertEqual(object["deviceId"] as? String, "d-1")
        XCTAssertEqual(object["agent"] as? String, "claude")
        XCTAssertEqual(object["planMode"] as? Bool, true)
        XCTAssertNil(object.index(forKey: "prompt"))
        XCTAssertNil(object.index(forKey: "account"))
        XCTAssertNil(object.index(forKey: "resume"))
        // EXP-897: a plain start never mentions the stack at all.
        XCTAssertNil(object.index(forKey: "stack"))
    }

    // EXP-897: a STACKED start is the single-issue input carrying `stack:
    // true` — the flag is sent only when it is true, and only here (the batch
    // and action inputs have no such field).
    func testIssueStartCarriesTheStackFlagWhenStacked() throws {
        let object = try json(StartSessionInput(
            issueId: "i-1", deviceId: "d-1", agent: nil, model: nil,
            subagentModel: nil, effort: nil, ultracode: nil, planMode: nil,
            resume: nil, account: nil, prompt: nil, stack: true
        ))
        XCTAssertEqual(object["stack"] as? Bool, true)
        XCTAssertEqual(object["issueId"] as? String, "i-1")
    }

    func testIssueStartCarriesThePromptVerbatim() throws {
        let prompt = "Focus on the parser [Image #1]\n\n![image](/api/attachments/att-1)"
        let object = try json(StartSessionInput(
            issueId: "i-1", deviceId: "d-1", agent: nil, model: nil,
            subagentModel: nil, effort: nil, ultracode: nil, planMode: nil,
            resume: true, account: "profile-2", prompt: prompt, stack: nil
        ))
        XCTAssertEqual(object["prompt"] as? String, prompt)
        XCTAssertEqual(object["account"] as? String, "profile-2")
        XCTAssertEqual(object["resume"] as? Bool, true)
        // Nil options stay absent (the desktop's own defaults apply).
        XCTAssertNil(object.index(forKey: "agent"))
        XCTAssertNil(object.index(forKey: "planMode"))
    }

    // MARK: - Batch

    func testBatchStartOmitsPromptWhenUnset() throws {
        let object = try json(StartBatchSessionInput(
            issueIds: ["a", "b"], deviceId: "d-1", agent: "codex", model: "",
            subagentModel: nil, effort: "high", ultracode: nil, planMode: nil,
            account: nil, prompt: nil
        ))
        XCTAssertEqual(object["issueIds"] as? [String], ["a", "b"])
        XCTAssertNil(object.index(forKey: "prompt"))
        XCTAssertNil(object.index(forKey: "issueId"))
        // A batch never carries `resume` at all.
        XCTAssertNil(object.index(forKey: "resume"))
    }

    func testBatchStartCarriesThePrompt() throws {
        let object = try json(StartBatchSessionInput(
            issueIds: ["a", "b"], deviceId: "d-1", agent: nil, model: nil,
            subagentModel: nil, effort: nil, ultracode: nil, planMode: nil,
            account: nil, prompt: "Share one migration."
        ))
        XCTAssertEqual(object["prompt"] as? String, "Share one migration.")
    }

    // MARK: - Action

    func testActionStartOmitsPromptTeamAndInputsWhenUnset() throws {
        let object = try json(StartActionSessionInput(
            actionId: "act-1", teamId: nil, workflowId: nil, deviceId: "d-1",
            agent: nil, model: nil, subagentModel: nil, effort: nil,
            ultracode: nil, planMode: nil, inputs: nil, account: nil, prompt: nil
        ))
        XCTAssertEqual(object["actionId"] as? String, "act-1")
        XCTAssertNil(object.index(forKey: "prompt"))
        XCTAssertNil(object.index(forKey: "teamId"))
        XCTAssertNil(object.index(forKey: "inputs"))
    }

    // The hidden Chat builtin: the chat text IS the prompt, the optional repo
    // the only input, and `teamId` rides because the virtual row has no
    // server-side id.
    func testChatStartCarriesPromptTeamAndRepo() throws {
        let object = try json(StartActionSessionInput(
            actionId: DomainContract.builtinChatId, teamId: "t-1", workflowId: nil,
            deviceId: "d-1", agent: "claude", model: "opus", subagentModel: nil,
            effort: "", ultracode: false, planMode: false,
            inputs: ["repo": "repo-1"], account: nil, prompt: "Summarize the open bugs"
        ))
        XCTAssertEqual(object["actionId"] as? String, "builtin:chat")
        XCTAssertEqual(object["teamId"] as? String, "t-1")
        XCTAssertEqual(object["prompt"] as? String, "Summarize the open bugs")
        XCTAssertEqual(object["inputs"] as? [String: String], ["repo": "repo-1"])
        // `prompt` is never an input key any more.
        XCTAssertNil((object["inputs"] as? [String: String])?["prompt"])
    }

    // MARK: - Resume (EXP-637, EXP-849)

    // A plain Resume names nothing but the run and its machine: the recorded
    // agent and options are the device's to re-apply, and the server REFUSES
    // any of them here.
    func testResumeOmitsTheAccountWhenUnset() throws {
        let object = try json(ResumeSessionInput(
            resumeSessionId: "sess-1", deviceId: "d-1", account: nil
        ))
        XCTAssertEqual(object["resumeSessionId"] as? String, "sess-1")
        XCTAssertEqual(object["deviceId"] as? String, "d-1")
        XCTAssertNil(object.index(forKey: "account"))
        // Still the one subject, with no launch options smuggled alongside.
        XCTAssertNil(object.index(forKey: "agent"))
        XCTAssertNil(object.index(forKey: "prompt"))
        XCTAssertNil(object.index(forKey: "resume"))
    }

    // EXP-849: a "switch account" IS a resume naming another login profile —
    // the ONE option a resume may carry.
    func testResumeCarriesTheAccountProfile() throws {
        let object = try json(ResumeSessionInput(
            resumeSessionId: "sess-1", deviceId: "d-1", account: "profile-2"
        ))
        XCTAssertEqual(object["account"] as? String, "profile-2")
        XCTAssertEqual(object.count, 3)
    }

    // MARK: - Plan workflow (EXP-981)

    // The hidden Plan-workflow builtin: `workflowId` names the DRAFT it plans
    // (the server writes the prompt's `Workflow: <uuid>` first line itself),
    // `teamId` rides like every builtin, and the free text — additional
    // instructions — is optional.
    func testPlanWorkflowStartCarriesTheWorkflowId() throws {
        let object = try json(StartActionSessionInput(
            actionId: DomainContract.builtinPlanWorkflowId, teamId: "t-1",
            workflowId: "wf-1", deviceId: "d-1", agent: nil, model: nil,
            subagentModel: nil, effort: nil, ultracode: nil, planMode: nil,
            inputs: nil, account: nil, prompt: nil
        ))
        XCTAssertEqual(object["actionId"] as? String, "builtin:plan-workflow")
        XCTAssertEqual(object["workflowId"] as? String, "wf-1")
        XCTAssertEqual(object["teamId"] as? String, "t-1")
        XCTAssertNil(object.index(forKey: "prompt"))
        XCTAssertNil(object.index(forKey: "inputs"))
    }

    // Every other subject leaves the field out entirely — the server refuses
    // `workflowId` beside anything but that builtin.
    func testOtherStartsNeverMentionTheWorkflow() throws {
        let action = try json(StartActionSessionInput(
            actionId: "act-1", teamId: nil, workflowId: nil, deviceId: "d-1",
            agent: nil, model: nil, subagentModel: nil, effort: nil,
            ultracode: nil, planMode: nil, inputs: nil, account: nil, prompt: nil
        ))
        XCTAssertNil(action.index(forKey: "workflowId"))
    }

    // MARK: - Subagent model (EXP-981)

    // Claude-only, and "default" is the ABSENT field: the server validates the
    // value against the contract `codingModel` list, which has no blank, so a
    // `""` would be refused outright.
    func testSubagentModelRidesEveryStartAndIsOmittedByDefault() throws {
        let issue = try json(StartSessionInput(
            issueId: "i-1", deviceId: "d-1", agent: "claude", model: "opus",
            subagentModel: "sonnet", effort: nil, ultracode: nil, planMode: nil,
            resume: nil, account: nil, prompt: nil, stack: nil
        ))
        XCTAssertEqual(issue["subagentModel"] as? String, "sonnet")
        let batch = try json(StartBatchSessionInput(
            issueIds: ["a", "b"], deviceId: "d-1", agent: "claude", model: nil,
            subagentModel: nil, effort: nil, ultracode: nil, planMode: nil,
            account: nil, prompt: nil
        ))
        XCTAssertNil(batch.index(forKey: "subagentModel"))
    }

    // MARK: - Options → wire

    func testSteerStartOptionsCarryTheAccountProfile() {
        let options = SteerStartOptions(agent: "claude", account: "profile-1")
        XCTAssertEqual(options.account, "profile-1")
        XCTAssertNil(SteerStartOptions().account)
    }
}
