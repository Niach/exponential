import Foundation
import XCTest
@testable import ExpCore

// Triggers (EXP-530; EXP-1291: a team agent carries them): the tolerant
// when-part parser (any malformation reads as "no trigger", never a crash),
// the stored `AgentTrigger` element (id + enabled + when) and the wire
// encoding of the whole-array `triggers` write. The Schedule row's sentence
// is `AgentSettings.triggerSummary` (`AgentSettingsTests`).
final class TriggerWhenTests: XCTestCase {

    // MARK: - Tolerant parse

    func testParsesADailySchedule() {
        let raw = #"{"kind":"schedule","interval":"daily","minuteOfDay":420}"#
        guard case let .schedule(s)? = TriggerWhen.parse(raw) else {
            return XCTFail("expected a schedule trigger")
        }
        XCTAssertEqual(s.interval, "daily")
        XCTAssertEqual(s.minuteOfDay, 420)
        XCTAssertNil(s.weekday)
        XCTAssertNil(s.dayOfMonth)
    }

    func testParsesWeeklyAndMonthlySchedules() {
        let weekly = #"{"kind":"schedule","interval":"weekly","minuteOfDay":540,"weekday":1}"#
        guard case let .schedule(w)? = TriggerWhen.parse(weekly) else {
            return XCTFail("expected weekly")
        }
        XCTAssertEqual(w.weekday, 1)

        let monthly = #"{"kind":"schedule","interval":"monthly","minuteOfDay":540,"dayOfMonth":5}"#
        guard case let .schedule(m)? = TriggerWhen.parse(monthly) else {
            return XCTFail("expected monthly")
        }
        XCTAssertEqual(m.dayOfMonth, 5)
    }

    // The when-part reader ignores the keys that sit beside it in a stored
    // trigger.
    func testStoredKeysAreIgnoredByTheWhenPart() {
        let raw = #"{"id":"tr-1","kind":"schedule","enabled":false,"interval":"daily","minuteOfDay":420}"#
        guard case let .schedule(s)? = TriggerWhen.parse(raw) else {
            return XCTFail("expected a schedule trigger")
        }
        XCTAssertEqual(s.minuteOfDay, 420)
    }

    // SLOP-2: an absent source is Exponential's; any OTHER source is a future
    // one this build cannot fire on, so the trigger is unreadable.
    func testEventSourceGate() {
        XCTAssertNotNil(TriggerWhen.parse(#"{"kind":"event","event":"created"}"#))
        XCTAssertNotNil(TriggerWhen.parse(
            #"{"kind":"event","source":"exponential","event":"created"}"#
        ))
        XCTAssertNil(TriggerWhen.parse(
            #"{"kind":"event","source":"github","event":"created"}"#
        ))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"event","source":7,"event":"created"}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"event","source":null,"event":"created"}"#))
    }

    func testParsesAnEventTriggerWithFilters() {
        let raw = #"""
        {"kind":"event","event":"status_changed",
         "filters":{"boardIds":["b1","b2"],"toStatusIds":["s1"]}}
        """#
        guard case let .event(e)? = TriggerWhen.parse(raw) else {
            return XCTFail("expected an event trigger")
        }
        XCTAssertEqual(e.event, "status_changed")
        XCTAssertEqual(e.filters.boardIds, ["b1", "b2"])
        XCTAssertEqual(e.filters.toStatusIds, ["s1"])
        XCTAssertEqual(e.filters.totalCount, 3)
    }

    func testEveryContractEventValueParses() {
        for event in DomainContract.agentTriggerEventValues {
            let raw = #"{"kind":"event","event":"\#(event)"}"#
            XCTAssertNotNil(TriggerWhen.parse(raw), "event \(event) must parse")
        }
    }

    // ANY malformation is "no trigger" — a future server shape must never
    // crash or half-render on this build.
    func testMalformedTriggersReadAsNoTrigger() {
        XCTAssertNil(TriggerWhen.parse(nil))
        XCTAssertNil(TriggerWhen.parse(""))
        XCTAssertNil(TriggerWhen.parse("not json"))
        XCTAssertNil(TriggerWhen.parse("[1,2,3]"))
        // Unknown kind / event.
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"webhook"}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"event","event":"issue_teleported"}"#))
        // Schedule field violations.
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"schedule","interval":"hourly","minuteOfDay":0}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"schedule","interval":"daily","minuteOfDay":1440}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"schedule","interval":"daily"}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"schedule","interval":"weekly","minuteOfDay":0}"#))
        XCTAssertNil(TriggerWhen.parse(#"{"kind":"schedule","interval":"monthly","minuteOfDay":0,"dayOfMonth":29}"#))
    }

    func testUnknownFilterListsAreToleratedAsEmpty() {
        let raw = #"{"kind":"event","event":"created","filters":{"boardIds":"oops"}}"#
        guard case let .event(e)? = TriggerWhen.parse(raw) else {
            return XCTFail("expected an event trigger")
        }
        XCTAssertTrue(e.filters.isEmpty)
    }

    // MARK: - Stored triggers (id + enabled + when)

    private let daily = TriggerWhen.schedule(ScheduleTrigger(interval: "daily", minuteOfDay: 540))
    private let created = TriggerWhen.event(EventTrigger(event: "created"))

    func testParsesTheStoredTriggersInOrder() {
        let raw = #"""
        [{"id":"tr-1","enabled":true,"kind":"schedule","interval":"daily","minuteOfDay":540},
         {"id":"tr-2","enabled":false,"kind":"event","source":"exponential",
          "event":"created","filters":{"boardIds":["b1"]}}]
        """#
        XCTAssertEqual(AgentTrigger.parseList(raw), [
            AgentTrigger(id: "tr-1", enabled: true, when: daily),
            AgentTrigger(
                id: "tr-2",
                enabled: false,
                when: .event(EventTrigger(event: "created", filters: TriggerFilters(boardIds: ["b1"])))
            ),
        ])
    }

    // Tolerant read: an element without a string id, or with an unreadable
    // when-part, is SKIPPED — its readable neighbours stay.
    func testUnreadableStoredTriggersAreSkipped() {
        let raw = #"""
        [{"kind":"schedule","interval":"daily","minuteOfDay":540},
         {"id":7,"kind":"schedule","interval":"daily","minuteOfDay":540},
         {"id":"","kind":"schedule","interval":"daily","minuteOfDay":540},
         {"id":"bad-when","kind":"webhook"},
         {"id":"future-source","kind":"event","source":"github","event":"created"},
         "nope", 3, null,
         {"id":"ok","kind":"event","event":"created"}]
        """#
        XCTAssertEqual(AgentTrigger.parseList(raw).map(\.id), ["ok"])
    }

    func testAnythingButAnArrayIsNoTriggers() {
        XCTAssertEqual(AgentTrigger.parseList(nil), [])
        XCTAssertEqual(AgentTrigger.parseList(""), [])
        XCTAssertEqual(AgentTrigger.parseList("not json"), [])
        XCTAssertEqual(AgentTrigger.parseList("[]"), [])
        XCTAssertEqual(AgentTrigger.parseList(#"{"id":"tr-1","kind":"event","event":"created"}"#), [])
    }

    // Only an explicit `false` pauses; a missing flag — or anything that is
    // not a JSON boolean — is an enabled trigger.
    func testEnabledDefaults() {
        func parse(_ extra: String) -> AgentTrigger? {
            AgentTrigger.parseList(#"[{"id":"tr-1","kind":"event","event":"created"\#(extra)}]"#).first
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

    // A NEW element carries no id (the server mints it) and an event names
    // its source.
    func testNewTriggerElementEncoding() throws {
        let schedule = try XCTUnwrap(try json(AgentTriggerInput(when: daily)) as? [String: Any])
        XCTAssertEqual(schedule.keys.sorted(), ["enabled", "interval", "kind", "minuteOfDay"])
        XCTAssertEqual(schedule["enabled"] as? Bool, true)
        XCTAssertEqual(schedule["kind"] as? String, "schedule")
        XCTAssertEqual(schedule["minuteOfDay"] as? Int, 540)

        let event = try XCTUnwrap(try json(AgentTriggerInput(
            id: "tr-2",
            enabled: false,
            when: .event(EventTrigger(event: "status_changed", filters: TriggerFilters(toStatusIds: ["s1"])))
        )) as? [String: Any])
        XCTAssertEqual(event.keys.sorted(), ["enabled", "event", "filters", "id", "kind", "source"])
        XCTAssertEqual(event["id"] as? String, "tr-2")
        XCTAssertEqual(event["enabled"] as? Bool, false)
        XCTAssertEqual(event["source"] as? String, "exponential")
        XCTAssertEqual((event["filters"] as? [String: Any])?["toStatusIds"] as? [String], ["s1"])
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
        XCTAssertEqual(AgentTrigger(id: "a", when: daily).summary, "Daily at 09:00")
        XCTAssertEqual(AgentTrigger(id: "b", enabled: false, when: created).summary, "When an issue is created · Paused")
    }
}
