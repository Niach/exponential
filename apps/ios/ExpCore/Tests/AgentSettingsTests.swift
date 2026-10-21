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
        let schedule = try XCTUnwrap(copy["schedule"] as? [String: String])
        XCTAssertEqual(schedule["cron"], AgentSettings.Copy.scheduleCron)
        XCTAssertEqual(schedule["cronTz"], AgentSettings.Copy.scheduleCronTz)
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
}
