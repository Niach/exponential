import XCTest
@testable import ExpCore

// EXP-1291: the `sessions.*` / `pullRequests.*` wire payloads. `sessions.send`
// takes ONE flat object whose present keys pick the form, so every nil must
// be OMITTED (the router's fields are `.optional()`, never `.nullable()`),
// and each form carries only its own keys.
final class SessionsApiWireTests: XCTestCase {
    private func json(_ value: some Encodable) throws -> [String: Any] {
        let data = try JSONEncoder().encode(value)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func testNewThreadCarriesTheRootFacetsAndOmitsEveryNil() throws {
        let bare = try json(SessionsSendInput.newThread(teamId: "t-1", deviceId: "dev-1", text: "Go"))
        XCTAssertEqual(bare.keys.sorted(), ["deviceId", "teamId", "text"])

        let full = try json(SessionsSendInput.newThread(
            teamId: "t-1",
            deviceId: "dev-1",
            options: SessionLaunchOptions(
                cli: "claude", account: "p-1", model: "fable", effort: "",
                planMode: true, computerUse: false, ultracode: true, mcpServerIds: ["m-1"]
            ),
            text: "Fix it",
            attachmentIds: ["att-1"],
            issueIds: ["i-1", "i-2"],
            actionId: "a-1",
            inputs: ["repo": "r-1"],
            repositoryId: "r-1"
        ))
        XCTAssertEqual(
            full.keys.sorted(),
            ["account", "actionId", "attachmentIds", "cli", "computerUse", "deviceId", "effort",
             "inputs", "issueIds", "mcpServerIds", "model", "planMode", "repositoryId", "teamId",
             "text", "ultracode"]
        )
        XCTAssertEqual(full["issueIds"] as? [String], ["i-1", "i-2"])
        XCTAssertEqual(full["inputs"] as? [String: String], ["repo": "r-1"])
        XCTAssertEqual(full["effort"] as? String, "")
        XCTAssertEqual(full["planMode"] as? Bool, true)
        XCTAssertEqual(full["computerUse"] as? Bool, false)
    }

    func testNewAgentNestsItsFacetsWithWholeTriggers() throws {
        let daily = TriggerWhen.schedule(ScheduleTrigger(interval: "daily", minuteOfDay: 540))
        let object = try json(SessionsSendInput.newAgent(
            teamId: "t-1",
            deviceId: "dev-1",
            options: SessionLaunchOptions(account: "p-1"),
            agent: AgentFacetsInput(
                name: "Caretaker", icon: "bot", color: "#4f46e5",
                instructions: "Keep the board tidy",
                triggers: [AgentTriggerInput(when: daily)]
            )
        ))
        XCTAssertEqual(object.keys.sorted(), ["account", "agent", "deviceId", "teamId"])
        let agent = try XCTUnwrap(object["agent"] as? [String: Any])
        XCTAssertEqual(
            agent.keys.sorted(), ["color", "icon", "instructions", "name", "triggers"]
        )
        let trigger = try XCTUnwrap((agent["triggers"] as? [[String: Any]])?.first)
        XCTAssertNil(trigger["id"])
        XCTAssertEqual(trigger["kind"] as? String, "schedule")
        XCTAssertEqual(trigger["minuteOfDay"] as? Int, 540)
        XCTAssertEqual(trigger["enabled"] as? Bool, true)
    }

    func testChildAndMessageFormsCarryOnlyTheirKeys() throws {
        let child = try json(SessionsSendInput.child(parentId: "s-1", deviceId: "dev-2", text: "Part two"))
        XCTAssertEqual(child.keys.sorted(), ["deviceId", "parentId", "text"])

        let message = try json(SessionsSendInput.message(sessionId: "s-1", text: "Ship it"))
        XCTAssertEqual(message.keys.sorted(), ["sessionId", "text"])
        let files = try json(SessionsSendInput.message(sessionId: "s-1", attachmentIds: ["att-1"]))
        XCTAssertEqual(files.keys.sorted(), ["attachmentIds", "sessionId"])
    }

    /// EXP-1323: a `/btw` aside names its parent, the question and the flag.
    func testAnAsideSendsParentTextAndTheFlag() throws {
        let wire = try json(SessionsSendInput.aside(parentId: "s-1", text: "what does useCart do?"))
        XCTAssertEqual(wire.keys.sorted(), ["aside", "parentId", "text"])
        XCTAssertEqual(wire["aside"] as? Bool, true)
        XCTAssertEqual(wire["parentId"] as? String, "s-1")
    }

    /// EXP-1323: `sessions.startSuggestion` omits the unpicked runner fields.
    func testStartSuggestionOmitsUnpickedFields() throws {
        let bare = try json(StartSuggestionInput(
            messageId: "m", itemId: "i", where: .here, deviceId: nil, account: nil, model: nil
        ))
        XCTAssertEqual(bare.keys.sorted(), ["itemId", "messageId", "where"])
        XCTAssertEqual(bare["where"] as? String, "here")
        let picked = try json(StartSuggestionInput(
            messageId: "m", itemId: "i", where: .child, deviceId: "dev", account: nil, model: "opus"
        ))
        XCTAssertEqual(picked.keys.sorted(), ["deviceId", "itemId", "messageId", "model", "where"])
    }

    func testAccountSwitchSendsOnlySessionIdAndAccount() throws {
        let wire = try json(SessionsSendInput.switchAccount(sessionId: "s-1", account: "work"))
        XCTAssertEqual(wire.keys.sorted(), ["account", "sessionId"])
        XCTAssertEqual(wire["account"] as? String, "work")
    }

    func testSendResultDecodesWithAndWithoutAMessage() throws {
        let withMessage = try JSONDecoder().decode(
            SessionsSendResult.self,
            from: Data(#"{"sessionId":"s-1","messageId":"m-1","txid":42}"#.utf8)
        )
        XCTAssertEqual(withMessage, SessionsSendResult(sessionId: "s-1", messageId: "m-1"))
        let start = try JSONDecoder().decode(
            SessionsSendResult.self,
            from: Data(#"{"sessionId":"s-2","messageId":null,"txid":43}"#.utf8)
        )
        XCTAssertNil(start.messageId)
    }

    func testAnswerOmitsTheUnusedHalf() throws {
        let question = try json(AnswerMessageInput(
            messageId: "m-1", answer: "Board A", decision: nil, fresh: nil, feedback: nil
        ))
        XCTAssertEqual(question.keys.sorted(), ["answer", "messageId"])
        let plan = try json(AnswerMessageInput(
            messageId: "m-2", answer: nil, decision: .rejected, fresh: nil, feedback: "Smaller steps"
        ))
        XCTAssertEqual(plan.keys.sorted(), ["decision", "feedback", "messageId"])
        XCTAssertEqual(plan["decision"] as? String, "rejected")
        // ask-dock.json: "Yes, and start with a fresh context".
        let fresh = try json(AnswerMessageInput(
            messageId: "m-3", answer: nil, decision: .approved, fresh: true, feedback: nil
        ))
        XCTAssertEqual(fresh.keys.sorted(), ["decision", "fresh", "messageId"])
        XCTAssertEqual(fresh["fresh"] as? Bool, true)
    }

    // EXP-1280: a plan's answers by question id + the surface's data model;
    // a question card's picked labels.
    func testAnswerCarriesAnswersAndSurfaceData() throws {
        let plan = try json(AnswerMessageInput(
            messageId: "m-4", answer: nil, decision: .approved, fresh: nil, feedback: nil,
            answers: .plan(["layout": ["Two columns"], "platforms": ["iOS", "Android"]]),
            surfaceData: .object(["accent": .string("blue"), "count": .number(2), "on": .bool(true)])
        ))
        XCTAssertEqual(plan.keys.sorted(), ["answers", "decision", "messageId", "surfaceData"])
        XCTAssertEqual(plan["answers"] as? [String: [String]], ["layout": ["Two columns"], "platforms": ["iOS", "Android"]])
        let data = try XCTUnwrap(plan["surfaceData"] as? [String: Any])
        XCTAssertEqual(data["accent"] as? String, "blue")
        XCTAssertEqual(data["count"] as? Double, 2)
        XCTAssertEqual(data["on"] as? Bool, true)
        let question = try json(AnswerMessageInput(
            messageId: "m-5", answer: "Web, Android", decision: nil, fresh: nil, feedback: nil,
            answers: .question(["Web", "Android"])
        ))
        XCTAssertEqual(question.keys.sorted(), ["answer", "answers", "messageId"])
        XCTAssertEqual(question["answers"] as? [String], ["Web", "Android"])
    }

    // Omit keeps, an explicit null clears (`account` / `model` only), and an
    // empty trigger array still travels.
    func testUpdateAgentOmitVersusNull() throws {
        let empty = try json(UpdateAgentInput(sessionId: "ag-1", patch: AgentPatch()))
        XCTAssertEqual(empty.keys.sorted(), ["sessionId"])
        XCTAssertTrue(AgentPatch().isEmpty)

        let cleared = try json(UpdateAgentInput(
            sessionId: "ag-1",
            patch: AgentPatch(name: "Renamed", triggers: [], account: .some(nil), model: .some(nil))
        ))
        XCTAssertEqual(cleared.keys.sorted(), ["account", "model", "name", "sessionId", "triggers"])
        XCTAssertTrue(cleared["account"] is NSNull)
        XCTAssertTrue(cleared["model"] is NSNull)
        XCTAssertEqual((cleared["triggers"] as? [Any])?.count, 0)

        let moved = try json(UpdateAgentInput(
            sessionId: "ag-1", patch: AgentPatch(deviceId: "dev-2", account: .some("p-2"), sortOrder: 3)
        ))
        XCTAssertEqual(moved["deviceId"] as? String, "dev-2")
        XCTAssertEqual(moved["account"] as? String, "p-2")
        XCTAssertEqual(moved["sortOrder"] as? Double, 3)
    }

    func testPullRequestMergeOmitsAnUnsetStack() throws {
        let single = try json(MergePullRequestInput(pullRequestId: "pr-1", stack: nil))
        XCTAssertEqual(single.keys.sorted(), ["pullRequestId"])
        let stack = try json(MergePullRequestInput(pullRequestId: "pr-1", stack: true))
        XCTAssertEqual(stack["stack"] as? Bool, true)

        let result = try JSONDecoder().decode(
            PullRequestMergeResult.self,
            from: Data(#"{"merged":false,"error":"Conflicts","txid":1}"#.utf8)
        )
        XCTAssertEqual(result, PullRequestMergeResult(merged: false, error: "Conflicts"))
    }
}
