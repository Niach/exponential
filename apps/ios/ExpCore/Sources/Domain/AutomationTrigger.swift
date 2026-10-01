import Foundation

// The WHEN-part of a trigger (EXP-530; SLOP-2: an action carries its
// triggers — `ActionTrigger` is the stored element, this is the schedule
// ("daily at 07:00") or issue event ("when status changes") inside it). The
// bound device watches for it and fires locally; there is no server scheduler.
// Parsing is deliberately tolerant — an unknown kind/event/source or
// malformed JSON reads as "no trigger", never a crash — so a future trigger
// shape can't brick this client. Mirrors the web's `lib/action-triggers.ts`
// byte-for-byte.

/// The event-trigger filter lists. Empty/absent lists mean "no filter on that
/// axis"; `priorities` carries wire priority values, the id lists uuids.
public struct AutomationTriggerFilters: Sendable, Equatable {
    public let boardIds: [String]
    public let labelIds: [String]
    public let priorities: [String]
    public let toStatusIds: [String]

    public init(
        boardIds: [String] = [],
        labelIds: [String] = [],
        priorities: [String] = [],
        toStatusIds: [String] = []
    ) {
        self.boardIds = boardIds
        self.labelIds = labelIds
        self.priorities = priorities
        self.toStatusIds = toStatusIds
    }

    /// Total picked entries across every list — the ` · N filters` count.
    public var totalCount: Int {
        boardIds.count + labelIds.count + priorities.count + toStatusIds.count
    }

    public var isEmpty: Bool { totalCount == 0 }
}

/// `kind: "schedule"` — fires on the bound device's LOCAL clock.
public struct AutomationScheduleTrigger: Sendable, Equatable {
    /// Contract `actionScheduleIntervalValues`: daily | weekly | monthly.
    public let interval: String
    /// Minutes past local midnight, 0..<1440.
    public let minuteOfDay: Int
    /// 1 = Monday … 7 = Sunday; present iff weekly.
    public let weekday: Int?
    /// 1…28; present iff monthly.
    public let dayOfMonth: Int?

    public init(
        interval: String,
        minuteOfDay: Int,
        weekday: Int? = nil,
        dayOfMonth: Int? = nil
    ) {
        self.interval = interval
        self.minuteOfDay = minuteOfDay
        self.weekday = weekday
        self.dayOfMonth = dayOfMonth
    }
}

/// `kind: "event"` — fires when a matching issue event syncs to the device.
public struct AutomationEventTrigger: Sendable, Equatable {
    /// Contract `actionTriggerEventValues` (created, status_changed, …).
    public let event: String
    public let filters: AutomationTriggerFilters

    public init(
        event: String,
        filters: AutomationTriggerFilters = AutomationTriggerFilters()
    ) {
        self.event = event
        self.filters = filters
    }
}

public enum AutomationTrigger: Sendable, Equatable {
    case schedule(AutomationScheduleTrigger)
    case event(AutomationEventTrigger)

    // MARK: - Tolerant parse

    /// Parse the stored/synced JSON string. ANY malformation — unknown kind,
    /// unknown event/interval, missing schedule fields, non-object JSON —
    /// reads as nil ("no trigger"): a future server shape must never crash or
    /// half-render on this build.
    public static func parse(_ raw: String?) -> AutomationTrigger? {
        guard let raw, !raw.isEmpty,
              let data = raw.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data),
              let object = json as? [String: Any]
        else { return nil }
        return parse(object: object)
    }

    static func parse(object: [String: Any]) -> AutomationTrigger? {
        guard let kind = object["kind"] as? String else { return nil }

        switch kind {
        case "schedule":
            guard let interval = object["interval"] as? String,
                  DomainContract.actionScheduleIntervalValues.contains(interval),
                  let minuteOfDay = intValue(object["minuteOfDay"]),
                  (0..<1440).contains(minuteOfDay)
            else { return nil }
            let weekday = intValue(object["weekday"])
            let dayOfMonth = intValue(object["dayOfMonth"])
            if interval == "weekly" {
                guard let weekday, (1...7).contains(weekday) else { return nil }
            }
            if interval == "monthly" {
                guard let dayOfMonth, (1...28).contains(dayOfMonth) else { return nil }
            }
            return .schedule(AutomationScheduleTrigger(
                interval: interval,
                minuteOfDay: minuteOfDay,
                weekday: interval == "weekly" ? weekday : nil,
                dayOfMonth: interval == "monthly" ? dayOfMonth : nil
            ))
        case "event":
            // A FUTURE event source reads as "never fires" on this build. An
            // ABSENT source is Exponential's (rows from before sources
            // existed); a present one must say so.
            if let source = object["source"] {
                guard (source as? String) == "exponential" else { return nil }
            }
            guard let event = object["event"] as? String,
                  DomainContract.actionTriggerEventValues.contains(event)
            else { return nil }
            let rawFilters = object["filters"] as? [String: Any] ?? [:]
            let filters = AutomationTriggerFilters(
                boardIds: idList(rawFilters["boardIds"]),
                labelIds: idList(rawFilters["labelIds"]),
                priorities: priorityList(rawFilters["priorities"]),
                toStatusIds: idList(rawFilters["toStatusIds"])
            )
            return .event(AutomationEventTrigger(event: event, filters: filters))
        default:
            return nil
        }
    }

    /// Postgres jsonb numbers may decode as Int, Double or NSNumber.
    private static func intValue(_ value: Any?) -> Int? {
        switch value {
        case let int as Int: int
        case let double as Double: double == double.rounded() ? Int(double) : nil
        default: nil
        }
    }

    /// Id lists drop empty entries (web `idList` parity — the filter count
    /// must agree across clients).
    private static func idList(_ value: Any?) -> [String] {
        (value as? [Any])?.compactMap { $0 as? String }.filter { !$0.isEmpty } ?? []
    }

    /// Priorities additionally validate against the contract vocabulary
    /// (web `priorityList` parity — an unknown value drops, never counts).
    private static func priorityList(_ value: Any?) -> [String] {
        idList(value).filter { DomainContract.issuePriorityValues.contains($0) }
    }

    // MARK: - Wire encoding

    /// The when-part as a JSON object with the server's field names (kind, then
    /// interval/minuteOfDay/weekday/dayOfMonth or event/filters). Empty filter
    /// lists are OMITTED, matching what the pickers produce.
    public var wireObject: [String: Any] {
        switch self {
        case let .schedule(s):
            var out: [String: Any] = [
                "kind": "schedule",
                "interval": s.interval,
                "minuteOfDay": s.minuteOfDay,
            ]
            if let weekday = s.weekday { out["weekday"] = weekday }
            if let dayOfMonth = s.dayOfMonth { out["dayOfMonth"] = dayOfMonth }
            return out
        case let .event(e):
            var out: [String: Any] = [
                "kind": "event",
                "event": e.event,
            ]
            var filters: [String: Any] = [:]
            if !e.filters.boardIds.isEmpty { filters["boardIds"] = e.filters.boardIds }
            if !e.filters.labelIds.isEmpty { filters["labelIds"] = e.filters.labelIds }
            if !e.filters.priorities.isEmpty { filters["priorities"] = e.filters.priorities }
            if !e.filters.toStatusIds.isEmpty { filters["toStatusIds"] = e.filters.toStatusIds }
            if !filters.isEmpty { out["filters"] = filters }
            return out
        }
    }

    /// Compact JSON string in the CANONICAL key order every client emits
    /// (web `JSON.stringify` insertion order, Android `toWireJsonString`,
    /// desktop's preserve_order serde_json) — the machine-readable trigger
    /// block must be byte-identical across the four clients, so this is
    /// hand-composed rather than serialized (JSONSerialization only offers
    /// alphabetical order).
    public var wireJSONString: String {
        "{" + wireJSONParts.joined(separator: ",") + "}"
    }

    /// `wireJSONString`'s `"key":value` members, in order — the trigger block
    /// appends the runner's keys after them, inside the same object.
    var wireJSONParts: [String] {
        func list(_ values: [String]) -> String {
            "[" + values.map(Self.jsonQuoted).joined(separator: ",") + "]"
        }
        switch self {
        case let .schedule(s):
            var parts = [
                "\"kind\":\"schedule\"",
                "\"interval\":\(Self.jsonQuoted(s.interval))",
                "\"minuteOfDay\":\(s.minuteOfDay)",
            ]
            if let weekday = s.weekday { parts.append("\"weekday\":\(weekday)") }
            if let dayOfMonth = s.dayOfMonth { parts.append("\"dayOfMonth\":\(dayOfMonth)") }
            return parts
        case let .event(e):
            var parts = [
                "\"kind\":\"event\"",
                "\"event\":\(Self.jsonQuoted(e.event))",
            ]
            var filters: [String] = []
            if !e.filters.boardIds.isEmpty {
                filters.append("\"boardIds\":\(list(e.filters.boardIds))")
            }
            if !e.filters.labelIds.isEmpty {
                filters.append("\"labelIds\":\(list(e.filters.labelIds))")
            }
            if !e.filters.priorities.isEmpty {
                filters.append("\"priorities\":\(list(e.filters.priorities))")
            }
            if !e.filters.toStatusIds.isEmpty {
                filters.append("\"toStatusIds\":\(list(e.filters.toStatusIds))")
            }
            if !filters.isEmpty {
                parts.append("\"filters\":{" + filters.joined(separator: ",") + "}")
            }
            return parts
        }
    }

    /// One JSON-escaped, quoted string (delegates the escaping rules to
    /// JSONSerialization via a single-element array).
    static func jsonQuoted(_ value: String) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: [value]),
              let text = String(data: data, encoding: .utf8),
              text.count >= 2
        else { return "\"\"" }
        return String(text.dropFirst().dropLast())
    }
}

// Encodable so an `actions.update { triggers }` element (`ActionTriggerInput`)
// embeds the when-part's keys beside its runner's.
extension AutomationTrigger: Encodable {
    private enum WireKeys: String, CodingKey {
        case kind, interval, minuteOfDay, weekday, dayOfMonth
        case event, filters
    }

    private enum FilterKeys: String, CodingKey {
        case boardIds, labelIds, priorities, toStatusIds
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: WireKeys.self)
        switch self {
        case let .schedule(s):
            try c.encode("schedule", forKey: .kind)
            try c.encode(s.interval, forKey: .interval)
            try c.encode(s.minuteOfDay, forKey: .minuteOfDay)
            try c.encodeIfPresent(s.weekday, forKey: .weekday)
            try c.encodeIfPresent(s.dayOfMonth, forKey: .dayOfMonth)
        case let .event(e):
            try c.encode("event", forKey: .kind)
            try c.encode(e.event, forKey: .event)
            if !e.filters.isEmpty {
                var f = c.nestedContainer(keyedBy: FilterKeys.self, forKey: .filters)
                if !e.filters.boardIds.isEmpty { try f.encode(e.filters.boardIds, forKey: .boardIds) }
                if !e.filters.labelIds.isEmpty { try f.encode(e.filters.labelIds, forKey: .labelIds) }
                if !e.filters.priorities.isEmpty {
                    try f.encode(e.filters.priorities, forKey: .priorities)
                }
                if !e.filters.toStatusIds.isEmpty {
                    try f.encode(e.filters.toStatusIds, forKey: .toStatusIds)
                }
            }
        }
    }
}

// MARK: - Display

public enum AutomationTriggerDisplay {
    /// 1 = Monday … 7 = Sunday (the wire convention, NOT Calendar's Sun-first).
    public static let weekdayNames = [
        "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
    ]

    /// `07:00` — zero-padded 24h clock from minutes past midnight.
    public static func clock(_ minuteOfDay: Int) -> String {
        String(format: "%02d:%02d", minuteOfDay / 60, minuteOfDay % 60)
    }

    /// The one-line trigger sentence, byte-matching the web's
    /// `triggerSummary`: `Daily at 07:00` / `Weekly on Monday at 09:00` /
    /// `Monthly on day 5 at 09:00`; `When status changes` (+ ` · N filters`).
    public static func summary(_ trigger: AutomationTrigger) -> String {
        switch trigger {
        case let .schedule(s):
            let time = clock(s.minuteOfDay)
            switch s.interval {
            case "weekly":
                let day = weekdayNames[((s.weekday ?? 1) - 1 + 7) % 7]
                return "Weekly on \(day) at \(time)"
            case "monthly":
                return "Monthly on day \(s.dayOfMonth ?? 1) at \(time)"
            default:
                return "Daily at \(time)"
            }
        case let .event(e):
            let base = switch e.event {
            case "created": "When an issue is created"
            case "status_changed": "When status changes"
            case "assignee_changed": "When the assignee changes"
            case "label_added": "When a label is added"
            case "priority_changed": "When priority changes"
            case "pr_opened": "When a pull request is opened"
            case "pr_merged": "When a pull request is merged"
            default: "When an issue changes"
            }
            let count = e.filters.totalCount
            guard count > 0 else { return base }
            return "\(base) · \(count) \(count == 1 ? "filter" : "filters")"
        }
    }

    /// The sentence as a Triggers row prints it. A schedule fires on the
    /// BOUND MACHINE's wall clock, so the recurrence carries the caveat the
    /// row used to hang off an absolute next-run date (EXP-812).
    public static func rowSentence(_ trigger: AutomationTrigger) -> String {
        trigger.isSchedule ? "\(summary(trigger)) (device time)" : summary(trigger)
    }

    /// The event PICKER label (web `TRIGGER_EVENT_LABELS`) — `summary` above
    /// derives its "When …" sentence from the same vocabulary, so the two
    /// surfaces can never disagree.
    public static func eventLabel(_ value: String) -> String {
        switch value {
        case "created": "An issue is created"
        case "status_changed": "Status changes"
        case "assignee_changed": "The assignee changes"
        case "label_added": "A label is added"
        case "priority_changed": "Priority changes"
        case "pr_opened": "A pull request is opened"
        case "pr_merged": "A pull request is merged"
        default: value.replacingOccurrences(of: "_", with: " ")
        }
    }

    /// The next occurrence STRICTLY AFTER `after`, computed in the given
    /// calendar's timezone. This is the DEVICE-VIEWER's local wall clock —
    /// the bound device fires on ITS OWN local time, which is why no surface
    /// prints this as an absolute date any more (EXP-812: the calendar moved
    /// it under every screenshot). A Triggers row labels the RECURRENCE
    /// "(device time)" instead. Nil for event triggers has no meaning here;
    /// pass a schedule.
    public static func nextScheduleRun(
        _ schedule: AutomationScheduleTrigger,
        after: Date,
        calendar: Calendar = .current
    ) -> Date? {
        var components = DateComponents()
        components.hour = schedule.minuteOfDay / 60
        components.minute = schedule.minuteOfDay % 60
        switch schedule.interval {
        case "weekly":
            guard let weekday = schedule.weekday, (1...7).contains(weekday) else { return nil }
            // Wire 1=Mon…7=Sun → Calendar 1=Sun…7=Sat.
            components.weekday = weekday % 7 + 1
        case "monthly":
            guard let day = schedule.dayOfMonth, (1...28).contains(day) else { return nil }
            components.day = day
        case "daily":
            break
        default:
            return nil
        }
        return calendar.nextDate(
            after: after,
            matching: components,
            matchingPolicy: .nextTime
        )
    }
}
