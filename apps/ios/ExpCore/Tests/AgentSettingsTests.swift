import Foundation
import XCTest
@testable import ExpCore

// EXP-1291 Threads v2: Team settings → Agents, locked ×4 (TS
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
        let testIds = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(testIds["list"], AgentSettings.TestIds.list)
        XCTAssertEqual(testIds["page"], AgentSettings.TestIds.page)
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
        let copy = try XCTUnwrap(block["copy"] as? [String: String])
        XCTAssertEqual(copy, [
            "title": AgentSettings.AssistantCopy.title,
            "name": AgentSettings.AssistantCopy.name,
            "runsOn": AgentSettings.AssistantCopy.runsOn,
            "checkIn": AgentSettings.AssistantCopy.checkIn,
            "voice": AgentSettings.AssistantCopy.voice,
            "noDevice": AgentSettings.AssistantCopy.noDevice,
            "deleteRefused": AgentSettings.AssistantCopy.deleteRefused,
            "scheduleRefused": AgentSettings.AssistantCopy.scheduleRefused,
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
        XCTAssertEqual(block["brief"] as? String, AgentSettings.assistantBrief)
        XCTAssertEqual(DomainContract.agentRoleValues, ["assistant", "agent"])
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
                enabled: true, when: .schedule(ScheduleTrigger(cron: schedule.cron!, tz: "Europe/Vienna"))
            )])
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
