import Foundation
import XCTest
@testable import ExpCore

// NEW-2 agent triggers (`agents.triggers`): the tolerant when-part parser
// (`{kind: schedule, cron, tz?}` / `{kind: event, name}`; any malformation
// reads as "no trigger", never a crash), the stored `AgentTrigger` element
// (id + enabled + when) and the wire encoding of the whole-array `triggers`
// write. The Triggers row's sentence is `AgentSettings.triggerSummary`
// (`AgentSettingsTests`).
final class TriggerWhenTests: XCTestCase {

    // MARK: - Tolerant parse

    func testParsesACronSchedule() {
        let raw = #"{"kind":"schedule","cron":"0 9 * * 1-5"}"#
        guard case let .schedule(s)? = TriggerWhen.parse(raw) else {
            return XCTFail("expected a schedule trigger")
        }
        XCTAssertEqual(s.cron, "0 9 * * 1-5")
        XCTAssertNil(s.tz)
    }

    func testParsesAScheduleInAZone() {
        let raw = #"{"kind":"schedule","cron":"*/10 * * * *","tz":"Europe/Vienna"}"#
        XCTAssertEqual(
            TriggerWhen.parse(raw),
            .schedule(ScheduleTrigger(cron: "*/10 * * * *", tz: "Europe/Vienna"))
        )
        // An empty zone = UTC (none).
        XCTAssertEqual(
            TriggerWhen.parse(#"{"kind":"schedule","cron":"0 0 * * *","tz":""}"#),
            .schedule(ScheduleTrigger(cron: "0 0 * * *"))
        )
    }

    // The when-part reader ignores the keys that sit beside it in a stored
    // trigger.
    func testStoredKeysAreIgnoredByTheWhenPart() {
        let raw = #"{"id":"tr-1","kind":"schedule","enabled":false,"cron":"0 7 * * *"}"#
        XCTAssertEqual(TriggerWhen.parse(raw), .schedule(ScheduleTrigger(cron: "0 7 * * *")))
    }

    func testParsesBuiltinAndCustomEventNames() {
        for name in DomainContract.eventBuiltinNames + ["ui-changed", "deploy:prod"] {
            XCTAssertEqual(
                TriggerWhen.parse(#"{"kind":"event","name":"\#(name)"}"#),
                .event(EventTrigger(name: name)),
                name
            )
        }
    }

    // ANY malformation is "no trigger" — a future server shape must never
    // crash or half-render on this build.
    func testMalformedTriggersReadAsNoTrigger() {
        XCTAssertNil(TriggerWhen.parse(nil))
        XCTAssertNil(TriggerWhen.parse(""))
        XCTAssertNil(TriggerWhen.parse("not json"))
        XCTAssertNil(TriggerWhen.parse("[1,2,3]"))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"webhook"}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"schedule"}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"schedule","cron":""}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"schedule","cron":7}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"event"}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"event","name":""}"#))
        // The pre-NEW-2 shapes are no triggers any more.
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"schedule","interval":"daily","minuteOfDay":420}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"event","event":"created"}"#))
    }

    // MARK: - Stored triggers (id + enabled + when)

    private let daily = TriggerWhen.schedule(ScheduleTrigger(cron: "0 9 * * *"))
    private let created = TriggerWhen.event(EventTrigger(name: "issue.created"))

    func testParsesTheStoredTriggersInOrder() {
        let raw = #"""
        [{"id":"tr-1","enabled":true,"kind":"schedule","cron":"0 9 * * *"},
         {"id":"tr-2","enabled":false,"kind":"event","name":"issue.created"}]
        """#
        XCTAssertEqual(AgentTrigger.parseList(raw), [
            AgentTrigger(id: "tr-1", enabled: true, when: daily),
            AgentTrigger(id: "tr-2", enabled: false, when: created),
        ])
    }

    // Tolerant read: an element without a string id, or with an unreadable
    // when-part, is SKIPPED — its readable neighbours stay.
    func testUnreadableStoredTriggersAreSkipped() {
        let raw = #"""
        [{"kind":"schedule","cron":"0 9 * * *"},
         {"id":7,"kind":"schedule","cron":"0 9 * * *"},
         {"id":"","kind":"schedule","cron":"0 9 * * *"},
         {"id":"bad-when","kind":"webhook"},
         {"id":"old-shape","kind":"event","event":"created"},
         "nope", 3, null,
         {"id":"ok","kind":"event","name":"issue.created"}]
        """#
        XCTAssertEqual(AgentTrigger.parseList(raw).map(\.id), ["ok"])
    }

    func testAnythingButAnArrayIsNoTriggers() {
        XCTAssertEqual(AgentTrigger.parseList(nil), [])
        XCTAssertEqual(AgentTrigger.parseList(""), [])
        XCTAssertEqual(AgentTrigger.parseList("not json"), [])
        XCTAssertEqual(AgentTrigger.parseList("[]"), [])
        XCTAssertEqual(AgentTrigger.parseList(#"{"id":"tr-1","kind":"event","name":"issue.created"}"#), [])
    }

    // Only an explicit `false` pauses; a missing flag — or anything that is
    // not a JSON boolean — is an enabled trigger.
    func testEnabledDefaults() {
        func parse(_ extra: String) -> AgentTrigger? {
            AgentTrigger.parseList(#"[{"id":"tr-1","kind":"event","name":"issue.created"\#(extra)}]"#).first
        }
        XCTAssertEqual(parse("")?.enabled, true)
        XCTAssertEqual(parse(#","enabled":true"#)?.enabled, true)
        XCTAssertEqual(parse(#","enabled":false"#)?.enabled, false)
        XCTAssertEqual(parse(#","enabled":0"#)?.enabled, true)
        XCTAssertEqual(parse(#","enabled":"false""#)?.enabled, true)
        XCTAssertEqual(parse(#","enabled":null"#)?.enabled, true)
    }

    // MARK: - The whole-array `triggers` write

    private func json(_ value: some Encodable) throws -> Any {
        try JSONSerialization.jsonObject(with: JSONEncoder().encode(value))
    }

    // A NEW element carries no id (the server mints it); the server's strict
    // schema takes exactly these keys.
    func testNewTriggerElementEncoding() throws {
        let schedule = try XCTUnwrap(try json(AgentTriggerInput(when: daily)) as? [String: Any])
        XCTAssertEqual(schedule.keys.sorted(), ["cron", "enabled", "kind"])
        XCTAssertEqual(schedule["enabled"] as? Bool, true)
        XCTAssertEqual(schedule["kind"] as? String, "schedule")
        XCTAssertEqual(schedule["cron"] as? String, "0 9 * * *")

        let zoned = try XCTUnwrap(try json(AgentTriggerInput(
            when: .schedule(ScheduleTrigger(cron: "0 9 * * *", tz: "Europe/Vienna"))
        )) as? [String: Any])
        XCTAssertEqual(zoned.keys.sorted(), ["cron", "enabled", "kind", "tz"])
        XCTAssertEqual(zoned["tz"] as? String, "Europe/Vienna")

        let event = try XCTUnwrap(try json(AgentTriggerInput(
            id: "tr-2",
            enabled: false,
            when: .event(EventTrigger(name: "pr.merged"))
        )) as? [String: Any])
        XCTAssertEqual(event.keys.sorted(), ["enabled", "id", "kind", "name"])
        XCTAssertEqual(event["id"] as? String, "tr-2")
        XCTAssertEqual(event["enabled"] as? Bool, false)
        XCTAssertEqual(event["name"] as? String, "pr.merged")
    }

    // What is written reads back as the same trigger.
    func testEncodedElementRoundTripsThroughTheParser() throws {
        let trigger = AgentTrigger(id: "tr-1", enabled: false, when: created)
        let data = try JSONEncoder().encode([AgentTriggerInput(trigger)])
        XCTAssertEqual(AgentTrigger.parseList(String(data: data, encoding: .utf8)), [trigger])
    }

    // The write is a WHOLE-ARRAY replace: every helper sends all the other
    // triggers back untouched, ids kept.
    func testWholeArrayWrites() {
        let existing = [AgentTrigger(id: "a", when: daily), AgentTrigger(id: "b", when: created)]
        let draft = AgentTriggerInput(id: "ignored", when: daily)

        let added = existing.adding(draft)
        XCTAssertEqual(added.map(\.id), ["a", "b", nil])

        let replaced = existing.replacing(id: "b", with: draft)
        XCTAssertEqual(replaced.map(\.id), ["a", "b"])
        XCTAssertEqual(replaced.map(\.when), [daily, daily])

        let toggled = existing.settingEnabled(id: "a", false)
        XCTAssertEqual(toggled.map(\.id), ["a", "b"])
        XCTAssertEqual(toggled.map(\.enabled), [false, true])

        XCTAssertEqual(existing.removing(id: "a").map(\.id), ["b"])
    }

    func testSummaryReadsTheAgentSettingsSentence() {
        XCTAssertEqual(AgentTrigger(id: "a", when: daily).summary, "Every day at 09:00")
        XCTAssertEqual(AgentTrigger(id: "b", enabled: false, when: created).summary, "When an issue is created · Paused")
    }

    // MARK: - The contract constants (NEW-2)

    /// The generated agent-trigger / events constants against contract.json
    /// (they replaced agentTriggerEventValues, agentScheduleIntervalValues,
    /// agentTriggerMaxFilterIds, triggerCooldownMs, triggerEventCatchupMs).
    func testGeneratedTriggerAndEventConstantsMatchTheContract() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/contract.json")
        let contract = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any]
        )
        let trigger = try XCTUnwrap(contract["agentTrigger"] as? [String: Any])
        XCTAssertEqual(trigger["kindValues"] as? [String], DomainContract.agentTriggerKindValues)
        XCTAssertEqual(DomainContract.agentTriggerKindValues, ["schedule", "event"])
        XCTAssertEqual(trigger["maxPerAgent"] as? Int, DomainContract.agentTriggerMaxPerAgent)
        XCTAssertEqual(trigger["cronMax"] as? Int, DomainContract.agentTriggerCronMax)
        XCTAssertEqual(trigger["tzMax"] as? Int, DomainContract.agentTriggerTzMax)
        XCTAssertEqual(trigger["catchUpHours"] as? Int, DomainContract.agentTriggerCatchUpHours)
        let events = try XCTUnwrap(contract["events"] as? [String: Any])
        XCTAssertEqual(events["builtinNames"] as? [String], DomainContract.eventBuiltinNames)
        XCTAssertEqual(events["emitterValues"] as? [String], DomainContract.eventEmitterValues)
        XCTAssertEqual(events["nameMax"] as? Int, DomainContract.eventNameMax)
        XCTAssertEqual(events["payloadMaxBytes"] as? Int, DomainContract.eventPayloadMaxBytes)
        XCTAssertEqual(events["retentionDays"] as? Int, DomainContract.eventRetentionDays)
    }
}
