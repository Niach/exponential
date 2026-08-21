import Foundation
import XCTest
@testable import ExpCore

// EXP-583 automations: the tolerant trigger parser (any malformation reads as
// "no trigger", never a crash), the summary sentences (byte-matching the web's
// `triggerSummary`), the when-part-only wire encoding, the machine-readable
// automation note the create sheet appends, and the next-run schedule math.
final class AutomationTriggerTests: XCTestCase {

    // MARK: - Tolerant parse

    func testParsesADailySchedule() {
        let raw = #"{"kind":"schedule","interval":"daily","minuteOfDay":420}"#
        guard case let .schedule(s)? = AutomationTrigger.parse(raw) else {
            return XCTFail("expected a schedule trigger")
        }
        XCTAssertEqual(s.interval, "daily")
        XCTAssertEqual(s.minuteOfDay, 420)
        XCTAssertNil(s.weekday)
        XCTAssertNil(s.dayOfMonth)
    }

    func testParsesWeeklyAndMonthlySchedules() {
        let weekly = #"{"kind":"schedule","interval":"weekly","minuteOfDay":540,"weekday":1}"#
        guard case let .schedule(w)? = AutomationTrigger.parse(weekly) else {
            return XCTFail("expected weekly")
        }
        XCTAssertEqual(w.weekday, 1)

        let monthly = #"{"kind":"schedule","interval":"monthly","minuteOfDay":540,"dayOfMonth":5}"#
        guard case let .schedule(m)? = AutomationTrigger.parse(monthly) else {
            return XCTFail("expected monthly")
        }
        XCTAssertEqual(m.dayOfMonth, 5)
    }

    // EXP-583: device/enabled moved onto the automations ROW — a legacy
    // EXP-530 payload still parses, its extra keys simply ignored.
    func testLegacyDeviceAndEnabledKeysAreIgnored() {
        let raw = #"{"kind":"schedule","deviceId":"dev-1","enabled":false,"interval":"daily","minuteOfDay":420}"#
        guard case let .schedule(s)? = AutomationTrigger.parse(raw) else {
            return XCTFail("expected a schedule trigger")
        }
        XCTAssertEqual(s.minuteOfDay, 420)
        // …and a trigger with no device at all is perfectly valid now.
        XCTAssertNotNil(AutomationTrigger.parse(#"{"kind":"event","event":"created"}"#))
    }

    func testParsesAnEventTriggerWithFilters() {
        let raw = #"""
        {"kind":"event","event":"status_changed",
         "filters":{"boardIds":["b1","b2"],"toStatusIds":["s1"]}}
        """#
        guard case let .event(e)? = AutomationTrigger.parse(raw) else {
            return XCTFail("expected an event trigger")
        }
        XCTAssertEqual(e.event, "status_changed")
        XCTAssertEqual(e.filters.boardIds, ["b1", "b2"])
        XCTAssertEqual(e.filters.toStatusIds, ["s1"])
        XCTAssertEqual(e.filters.totalCount, 3)
    }

    func testEveryContractEventValueParses() {
        for event in DomainContract.actionTriggerEventValues {
            let raw = #"{"kind":"event","event":"\#(event)"}"#
            XCTAssertNotNil(AutomationTrigger.parse(raw), "event \(event) must parse")
        }
    }

    // ANY malformation is "no trigger" — a future server shape must never
    // crash or half-render on this build.
    func testMalformedTriggersReadAsNoTrigger() {
        XCTAssertNil(AutomationTrigger.parse(nil))
        XCTAssertNil(AutomationTrigger.parse(""))
        XCTAssertNil(AutomationTrigger.parse("not json"))
        XCTAssertNil(AutomationTrigger.parse("[1,2,3]"))
        // Unknown kind / event.
        XCTAssertNil(AutomationTrigger.parse(#"{"kind":"webhook"}"#))
        XCTAssertNil(AutomationTrigger.parse(#"{"kind":"event","event":"issue_teleported"}"#))
        // Schedule field violations.
        XCTAssertNil(AutomationTrigger.parse(#"{"kind":"schedule","interval":"hourly","minuteOfDay":0}"#))
        XCTAssertNil(AutomationTrigger.parse(#"{"kind":"schedule","interval":"daily","minuteOfDay":1440}"#))
        XCTAssertNil(AutomationTrigger.parse(#"{"kind":"schedule","interval":"daily"}"#))
        XCTAssertNil(AutomationTrigger.parse(#"{"kind":"schedule","interval":"weekly","minuteOfDay":0}"#))
        XCTAssertNil(AutomationTrigger.parse(#"{"kind":"schedule","interval":"monthly","minuteOfDay":0,"dayOfMonth":29}"#))
    }

    func testUnknownFilterListsAreToleratedAsEmpty() {
        let raw = #"{"kind":"event","event":"created","filters":{"boardIds":"oops"}}"#
        guard case let .event(e)? = AutomationTrigger.parse(raw) else {
            return XCTFail("expected an event trigger")
        }
        XCTAssertTrue(e.filters.isEmpty)
    }

    // MARK: - Wire encoding

    func testWireJSONRoundTrips() throws {
        let trigger = AutomationTrigger.event(AutomationEventTrigger(
            event: "label_added",
            filters: AutomationTriggerFilters(boardIds: ["b1"], labelIds: ["l1"])
        ))
        // Canonical key order (web/Android/desktop byte parity), filters in
        // boardIds/labelIds/priorities/toStatusIds order, empties omitted.
        XCTAssertEqual(
            trigger.wireJSONString,
            #"{"kind":"event","event":"label_added","filters":{"boardIds":["b1"],"labelIds":["l1"]}}"#
        )
        // The compact string re-parses into the identical trigger.
        XCTAssertEqual(AutomationTrigger.parse(trigger.wireJSONString), trigger)
        // …and so does the Encodable form the automations input embeds.
        let encoded = String(data: try JSONEncoder().encode(trigger), encoding: .utf8)
        XCTAssertEqual(AutomationTrigger.parse(encoded), trigger)
    }

    func testScheduleWireJSONOmitsIrrelevantFields() {
        let daily = AutomationTrigger.schedule(AutomationScheduleTrigger(
            interval: "daily", minuteOfDay: 420
        ))
        // Canonical key order — byte-locked against web JSON.stringify,
        // Android toWireJsonString and desktop's preserve_order serde_json.
        XCTAssertEqual(
            daily.wireJSONString,
            #"{"kind":"schedule","interval":"daily","minuteOfDay":420}"#
        )
        XCTAssertEqual(AutomationTrigger.parse(daily.wireJSONString), daily)

        let weekly = AutomationTrigger.schedule(AutomationScheduleTrigger(
            interval: "weekly", minuteOfDay: 540, weekday: 1
        ))
        XCTAssertEqual(
            weekly.wireJSONString,
            #"{"kind":"schedule","interval":"weekly","minuteOfDay":540,"weekday":1}"#
        )
    }

    // MARK: - The creator-run automation note

    // Byte-locked against the web's `formatAutomationBlock`
    // (apps/web/src/lib/action-triggers.ts): key order deviceId, trigger,
    // then agent/model/effort only when set; compact JSON.
    func testAutomationNoteMatchesTheWebBlock() {
        let spec = AutomationSpec(
            trigger: .schedule(AutomationScheduleTrigger(interval: "daily", minuteOfDay: 540)),
            deviceId: "dev-1"
        )
        XCTAssertEqual(
            AutomationNote.format(spec),
            "\n\nAutomation — after creating the action, call exponential_automations_create with its id and exactly these fields: `{\"deviceId\":\"dev-1\",\"trigger\":{\"kind\":\"schedule\",\"interval\":\"daily\",\"minuteOfDay\":540}}`. An automated run fills no inputs, so declare none as required."
        )
    }

    func testAutomationNoteCarriesTheLaunchFieldsInOrder() {
        let spec = AutomationSpec(
            trigger: .event(AutomationEventTrigger(event: "created")),
            deviceId: "dev-2",
            agent: "codex",
            model: "gpt-5.6-sol",
            effort: "high"
        )
        XCTAssertEqual(
            AutomationNote.format(spec),
            "\n\nAutomation — after creating the action, call exponential_automations_create with its id and exactly these fields: `{\"deviceId\":\"dev-2\",\"trigger\":{\"kind\":\"event\",\"event\":\"created\"},\"agent\":\"codex\",\"model\":\"gpt-5.6-sol\",\"effort\":\"high\"}`. An automated run fills no inputs, so declare none as required."
        )
        // Blank launch fields are omitted (the web's falsy check).
        let blank = AutomationSpec(
            trigger: .event(AutomationEventTrigger(event: "created")),
            deviceId: "dev-2",
            agent: "",
            model: nil,
            effort: ""
        )
        XCTAssertFalse(AutomationNote.format(blank).contains("agent"))
        XCTAssertFalse(AutomationNote.format(blank).contains("effort"))
    }

    // MARK: - Summary sentences (byte-matched to the web)

    private func schedule(
        _ interval: String, minute: Int, weekday: Int? = nil, day: Int? = nil
    ) -> AutomationTrigger {
        .schedule(AutomationScheduleTrigger(
            interval: interval, minuteOfDay: minute, weekday: weekday, dayOfMonth: day
        ))
    }

    func testScheduleSummaries() {
        XCTAssertEqual(
            AutomationTriggerDisplay.summary(schedule("daily", minute: 420)),
            "Daily at 07:00"
        )
        XCTAssertEqual(
            AutomationTriggerDisplay.summary(schedule("weekly", minute: 540, weekday: 1)),
            "Weekly on Monday at 09:00"
        )
        XCTAssertEqual(
            AutomationTriggerDisplay.summary(schedule("weekly", minute: 0, weekday: 7)),
            "Weekly on Sunday at 00:00"
        )
        XCTAssertEqual(
            AutomationTriggerDisplay.summary(schedule("monthly", minute: 540, day: 5)),
            "Monthly on day 5 at 09:00"
        )
    }

    func testEventSummaries() {
        func event(_ name: String, filters: AutomationTriggerFilters = AutomationTriggerFilters()) -> AutomationTrigger {
            .event(AutomationEventTrigger(event: name, filters: filters))
        }
        XCTAssertEqual(AutomationTriggerDisplay.summary(event("created")), "When an issue is created")
        XCTAssertEqual(AutomationTriggerDisplay.summary(event("status_changed")), "When status changes")
        XCTAssertEqual(AutomationTriggerDisplay.summary(event("assignee_changed")), "When the assignee changes")
        XCTAssertEqual(AutomationTriggerDisplay.summary(event("label_added")), "When a label is added")
        XCTAssertEqual(AutomationTriggerDisplay.summary(event("priority_changed")), "When priority changes")
        XCTAssertEqual(AutomationTriggerDisplay.summary(event("pr_opened")), "When a pull request is opened")
        XCTAssertEqual(AutomationTriggerDisplay.summary(event("pr_merged")), "When a pull request is merged")
        // The filter count spans EVERY list, and one filter reads singular
        // (web/Android/desktop parity).
        XCTAssertEqual(
            AutomationTriggerDisplay.summary(event(
                "status_changed",
                filters: AutomationTriggerFilters(boardIds: ["b1", "b2"], toStatusIds: ["s1"])
            )),
            "When status changes · 3 filters"
        )
        XCTAssertEqual(
            AutomationTriggerDisplay.summary(event(
                "created",
                filters: AutomationTriggerFilters(priorities: ["urgent"])
            )),
            "When an issue is created · 1 filter"
        )
    }

    // MARK: - Next run

    /// A fixed UTC calendar so the boundary math is deterministic on any
    /// machine (callers pass the viewer's `.current`).
    private var utc: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar
    }

    /// 2026-07-17 is a Friday.
    private func date(_ iso: String) -> Date {
        let formatter = ISO8601DateFormatter()
        return formatter.date(from: iso)!
    }

    func testDailyNextRunBoundaries() {
        guard case let .schedule(s) = schedule("daily", minute: 420) else { return XCTFail() }
        // Before today's 07:00 → today.
        XCTAssertEqual(
            AutomationTriggerDisplay.nextScheduleRun(s, after: date("2026-07-17T05:00:00Z"), calendar: utc),
            date("2026-07-17T07:00:00Z")
        )
        // Exactly at the occurrence → strictly after, so tomorrow.
        XCTAssertEqual(
            AutomationTriggerDisplay.nextScheduleRun(s, after: date("2026-07-17T07:00:00Z"), calendar: utc),
            date("2026-07-18T07:00:00Z")
        )
    }

    func testWeeklyNextRunUsesTheWireWeekdayConvention() {
        // weekday 1 = Monday; 2026-07-17 is a Friday → next Monday is the 20th.
        guard case let .schedule(s) = schedule("weekly", minute: 540, weekday: 1) else { return XCTFail() }
        XCTAssertEqual(
            AutomationTriggerDisplay.nextScheduleRun(s, after: date("2026-07-17T12:00:00Z"), calendar: utc),
            date("2026-07-20T09:00:00Z")
        )
        // weekday 7 = Sunday → the 19th.
        guard case let .schedule(sun) = schedule("weekly", minute: 540, weekday: 7) else { return XCTFail() }
        XCTAssertEqual(
            AutomationTriggerDisplay.nextScheduleRun(sun, after: date("2026-07-17T12:00:00Z"), calendar: utc),
            date("2026-07-19T09:00:00Z")
        )
    }

    func testMonthlyNextRunRollsIntoTheNextMonth() {
        guard case let .schedule(s) = schedule("monthly", minute: 540, day: 5) else { return XCTFail() }
        XCTAssertEqual(
            AutomationTriggerDisplay.nextScheduleRun(s, after: date("2026-07-17T12:00:00Z"), calendar: utc),
            date("2026-08-05T09:00:00Z")
        )
        XCTAssertEqual(
            AutomationTriggerDisplay.nextScheduleRun(s, after: date("2026-07-01T00:00:00Z"), calendar: utc),
            date("2026-07-05T09:00:00Z")
        )
    }

    // MARK: - Entity plumbing

    func testAutomationDtoParsesTheEntityTrigger() {
        let entity = AutomationEntity(
            id: "au1",
            teamId: "t1",
            actionId: "a1",
            deviceId: "dev-1",
            enabled: false,
            trigger: #"{"kind":"schedule","interval":"daily","minuteOfDay":420}"#,
            agent: "claude",
            model: "opus",
            effort: "high",
            sortOrder: 3,
            createdAt: "2026-01-01T00:00:00Z",
            updatedAt: "2026-01-01T00:00:00Z"
        )
        let dto = AutomationDto(entity: entity)
        XCTAssertEqual(dto.actionId, "a1")
        XCTAssertEqual(dto.deviceId, "dev-1")
        XCTAssertFalse(dto.enabled)
        XCTAssertEqual(dto.agent, "claude")
        XCTAssertEqual(dto.sortOrder, 3)
        guard case .schedule? = dto.parsedTrigger else {
            return XCTFail("entity trigger must reach the DTO")
        }
    }
}
