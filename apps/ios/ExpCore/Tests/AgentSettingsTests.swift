import Foundation
import GRDB
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
        XCTAssertEqual(copy["record"] as? String, AgentSettings.Copy.record)
        XCTAssertEqual(copy["recordOps"] as? [String: String], AgentSettings.Copy.recordOps)
        XCTAssertEqual(copy["webhook"] as? String, AgentSettings.Copy.webhook)
        XCTAssertEqual(copy["runs"] as? String, AgentSettings.Copy.runs)
        XCTAssertEqual(copy["handsOff"] as? String, AgentSettings.Copy.handsOff)
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

    // #EXP-1850: a `run` trigger's second line.
    func testTriggerDetailReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try fixture()["detailCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            var object = try XCTUnwrap(testCase["trigger"] as? [String: Any])
            object["id"] = "t"
            let raw = try XCTUnwrap(String(data: JSONSerialization.data(withJSONObject: [object]), encoding: .utf8))
            let trigger = try XCTUnwrap(AgentTrigger.parseList(raw).first, raw)
            XCTAssertEqual(AgentSettings.triggerDetail(trigger), testCase["expected"] as? String, raw)
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

    /// THE Runs on rows: offline listed, disabled unless bound; raw bound id leads.
    func testRunsOnRowsReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try assistantBlock()["runsOnRowCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let devices = try XCTUnwrap(testCase["devices"] as? [[String: Any]]).map { device in
                (
                    deviceId: try XCTUnwrap(device["deviceId"] as? String, name),
                    kind: device["kind"] as? String,
                    online: try XCTUnwrap(device["online"] as? Bool, name)
                )
            }
            let expected = try XCTUnwrap(testCase["expected"] as? [[String: Any]], name).map { row in
                AgentSettings.RunsOnRow(
                    id: try XCTUnwrap(row["id"] as? String, name),
                    description: row["description"] as? String,
                    disabled: try XCTUnwrap(row["disabled"] as? Bool, name)
                )
            }
            XCTAssertEqual(
                AgentSettings.runsOnRows(
                    devices,
                    boundId: testCase["boundId"] as? String,
                    assistant: try XCTUnwrap(testCase["assistant"] as? Bool, name)
                ),
                expected,
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
            XCTAssertEqual(want["perTeam"] as? Bool ?? false, section.perTeam, section.id)
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

    /// #EXP-1988: `assistant.teamPicks.pickAgentCases`.
    func testAssistantPickAgentReplaysEveryCase() throws {
        let block = try XCTUnwrap(try assistantBlock()["teamPicks"] as? [String: Any])
        let cases = try XCTUnwrap(block["pickAgentCases"] as? [[String: Any]])
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
                AgentSettings.assistantPickAgent(
                    agents,
                    teamId: try XCTUnwrap(testCase["teamId"] as? String),
                    teamIds: try XCTUnwrap(testCase["teamIds"] as? [String]),
                    me: testCase["me"] as? String,
                    pageAgentId: testCase["pageAgentId"] as? String
                ),
                testCase["expected"] as? String,
                name
            )
        }
    }

    /// #EXP-1988: `assistant.teamPicks.teamPickCases`.
    func testTeamPickReplaysEveryCase() throws {
        let block = try XCTUnwrap(try assistantBlock()["teamPicks"] as? [String: Any])
        let cases = try XCTUnwrap(block["teamPickCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            XCTAssertEqual(
                AgentSettings.teamPick(
                    try XCTUnwrap(testCase["ids"] as? [String]),
                    rowTeams: try XCTUnwrap(testCase["rowTeams"] as? [String: String]),
                    teamId: try XCTUnwrap(testCase["teamId"] as? String)
                ),
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

    /// #EXP-2079: `assistant.chatTeams.cases` (supersedes #EXP-1714's
    /// single lookup team).
    /// #EXP-2094: removing the Teams chip widens the chat teams at once,
    /// before any send; a moved stored pick re-seeds the composer's.
    func testTeamsPickDraftReadsTheComposersPickAtOnce() {
        let synced = ["a", "b"]
        let stored: [String]? = ["a"]
        var draft = AgentSettings.TeamsPickDraft()
        func teams() -> [String] {
            AgentSettings.chatTeams(ownTeamId: "personal", syncedTeamIds: synced, pickedTeamIds: draft.picked(stored: stored))
        }
        XCTAssertEqual(draft.picked(stored: stored), ["a"])
        XCTAssertEqual(teams(), ["a"])
        draft.pick(nil)
        XCTAssertNil(draft.picked(stored: stored))
        XCTAssertEqual(teams(), ["a", "b"])
        draft.pick(["b"])
        XCTAssertEqual(teams(), ["b"])
        draft.reseed()
        XCTAssertEqual(draft.picked(stored: ["b"]), ["b"])
        XCTAssertEqual(teams(), ["a"])
    }

    func testChatTeamsReplaysEveryCase() throws {
        let block = try XCTUnwrap(try assistantBlock()["chatTeams"] as? [String: Any])
        let cases = try XCTUnwrap(block["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            XCTAssertEqual(
                AgentSettings.chatTeams(
                    ownTeamId: testCase["ownTeamId"] as? String,
                    syncedTeamIds: try XCTUnwrap(testCase["syncedTeamIds"] as? [String]),
                    pickedTeamIds: testCase["pickedTeamIds"] as? [String]
                ),
                testCase["expected"] as? [String],
                testCase["name"] as? String ?? "?"
            )
        }
    }

    /// #EXP-2094: the Home launch composer addressed to my Assistant reads
    /// the chat teams with no own team; a team agent or Temporary session
    /// keeps the composer's team.
    func testHomeComposerTeams() {
        let synced = ["t-a", "t-b", "t-c"]
        func teams(_ assistant: Bool, _ teamId: String?, _ picked: [String]?) -> [String] {
            AgentSettings.homeComposerTeams(toMyAssistant: assistant, teamId: teamId, syncedTeamIds: synced, pickedTeamIds: picked)
        }
        XCTAssertEqual(teams(true, "t-b", nil), synced, "nothing picked: every synced team, never the active one alone")
        XCTAssertEqual(teams(true, "t-b", ["t-c", "t-a"]), ["t-a", "t-c"], "the Teams pick, in the synced order")
        XCTAssertEqual(teams(true, "t-b", ["t-gone"]), synced, "nothing of the pick synced: every synced team")
        XCTAssertEqual(teams(false, "t-b", ["t-a"]), ["t-b"], "a team agent or Temporary session keeps its team")
        XCTAssertEqual(teams(false, nil, nil), [])
    }

    /// #EXP-2095: `assistant.chatTeams.memberCases`, the `@` list's one order.
    func testChatMembersReplaysEveryCase() throws {
        let block = try XCTUnwrap(try assistantBlock()["chatTeams"] as? [String: Any])
        let cases = try XCTUnwrap(block["memberCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let members = try XCTUnwrap(testCase["members"] as? [[String: Any]])
            let sorted = AgentSettings.chatMembers(
                members,
                id: { $0["id"] as? String ?? "" },
                name: { $0["name"] as? String },
                email: { $0["email"] as? String }
            )
            XCTAssertEqual(
                sorted.compactMap { $0["id"] as? String },
                testCase["expected"] as? [String],
                testCase["name"] as? String ?? "?"
            )
        }
    }

    /// #EXP-2095: the synced rows take the same order.
    func testChatMembersOrdersTheUserRows() {
        func user(_ id: String, _ name: String?, _ email: String) -> UserEntity {
            UserEntity(id: id, name: name, email: email, image: nil, createdAt: "", updatedAt: "")
        }
        let roster = [
            user("u-j", "Jonas", "jonas@example.com"), user("u-s", "Sofia", "sofia@example.com"),
            user("u-a", "Alex", "alex@example.com"), user("u-h", nil, "hannes@example.com"),
            user("u-m", "Mira", "mira@example.com"),
        ]
        XCTAssertEqual(AgentSettings.chatMembers(roster).map(\.id), ["u-a", "u-h", "u-j", "u-m", "u-s"])
    }

    /// #EXP-2079: `assistant.chatTeams.deviceCases`.
    func testOwnDevicesReplaysEveryCase() throws {
        let block = try XCTUnwrap(try assistantBlock()["chatTeams"] as? [String: Any])
        let cases = try XCTUnwrap(block["deviceCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            XCTAssertEqual(
                AgentSettings.ownDevices(
                    sessionTeamSynced: try XCTUnwrap(testCase["sessionTeamSynced"] as? Bool),
                    starterIsViewer: try XCTUnwrap(testCase["starterIsViewer"] as? Bool)
                ),
                testCase["expected"] as? Bool,
                testCase["name"] as? String ?? "?"
            )
        }
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

    // MARK: - #EXP-1847 every agent's Voice (`voice`)

    func testVoiceCopyAndLimitsMatchTheFixture() throws {
        let voice = try XCTUnwrap(try fixture()["voice"] as? [String: Any])
        XCTAssertEqual(voice["rows"] as? [String], AgentSettings.VoiceRows.rows)
        let copy = try XCTUnwrap(voice["copy"] as? [String: Any])
        typealias C = AgentSettings.VoiceRows.Copy
        XCTAssertEqual(copy["label"] as? String, C.label)
        XCTAssertEqual(copy["language"] as? String, C.language)
        XCTAssertEqual(copy["clip"] as? String, C.clip)
        XCTAssertEqual(copy["clipBuiltIn"] as? String, C.clipBuiltIn)
        XCTAssertEqual(copy["clipUpload"] as? String, C.clipUpload)
        XCTAssertEqual(copy["clipRemove"] as? String, C.clipRemove)
        XCTAssertEqual(copy["transcript"] as? String, C.transcript)
        XCTAssertEqual(copy["transcriptPlaceholder"] as? String, C.transcriptPlaceholder)
        XCTAssertEqual(copy["wakePhrase"] as? String, C.wakePhrase)
        XCTAssertEqual(copy["wakePhrasePlaceholder"] as? String, C.wakePhrasePlaceholder)
        let errors = try XCTUnwrap(copy["errors"] as? [String: String])
        XCTAssertEqual(errors["clipType"], C.Errors.clipType)
        XCTAssertEqual(errors["clipSize"], C.Errors.clipSize)
        let upload = try XCTUnwrap(voice["clipUpload"] as? [String: Any])
        XCTAssertEqual(upload["route"] as? String, AgentSettings.VoiceRows.ClipUpload.route)
        XCTAssertEqual(upload["accept"] as? [String], AgentSettings.VoiceRows.ClipUpload.accept)
        XCTAssertEqual(upload["maxBytes"] as? Int, AgentSettings.VoiceRows.ClipUpload.maxBytes)
        let limits = try XCTUnwrap(voice["limits"] as? [String: Int])
        XCTAssertEqual(limits["transcript"], AgentSettings.VoiceRows.Limits.transcript)
        XCTAssertEqual(limits["wakePhrase"], AgentSettings.VoiceRows.Limits.wakePhrase)
        XCTAssertEqual(limits["wakePhraseMinLetters"], AgentSettings.VoiceRows.Limits.wakePhraseMinLetters)
        XCTAssertEqual(AgentSettings.VoiceRows.clipPath(agentId: "a-1"), "/api/agents/a-1/voice-clip")
    }

    func testAgentVoiceOfReplaysEveryCase() throws {
        let voice = try XCTUnwrap(try fixture()["voice"] as? [String: Any])
        let cases = try XCTUnwrap(voice["voiceOfCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? ""
            let agent = try XCTUnwrap(entry["agent"] as? [String: Any], name)
            let member = try XCTUnwrap(entry["member"] as? [String: String], name)
            let expected = try XCTUnwrap(entry["expected"] as? [String: Any], name)
            let raw = try JSONSerialization.data(withJSONObject: agent["voice"] ?? [:])
            let resolved = AgentSettings.agentVoiceOf(
                role: agent["role"] as? String,
                voice: AgentVoice.parse(String(data: raw, encoding: .utf8)),
                member: Voice.Settings(
                    wakePhrase: member["wakePhrase"] ?? "",
                    language: member["language"] ?? "en",
                    voiceFile: member["voiceFile"] ?? "",
                    voiceTranscript: member["voiceTranscript"] ?? ""
                )
            )
            let clip: AgentSettings.ResolvedClip? = (expected["clip"] as? [String: String]).flatMap { clip in
                if let id = clip["attachmentId"] { return .attachment(id: id) }
                return clip["path"].map { .path($0) }
            }
            XCTAssertEqual(
                resolved,
                AgentSettings.ResolvedVoice(
                    language: expected["language"] as? String ?? "",
                    clip: clip,
                    clipLabel: expected["clipLabel"] as? String ?? "",
                    transcript: expected["transcript"] as? String ?? "",
                    wakePhrase: expected["wakePhrase"] as? String ?? ""
                ),
                name
            )
        }
    }

    private struct ChatSession: AgentChatSession {
        let id: String
        let parentId: String?
        let agentId: String?
        let userId: String
    }

    /// #EXP-1990 `groupChat.chatCases`: the agent's chat for the viewer.
    func testGroupChatChatCases() throws {
        let group = try XCTUnwrap(try fixture()["groupChat"] as? [String: Any])
        let cases = try XCTUnwrap(group["chatCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"] as? String ?? ""
            let agent = try XCTUnwrap(c["agent"] as? [String: Any], name)
            let sessions = try XCTUnwrap(c["sessions"] as? [[String: Any]], name).map {
                ChatSession(
                    id: $0["id"] as? String ?? "",
                    parentId: $0["parentId"] as? String,
                    agentId: $0["agentId"] as? String,
                    userId: $0["userId"] as? String ?? ""
                )
            }
            let got = AgentSettings.agentChatOf(
                agentId: try XCTUnwrap(agent["agentId"] as? String, name),
                groupChatId: agent["groupChatId"] as? String,
                sessions: sessions,
                me: c["me"] as? String
            )
            XCTAssertEqual(got?.id, c["expected"] as? String, name)
        }
    }

    /// #EXP-1990: the store lookup reads the agent's `group_chat_id` first.
    func testAgentChatOverTheStore() throws {
        let pool = try DatabasePool(path: NSTemporaryDirectory() + "exp1990-agent-chat-\(UUID().uuidString).sqlite")
        try DatabaseManager.runMigrations(on: pool)
        func session(_ id: String, user: String) throws -> SessionEntity {
            try JSONDecoder().decode(SessionEntity.self, from: Data("""
                {"id":"\(id)","team_id":"t","root_id":"\(id)","user_id":"\(user)","agent_id":"ag1","device_id":"d",
                 "created_at":"x","updated_at":"x"}
                """.utf8))
        }
        try pool.write { db in
            try AgentDefinitionEntity(id: "ag1", teamId: "t", name: "Release", createdAt: "x", updatedAt: "x").save(db)
            try session("c-me", user: "me").save(db)
            try session("g1", user: "owner").save(db)
        }
        // Switch off: the viewer's own root; a teammate's never.
        XCTAssertEqual(try pool.read { try AgentSettings.agentChat(db: $0, agentId: "ag1", me: "me")?.id }, "c-me")
        XCTAssertNil(try pool.read { try AgentSettings.agentChat(db: $0, agentId: "ag1", me: "other") })
        XCTAssertFalse(try pool.read { try AgentSettings.isGroupChatRoot(db: $0, rootId: "g1", agentId: "ag1") })
        // Switch on: the group chat for every member, whoever started it.
        try pool.write { db in
            try db.execute(sql: #"UPDATE "agents" SET "group_chat_id" = 'g1' WHERE "id" = 'ag1'"#)
        }
        XCTAssertEqual(try pool.read { try AgentSettings.agentChat(db: $0, agentId: "ag1", me: "me")?.id }, "g1")
        XCTAssertEqual(try pool.read { try AgentSettings.agentChat(db: $0, agentId: "ag1", me: "other")?.id }, "g1")
        XCTAssertTrue(try pool.read { try AgentSettings.isGroupChatRoot(db: $0, rootId: "g1", agentId: "ag1") })
        XCTAssertFalse(try pool.read { try AgentSettings.isGroupChatRoot(db: $0, rootId: "c-me", agentId: "ag1") })
        // A group chat the viewer does not sync: none, never their old one.
        try pool.write { db in
            try db.execute(sql: #"UPDATE "agents" SET "group_chat_id" = 'g-missing' WHERE "id" = 'ag1'"#)
        }
        XCTAssertNil(try pool.read { try AgentSettings.agentChat(db: $0, agentId: "ag1", me: "me") })
    }

    func testAgentVoiceParsesTheJsonbAndRoundTrips() throws {
        XCTAssertEqual(AgentVoice.parse(nil), AgentVoice())
        XCTAssertEqual(AgentVoice.parse("garbage"), AgentVoice())
        XCTAssertEqual(AgentVoice.parse(#"{"language":5,"clipId":"c"}"#), AgentVoice(clipId: "c"))
        let voice = AgentVoice(language: "de", clipId: "c-1", clipName: "a.wav", transcript: "Hi", wakePhrase: "Hey Sloppy")
        let text = try XCTUnwrap(String(data: try JSONEncoder().encode(voice), encoding: .utf8))
        XCTAssertEqual(AgentVoice.parse(text), voice)
        XCTAssertEqual(String(data: try JSONEncoder().encode(AgentVoice()), encoding: .utf8), "{}")
    }

    // MARK: - #EXP-1859 wake phrases per agent (`voice.wake`)

    private func wakeFixture() throws -> [String: Any] {
        let voice = try XCTUnwrap(try fixture()["voice"] as? [String: Any])
        return try XCTUnwrap(voice["wake"] as? [String: Any])
    }

    private func wakeAgents(_ wake: [String: Any]) throws -> [AgentSettings.WakeAgent] {
        try XCTUnwrap(wake["agents"] as? [[String: Any]]).map { agent in
            let raw = try JSONSerialization.data(withJSONObject: agent["voice"] ?? [:])
            return AgentSettings.WakeAgent(
                id: try XCTUnwrap(agent["id"] as? String),
                role: try XCTUnwrap(agent["role"] as? String),
                name: try XCTUnwrap(agent["name"] as? String),
                voice: AgentVoice.parse(String(data: raw, encoding: .utf8))
            )
        }
    }

    /// The fixture's base `settings` with a case's overrides (an explicit
    /// null clears).
    private func wakeSettings(_ wake: [String: Any], _ overrides: [String: Any]?) throws -> Voice.Settings {
        var merged = try XCTUnwrap(wake["settings"] as? [String: Any])
        for (key, value) in overrides ?? [:] { merged[key] = value }
        func text(_ key: String) -> String? { merged[key] as? String }
        return Voice.Settings(
            voiceDeviceId: text("voiceDeviceId"),
            agentId: text("agentId"),
            wakeWord: merged["wakeWord"] as? Bool ?? false,
            wakePhrase: text("wakePhrase") ?? "",
            wakeAgentIds: merged["wakeAgentIds"] as? [String] ?? [],
            language: text("language") ?? "en",
            voiceFile: text("voiceFile") ?? "",
            voiceTranscript: text("voiceTranscript") ?? ""
        )
    }

    func testWakeCopyAndConstantsMatchTheFixture() throws {
        let wake = try wakeFixture()
        typealias W = AgentSettings.Wake
        XCTAssertEqual(wake["maxListeners"] as? Int, W.maxListeners)
        XCTAssertEqual(wake["maxIds"] as? Int, W.maxIds)
        let copy = try XCTUnwrap(wake["copy"] as? [String: Any])
        XCTAssertEqual(copy["wakeWord"] as? String, W.Copy.wakeWord)
        XCTAssertEqual(W.Copy.wakeWord, Voice.Copy.wakeWord)
        let errors = try XCTUnwrap(copy["errors"] as? [String: String])
        XCTAssertEqual(errors["wakeTooShort"], W.Copy.Errors.wakeTooShort)
        XCTAssertEqual(errors["wakeClash"], W.Copy.Errors.wakeClash)
        XCTAssertEqual(errors["wakeCap"], W.Copy.Errors.wakeCap)
        XCTAssertTrue(AgentSettings.VoiceRows.rows.contains(W.rowId))
        // talk-listen.json: the matcher's ratio and the worker's cap.
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/etalk/fixtures/talk-listen.json")
        let talk = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        let talkWake = try XCTUnwrap(talk["wake"] as? [String: Any])
        let constants = try XCTUnwrap(talkWake["constants"] as? [String: Any])
        XCTAssertEqual(constants["maxEditRatio"] as? Double, W.maxEditRatio)
        let multi = try XCTUnwrap(talkWake["multi"] as? [String: Any])
        XCTAssertEqual((multi["constants"] as? [String: Any])?["maxListeners"] as? Int, W.maxListeners)
    }

    func testWakeClashReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try wakeFixture()["clashCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let a = try XCTUnwrap(entry["a"] as? String)
            let b = try XCTUnwrap(entry["b"] as? String)
            XCTAssertEqual(AgentSettings.wakeClash(a, b), entry["clash"] as? Bool, "\(a) / \(b)")
        }
    }

    func testWakeListenersReplaysEveryCase() throws {
        let wake = try wakeFixture()
        let agents = try wakeAgents(wake)
        let cases = try XCTUnwrap(wake["listenersCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? ""
            let settings = try wakeSettings(wake, entry["settings"] as? [String: Any])
            let expected = try XCTUnwrap(entry["expected"] as? [[String: String]], name).map {
                AgentSettings.WakeListener(agentId: $0["agentId"] ?? "", phrase: $0["phrase"] ?? "", language: $0["language"] ?? "")
            }
            XCTAssertEqual(AgentSettings.wakeListeners(settings, agents: agents), expected, name)
        }
    }

    func testWakeRowReplaysEveryCase() throws {
        let wake = try wakeFixture()
        let agents = try wakeAgents(wake)
        let cases = try XCTUnwrap(wake["rowCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? ""
            let settings = try wakeSettings(wake, entry["settings"] as? [String: Any])
            let expected = try XCTUnwrap(entry["expected"] as? [String: Any], name)
            XCTAssertEqual(
                AgentSettings.wakeRow(settings, agents: agents, agentId: try XCTUnwrap(entry["agentId"] as? String)),
                AgentSettings.WakeRowState(on: try XCTUnwrap(expected["on"] as? Bool), error: expected["error"] as? String),
                name
            )
        }
    }

    func testWakeToggleReplaysEveryCase() throws {
        let wake = try wakeFixture()
        let agents = try wakeAgents(wake)
        let cases = try XCTUnwrap(wake["toggleCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? ""
            let settings = try wakeSettings(wake, entry["settings"] as? [String: Any])
            let expected = try XCTUnwrap(entry["expected"] as? [String: Any], name)
            XCTAssertEqual(
                AgentSettings.wakeToggle(
                    settings,
                    agents: agents,
                    agentId: try XCTUnwrap(entry["agentId"] as? String),
                    on: try XCTUnwrap(entry["on"] as? Bool)
                ),
                AgentSettings.WakeToggleResult(
                    wakeWord: try XCTUnwrap(expected["wakeWord"] as? Bool, name),
                    wakeAgentIds: try XCTUnwrap(expected["wakeAgentIds"] as? [String], name),
                    error: expected["error"] as? String
                ),
                name
            )
        }
    }

    func testVoiceSettingsDecodeWakeAgentIds() throws {
        let old = try JSONDecoder().decode(Voice.Settings.self, from: Data(#"{"wakeWord":true}"#.utf8))
        XCTAssertEqual(old.wakeAgentIds, [])
        let new = try JSONDecoder().decode(Voice.Settings.self, from: Data(#"{"wakeAgentIds":["a-1","a-2"]}"#.utf8))
        XCTAssertEqual(new.wakeAgentIds, ["a-1", "a-2"])
    }
}
