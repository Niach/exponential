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

    func testNewChatCarriesTheRootFacetsAndOmitsEveryNil() throws {
        let bare = try json(SessionsSendInput.newChat(teamId: "t-1", deviceId: "dev-1", text: "Go"))
        XCTAssertEqual(bare.keys.sorted(), ["deviceId", "teamId", "text"])

        let full = try json(SessionsSendInput.newChat(
            teamId: "t-1",
            deviceId: "dev-1",
            options: SessionLaunchOptions(
                cli: "claude", account: "p-1", model: "fable", effort: "",
                computerUse: false, ultracode: true, mcpServerIds: ["m-1"],
                skillIds: ["s-1"]
            ),
            text: "Fix it",
            attachmentIds: ["att-1"],
            issueIds: ["i-1", "i-2"],
            promptId: "p-1",
            repositoryId: "r-1"
        ))
        XCTAssertEqual(
            full.keys.sorted(),
            ["account", "attachmentIds", "cli", "computerUse", "deviceId", "effort",
             "issueIds", "mcpServerIds", "model", "promptId", "repositoryId", "skillIds",
             "teamId", "text", "ultracode"]
        )
        XCTAssertEqual(full["skillIds"] as? [String], ["s-1"])
        XCTAssertEqual(full["issueIds"] as? [String], ["i-1", "i-2"])
        XCTAssertEqual(full["promptId"] as? String, "p-1")
        XCTAssertEqual(full["effort"] as? String, "")
        XCTAssertEqual(full["computerUse"] as? Bool, false)

        // A blank model is never sent (absent = blank on the server).
        let blank = try json(SessionsSendInput.newChat(
            teamId: "t-1", deviceId: "dev-1", options: SessionLaunchOptions(model: "", effort: ""), text: "Go"
        ))
        XCTAssertEqual(blank.keys.sorted(), ["deviceId", "effort", "teamId", "text"])

        // #EXP-1661: an empty skill pick never rides the wire.
        let noSkills = try json(SessionsSendInput.newChat(
            teamId: "t-1", deviceId: "dev-1", options: SessionLaunchOptions(skillIds: []), text: "Go"
        ))
        XCTAssertEqual(noSkills.keys.sorted(), ["deviceId", "teamId", "text"])
    }

    /// #EXP-1715 (composer-menu.json `teamsPick`): the Assistant's Teams
    /// pick rides as `scopeTeamIds`: every team = an explicit null, a subset
    /// = the ids; no scope = no key (any other agent, any other session).
    func testTeamsScopeRidesAsScopeTeamIds() throws {
        let all = try json(SessionsSendInput.agent(agentId: "as-1", text: "Hi", scope: .allTeams))
        XCTAssertEqual(all.keys.sorted(), ["agentId", "scopeTeamIds", "text"])
        XCTAssertTrue(all["scopeTeamIds"] is NSNull)
        let some = try json(SessionsSendInput.message(sessionId: "s-1", text: "Hi", scope: .teams(["t-2"])))
        XCTAssertEqual(some.keys.sorted(), ["scopeTeamIds", "sessionId", "text"])
        XCTAssertEqual(some["scopeTeamIds"] as? [String], ["t-2"])
        let none = try json(SessionsSendInput.message(sessionId: "s-1", text: "Hi"))
        XCTAssertEqual(none.keys.sorted(), ["sessionId", "text"])
        XCTAssertEqual(TeamsScope(nil), .allTeams)
        XCTAssertEqual(TeamsScope(["t-1"]).picked, ["t-1"])
        // A sealed send keeps the scope (`with(text:)`).
        let sealed = SessionsSendInput.agent(agentId: "as-1", text: "x", scope: .teams(["t-1"])).with(text: "y")
        XCTAssertEqual(sealed, .agent(agentId: "as-1", text: "y", scope: .teams(["t-1"])))
        let forward = try json(SessionBrief.agentForwardInput(
            agentId: "as-1", text: "Hi", attachmentIds: [], issueIds: [], identifiers: [], scope: .allTeams
        ))
        XCTAssertTrue(forward["scopeTeamIds"] is NSNull)
    }

    /// #EXP-1729 (apps.json `editorHeader`): an app editor's send carries the
    /// composer's Account and Model picks; none = no keys, a blank model never.
    func testAppSendCarriesTheEditorPicks() throws {
        let bare = try json(SessionsSendInput.app(appId: "a-1", deviceId: "dev-1", text: "Go"))
        XCTAssertEqual(bare.keys.sorted(), ["appId", "deviceId", "text"])
        let picked = try json(SessionsSendInput.app(appId: "a-1", deviceId: "dev-1", text: "Go", cli: "claude", account: "p-1", model: "opus"))
        XCTAssertEqual(picked.keys.sorted(), ["account", "appId", "cli", "deviceId", "model", "text"])
        XCTAssertEqual(picked["account"] as? String, "p-1")
        XCTAssertEqual(picked["model"] as? String, "opus")
        let blank = try json(SessionsSendInput.app(appId: "a-1", deviceId: nil, text: "Go", model: ""))
        XCTAssertEqual(blank.keys.sorted(), ["appId", "text"])
        XCTAssertEqual(
            SessionsSendInput.app(appId: "a-1", deviceId: "d", text: "x", cli: "codex", account: "p", model: "m").with(text: "y"),
            .app(appId: "a-1", deviceId: "d", text: "y", cli: "codex", account: "p", model: "m")
        )
    }

    /// NEW-2: a message to an agent names the agent and nothing of a runner
    /// (it runs on the agent's own device); `agents.create` carries the
    /// definition with its WHOLE trigger array.
    func testAgentMessageAndCreateCarryOnlyTheirKeys() throws {
        let message = try json(SessionsSendInput.agent(agentId: "ag-1", text: "Triage the inbox"))
        XCTAssertEqual(message.keys.sorted(), ["agentId", "text"])
        let files = try json(SessionsSendInput.agent(agentId: "ag-1", attachmentIds: ["att-1"]))
        XCTAssertEqual(files.keys.sorted(), ["agentId", "attachmentIds"])
        // EXP-1450: the composer's issue chips forward onto the agent.
        let issues = try json(SessionsSendInput.agent(agentId: "ag-1", issueIds: ["i-1"]))
        XCTAssertEqual(issues.keys.sorted(), ["agentId", "issueIds"])
        XCTAssertEqual(issues["issueIds"] as? [String], ["i-1"])

        let bare = try json(AgentCreateInput(teamId: "t-1", name: "Caretaker", deviceId: "dev-1"))
        XCTAssertEqual(bare.keys.sorted(), ["deviceId", "instructions", "name", "teamId", "triggers"])

        let cron = TriggerWhen.schedule(ScheduleTrigger(cron: "0 9 * * *", tz: "Europe/Vienna"))
        let object = try json(AgentCreateInput(
            teamId: "t-1", name: "Caretaker", icon: "bot", color: "#4f46e5",
            instructions: "Keep the board tidy",
            triggers: [AgentTriggerInput(when: cron), AgentTriggerInput(when: .event(EventTrigger(name: "pr.merged")))],
            deviceId: "dev-1", cli: "claude", account: "p-1", model: "opus"
        ))
        XCTAssertEqual(
            object.keys.sorted(),
            ["account", "cli", "color", "deviceId", "icon", "instructions", "model", "name", "teamId", "triggers"]
        )
        let triggers = try XCTUnwrap(object["triggers"] as? [[String: Any]])
        XCTAssertNil(triggers[0]["id"])
        XCTAssertEqual(triggers[0]["kind"] as? String, "schedule")
        XCTAssertEqual(triggers[0]["cron"] as? String, "0 9 * * *")
        XCTAssertEqual(triggers[0]["tz"] as? String, "Europe/Vienna")
        XCTAssertEqual(triggers[0]["enabled"] as? Bool, true)
        XCTAssertEqual(triggers[1].keys.sorted(), ["enabled", "kind", "name"])
    }

    func testChildAndMessageFormsCarryOnlyTheirKeys() throws {
        let child = try json(SessionsSendInput.child(parentId: "s-1", deviceId: "dev-2", text: "Part two"))
        XCTAssertEqual(child.keys.sorted(), ["deviceId", "parentId", "text"])

        let message = try json(SessionsSendInput.message(sessionId: "s-1", text: "Ship it"))
        XCTAssertEqual(message.keys.sorted(), ["sessionId", "text"])
        let files = try json(SessionsSendInput.message(sessionId: "s-1", attachmentIds: ["att-1"]))
        XCTAssertEqual(files.keys.sorted(), ["attachmentIds", "sessionId"])
    }

    /// NEW-3: a message to a team agent names the agent, never a session.
    func testAnAgentMessageSendsTheAgentId() throws {
        let wire = try json(SessionsSendInput.agent(agentId: "a-1", text: "Hi", attachmentIds: ["att-1"]))
        XCTAssertEqual(wire.keys.sorted(), ["agentId", "attachmentIds", "text"])
        XCTAssertEqual(wire["agentId"] as? String, "a-1")
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

    func testStopResultDecodesDeliveredLeniently() throws {
        func decode(_ text: String) throws -> SessionsStopResult {
            try JSONDecoder().decode(SessionsStopResult.self, from: Data(text.utf8))
        }
        XCTAssertEqual(try decode(#"{"txid":1,"delivered":false}"#).delivered, false)
        XCTAssertEqual(try decode(#"{"txid":1,"delivered":true}"#).delivered, true)
        XCTAssertNil(try decode(#"{"txid":1}"#).delivered)
        XCTAssertNil(try decode(#"{"txid":1,"delivered":"no"}"#).delivered)
        XCTAssertNil(try decode("null").delivered)
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

    // #EXP-1717 `agents.setCoordinator`: the team and the agent, an
    // explicit null clearing it.
    func testSetCoordinatorSendsAnExplicitNull() throws {
        let set = try json(SetCoordinatorInput(teamId: "t-1", agentId: "ag-1"))
        XCTAssertEqual(set.keys.sorted(), ["agentId", "teamId"])
        XCTAssertEqual(set["agentId"] as? String, "ag-1")
        let cleared = try json(SetCoordinatorInput(teamId: "t-1", agentId: nil))
        XCTAssertEqual(cleared.keys.sorted(), ["agentId", "teamId"])
        XCTAssertTrue(cleared["agentId"] is NSNull)
    }

    // #EXP-1735 `agents.setLeadAssistant`: the team and the member, an
    // explicit null clearing it; `agents.lead` answers a member or null.
    func testSetLeadAssistantSendsAnExplicitNull() throws {
        let set = try json(SetLeadAssistantInput(teamId: "t-1", userId: "u-1"))
        XCTAssertEqual(set.keys.sorted(), ["teamId", "userId"])
        XCTAssertEqual(set["userId"] as? String, "u-1")
        let cleared = try json(SetLeadAssistantInput(teamId: "t-1", userId: nil))
        XCTAssertEqual(cleared.keys.sorted(), ["teamId", "userId"])
        XCTAssertTrue(cleared["userId"] is NSNull)
    }

    func testLeadDecodesAMemberOrNull() throws {
        let some = try JSONDecoder().decode(
            TeamLeadResult.self, from: Data(#"{"lead":{"agentId":"ag-1","userId":"u-1"}}"#.utf8)
        )
        XCTAssertEqual(some.lead, TeamLead(agentId: "ag-1", userId: "u-1"))
        let none = try JSONDecoder().decode(TeamLeadResult.self, from: Data(#"{"lead":null}"#.utf8))
        XCTAssertNil(none.lead)
    }

    // `agents.update`: omit keeps, an explicit null clears (`account` /
    // `model` only), and an empty trigger array still travels.
    func testUpdateAgentOmitVersusNull() throws {
        let empty = try json(UpdateAgentInput(id: "ag-1", patch: AgentPatch()))
        XCTAssertEqual(empty.keys.sorted(), ["id"])
        XCTAssertTrue(AgentPatch().isEmpty)

        let cleared = try json(UpdateAgentInput(
            id: "ag-1",
            patch: AgentPatch(name: "Renamed", triggers: [], account: .some(nil), model: .some(nil))
        ))
        XCTAssertEqual(cleared.keys.sorted(), ["account", "id", "model", "name", "triggers"])
        XCTAssertTrue(cleared["account"] is NSNull)
        XCTAssertTrue(cleared["model"] is NSNull)
        XCTAssertEqual((cleared["triggers"] as? [Any])?.count, 0)

        let moved = try json(UpdateAgentInput(
            id: "ag-1", patch: AgentPatch(deviceId: "dev-2", cli: "codex", account: .some("p-2"), sortOrder: 3)
        ))
        XCTAssertEqual(moved["cli"] as? String, "codex")
        XCTAssertEqual(moved["deviceId"] as? String, "dev-2")
        XCTAssertEqual(moved["account"] as? String, "p-2")
        XCTAssertEqual(moved["sortOrder"] as? Double, 3)

        // #EXP-1661: the whole skill list, an EMPTY one included ("none").
        let skills = try json(UpdateAgentInput(id: "ag-1", patch: AgentPatch(skillIds: ["s-1", "s-2"])))
        XCTAssertEqual(skills.keys.sorted(), ["id", "skillIds"])
        XCTAssertEqual(skills["skillIds"] as? [String], ["s-1", "s-2"])
        let none = try json(UpdateAgentInput(id: "ag-1", patch: AgentPatch(skillIds: [])))
        XCTAssertEqual((none["skillIds"] as? [Any])?.count, 0)
        XCTAssertFalse(AgentPatch(skillIds: []).isEmpty)

        // #EXP-1793: the whole MCP server list, an EMPTY one included.
        let mcp = try json(UpdateAgentInput(id: "ag-1", patch: AgentPatch(mcpServerIds: ["m-1"])))
        XCTAssertEqual(mcp.keys.sorted(), ["id", "mcpServerIds"])
        XCTAssertEqual(mcp["mcpServerIds"] as? [String], ["m-1"])
        let noMcp = try json(UpdateAgentInput(id: "ag-1", patch: AgentPatch(mcpServerIds: [])))
        XCTAssertEqual((noMcp["mcpServerIds"] as? [Any])?.count, 0)
        XCTAssertFalse(AgentPatch(mcpServerIds: []).isEmpty)
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

// NEW-2: `agents.get` answers `{agent}` with the server-only instructions.
final class AgentsApiWireTests: XCTestCase {
    func testAgentDetailsDecodeFromTheGetEnvelopeRow() throws {
        let details = try JSONDecoder().decode(AgentDetails.self, from: Data(#"""
            {"id":"ag-1","teamId":"t-1","name":"Triage","instructions":"Keep it tidy",
             "triggers":[],"deviceId":"dev-1","sortOrder":0}
            """#.utf8))
        XCTAssertEqual(details, AgentDetails(id: "ag-1", instructions: "Keep it tidy"))
        let blank = try JSONDecoder().decode(AgentDetails.self, from: Data(#"{"id":"ag-2"}"#.utf8))
        XCTAssertEqual(blank.instructions, "")
        XCTAssertEqual(blank.role, "agent")
    }

    // S-3: `agents.ensureAssistant` answers `{agent}` or `{agent: null}`.
    func testEnsureAssistantDecodesTheRowOrNull() throws {
        let made = try JSONDecoder().decode(EnsureAssistantResult.self, from: Data(#"""
            {"agent":{"id":"as-1","teamId":"t-1","name":"Assistant","role":"assistant","instructions":""}}
            """#.utf8))
        XCTAssertEqual(made.agent, AgentDetails(id: "as-1", instructions: "", role: "assistant"))
        XCTAssertEqual(made.agent?.isAssistant, true)
        let none = try JSONDecoder().decode(EnsureAssistantResult.self, from: Data(#"{"agent":null}"#.utf8))
        XCTAssertNil(none.agent)
    }
}
