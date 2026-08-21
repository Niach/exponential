import Foundation

// EXP-583 automations: an automation is its OWN row (`automations`, the 19th
// Electric shape) binding ONE action to ONE device with a schedule ("daily at
// 07:00") or an issue event ("when status changes") the bound device watches
// for and fires locally (there is no server scheduler). The trigger jsonb is
// the WHEN-PART ONLY — deviceId/enabled/agent/model/effort are columns on the
// row, not fields of the trigger (they were, until EXP-530 split apart here).
// Parsing is deliberately tolerant — an unknown kind/event or malformed JSON
// reads as "no trigger", never a crash — so a future trigger shape can't brick
// this client. Mirrors the web's `lib/action-triggers.ts` byte-for-byte.

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

    /// The wire JSON object with the server's field names (kind, then
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
    /// desktop's preserve_order serde_json) — the machine-readable automation
    /// note must be byte-identical across the four clients, so this is
    /// hand-composed rather than serialized (JSONSerialization only offers
    /// alphabetical order).
    public var wireJSONString: String {
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
            return "{" + parts.joined(separator: ",") + "}"
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
            return "{" + parts.joined(separator: ",") + "}"
        }
    }

    /// One JSON-escaped, quoted string (delegates the escaping rules to
    /// JSONSerialization via a single-element array).
    private static func jsonQuoted(_ value: String) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: [value]),
              let text = String(data: data, encoding: .utf8),
              text.count >= 2
        else { return "\"\"" }
        return String(text.dropFirst().dropLast())
    }
}

// Encodable so an `automations.create/update { trigger }` input embeds it as
// the full JSON OBJECT (the server replaces the whole value, never merges).
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

// MARK: - The creator-run automation note

/// What an "Action + automation" suggestion asks the creator agent to set up
/// alongside the new action — the iOS twin of the web's `AutomationSpec`.
public struct AutomationSpec: Sendable, Equatable {
    public let trigger: AutomationTrigger
    public let deviceId: String
    public let agent: String?
    public let model: String?
    public let effort: String?

    public init(
        trigger: AutomationTrigger,
        deviceId: String,
        agent: String? = nil,
        model: String? = nil,
        effort: String? = nil
    ) {
        self.trigger = trigger
        self.deviceId = deviceId
        self.agent = agent
        self.model = model
        self.effort = effort
    }
}

public enum AutomationNote {
    /// The machine-readable block the create sheet appends to the builtin
    /// "Create action" description: the creator agent creates the action,
    /// then copies this JSON verbatim into `exponential_automations_create`
    /// (adding the new action's id). BYTE-IDENTICAL to the web's
    /// `formatAutomationBlock` — key order deviceId, trigger, then agent,
    /// model, effort only when set; compact JSON, no spaces.
    public static func format(_ spec: AutomationSpec) -> String {
        var parts = [
            "\"deviceId\":\(jsonQuoted(spec.deviceId))",
            "\"trigger\":\(spec.trigger.wireJSONString)",
        ]
        if let agent = spec.agent, !agent.isEmpty {
            parts.append("\"agent\":\(jsonQuoted(agent))")
        }
        if let model = spec.model, !model.isEmpty {
            parts.append("\"model\":\(jsonQuoted(model))")
        }
        if let effort = spec.effort, !effort.isEmpty {
            parts.append("\"effort\":\(jsonQuoted(effort))")
        }
        let payload = "{" + parts.joined(separator: ",") + "}"
        return "\n\nAutomation — after creating the action, call exponential_automations_create with its id and exactly these fields: `\(payload)`. An automated run fills no inputs, so declare none as required."
    }

    private static func jsonQuoted(_ value: String) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: [value]),
              let text = String(data: data, encoding: .utf8),
              text.count >= 2
        else { return "\"\"" }
        return String(text.dropFirst().dropLast())
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
    /// the bound device fires on ITS OWN local time, so callers must label
    /// the result "(device time)". Nil for event triggers has no meaning
    /// here; pass a schedule.
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
