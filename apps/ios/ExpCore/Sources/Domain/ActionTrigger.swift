import Foundation

// EXP-530 action automations: ONE optional `trigger` (jsonb, synced on the
// actions shape) per action — a schedule ("daily at 07:00") or an event
// ("when status changes") the BOUND DEVICE watches for and fires locally
// (there is no server scheduler). Mobile only ever reads and toggles it:
// parsing is deliberately tolerant — an unknown kind/event or malformed JSON
// reads as "no automation", never a crash — so a future trigger shape can't
// brick this client. Mirrors the web's `lib/action-triggers.ts` summary
// strings byte-for-byte.

/// The event-trigger filter lists. Empty/absent lists mean "no filter on that
/// axis"; `priorities` carries wire priority values, the id lists uuids.
public struct ActionTriggerFilters: Sendable, Equatable {
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
public struct ActionScheduleTrigger: Sendable, Equatable {
    public let deviceId: String
    /// Paused automations keep their config; a missing wire flag reads true.
    public var enabled: Bool
    /// Contract `actionScheduleIntervalValues`: daily | weekly | monthly.
    public let interval: String
    /// Minutes past local midnight, 0..<1440.
    public let minuteOfDay: Int
    /// 1 = Monday … 7 = Sunday; present iff weekly.
    public let weekday: Int?
    /// 1…28; present iff monthly.
    public let dayOfMonth: Int?

    public init(
        deviceId: String,
        enabled: Bool = true,
        interval: String,
        minuteOfDay: Int,
        weekday: Int? = nil,
        dayOfMonth: Int? = nil
    ) {
        self.deviceId = deviceId
        self.enabled = enabled
        self.interval = interval
        self.minuteOfDay = minuteOfDay
        self.weekday = weekday
        self.dayOfMonth = dayOfMonth
    }
}

/// `kind: "event"` — fires when a matching issue event syncs to the device.
public struct ActionEventTrigger: Sendable, Equatable {
    public let deviceId: String
    public var enabled: Bool
    /// Contract `actionTriggerEventValues` (created, status_changed, …).
    public let event: String
    public let filters: ActionTriggerFilters

    public init(
        deviceId: String,
        enabled: Bool = true,
        event: String,
        filters: ActionTriggerFilters = ActionTriggerFilters()
    ) {
        self.deviceId = deviceId
        self.enabled = enabled
        self.event = event
        self.filters = filters
    }
}

public enum ActionTrigger: Sendable, Equatable {
    case schedule(ActionScheduleTrigger)
    case event(ActionEventTrigger)

    public var deviceId: String {
        switch self {
        case let .schedule(s): s.deviceId
        case let .event(e): e.deviceId
        }
    }

    public var enabled: Bool {
        switch self {
        case let .schedule(s): s.enabled
        case let .event(e): e.enabled
        }
    }

    /// The same trigger with the paused flag flipped — the Automations
    /// toggle's payload (`actions.update` replaces the whole object).
    public func withEnabled(_ enabled: Bool) -> ActionTrigger {
        switch self {
        case var .schedule(s):
            s.enabled = enabled
            return .schedule(s)
        case var .event(e):
            e.enabled = enabled
            return .event(e)
        }
    }

    // MARK: - Tolerant parse

    /// Parse the stored/synced JSON string. ANY malformation — unknown kind,
    /// unknown event/interval, missing device or schedule fields, non-object
    /// JSON — reads as nil ("no automation"): a future server shape must
    /// never crash or half-render on this build.
    public static func parse(_ raw: String?) -> ActionTrigger? {
        guard let raw, !raw.isEmpty,
              let data = raw.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data),
              let object = json as? [String: Any]
        else { return nil }
        return parse(object: object)
    }

    static func parse(object: [String: Any]) -> ActionTrigger? {
        guard let kind = object["kind"] as? String,
              let deviceId = object["deviceId"] as? String, !deviceId.isEmpty
        else { return nil }
        // Missing `enabled` defaults true (the wire may omit it).
        let enabled = (object["enabled"] as? Bool) ?? true

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
            return .schedule(ActionScheduleTrigger(
                deviceId: deviceId,
                enabled: enabled,
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
            let filters = ActionTriggerFilters(
                boardIds: stringList(rawFilters["boardIds"]),
                labelIds: stringList(rawFilters["labelIds"]),
                priorities: stringList(rawFilters["priorities"]),
                toStatusIds: stringList(rawFilters["toStatusIds"])
            )
            return .event(ActionEventTrigger(
                deviceId: deviceId,
                enabled: enabled,
                event: event,
                filters: filters
            ))
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

    private static func stringList(_ value: Any?) -> [String] {
        (value as? [Any])?.compactMap { $0 as? String } ?? []
    }

    // MARK: - Wire encoding

    /// The wire JSON object with the server's field names (kind, deviceId,
    /// enabled, interval/minuteOfDay/weekday/dayOfMonth or event/filters).
    /// Empty filter lists are OMITTED, matching what the pickers produce.
    public var wireObject: [String: Any] {
        switch self {
        case let .schedule(s):
            var out: [String: Any] = [
                "kind": "schedule",
                "deviceId": s.deviceId,
                "enabled": s.enabled,
                "interval": s.interval,
                "minuteOfDay": s.minuteOfDay,
            ]
            if let weekday = s.weekday { out["weekday"] = weekday }
            if let dayOfMonth = s.dayOfMonth { out["dayOfMonth"] = dayOfMonth }
            return out
        case let .event(e):
            var out: [String: Any] = [
                "kind": "event",
                "deviceId": e.deviceId,
                "enabled": e.enabled,
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

    /// Compact key-sorted JSON string of `wireObject` — rides the create
    /// sheet's machine-readable description block.
    public var wireJSONString: String {
        guard let data = try? JSONSerialization.data(
            withJSONObject: wireObject,
            options: [.sortedKeys]
        ) else { return "{}" }
        return String(data: data, encoding: .utf8) ?? "{}"
    }
}

// Encodable so an `actions.update { id, trigger }` input can embed it as the
// full JSON OBJECT (the server replaces the whole value, never merges).
extension ActionTrigger: Encodable {
    private enum WireKeys: String, CodingKey {
        case kind, deviceId, enabled, interval, minuteOfDay, weekday, dayOfMonth
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
            try c.encode(s.deviceId, forKey: .deviceId)
            try c.encode(s.enabled, forKey: .enabled)
            try c.encode(s.interval, forKey: .interval)
            try c.encode(s.minuteOfDay, forKey: .minuteOfDay)
            try c.encodeIfPresent(s.weekday, forKey: .weekday)
            try c.encodeIfPresent(s.dayOfMonth, forKey: .dayOfMonth)
        case let .event(e):
            try c.encode("event", forKey: .kind)
            try c.encode(e.deviceId, forKey: .deviceId)
            try c.encode(e.enabled, forKey: .enabled)
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

public enum ActionTriggerDisplay {
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
    public static func summary(_ trigger: ActionTrigger) -> String {
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

    /// The next occurrence STRICTLY AFTER `after`, computed in the given
    /// calendar's timezone. This is the DEVICE-VIEWER's local wall clock —
    /// the bound device fires on ITS OWN local time, so callers must label
    /// the result "(device time)". Nil for event triggers has no meaning
    /// here; pass a schedule.
    public static func nextScheduleRun(
        _ schedule: ActionScheduleTrigger,
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
