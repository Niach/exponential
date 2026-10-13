import Foundation

// The WHEN-part of a trigger (EXP-530; EXP-1291: a team agent carries its
// triggers — `AgentTrigger` is the stored element, this is the schedule
// ("daily at 07:00") or issue event ("when status changes") inside it). The
// agent's device watches for it and fires locally; there is no server
// scheduler. Parsing is deliberately tolerant — an unknown kind/event/source
// or malformed JSON reads as "no trigger", never a crash — so a future
// trigger shape can't brick this client.

/// The event-trigger filter lists. Empty/absent lists mean "no filter on that
/// axis"; `priorities` carries wire priority values, the id lists uuids.
public struct TriggerFilters: Sendable, Equatable {
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
public struct ScheduleTrigger: Sendable, Equatable {
    /// Contract `agentScheduleIntervalValues`: daily | weekly | monthly.
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
public struct EventTrigger: Sendable, Equatable {
    /// Contract `agentTriggerEventValues` (created, status_changed, …).
    public let event: String
    public let filters: TriggerFilters

    public init(
        event: String,
        filters: TriggerFilters = TriggerFilters()
    ) {
        self.event = event
        self.filters = filters
    }
}

public enum TriggerWhen: Sendable, Equatable {
    case schedule(ScheduleTrigger)
    case event(EventTrigger)

    // MARK: - Tolerant parse

    /// Parse the stored/synced JSON string. ANY malformation — unknown kind,
    /// unknown event/interval, missing schedule fields, non-object JSON —
    /// reads as nil ("no trigger"): a future server shape must never crash or
    /// half-render on this build.
    public static func parse(_ raw: String?) -> TriggerWhen? {
        guard let raw, !raw.isEmpty,
              let data = raw.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data),
              let object = json as? [String: Any]
        else { return nil }
        return parse(object: object)
    }

    static func parse(object: [String: Any]) -> TriggerWhen? {
        guard let kind = object["kind"] as? String else { return nil }

        switch kind {
        case "schedule":
            guard let interval = object["interval"] as? String,
                  DomainContract.agentScheduleIntervalValues.contains(interval),
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
            return .schedule(ScheduleTrigger(
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
                  DomainContract.agentTriggerEventValues.contains(event)
            else { return nil }
            let rawFilters = object["filters"] as? [String: Any] ?? [:]
            let filters = TriggerFilters(
                boardIds: idList(rawFilters["boardIds"]),
                labelIds: idList(rawFilters["labelIds"]),
                priorities: priorityList(rawFilters["priorities"]),
                toStatusIds: idList(rawFilters["toStatusIds"])
            )
            return .event(EventTrigger(event: event, filters: filters))
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
}

// Encodable so an agent's `triggers` element (`AgentTriggerInput`) embeds the
// when-part's keys beside its id and enabled flag.
extension TriggerWhen: Encodable {
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

public enum TriggerDisplay {
    /// The event PICKER label (the Schedule row's sentence is
    /// `AgentSettings.triggerSummary`).
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
}
