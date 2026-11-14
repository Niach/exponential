import Foundation
import XCTest
@testable import ExpCore

// EXP-1291: Team settings → Agents, locked ×4 (TS
// `agent-settings.test.ts`, desktop `domain::agent_settings`, Android
// `AgentSettingsTest`) against fixtures/agent-settings.json: the fields, the
// copy, `summaryCases`, `validationCases` and (EXP-1306) `accountValueCases`
// (the WHOLE expected value).
final class AgentSettingsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/agent-settings.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testFieldsMatchTheFixture() throws {
        let fields = try XCTUnwrap(try fixture()["fields"] as? [[String: Any]])
        XCTAssertEqual(fields.count, AgentSettings.fields.count)
        for (expected, field) in zip(fields, AgentSettings.fields) {
            XCTAssertEqual(expected["key"] as? String, field.key)
            XCTAssertEqual(expected["label"] as? String, field.label)
            XCTAssertEqual(expected["control"] as? String, field.control)
            XCTAssertEqual(expected["required"] as? Bool, field.required)
            XCTAssertEqual(expected["ownerOnly"] as? Bool ?? false, field.ownerOnly)
        }
    }

    func testCopyMatchesTheFixture() throws {
        let fixture = try fixture()
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(copy["title"] as? String, AgentSettings.Copy.title)
        XCTAssertEqual(copy["newAgent"] as? String, AgentSettings.Copy.newAgent)
        XCTAssertEqual(copy["namePlaceholder"] as? String, AgentSettings.Copy.namePlaceholder)
        XCTAssertEqual(copy["instructionsPlaceholder"] as? String, AgentSettings.Copy.instructionsPlaceholder)
        XCTAssertEqual(copy["addTrigger"] as? String, AgentSettings.Copy.addTrigger)
        XCTAssertEqual(copy["noSchedule"] as? String, AgentSettings.Copy.noSchedule)
        XCTAssertEqual(copy["paused"] as? String, AgentSettings.Copy.paused)
        XCTAssertEqual(copy["deleteConfirm"] as? String, AgentSettings.Copy.deleteConfirm)
        // NEW-2: a schedule's sentence moved to the schedule picker.
        XCTAssertNil(copy["schedule"])
        XCTAssertEqual(copy["events"] as? [String: String], AgentSettings.Copy.events)
        XCTAssertEqual(Set(AgentSettings.Copy.events.keys), Set(DomainContract.eventBuiltinNames))
        XCTAssertEqual(copy["event"] as? String, AgentSettings.Copy.event)
        let errors = try XCTUnwrap(copy["errors"] as? [String: String])
        XCTAssertEqual(errors["nameRequired"], AgentSettings.Copy.nameRequired)
        XCTAssertEqual(errors["nameTooLong"], AgentSettings.Copy.nameTooLong)
        XCTAssertEqual(errors["deviceRequired"], AgentSettings.Copy.deviceRequired)
        XCTAssertEqual(errors["instructionsTooLong"], AgentSettings.Copy.instructionsTooLong)
        XCTAssertEqual(copy["accountLastUsed"] as? String, AgentSettings.Copy.accountLastUsed)
        XCTAssertEqual(copy["triggers"] as? String, AgentSettings.Copy.triggers)
        XCTAssertEqual(copy["runsOn"] as? String, AgentSettings.Copy.runsOn)
        XCTAssertEqual(copy["checkIn"] as? String, AgentSettings.Copy.checkIn)
        let testIds = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(testIds["list"], AgentSettings.TestIds.list)
        XCTAssertEqual(testIds["page"], AgentSettings.TestIds.page)
    }

    // #EXP-1793: the agent page's MCP servers row and its value.
    func testMcpRowMatchesTheFixture() throws {
        let row = try XCTUnwrap(try fixture()["mcpRow"] as? [String: Any])
        XCTAssertEqual(row["id"] as? String, AgentSettings.McpRow.id)
        XCTAssertEqual(row["label"] as? String, AgentSettings.McpRow.label)
        XCTAssertEqual(row["after"] as? String, AgentSettings.McpRow.after)
        XCTAssertEqual(row["none"] as? String, AgentSettings.McpRow.none)
        XCTAssertEqual(row["max"] as? Int, AgentSettings.McpRow.max)
    }

    func testMcpValueCases() throws {
        let cases = try XCTUnwrap(try fixture()["mcpValueCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let names = try XCTUnwrap(c["names"] as? [String])
            XCTAssertEqual(AgentSettings.agentMcpServersValue(names), c["expected"] as? String, c["name"] as? String ?? "")
        }
    }

    func testTriggerSummaryReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try fixture()["summaryCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let object = try XCTUnwrap(testCase["trigger"] as? [String: Any])
            let when = try XCTUnwrap(TriggerWhen.parse(object: object), "\(object)")
            let enabled = object["enabled"] as? Bool ?? true
            XCTAssertEqual(
                AgentSettings.triggerSummary(when, enabled: enabled),
                testCase["expected"] as? String
            )
        }
    }

    func testValidateReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try fixture()["validationCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let form = try XCTUnwrap(testCase["form"] as? [String: Any])
            let errors = AgentSettings.validate(AgentSettings.Form(
                name: try XCTUnwrap(form["name"] as? String),
                deviceId: form["deviceId"] as? String,
                instructions: try XCTUnwrap(form["instructions"] as? String)
            ))
            let expected = try XCTUnwrap(testCase["expected"] as? [[String: String]]).map {
                AgentSettings.FormError(field: $0["field"] ?? "", message: $0["message"] ?? "")
            }
            XCTAssertEqual(errors, expected, name)
        }
    }

    func testAccountValueReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try fixture()["accountValueCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let options = try XCTUnwrap(input["options"] as? [[String: Any]]).map { row in
                AccountOption(
                    id: try XCTUnwrap(row["id"] as? String),
                    agent: try XCTUnwrap(row["agent"] as? String),
                    email: try XCTUnwrap(row["email"] as? String),
                    isLastUsed: try XCTUnwrap(row["isLastUsed"] as? Bool)
                )
            }
            let value = AgentSettings.agentAccountValue(
                options: options, account: input["account"] as? String, cli: input["cli"] as? String
            )
            let expected = testCase["expected"] as? [String: String]
            XCTAssertEqual(value?.agent, expected?["agent"], name)
            XCTAssertEqual(value?.email, expected?["email"], name)
            XCTAssertEqual(value == nil, expected == nil, name)
        }
    }

    /// #EXP-1383 `actor.actorCases`.
    func testIssueActorReplaysEveryCase() throws {
        let actor = try XCTUnwrap(try fixture()["actor"] as? [String: Any])
        let cases = try XCTUnwrap(actor["actorCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let agents = try XCTUnwrap(testCase["agents"] as? [[String: Any]]).map { row in
                AgentSettings.ActorAgent(
                    id: row["id"] as? String ?? "",
                    name: row["name"] as? String ?? "",
                    icon: row["icon"] as? String,
                    color: row["color"] as? String
                )
            }
            let got = AgentSettings.issueActor(
                actorAgentId: input["actorAgentId"] as? String,
                actorUserId: input["actorUserId"] as? String,
                agents: agents
            )
            let expected: AgentSettings.IssueActor? = (testCase["expected"] as? [String: Any]).map { row in
                if row["kind"] as? String == "agent" {
                    return .agent(AgentSettings.ActorAgent(
                        id: row["id"] as? String ?? "",
                        name: row["name"] as? String ?? "",
                        icon: row["icon"] as? String,
                        color: row["color"] as? String
                    ))
                }
                return .user(id: row["id"] as? String ?? "")
            }
            XCTAssertEqual(got, expected, name)
        }
    }

    /// #EXP-1383 `assignee.optionCases` + `assignee.copy`.
    func testAssigneeOptionsReplayEveryCase() throws {
        let assignee = try XCTUnwrap(try fixture()["assignee"] as? [String: Any])
        let copy = try XCTUnwrap(assignee["copy"] as? [String: String])
        XCTAssertEqual(copy, [
            "unassigned": AgentSettings.AssigneeCopy.unassigned,
            "nudge": AgentSettings.AssigneeCopy.nudge,
        ])
        let cases = try XCTUnwrap(assignee["optionCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let members = try XCTUnwrap(input["members"] as? [[String: Any]]).compactMap { $0["id"] as? String }
            let agents = try XCTUnwrap(input["agents"] as? [[String: Any]]).map { row in
                AgentSettings.AssigneeAgentOption(
                    id: row["id"] as? String ?? "",
                    name: row["name"] as? String ?? "",
                    sortOrder: (row["sortOrder"] as? NSNumber)?.doubleValue ?? 0
                )
            }
            XCTAssertEqual(
                AgentSettings.assigneeOptions(memberIds: members, agents: agents),
                testCase["expected"] as? [String],
                name
            )
        }
    }

    /// The two-column write and the ONE picker value.
    func testIssueAssigneeWrites() {
        XCTAssertEqual(IssueAssignee(assigneeId: "U1", assigneeAgentId: "G1"), .agent("G1"))
        XCTAssertEqual(IssueAssignee(pickerValue: "agent:G1").assigneeAgentId, "G1")
        XCTAssertNil(IssueAssignee(pickerValue: "agent:G1").assigneeId)
        XCTAssertEqual(IssueAssignee(pickerValue: "U1"), .user("U1"))
        XCTAssertTrue(IssueAssignee(pickerValue: "").isUnassigned)
        XCTAssertEqual(IssueAssignee.agent("G1").pickerValue, "agent:G1")
        XCTAssertEqual(IssueAssignee.user("U1").pickerValue, "U1")
        XCTAssertEqual(IssueAssignee.unassigned.pickerValue, "")
        XCTAssertTrue(AgentSettings.showsAssignee(memberCount: 1, agentCount: 1))
        XCTAssertFalse(AgentSettings.showsAssignee(memberCount: 1, agentCount: 0))
        XCTAssertTrue(AgentSettings.showsAssignee(memberCount: 2, agentCount: 0))
    }

    /// The wire: both columns ride every assignee write, the cleared one null.
    func testAssigneeWritesSendBothColumns() throws {
        func json<T: Encodable>(_ value: T) throws -> [String: Any] {
            try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(value)) as? [String: Any])
        }
        let agent = try json(UpdateIssueInput(id: "i", assignee: .agent("G1")))
        XCTAssertEqual(agent["assigneeAgentId"] as? String, "G1")
        XCTAssertTrue(agent["assigneeId"] is NSNull)
        let person = try json(UpdateIssueInput(id: "i", assignee: .user("U1")))
        XCTAssertEqual(person["assigneeId"] as? String, "U1")
        XCTAssertTrue(person["assigneeAgentId"] is NSNull)
        let bulk = try json(BulkUpdateIssuesInput(ids: ["i"], assignee: .unassigned))
        XCTAssertTrue(bulk["assigneeId"] is NSNull)
        XCTAssertTrue(bulk["assigneeAgentId"] is NSNull)
        let draft = try json(UpsertIssueDraftInput(
            id: "d", teamId: "t", boardId: "b", title: "", description: "", assigneeId: "U1"
        ))
        XCTAssertEqual(draft["assigneeId"] as? String, "U1")
        XCTAssertTrue(draft["assigneeAgentId"] is NSNull)
    }
    // MARK: - S-3 the Assistant (`assistant`)

    private func assistantBlock() throws -> [String: Any] {
        try XCTUnwrap(try fixture()["assistant"] as? [String: Any])
    }

    func testAssistantCopySectionsSchedulesAndBriefMatchTheFixture() throws {
        let block = try assistantBlock()
        var copy = try XCTUnwrap(block["copy"] as? [String: String])
        // #EXP-1439: the decision push's title is the server's (notifications.ts), no client draws it.
        XCTAssertNotNil(copy.removeValue(forKey: "decisionPush"))
        XCTAssertEqual(copy, [
            "title": AgentSettings.AssistantCopy.title,
            "name": AgentSettings.AssistantCopy.name,
            "runsOn": AgentSettings.AssistantCopy.runsOn,
            "checkIn": AgentSettings.AssistantCopy.checkIn,
            "voice": AgentSettings.AssistantCopy.voice,
            "noDevice": AgentSettings.AssistantCopy.noDevice,
            "deleteRefused": AgentSettings.AssistantCopy.deleteRefused,
            "scheduleRefused": AgentSettings.AssistantCopy.scheduleRefused,
            "promptFixed": AgentSettings.AssistantCopy.promptFixed,
            "legacyInstructions": AgentSettings.AssistantCopy.legacyInstructions,
        ])
        XCTAssertEqual(block["sections"] as? [String], AgentSettings.assistantSections)
        let schedules = try XCTUnwrap(block["schedules"] as? [[String: Any]])
        XCTAssertEqual(schedules.count, AgentSettings.assistantSchedules.count)
        for (want, schedule) in zip(schedules, AgentSettings.assistantSchedules) {
            XCTAssertEqual(want["id"] as? String, schedule.id.rawValue)
            XCTAssertEqual(want["label"] as? String, schedule.label)
            XCTAssertEqual(want["cron"] as? String, schedule.cron)
        }
        XCTAssertEqual(block["defaultSchedule"] as? String, AgentSettings.assistantDefaultSchedule.rawValue)
        let prompt = try XCTUnwrap(block["prompt"] as? [String: Any])
        XCTAssertEqual(prompt["version"] as? Int, AgentSettings.assistantPromptVersion)
        XCTAssertEqual(prompt["text"] as? String, AgentSettings.assistantPrompt)
        // #EXP-1717: the coordinator role joined the contract.
        XCTAssertEqual(DomainContract.agentRoleValues, ["assistant", "agent", "coordinator"])
    }

    func testAssistantModelValueReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try assistantBlock()["modelValueCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            XCTAssertEqual(
                AgentSettings.assistantModelValue(model: input["model"] as? String, cli: input["cli"] as? String),
                testCase["expected"] as? String,
                name
            )
        }
    }

    /// NEW-99: an Assistant's Runs on picker lists desktops only.
    func testRunsOnDeviceIdsReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try assistantBlock()["runsOnCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let devices = try XCTUnwrap(testCase["devices"] as? [[String: Any]]).map { device in
                (deviceId: try XCTUnwrap(device["deviceId"] as? String, name), kind: device["kind"] as? String)
            }
            XCTAssertEqual(
                AgentSettings.runsOnDeviceIds(
                    devices,
                    boundId: testCase["boundId"] as? String,
                    assistant: try XCTUnwrap(testCase["assistant"] as? Bool, name)
                ),
                try XCTUnwrap(testCase["expected"] as? [String], name),
                name
            )
        }
    }

    func testAssistantScheduleOfReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try assistantBlock()["scheduleCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let triggers = try XCTUnwrap(testCase["triggers"] as? [[String: Any]]).enumerated().map { index, raw in
                var object = raw
                object["id"] = "t\(index)"
                return try XCTUnwrap(AgentTrigger.parse(object: object), name)
            }
            XCTAssertEqual(
                AgentSettings.assistantScheduleOf(triggers)?.rawValue,
                testCase["expected"] as? String,
                name
            )
        }
    }

    func testAssistantScheduleTriggersRoundTrip() throws {
        XCTAssertEqual(AgentSettings.assistantScheduleTriggers(.off), [])
        for schedule in AgentSettings.assistantSchedules where schedule.id != .off {
            let saved = AgentSettings.assistantScheduleTriggers(schedule.id, tz: "Europe/Vienna")
            XCTAssertEqual(saved, [AgentTriggerInput(
                enabled: true,
                when: .schedule(ScheduleTrigger(cron: schedule.cron!, tz: "Europe/Vienna")),
                preset: schedule.id.rawValue
            )])
            XCTAssertEqual(AgentSettings.checkInOf(saved), schedule.id)
            XCTAssertEqual(
                AgentSettings.assistantScheduleOf(saved.map { (when: $0.when, enabled: $0.enabled) }),
                schedule.id
            )
        }
        XCTAssertNil(AgentSettings.assistantScheduleTriggers(.hourly, tz: "").first.flatMap {
            if case let .schedule(s) = $0.when { return s.tz }
            return nil
        })
    }

    // MARK: - #EXP-1846 one agent page, the Check in (`page`, `checkIn`)

    func testPageSectionsMatchTheFixture() throws {
        let page = try XCTUnwrap(try fixture()["page"] as? [String: Any])
        let sections = try XCTUnwrap(page["sections"] as? [[String: Any]])
        XCTAssertEqual(sections.count, AgentSettings.pageSections.count)
        for (want, section) in zip(sections, AgentSettings.pageSections) {
            XCTAssertEqual(want["id"] as? String, section.id)
            XCTAssertEqual(want["label"] as? String, section.label, section.id)
            XCTAssertEqual(want["rows"] as? [String] ?? [], section.rows, section.id)
            XCTAssertEqual(want["assistantOnly"] as? Bool ?? false, section.assistantOnly, section.id)
            XCTAssertEqual(want["teamAgentOnly"] as? Bool ?? false, section.teamAgentOnly, section.id)
            XCTAssertEqual(want["ownerOnly"] as? Bool ?? false, section.ownerOnly, section.id)
        }
    }

    private func checkInBlock() throws -> [String: Any] {
        try XCTUnwrap(try fixture()["checkIn"] as? [String: Any])
    }

    /// One fixture trigger as a write element (its id optional).
    private func triggerInput(_ object: [String: Any], _ name: String) throws -> AgentTriggerInput {
        let when = try XCTUnwrap(TriggerWhen.parse(object: object), name)
        return AgentTriggerInput(
            id: object["id"] as? String,
            enabled: object["enabled"] as? Bool ?? true,
            when: when,
            preset: object["preset"] as? String
        )
    }

    func testCheckInOfReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try checkInBlock()["checkInCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let triggers = try XCTUnwrap(testCase["triggers"] as? [[String: Any]]).map { try triggerInput($0, name) }
            XCTAssertEqual(AgentSettings.checkInOf(triggers).rawValue, testCase["expected"] as? String, name)
            // The stored (parsed) model reads the same.
            let stored = try XCTUnwrap(testCase["triggers"] as? [[String: Any]]).enumerated().map { index, raw in
                var object = raw
                object["id"] = "t\(index)"
                return try XCTUnwrap(AgentTrigger.parse(object: object), name)
            }
            XCTAssertEqual(AgentSettings.checkInOf(stored).rawValue, testCase["expected"] as? String, name)
        }
    }

    func testWithCheckInReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try checkInBlock()["withCheckInCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let triggers = try XCTUnwrap(testCase["triggers"] as? [[String: Any]]).map { try triggerInput($0, name) }
            let preset = try XCTUnwrap(
                AgentSettings.AssistantScheduleId(rawValue: try XCTUnwrap(testCase["preset"] as? String, name)), name
            )
            let written = AgentSettings.withCheckIn(triggers, preset, tz: testCase["tz"] as? String)
            // The WHOLE wire array, as the server receives it.
            let json = try JSONSerialization.jsonObject(with: try JSONEncoder().encode(written))
            let got = try XCTUnwrap(json as? [[String: Any]], name)
            let expected = try XCTUnwrap(testCase["expected"] as? [[String: Any]], name)
            XCTAssertEqual(got as NSArray, expected as NSArray, name)
        }
    }

    func testListedTriggersReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try checkInBlock()["listedCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let triggers = try XCTUnwrap(testCase["triggers"] as? [[String: Any]]).map {
                try XCTUnwrap(AgentTrigger.parse(object: $0), name)
            }
            XCTAssertEqual(
                AgentSettings.listedTriggers(triggers).map(\.id),
                try XCTUnwrap(testCase["expected"] as? [String], name),
                name
            )
        }
    }

    /// Every whole-array write keeps the Check in tag (toggle, edit, delete).
    func testWholeArrayWritesKeepThePreset() {
        let checkIn = AgentTrigger(
            id: "ci", when: .schedule(ScheduleTrigger(cron: "0 * * * *")), preset: "1h"
        )
        let event = AgentTrigger(id: "ev", when: .event(EventTrigger(name: "pr.merged")))
        let stored = [checkIn, event]
        XCTAssertEqual(stored.settingEnabled(id: "ev", false).first?.preset, "1h")
        XCTAssertEqual(stored.removing(id: "ev").first?.preset, "1h")
        XCTAssertEqual(
            stored.replacing(id: "ev", with: AgentTriggerInput(when: .event(EventTrigger(name: "pr.opened")))).first?.preset,
            "1h"
        )
        XCTAssertEqual(stored.adding(AgentTriggerInput(when: .event(EventTrigger(name: "pr.opened")))).first?.preset, "1h")
        XCTAssertEqual(AgentSettings.checkInOf(stored.settingEnabled(id: "ev", false)), .hourly)
    }

    func testVisibleAgentReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try assistantBlock()["visibleCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let agent = try XCTUnwrap(testCase["agent"] as? [String: Any])
            XCTAssertEqual(
                AgentSettings.visibleAgent(
                    role: agent["role"] as? String,
                    ownerId: agent["ownerId"] as? String,
                    me: testCase["me"] as? String
                ),
                testCase["expected"] as? Bool,
                name
            )
        }
    }

    /// #EXP-1714: `assistant.teamAgentsCases`.
    func testAgentsInTeamReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try assistantBlock()["teamAgentsCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let agents = try XCTUnwrap(testCase["agents"] as? [[String: Any]]).map { row in
                AgentLike(
                    id: try XCTUnwrap(row["id"] as? String),
                    teamId: try XCTUnwrap(row["teamId"] as? String),
                    name: row["id"] as? String ?? "",
                    icon: nil,
                    color: nil,
                    sortOrder: 0,
                    role: row["role"] as? String,
                    ownerId: row["ownerId"] as? String
                )
            }
            XCTAssertEqual(
                AgentSettings.agentsInTeam(
                    agents,
                    teamId: try XCTUnwrap(testCase["teamId"] as? String),
                    teamIds: try XCTUnwrap(testCase["teamIds"] as? [String]),
                    me: testCase["me"] as? String
                ).map(\.id),
                testCase["expected"] as? [String],
                name
            )
        }
    }

    /// #EXP-1714: my rows of an unsynced (personal) team show in every team.
    func testInTeamOrPersonal() {
        let synced: Set<String> = ["t-a", "t-b"]
        func shows(_ team: String, _ user: String?, in current: String = "t-a", me: String? = "u-me") -> Bool {
            AgentSettings.inTeamOrPersonal(teamId: team, userId: user, team: current, teamIds: synced, me: me)
        }
        XCTAssertTrue(shows("t-a", "u-x"), "the team's own rows")
        XCTAssertFalse(shows("t-b", "u-me"), "another synced team's rows stay there")
        XCTAssertTrue(shows("p-me", "u-me"), "my personal team's rows show in every team")
        XCTAssertFalse(shows("p-x", "u-x"), "another member's unsynced rows never")
        XCTAssertFalse(shows("p-me", "u-me", me: nil), "no viewer, no personal rows")
    }

    /// #EXP-1714: a composer's lookups fall back to the active team.
    func testLookupTeamId() {
        XCTAssertEqual(AgentSettings.lookupTeamId("t-b", teamIds: ["t-a", "t-b"], activeTeamId: "t-a"), "t-b")
        XCTAssertEqual(AgentSettings.lookupTeamId("p-me", teamIds: ["t-a", "t-b"], activeTeamId: "t-a"), "t-a")
        XCTAssertEqual(AgentSettings.lookupTeamId("p-me", teamIds: [], activeTeamId: "t-a"), "p-me")
        XCTAssertEqual(AgentSettings.lookupTeamId(nil, teamIds: ["t-a"], activeTeamId: "t-a"), "t-a")
    }

    /// #EXP-1717: the `coordinator` block (copy, test id, both case lists).
    private func coordinatorBlock() throws -> [String: Any] {
        try XCTUnwrap(try fixture()["coordinator"] as? [String: Any])
    }

    func testCoordinatorCopyAndTestId() throws {
        let block = try coordinatorBlock()
        let copy = try XCTUnwrap(block["copy"] as? [String: String])
        XCTAssertEqual(AgentSettings.CoordinatorCopy.label, copy["label"])
        XCTAssertEqual(AgentSettings.CoordinatorCopy.none, copy["none"])
        XCTAssertEqual(AgentSettings.CoordinatorCopy.refusedOwner, copy["refusedOwner"])
        XCTAssertEqual(AgentSettings.CoordinatorCopy.refusedAgent, copy["refusedAgent"])
        XCTAssertEqual(AgentSettings.coordinatorRowTestId, (block["testIds"] as? [String: String])?["row"])
    }

    func testCoordinatorOptionsReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try coordinatorBlock()["optionCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let agents = try XCTUnwrap(testCase["agents"] as? [[String: Any]]).map { row in
                AgentLike(
                    id: try XCTUnwrap(row["id"] as? String),
                    teamId: "t",
                    name: try XCTUnwrap(row["name"] as? String),
                    icon: nil,
                    color: nil,
                    sortOrder: try XCTUnwrap((row["sortOrder"] as? NSNumber)?.doubleValue),
                    role: row["role"] as? String,
                    ownerId: row["ownerId"] as? String
                )
            }
            XCTAssertEqual(AgentSettings.coordinatorOptions(agents), testCase["expected"] as? [String], name)
        }
    }

    func testCoordinatorOfReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try coordinatorBlock()["valueCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let agents = try XCTUnwrap(testCase["agents"] as? [[String: Any]]).map { row in
                AgentLike(
                    id: try XCTUnwrap(row["id"] as? String),
                    teamId: try XCTUnwrap(row["teamId"] as? String),
                    name: "",
                    icon: nil,
                    color: nil,
                    sortOrder: 0,
                    role: row["role"] as? String
                )
            }
            XCTAssertEqual(
                AgentSettings.coordinatorOf(
                    agents, teamId: try XCTUnwrap(testCase["teamId"] as? String), teamOf: \.teamId
                ),
                testCase["expected"] as? String,
                name
            )
        }
    }

    /// #EXP-1735: the `lead` block (copy, test id, option cases).
    private func leadBlock() throws -> [String: Any] {
        try XCTUnwrap(try fixture()["lead"] as? [String: Any])
    }

    func testLeadCopyAndTestId() throws {
        let block = try leadBlock()
        let copy = try XCTUnwrap(block["copy"] as? [String: String])
        XCTAssertEqual(AgentSettings.LeadCopy.label, copy["label"])
        XCTAssertEqual(AgentSettings.LeadCopy.none, copy["none"])
        XCTAssertEqual(AgentSettings.LeadCopy.refusedOwner, copy["refusedOwner"])
        XCTAssertEqual(AgentSettings.LeadCopy.refusedMember, copy["refusedMember"])
        XCTAssertEqual(AgentSettings.LeadCopy.noAssistant, copy["noAssistant"])
        XCTAssertEqual(AgentSettings.leadRowTestId, (block["testIds"] as? [String: String])?["row"])
    }

    func testLeadOptionsReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try leadBlock()["optionCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let members = try XCTUnwrap(testCase["members"] as? [[String: Any]]).map { row in
                AgentSettings.LeadMember(
                    userId: try XCTUnwrap(row["userId"] as? String),
                    name: try XCTUnwrap(row["name"] as? String)
                )
            }
            XCTAssertEqual(AgentSettings.leadOptions(members), testCase["expected"] as? [String], name)
        }
    }

    func testOrderedAgentsReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try assistantBlock()["orderCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let agents = try XCTUnwrap(testCase["agents"] as? [[String: Any]]).map { row in
                AgentLike(
                    id: try XCTUnwrap(row["id"] as? String),
                    teamId: "t",
                    name: try XCTUnwrap(row["name"] as? String),
                    icon: nil,
                    color: nil,
                    sortOrder: try XCTUnwrap((row["sortOrder"] as? NSNumber)?.doubleValue),
                    role: row["role"] as? String,
                    ownerId: row["ownerId"] as? String
                )
            }
            XCTAssertEqual(
                AgentSettings.orderedAgents(agents, me: testCase["me"] as? String).map(\.id),
                testCase["expected"] as? [String],
                name
            )
        }
    }
}
