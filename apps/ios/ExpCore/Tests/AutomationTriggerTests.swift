import Foundation
import XCTest
@testable import ExpCore

// Triggers (EXP-530; SLOP-2: an action carries them): the tolerant when-part
// parser (any malformation reads as "no trigger", never a crash), the stored
// `ActionTrigger` element (runner + when), the row glyphs, the summary
// sentences (byte-matching the web's `triggerSummary`), the wire encoding of
// the `actions.update` array, the machine-readable trigger block a suggestion
// appends, and the next-run schedule math.
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

    // The when-part reader ignores the runner keys that sit beside it in a
    // stored trigger.
    func testRunnerKeysAreIgnoredByTheWhenPart() {
        let raw = #"{"id":"tr-1","kind":"schedule","deviceId":"dev-1","enabled":false,"interval":"daily","minuteOfDay":420}"#
        guard case let .schedule(s)? = AutomationTrigger.parse(raw) else {
            return XCTFail("expected a schedule trigger")
        }
        XCTAssertEqual(s.minuteOfDay, 420)
    }

    // SLOP-2: an absent source is Exponential's; any OTHER source is a future
    // one this build cannot fire on, so the trigger is unreadable.
    func testEventSourceGate() {
        XCTAssertNotNil(AutomationTrigger.parse(#"{"kind":"event","event":"created"}"#))
        XCTAssertNotNil(AutomationTrigger.parse(
            #"{"kind":"event","source":"exponential","event":"created"}"#
        ))
        XCTAssertNil(AutomationTrigger.parse(
            #"{"kind":"event","source":"github","event":"created"}"#
        ))
        XCTAssertNil(AutomationTrigger.parse(#"{"kind":"event","source":7,"event":"created"}"#))
        XCTAssertNil(AutomationTrigger.parse(#"{"kind":"event","source":null,"event":"created"}"#))
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
        // …and so does the Encodable form an `actions.update` element embeds.
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

    // MARK: - Stored triggers (runner + when)

    func testParsesTheStoredTriggersInOrder() {
        let raw = #"""
        [{"id":"tr-1","enabled":true,"deviceId":"dev-1","agent":"claude","account":"p-1",
          "model":"opus","effort":"high","kind":"schedule","interval":"daily","minuteOfDay":540},
         {"id":"tr-2","enabled":false,"deviceId":"dev-2","kind":"event","source":"exponential",
          "event":"created","filters":{"boardIds":["b1"]}}]
        """#
        let triggers = ActionTrigger.parseList(raw)
        XCTAssertEqual(triggers, [
            ActionTrigger(
                id: "tr-1",
                enabled: true,
                deviceId: "dev-1",
                agent: "claude",
                account: "p-1",
                model: "opus",
                effort: "high",
                when: .schedule(AutomationScheduleTrigger(interval: "daily", minuteOfDay: 540))
            ),
            ActionTrigger(
                id: "tr-2",
                enabled: false,
                deviceId: "dev-2",
                when: .event(AutomationEventTrigger(
                    event: "created",
                    filters: AutomationTriggerFilters(boardIds: ["b1"])
                ))
            ),
        ])
    }

    // Tolerant read: an element without a string id or device, or with an
    // unreadable when-part, is SKIPPED — its readable neighbours stay.
    func testUnreadableStoredTriggersAreSkipped() {
        let raw = #"""
        [{"deviceId":"dev-1","kind":"schedule","interval":"daily","minuteOfDay":540},
         {"id":7,"deviceId":"dev-1","kind":"schedule","interval":"daily","minuteOfDay":540},
         {"id":"no-device","kind":"schedule","interval":"daily","minuteOfDay":540},
         {"id":"empty-device","deviceId":"","kind":"schedule","interval":"daily","minuteOfDay":540},
         {"id":"bad-when","deviceId":"dev-1","kind":"webhook"},
         {"id":"future-source","deviceId":"dev-1","kind":"event","source":"github","event":"created"},
         "nope", 3, null,
         {"id":"ok","deviceId":"dev-1","kind":"event","event":"created"}]
        """#
        XCTAssertEqual(ActionTrigger.parseList(raw).map(\.id), ["ok"])
    }

    func testAnythingButAnArrayIsNoTriggers() {
        XCTAssertEqual(ActionTrigger.parseList(nil), [])
        XCTAssertEqual(ActionTrigger.parseList(""), [])
        XCTAssertEqual(ActionTrigger.parseList("not json"), [])
        XCTAssertEqual(ActionTrigger.parseList("[]"), [])
        XCTAssertEqual(
            ActionTrigger.parseList(
                #"{"id":"tr-1","deviceId":"dev-1","kind":"event","event":"created"}"#
            ),
            []
        )
    }

    // Only an explicit `false` pauses; a missing flag — or anything that is
    // not a JSON boolean — is an enabled trigger. Blank pins read as unset.
    func testEnabledDefaultsAndBlankPins() {
        func parse(_ extra: String) -> ActionTrigger? {
            ActionTrigger.parseList(
                #"[{"id":"tr-1","deviceId":"dev-1","kind":"event","event":"created"\#(extra)}]"#
            ).first
        }
        XCTAssertEqual(parse("")?.enabled, true)
        XCTAssertEqual(parse(#","enabled":true"#)?.enabled, true)
        XCTAssertEqual(parse(#","enabled":false"#)?.enabled, false)
        XCTAssertEqual(parse(#","enabled":0"#)?.enabled, true)
        XCTAssertEqual(parse(#","enabled":"false""#)?.enabled, true)
        XCTAssertEqual(parse(#","enabled":null"#)?.enabled, true)
        let blank = parse(#","agent":"","account":null,"model":3"#)
        XCTAssertNil(blank?.agent)
        XCTAssertNil(blank?.account)
        XCTAssertNil(blank?.model)
        XCTAssertNil(blank?.effort)
    }

    // MARK: - Row glyphs

    private func stored(
        _ id: String, enabled: Bool = true, _ when: AutomationTrigger
    ) -> ActionTrigger {
        ActionTrigger(id: id, enabled: enabled, deviceId: "dev-1", when: when)
    }

    private let daily = AutomationTrigger.schedule(
        AutomationScheduleTrigger(interval: "daily", minuteOfDay: 540)
    )
    private let created = AutomationTrigger.event(AutomationEventTrigger(event: "created"))

    func testTriggerBadges() {
        XCTAssertEqual(TriggerBadges.of([]), TriggerBadges())
        XCTAssertTrue(TriggerBadges.of([]).isEmpty)
        // One kind only: the other glyph is not drawn at all.
        XCTAssertEqual(
            TriggerBadges.of([stored("a", daily)]),
            TriggerBadges(schedule: TriggerBadge(active: true))
        )
        // A kind is muted only when NONE of its triggers is enabled.
        XCTAssertEqual(
            TriggerBadges.of([
                stored("a", enabled: false, daily),
                stored("b", daily),
                stored("c", enabled: false, created),
            ]),
            TriggerBadges(
                schedule: TriggerBadge(active: true),
                event: TriggerBadge(active: false)
            )
        )
        // A suggestion's seed trigger draws its one glyph, active.
        XCTAssertEqual(
            TriggerBadges.of(suggested: created),
            TriggerBadges(event: TriggerBadge(active: true))
        )
        XCTAssertTrue(TriggerBadges.of(suggested: nil).isEmpty)
    }

    func testRowSentenceMarksSchedulesAsDeviceTime() {
        XCTAssertEqual(AutomationTriggerDisplay.rowSentence(daily), "Daily at 09:00 (device time)")
        XCTAssertEqual(AutomationTriggerDisplay.rowSentence(created), "When an issue is created")
    }

    // MARK: - Run titles

    // Byte-locked against the web's `actionRunTitle`: a run in its own
    // action's Runs is titled by what started it.
    func testActionRunTitleNamesWhatStartedTheRun() {
        XCTAssertEqual(ActionRunTitle.of(startedReason: "schedule"), "Scheduled run")
        XCTAssertEqual(ActionRunTitle.of(startedReason: "event"), "Event run")
        XCTAssertEqual(ActionRunTitle.of(startedReason: "agent"), "Agent run")
        XCTAssertEqual(ActionRunTitle.of(startedReason: "workflow"), "Agent run")
        XCTAssertEqual(ActionRunTitle.of(startedReason: "something-new"), "Agent run")
        XCTAssertEqual(ActionRunTitle.of(startedReason: nil), "Manual run")
        XCTAssertEqual(ActionRunTitle.of(startedReason: ""), "Manual run")
    }

    // MARK: - The `actions.update` triggers array

    private func json(_ value: some Encodable) throws -> Any {
        try JSONSerialization.jsonObject(with: JSONEncoder().encode(value))
    }

    // A NEW element carries no id (the server mints it), unset pins are
    // absent, and an event names its source.
    func testNewTriggerElementEncoding() throws {
        let schedule = try XCTUnwrap(try json(
            ActionTriggerInput(deviceId: "dev-1", when: daily)
        ) as? [String: Any])
        XCTAssertEqual(
            schedule.keys.sorted(),
            ["deviceId", "enabled", "interval", "kind", "minuteOfDay"]
        )
        XCTAssertEqual(schedule["enabled"] as? Bool, true)
        XCTAssertEqual(schedule["deviceId"] as? String, "dev-1")
        XCTAssertEqual(schedule["kind"] as? String, "schedule")
        XCTAssertEqual(schedule["minuteOfDay"] as? Int, 540)

        let event = try XCTUnwrap(try json(ActionTriggerInput(
            id: "tr-2",
            enabled: false,
            deviceId: "dev-2",
            agent: "claude",
            account: "p-1",
            model: "opus",
            effort: "high",
            when: .event(AutomationEventTrigger(
                event: "status_changed",
                filters: AutomationTriggerFilters(toStatusIds: ["s1"])
            ))
        )) as? [String: Any])
        XCTAssertEqual(
            event.keys.sorted(),
            ["account", "agent", "deviceId", "effort", "enabled", "event", "filters",
             "id", "kind", "model", "source"]
        )
        XCTAssertEqual(event["id"] as? String, "tr-2")
        XCTAssertEqual(event["enabled"] as? Bool, false)
        XCTAssertEqual(event["source"] as? String, "exponential")
        XCTAssertEqual((event["filters"] as? [String: Any])?["toStatusIds"] as? [String], ["s1"])
    }

    // What is written reads back as the same trigger.
    func testEncodedElementRoundTripsThroughTheParser() throws {
        let trigger = ActionTrigger(
            id: "tr-1",
            enabled: false,
            deviceId: "dev-1",
            agent: "codex",
            model: "gpt-5.6-sol",
            when: created
        )
        let data = try JSONEncoder().encode([ActionTriggerInput(trigger)])
        XCTAssertEqual(ActionTrigger.parseList(String(data: data, encoding: .utf8)), [trigger])
    }

    // The write is a WHOLE-ARRAY replace: every helper sends all the other
    // triggers back untouched, ids kept.
    func testWholeArrayWrites() {
        let existing = [stored("a", daily), stored("b", created)]
        let draft = ActionTriggerInput(id: "ignored", deviceId: "dev-9", when: created)

        let added = existing.adding(draft)
        XCTAssertEqual(added.map(\.id), ["a", "b", nil])
        XCTAssertEqual(added.last?.deviceId, "dev-9")

        let replaced = existing.replacing(id: "b", with: draft)
        XCTAssertEqual(replaced.map(\.id), ["a", "b"])
        XCTAssertEqual(replaced.map(\.deviceId), ["dev-1", "dev-9"])

        let toggled = existing.settingEnabled(id: "a", false)
        XCTAssertEqual(toggled.map(\.id), ["a", "b"])
        XCTAssertEqual(toggled.map(\.enabled), [false, true])

        XCTAssertEqual(existing.removing(id: "a").map(\.id), ["b"])
    }

    // MARK: - The creator-run trigger block

    // Byte-locked against the web's `formatTriggerBlock`
    // (apps/web/src/lib/action-triggers.ts): the when-part's keys in their
    // stored order, then deviceId, then agent/model/effort only when set;
    // compact JSON inside a one-element array.
    func testTriggerNoteMatchesTheWebBlock() {
        let spec = TriggerSpec(
            trigger: .schedule(AutomationScheduleTrigger(interval: "daily", minuteOfDay: 540)),
            deviceId: "d-1"
        )
        XCTAssertEqual(
            TriggerNote.format(spec),
            "\n\nTrigger — after creating the action, call exponential_actions_update with its id and `triggers` set to exactly this array: `[{\"kind\":\"schedule\",\"interval\":\"daily\",\"minuteOfDay\":540,\"deviceId\":\"d-1\"}]`. A triggered run fills no inputs, so declare none as required."
        )
    }

    func testTriggerNoteCarriesTheLaunchFieldsInOrder() {
        let spec = TriggerSpec(
            trigger: .event(AutomationEventTrigger(event: "created")),
            deviceId: "dev-2",
            agent: "codex",
            model: "gpt-5.6-sol",
            effort: "high"
        )
        XCTAssertEqual(
            TriggerNote.format(spec),
            "\n\nTrigger — after creating the action, call exponential_actions_update with its id and `triggers` set to exactly this array: `[{\"kind\":\"event\",\"event\":\"created\",\"deviceId\":\"dev-2\",\"agent\":\"codex\",\"model\":\"gpt-5.6-sol\",\"effort\":\"high\"}]`. A triggered run fills no inputs, so declare none as required."
        )
        // Blank launch fields are omitted (the web's falsy check).
        let blank = TriggerSpec(
            trigger: .event(AutomationEventTrigger(event: "created")),
            deviceId: "dev-2",
            agent: "",
            model: nil,
            effort: ""
        )
        XCTAssertFalse(TriggerNote.format(blank).contains("agent"))
        XCTAssertFalse(TriggerNote.format(blank).contains("effort"))
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

    func testActionDtoParsesTheEntityTriggers() {
        let entity = ActionEntity(
            id: "a1",
            teamId: "t1",
            repositoryId: nil,
            name: "Digest",
            description: nil,
            icon: nil,
            inputs: nil,
            triggers: #"[{"id":"tr-1","enabled":false,"deviceId":"dev-1","agent":"claude","kind":"schedule","interval":"daily","minuteOfDay":420},{"id":"broken"}]"#,
            sortOrder: 3,
            createdAt: "2026-01-01T00:00:00Z",
            updatedAt: "2026-01-01T00:00:00Z"
        )
        let dto = ActionDto(entity: entity)
        XCTAssertEqual(dto.triggers.map(\.id), ["tr-1"])
        XCTAssertEqual(dto.triggers.first?.deviceId, "dev-1")
        XCTAssertEqual(dto.triggers.first?.enabled, false)
        XCTAssertEqual(dto.triggers.first?.agent, "claude")
        guard case .schedule? = dto.triggers.first?.when else {
            return XCTFail("entity triggers must reach the DTO")
        }
    }

    // tRPC hands `triggers` back as a JSON array; an older server omits it.
    func testActionDtoDecodesTriggersOffTheWire() throws {
        func decode(_ triggers: String) throws -> ActionDto {
            let raw = #"""
            {"id":"a1","teamId":"t1","repositoryId":null,"name":"Digest","description":null,
             "body":"Do it","sortOrder":0,"createdAt":"2026-01-01T00:00:00Z",
             "updatedAt":"2026-01-01T00:00:00Z"\#(triggers)}
            """#
            return try JSONDecoder().decode(ActionDto.self, from: Data(raw.utf8))
        }
        XCTAssertEqual(try decode("").triggers, [])
        XCTAssertEqual(try decode(#","triggers":null"#).triggers, [])
        let decoded = try decode(
            #","triggers":[{"id":"tr-1","deviceId":"dev-1","kind":"event","source":"exponential","event":"created"},{"kind":"event"}]"#
        )
        XCTAssertEqual(decoded.triggers, [
            ActionTrigger(id: "tr-1", deviceId: "dev-1", when: created),
        ])
        XCTAssertEqual(decoded.body, "Do it")
    }

    // The busy flag clears when tRPC answers, the synced row echoes later: a
    // write in that gap builds on the mutation's answer, never on the stale row.
    func testWriteBaseHoldsTheMutationAnswerUntilTheRowEchoes() {
        func action(id: String = "a1", updatedAt: String, enabled: Bool) -> ActionDto {
            ActionDto(
                id: id, teamId: "t1", repositoryId: nil, name: "Digest", description: nil,
                body: "", sortOrder: 0, createdAt: "2026-01-01T00:00:00Z", updatedAt: updatedAt,
                triggers: [
                    ActionTrigger(id: "tr-1", enabled: enabled, deviceId: "dev-1", when: created),
                    ActionTrigger(id: "tr-2", enabled: true, deviceId: "dev-1", when: created),
                ]
            )
        }
        let stale = action(updatedAt: "2026-10-01 10:00:00.000000+00", enabled: true)
        let written = action(updatedAt: "2026-10-01T10:00:05.120Z", enabled: false)

        // No write yet: the synced row.
        XCTAssertEqual(TriggerWriteBase.triggers(synced: stale, written: nil), stale.triggers)
        // In the gap the answer wins, so a second toggle keeps the first.
        let base = TriggerWriteBase.triggers(synced: stale, written: written)
        XCTAssertEqual(base, written.triggers)
        XCTAssertEqual(
            base.settingEnabled(id: "tr-2", false).map(\.enabled), [false, false]
        )
        // The echo (Postgres text, microseconds) hands the row back the lead…
        let echoed = action(updatedAt: "2026-10-01 10:00:05.120456+00", enabled: false)
        XCTAssertEqual(TriggerWriteBase.triggers(synced: echoed, written: written), echoed.triggers)
        // …and so does a newer write from elsewhere.
        let newer = action(updatedAt: "2026-10-01 10:00:09+00", enabled: true)
        XCTAssertEqual(TriggerWriteBase.triggers(synced: newer, written: written), newer.triggers)
        // Another action's answer or an unreadable stamp never overrides the row.
        XCTAssertEqual(
            TriggerWriteBase.triggers(
                synced: stale, written: action(id: "a2", updatedAt: "2026-10-01T10:00:05Z", enabled: false)
            ),
            stale.triggers
        )
        XCTAssertEqual(
            TriggerWriteBase.triggers(synced: stale, written: action(updatedAt: "soon", enabled: false)),
            stale.triggers
        )
    }
}
