import Foundation
import XCTest
@testable import ExpCore

// EXP-850: the steer wire's `activity` payloads, decoded. Every case here is
// the JSON the engine actually publishes (`crates/steer/src/frames.rs`, pinned
// in `apps/steer-relay/STEER-WIRE-EXP-850.md`) — the EXP-850 additions
// verbatim from the wire captures, the older kinds in the shape the desktop
// has always sent them.
final class AgentActivityDecoderTests: XCTestCase {
    /// Decode one wire frame written as JSON text, exactly as the socket
    /// hands it over (`JSONSerialization`, so numbers arrive as `NSNumber`).
    private func decode(_ json: String) throws -> AgentActivityEvent? {
        let data = try XCTUnwrap(json.data(using: .utf8))
        let object = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: data) as? [String: Any]
        )
        return AgentActivityDecoder.decode(object)
    }

    // MARK: - Non-fatal by construction

    func testUnknownKindsAndUnusablePayloadsDecodeToNil() throws {
        // A newer desktop's kind — skipped, never fatal.
        XCTAssertNil(try decode(#"{"kind":"telepathy","mood":"blue"}"#))
        XCTAssertNil(AgentActivityDecoder.decode(nil))
        XCTAssertNil(AgentActivityDecoder.decode([:]))
        XCTAssertNil(AgentActivityDecoder.decode(["kind": 7]))
        // Row kinds whose required field is missing or blank.
        XCTAssertNil(try decode(#"{"kind":"narration","text":"   "}"#))
        XCTAssertNil(try decode(#"{"kind":"tool","detail":"x"}"#))
        XCTAssertNil(try decode(#"{"kind":"tool_update","status":"completed"}"#))
        XCTAssertNil(try decode(#"{"kind":"permission"}"#))
        XCTAssertNil(try decode(#"{"kind":"answer_ack"}"#))
        XCTAssertNil(try decode(#"{"kind":"subagent","id":"a1","status":"vanished"}"#))
        XCTAssertNil(try decode(#"{"kind":"workflow","name":"probe"}"#))
    }

    // MARK: - Transcript rows

    func testNarrationCarriesItsMessageLaneAndSplicePoint() throws {
        let event = try decode(#"""
        {"kind":"narration","text":"Looking at the file","messageId":"msg_1",
         "subagentId":"a1","beforeQuestionId":"ask_7"}
        """#)
        guard case let .narration(text, messageId, subagentId, before) = try XCTUnwrap(event)
        else { return XCTFail("not a narration") }
        XCTAssertEqual(text, "Looking at the file")
        XCTAssertEqual(messageId, "msg_1")
        XCTAssertEqual(subagentId, "a1")
        XCTAssertEqual(before, "ask_7")
    }

    func testAWaitToolRowKeepsItsContractKindAndLabel() throws {
        // The §1 capture, verbatim.
        let event = try decode(#"""
        {"kind":"tool","name":"TaskOutput","detail":"Sleep in the background",
         "id":"toolu_1","toolKind":"wait"}
        """#)
        guard case let .tool(name, detail, subagentId, callId, kind) = try XCTUnwrap(event)
        else { return XCTFail("not a tool") }
        XCTAssertEqual(name, "TaskOutput")
        XCTAssertEqual(detail, "Sleep in the background")
        XCTAssertNil(subagentId)
        XCTAssertEqual(callId, "toolu_1")
        XCTAssertEqual(kind, "wait")
        XCTAssertTrue(DomainContract.toolKindValues.contains("wait"))
    }

    func testAnUnknownToolKindIsDroppedRatherThanRendered() throws {
        let event = try decode(#"{"kind":"tool","name":"Telepathy","toolKind":"vibes"}"#)
        guard case let .tool(_, _, _, _, kind) = try XCTUnwrap(event)
        else { return XCTFail("not a tool") }
        XCTAssertNil(kind)
    }

    func testToolUpdateCarriesTheSettleTheDiffAndThePreview() throws {
        let event = try decode(#"""
        {"kind":"tool_update","id":"toolu_2","status":"failed","diff":"@@ -1 +1 @@",
         "preview":{"identifier":"EXP-849","count":3}}
        """#)
        guard case let .toolUpdate(update) = try XCTUnwrap(event)
        else { return XCTFail("not a tool_update") }
        XCTAssertEqual(update.id, "toolu_2")
        XCTAssertTrue(update.settles)
        XCTAssertTrue(update.failed)
        XCTAssertEqual(update.diff, "@@ -1 +1 @@")
        XCTAssertEqual(update.preview?.identifier, "EXP-849")
        XCTAssertEqual(update.preview?.count, 3)
        // A diff-only update never settles the call.
        let open = try decode(#"{"kind":"tool_update","id":"toolu_2","diff":"@@"}"#)
        guard case let .toolUpdate(second) = try XCTUnwrap(open)
        else { return XCTFail("not a tool_update") }
        XCTAssertFalse(second.settles)
    }

    func testDiffAndUserMessageAndPermission() throws {
        guard case let .diff(diff) = try XCTUnwrap(try decode(#"{"kind":"diff","diff":"@@ x"}"#))
        else { return XCTFail("not a diff") }
        XCTAssertEqual(diff, "@@ x")
        // An empty diff CLEARS the chip.
        guard case let .diff(empty) = try XCTUnwrap(try decode(#"{"kind":"diff","diff":""}"#))
        else { return XCTFail("not a diff") }
        XCTAssertNil(empty)

        guard case let .userMessage(text, subagentId) = try XCTUnwrap(
            try decode(#"{"kind":"user_message","text":"go on","subagentId":"a1"}"#)
        ) else { return XCTFail("not a user_message") }
        XCTAssertEqual(text, "go on")
        XCTAssertEqual(subagentId, "a1")

        guard case let .permission(tool, detail) = try XCTUnwrap(
            try decode(#"{"kind":"permission","tool":"Bash","detail":"rm -rf"}"#)
        ) else { return XCTFail("not a permission") }
        XCTAssertEqual(tool, "Bash")
        XCTAssertEqual(detail, "rm -rf")
    }

    func testQuestionDecodesItsStepsAndOptionFlags() throws {
        let event = try decode(#"""
        {"kind":"question","id":"q_1","askId":"ask_1","index":2,"total":3,
         "header":"Colors","text":"Which color?","multiSelect":true,"planMode":false,
         "options":[{"label":"Red","key":"1","description":"warm"},
                    {"label":"Type something.","key":"free","freeText":true},
                    {"label":"skipped","key":""}]}
        """#)
        guard case let .question(draft) = try XCTUnwrap(event)
        else { return XCTFail("not a question") }
        XCTAssertEqual(draft.wireId, "q_1")
        XCTAssertEqual(draft.askId, "ask_1")
        XCTAssertEqual(draft.index, 2)
        XCTAssertEqual(draft.total, 3)
        XCTAssertEqual(draft.header, "Colors")
        XCTAssertTrue(draft.multiSelect)
        XCTAssertFalse(draft.planMode)
        // A key-less option is unanswerable and is dropped.
        XCTAssertEqual(draft.options.map(\.key), ["1", "free"])
        XCTAssertEqual(draft.options[0].description, "warm")
        XCTAssertTrue(draft.options[1].freeText)
        // The local feed id is stamped by the caller, never by the wire.
        XCTAssertEqual(draft.question(id: 42).id, 42)
        // A card with no answerable option is not a card.
        XCTAssertNil(try decode(#"{"kind":"question","id":"q","text":"?","options":[]}"#))
    }

    func testQuestionResolvedAndAnswerAck() throws {
        guard case let .questionResolved(id, askId, answers, dismissed) = try XCTUnwrap(
            try decode(#"""
            {"kind":"question_resolved","askId":"ask_1","answers":["Red"],"dismissed":true}
            """#)
        ) else { return XCTFail("not a question_resolved") }
        XCTAssertNil(id)
        XCTAssertEqual(askId, "ask_1")
        XCTAssertEqual(answers, ["Red"])
        XCTAssertTrue(dismissed)

        guard case let .answerAck(acked) = try XCTUnwrap(
            try decode(#"{"kind":"answer_ack","id":"q_1"}"#)
        ) else { return XCTFail("not an answer_ack") }
        XCTAssertEqual(acked, "q_1")
    }

    // MARK: - EXP-856: the duplicate subagent edge (§4)

    func testTheDuplicateEdgeDecodesWithItsWorkflowAndSentence() throws {
        // The capture, verbatim.
        let event = try decode(#"""
        {"kind":"subagent","id":"a55b7012793deae02","agentType":"general-purpose",
         "status":"duplicate",
         "detail":"Second copy of slowpoke started while the first is still running (resumed by SendMessage)",
         "title":"slowpoke","workflowId":"toolu_017Lh63mYhRJ3MrA4A1PXytt"}
        """#)
        guard case let .subagent(id, agentType, status, detail, calls, title, workflowId) =
            try XCTUnwrap(event) else { return XCTFail("not a subagent") }
        XCTAssertEqual(id, "a55b7012793deae02")
        XCTAssertEqual(agentType, "general-purpose")
        XCTAssertEqual(status, .duplicate)
        XCTAssertEqual(
            detail,
            "Second copy of slowpoke started while the first is still running (resumed by SendMessage)"
        )
        XCTAssertNil(calls)
        XCTAssertEqual(title, "slowpoke")
        XCTAssertEqual(workflowId, "toolu_017Lh63mYhRJ3MrA4A1PXytt")
        XCTAssertEqual(AgentSubagentStatus.allCases.map(\.rawValue), DomainContract.subagentStatusValues)
    }

    func testAnOrdinarySubagentEdgeStillDecodesWithoutTheNewFields() throws {
        let event = try decode(#"""
        {"kind":"subagent","id":"a1","agentType":"general-purpose","status":"completed",
         "detail":"reviewed the diff","toolCalls":7}
        """#)
        guard case let .subagent(_, _, status, _, calls, title, workflowId) = try XCTUnwrap(event)
        else { return XCTFail("not a subagent") }
        XCTAssertEqual(status, .completed)
        XCTAssertEqual(calls, 7)
        XCTAssertNil(title)
        XCTAssertNil(workflowId)
    }

    // MARK: - EXP-850 §2: background_tasks

    func testBackgroundTasksDecodeTheFullListAndItsClose() throws {
        // The capture, verbatim.
        let event = try decode(#"""
        {"kind":"background_tasks","tasks":[{"id":"b4mwz6csc","kind":"shell",
         "description":"Sleep in the background","toolId":"toolu_01MCRoRaXN1cvEsHJzDEg2B3"}]}
        """#)
        guard case let .backgroundTasks(tasks) = try XCTUnwrap(event)
        else { return XCTFail("not background_tasks") }
        XCTAssertEqual(tasks.count, 1)
        XCTAssertEqual(tasks[0].id, "b4mwz6csc")
        XCTAssertEqual(tasks[0].kind, "shell")
        XCTAssertEqual(tasks[0].description, "Sleep in the background")
        XCTAssertEqual(tasks[0].toolId, "toolu_01MCRoRaXN1cvEsHJzDEg2B3")

        // The closing frame: an empty array closes the strip.
        guard case let .backgroundTasks(empty) = try XCTUnwrap(
            try decode(#"{"kind":"background_tasks","tasks":[]}"#)
        ) else { return XCTFail("not background_tasks") }
        XCTAssertTrue(empty.isEmpty)
    }

    func testABackgroundTaskKindThisBuildDoesNotKnowFoldsOntoOther() throws {
        let event = try decode(#"""
        {"kind":"background_tasks","tasks":[{"id":"b1","kind":"telepathy","description":"?"}]}
        """#)
        guard case let .backgroundTasks(tasks) = try XCTUnwrap(event)
        else { return XCTFail("not background_tasks") }
        XCTAssertEqual(tasks[0].kind, "other")
        XCTAssertNil(tasks[0].toolId)
        XCTAssertEqual(
            DomainContract.backgroundTaskKindValues, ["shell", "workflow", "agent", "other"]
        )
    }

    // MARK: - EXP-861: queue

    func testQueueDecodesTheFullQueueInOrderAndItsClear() throws {
        // The wire shape verbatim: the FULL queue, oldest first, `at` ignored.
        let event = try decode(#"""
        {"kind":"queue","messages":[{"id":"m1","text":"also check the tests"},
         {"id":"m2","text":"![image](/api/attachments/att-1) and this"}],"at":123}
        """#)
        guard case let .queue(messages) = try XCTUnwrap(event)
        else { return XCTFail("not queue") }
        XCTAssertEqual(messages, [
            QueuedMessage(id: "m1", text: "also check the tests"),
            QueuedMessage(id: "m2", text: "![image](/api/attachments/att-1) and this"),
        ])

        // An EMPTY array = nothing queued: the strip clears.
        guard case let .queue(empty) = try XCTUnwrap(
            try decode(#"{"kind":"queue","messages":[],"at":124}"#)
        ) else { return XCTFail("not queue") }
        XCTAssertTrue(empty.isEmpty)
    }

    func testAQueueRowWithoutAnIdOrTextIsSkippedAndAMissingListReadsEmpty() throws {
        guard case let .queue(messages) = try XCTUnwrap(try decode(#"""
        {"kind":"queue","messages":[{"id":"m1"},{"text":"orphan"},{"id":"m3","text":"kept"}]}
        """#)) else { return XCTFail("not queue") }
        XCTAssertEqual(messages, [QueuedMessage(id: "m3", text: "kept")])
        guard case let .queue(none) = try XCTUnwrap(try decode(#"{"kind":"queue"}"#))
        else { return XCTFail("not queue") }
        XCTAssertTrue(none.isEmpty)
    }

    // MARK: - EXP-850 §3: workflow

    func testTheWorkflowCardDecodesPhasesAgentsAndTelemetry() throws {
        // The capture, verbatim (field order IS serialization order).
        let event = try decode(#"""
        {"kind":"workflow","id":"toolu_017aGvi2moAfSykrRA4LmyT4","name":"wire-probe",
         "description":"Probe the workflow progress wire","status":"running",
         "phases":[{"index":1,"title":"Alpha"},{"index":2,"title":"Beta"}],
         "agents":[{"index":1,"label":"alpha:one","phaseIndex":1,"agentId":"a0ce244c651aaa623",
                    "model":"claude-haiku-4-5-20251001","state":"done","tokens":9629,
                    "toolCalls":0,"durationMs":1075,"resultPreview":"ok"},
                   {"index":2,"label":"alpha:two","phaseIndex":1,"state":"queued"}],
         "summary":"Dynamic workflow \"…\" completed"}
        """#)
        guard case let .workflow(workflow) = try XCTUnwrap(event)
        else { return XCTFail("not a workflow") }
        XCTAssertEqual(workflow.id, "toolu_017aGvi2moAfSykrRA4LmyT4")
        XCTAssertEqual(workflow.name, "wire-probe")
        XCTAssertEqual(workflow.description, "Probe the workflow progress wire")
        XCTAssertEqual(workflow.status, .running)
        XCTAssertEqual(workflow.phases.map(\.title), ["Alpha", "Beta"])
        XCTAssertEqual(workflow.agents.count, 2)
        let first = workflow.agents[0]
        XCTAssertEqual(first.label, "alpha:one")
        XCTAssertEqual(first.phaseIndex, 1)
        XCTAssertEqual(first.agentId, "a0ce244c651aaa623")
        XCTAssertEqual(first.model, "claude-haiku-4-5-20251001")
        XCTAssertEqual(first.state, .done)
        XCTAssertEqual(first.tokens, 9629)
        XCTAssertEqual(first.toolCalls, 0)
        XCTAssertEqual(first.durationMs, 1075)
        XCTAssertEqual(first.resultPreview, "ok")
        XCTAssertEqual(workflow.agents[1].state, .queued)
        XCTAssertNil(workflow.agents[1].agentId)
        XCTAssertEqual(workflow.summary, "Dynamic workflow \"…\" completed")
        XCTAssertEqual(workflow.agentIds, ["a0ce244c651aaa623"])
        XCTAssertEqual(workflow.caption, "Workflow wire-probe · 1/2 agents done · Alpha")
    }

    func testAWorkflowStateThisBuildDoesNotKnowStaysReadable() throws {
        let event = try decode(#"""
        {"kind":"workflow","id":"w1","name":"probe","status":"melting",
         "agents":[{"index":1,"state":"levitating"}]}
        """#)
        guard case let .workflow(workflow) = try XCTUnwrap(event)
        else { return XCTFail("not a workflow") }
        // An unknown status keeps the card readable (running) and an unknown
        // agent state parks the agent as queued — never a dropped card.
        XCTAssertEqual(workflow.status, .running)
        XCTAssertEqual(workflow.agents[0].state, .queued)
        XCTAssertEqual(workflow.agents[0].label, "Agent 1")
        XCTAssertEqual(
            DomainContract.workflowStatusValues, AgentWorkflowStatus.allCases.map(\.rawValue)
        )
        XCTAssertEqual(
            DomainContract.workflowAgentStateValues,
            AgentWorkflowAgentState.allCases.map(\.rawValue)
        )
    }

    // MARK: - EXP-850 §5: turn

    func testTheTurnEdgeCarriesItsClockAndTokens() throws {
        // The capture, verbatim.
        guard case let .turn(edge) = try XCTUnwrap(
            try decode(#"{"kind":"turn","state":"started","startedAt":1789204409163,"tokens":1432}"#)
        ) else { return XCTFail("not a turn") }
        XCTAssertEqual(edge.state, .started)
        XCTAssertEqual(edge.startedAt, 1_789_204_409_163)
        XCTAssertEqual(edge.tokens, 1432)

        // A pre-EXP-850 publisher sends the edge alone.
        guard case let .turn(bare) = try XCTUnwrap(try decode(#"{"kind":"turn","state":"ended"}"#))
        else { return XCTFail("not a turn") }
        XCTAssertEqual(bare.state, .ended)
        XCTAssertNil(bare.startedAt)
        XCTAssertNil(bare.tokens)
    }

    // MARK: - The latest-wins slots

    func testConfigStateUsageAndRateLimitDecodeIntoSlotUpdates() throws {
        guard case let .configState(config) = try XCTUnwrap(
            try decode(#"""
            {"kind":"config_state","options":[],"currentMode":"plan",
             "modes":[{"id":"plan","label":"Plan"},{"id":"bypassPermissions","label":"Build"}],
             "commands":[{"name":"clear","description":"Start fresh"}]}
            """#)
        ) else { return XCTFail("not a config_state") }
        guard case let .set(value) = config else { return XCTFail("expected a set") }
        XCTAssertEqual(value.currentMode, "plan")
        XCTAssertEqual(value.modes.map(\.id), ["plan", "bypassPermissions"])
        XCTAssertEqual(value.commands.map(\.name), ["clear"])
        // A frame with no `options` is not a config_state this build can read.
        guard case let .configState(keep) = try XCTUnwrap(try decode(#"{"kind":"config_state"}"#))
        else { return XCTFail("not a config_state") }
        XCTAssertEqual(keep, .keep)

        guard case let .usage(usage) = try XCTUnwrap(
            try decode(#"{"kind":"usage","contextUsed":1000,"contextSize":200000,"costUsd":0.5}"#)
        ) else { return XCTFail("not a usage") }
        XCTAssertEqual(usage, .set(AgentSessionUsage(contextUsed: 1000, contextSize: 200_000, costUsd: 0.5)))
        guard case let .usage(cleared) = try XCTUnwrap(
            try decode(#"{"kind":"usage","contextUsed":0,"contextSize":0}"#)
        ) else { return XCTFail("not a usage") }
        XCTAssertEqual(cleared, .clear)

        guard case let .rateLimit(limit) = try XCTUnwrap(
            try decode(#"{"kind":"rate_limit","status":"rejected","resetsAt":1789204409163}"#)
        ) else { return XCTFail("not a rate_limit") }
        XCTAssertEqual(limit, .set(AgentSessionRateLimit(status: "rejected", resetsAt: 1_789_204_409_163)))
        // `ok`/empty (and an unreadable payload) CLEAR the wall.
        guard case let .rateLimit(ok) = try XCTUnwrap(
            try decode(#"{"kind":"rate_limit","status":"ok"}"#)
        ) else { return XCTFail("not a rate_limit") }
        XCTAssertEqual(ok, .clear)
    }

    func testCompactionEdgesAreReadableInBothDirections() throws {
        guard case let .compaction(started) = try XCTUnwrap(
            try decode(#"{"kind":"compaction","phase":"started","trigger":"manual","at":1700}"#)
        ) else { return XCTFail("not a compaction") }
        XCTAssertTrue(started.started)
        XCTAssertEqual(started.trigger, "manual")
        XCTAssertEqual(started.at, 1700)
        guard case let .compaction(ended) = try XCTUnwrap(
            try decode(#"{"kind":"compaction","phase":"ended"}"#)
        ) else { return XCTFail("not a compaction") }
        XCTAssertTrue(ended.ended)
        XCTAssertFalse(ended.started)
    }
}
