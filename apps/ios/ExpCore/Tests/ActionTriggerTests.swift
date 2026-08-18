import Foundation
import XCTest
@testable import ExpCore

// EXP-530 action automations: the tolerant trigger parser (any malformation
// reads as "no automation", never a crash), the summary sentences (byte-
// matching the web's `triggerSummary`), the wire encoding the enabled toggle
// and the create sheet emit, and the next-run schedule math.
final class ActionTriggerTests: XCTestCase {

    // MARK: - Tolerant parse

    func testParsesADailySchedule() {
        let raw = #"{"kind":"schedule","deviceId":"dev-1","interval":"daily","minuteOfDay":420}"#
        guard case let .schedule(s)? = ActionTrigger.parse(raw) else {
            return XCTFail("expected a schedule trigger")
        }
        XCTAssertEqual(s.deviceId, "dev-1")
        XCTAssertEqual(s.interval, "daily")
        XCTAssertEqual(s.minuteOfDay, 420)
        XCTAssertNil(s.weekday)
        XCTAssertNil(s.dayOfMonth)
        // Missing `enabled` defaults true.
        XCTAssertTrue(s.enabled)
    }

    func testParsesWeeklyAndMonthlySchedules() {
        let weekly = #"{"kind":"schedule","deviceId":"d","enabled":false,"interval":"weekly","minuteOfDay":540,"weekday":1}"#
        guard case let .schedule(w)? = ActionTrigger.parse(weekly) else {
            return XCTFail("expected weekly")
        }
        XCTAssertEqual(w.weekday, 1)
        XCTAssertFalse(w.enabled)

        let monthly = #"{"kind":"schedule","deviceId":"d","interval":"monthly","minuteOfDay":540,"dayOfMonth":5}"#
        guard case let .schedule(m)? = ActionTrigger.parse(monthly) else {
            return XCTFail("expected monthly")
        }
        XCTAssertEqual(m.dayOfMonth, 5)
    }

    func testParsesAnEventTriggerWithFilters() {
        let raw = #"""
        {"kind":"event","deviceId":"dev-2","event":"status_changed",
         "filters":{"boardIds":["b1","b2"],"toStatusIds":["s1"]}}
        """#
        guard case let .event(e)? = ActionTrigger.parse(raw) else {
            return XCTFail("expected an event trigger")
        }
        XCTAssertEqual(e.event, "status_changed")
        XCTAssertEqual(e.filters.boardIds, ["b1", "b2"])
        XCTAssertEqual(e.filters.toStatusIds, ["s1"])
        XCTAssertEqual(e.filters.totalCount, 3)
        XCTAssertTrue(e.enabled)
    }

    func testEveryContractEventValueParses() {
        for event in DomainContract.actionTriggerEventValues {
            let raw = #"{"kind":"event","deviceId":"d","event":"\#(event)"}"#
            XCTAssertNotNil(ActionTrigger.parse(raw), "event \(event) must parse")
        }
    }

    // ANY malformation is "no automation" — a future server shape must never
    // crash or half-render on this build.
    func testMalformedTriggersReadAsNoAutomation() {
        XCTAssertNil(ActionTrigger.parse(nil))
        XCTAssertNil(ActionTrigger.parse(""))
        XCTAssertNil(ActionTrigger.parse("not json"))
        XCTAssertNil(ActionTrigger.parse("[1,2,3]"))
        // Unknown kind / event.
        XCTAssertNil(ActionTrigger.parse(#"{"kind":"webhook","deviceId":"d"}"#))
        XCTAssertNil(ActionTrigger.parse(#"{"kind":"event","deviceId":"d","event":"issue_teleported"}"#))
        // Missing/blank device.
        XCTAssertNil(ActionTrigger.parse(#"{"kind":"schedule","interval":"daily","minuteOfDay":0}"#))
        XCTAssertNil(ActionTrigger.parse(#"{"kind":"schedule","deviceId":"","interval":"daily","minuteOfDay":0}"#))
        // Schedule field violations.
        XCTAssertNil(ActionTrigger.parse(#"{"kind":"schedule","deviceId":"d","interval":"hourly","minuteOfDay":0}"#))
        XCTAssertNil(ActionTrigger.parse(#"{"kind":"schedule","deviceId":"d","interval":"daily","minuteOfDay":1440}"#))
        XCTAssertNil(ActionTrigger.parse(#"{"kind":"schedule","deviceId":"d","interval":"daily"}"#))
        XCTAssertNil(ActionTrigger.parse(#"{"kind":"schedule","deviceId":"d","interval":"weekly","minuteOfDay":0}"#))
        XCTAssertNil(ActionTrigger.parse(#"{"kind":"schedule","deviceId":"d","interval":"monthly","minuteOfDay":0,"dayOfMonth":29}"#))
    }

    func testUnknownFilterListsAreToleratedAsEmpty() {
        let raw = #"{"kind":"event","deviceId":"d","event":"created","filters":{"boardIds":"oops"}}"#
        guard case let .event(e)? = ActionTrigger.parse(raw) else {
            return XCTFail("expected an event trigger")
        }
        XCTAssertTrue(e.filters.isEmpty)
    }

    // MARK: - Enabled flip + wire encoding

    func testWithEnabledFlipsBothKinds() {
        let schedule = ActionTrigger.schedule(ActionScheduleTrigger(
            deviceId: "d", interval: "daily", minuteOfDay: 0
        ))
        XCTAssertFalse(schedule.withEnabled(false).enabled)
        let event = ActionTrigger.event(ActionEventTrigger(
            deviceId: "d", enabled: false, event: "created"
        ))
        XCTAssertTrue(event.withEnabled(true).enabled)
    }

    func testWireJSONRoundTrips() throws {
        let trigger = ActionTrigger.event(ActionEventTrigger(
            deviceId: "dev-9",
            enabled: false,
            event: "label_added",
            filters: ActionTriggerFilters(boardIds: ["b1"], labelIds: ["l1"])
        ))
        // The compact string re-parses into the identical trigger.
        XCTAssertEqual(ActionTrigger.parse(trigger.wireJSONString), trigger)
        // …and so does the Encodable form the actions.update input embeds.
        let encoded = String(data: try JSONEncoder().encode(trigger), encoding: .utf8)
        XCTAssertEqual(ActionTrigger.parse(encoded), trigger)
    }

    func testScheduleWireJSONOmitsIrrelevantFields() {
        let daily = ActionTrigger.schedule(ActionScheduleTrigger(
            deviceId: "d", interval: "daily", minuteOfDay: 420
        ))
        XCTAssertEqual(
            daily.wireJSONString,
            #"{"deviceId":"d","enabled":true,"interval":"daily","kind":"schedule","minuteOfDay":420}"#
        )
        XCTAssertEqual(ActionTrigger.parse(daily.wireJSONString), daily)
    }

    // MARK: - Summary sentences (byte-matched to the web)

    private func schedule(
        _ interval: String, minute: Int, weekday: Int? = nil, day: Int? = nil
    ) -> ActionTrigger {
        .schedule(ActionScheduleTrigger(
            deviceId: "d", interval: interval, minuteOfDay: minute,
            weekday: weekday, dayOfMonth: day
        ))
    }

    func testScheduleSummaries() {
        XCTAssertEqual(
            ActionTriggerDisplay.summary(schedule("daily", minute: 420)),
            "Daily at 07:00"
        )
        XCTAssertEqual(
            ActionTriggerDisplay.summary(schedule("weekly", minute: 540, weekday: 1)),
            "Weekly on Monday at 09:00"
        )
        XCTAssertEqual(
            ActionTriggerDisplay.summary(schedule("weekly", minute: 0, weekday: 7)),
            "Weekly on Sunday at 00:00"
        )
        XCTAssertEqual(
            ActionTriggerDisplay.summary(schedule("monthly", minute: 540, day: 5)),
            "Monthly on day 5 at 09:00"
        )
    }

    func testEventSummaries() {
        func event(_ name: String, filters: ActionTriggerFilters = ActionTriggerFilters()) -> ActionTrigger {
            .event(ActionEventTrigger(deviceId: "d", event: name, filters: filters))
        }
        XCTAssertEqual(ActionTriggerDisplay.summary(event("created")), "When an issue is created")
        XCTAssertEqual(ActionTriggerDisplay.summary(event("status_changed")), "When status changes")
        XCTAssertEqual(ActionTriggerDisplay.summary(event("assignee_changed")), "When the assignee changes")
        XCTAssertEqual(ActionTriggerDisplay.summary(event("label_added")), "When a label is added")
        XCTAssertEqual(ActionTriggerDisplay.summary(event("priority_changed")), "When priority changes")
        XCTAssertEqual(ActionTriggerDisplay.summary(event("pr_opened")), "When a pull request is opened")
        XCTAssertEqual(ActionTriggerDisplay.summary(event("pr_merged")), "When a pull request is merged")
        // The filter count spans EVERY list, and one filter reads singular
        // (web/Android/desktop parity).
        XCTAssertEqual(
            ActionTriggerDisplay.summary(event(
                "status_changed",
                filters: ActionTriggerFilters(boardIds: ["b1", "b2"], toStatusIds: ["s1"])
            )),
            "When status changes · 3 filters"
        )
        XCTAssertEqual(
            ActionTriggerDisplay.summary(event(
                "created",
                filters: ActionTriggerFilters(priorities: ["urgent"])
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
            ActionTriggerDisplay.nextScheduleRun(s, after: date("2026-07-17T05:00:00Z"), calendar: utc),
            date("2026-07-17T07:00:00Z")
        )
        // Exactly at the occurrence → strictly after, so tomorrow.
        XCTAssertEqual(
            ActionTriggerDisplay.nextScheduleRun(s, after: date("2026-07-17T07:00:00Z"), calendar: utc),
            date("2026-07-18T07:00:00Z")
        )
    }

    func testWeeklyNextRunUsesTheWireWeekdayConvention() {
        // weekday 1 = Monday; 2026-07-17 is a Friday → next Monday is the 20th.
        guard case let .schedule(s) = schedule("weekly", minute: 540, weekday: 1) else { return XCTFail() }
        XCTAssertEqual(
            ActionTriggerDisplay.nextScheduleRun(s, after: date("2026-07-17T12:00:00Z"), calendar: utc),
            date("2026-07-20T09:00:00Z")
        )
        // weekday 7 = Sunday → the 19th.
        guard case let .schedule(sun) = schedule("weekly", minute: 540, weekday: 7) else { return XCTFail() }
        XCTAssertEqual(
            ActionTriggerDisplay.nextScheduleRun(sun, after: date("2026-07-17T12:00:00Z"), calendar: utc),
            date("2026-07-19T09:00:00Z")
        )
    }

    func testMonthlyNextRunRollsIntoTheNextMonth() {
        guard case let .schedule(s) = schedule("monthly", minute: 540, day: 5) else { return XCTFail() }
        XCTAssertEqual(
            ActionTriggerDisplay.nextScheduleRun(s, after: date("2026-07-17T12:00:00Z"), calendar: utc),
            date("2026-08-05T09:00:00Z")
        )
        XCTAssertEqual(
            ActionTriggerDisplay.nextScheduleRun(s, after: date("2026-07-01T00:00:00Z"), calendar: utc),
            date("2026-07-05T09:00:00Z")
        )
    }

    // MARK: - Entity plumbing

    func testActionDtoParsesTheEntityTrigger() {
        let entity = ActionEntity(
            id: "a1",
            teamId: "t1",
            repositoryId: nil,
            name: "Digest",
            description: nil,
            icon: nil,
            inputs: nil,
            trigger: #"{"kind":"schedule","deviceId":"d","interval":"daily","minuteOfDay":420}"#,
            sortOrder: 0,
            createdAt: "2026-01-01T00:00:00Z",
            updatedAt: "2026-01-01T00:00:00Z"
        )
        let dto = ActionDto(entity: entity)
        guard case .schedule? = dto.parsedTrigger else {
            return XCTFail("entity trigger must reach the DTO")
        }
        // Builtins never carry one.
        XCTAssertNil(ActionDto.builtinCreateAction(teamId: "t1").parsedTrigger)
    }
}
