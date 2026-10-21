import Foundation

// The WHEN-part of an agent trigger (NEW-2: `agents.triggers`, contract
// `agentTrigger*`; `AgentTrigger` is the stored element, this is the part
// inside it). The SERVER fires them (`lib/agents/triggers.ts`); nothing on a
// device or a phone ever does. Two kinds:
//   schedule: a 5-field cron with minutes (`m h dom mon dow`) in an IANA
//             `tz` (absent = UTC);
//   event:    the NAME of an `events` row (a builtin, contract
//             `eventBuiltinNames`, or any custom name).
// Parsing is deliberately tolerant — an unknown kind or a missing field reads
// as "no trigger", never a crash — so a future trigger shape can't brick
// this client. Writes are validated by the server.

/// `kind: "schedule"` — a cron expression the server evaluates in `tz`.
public struct ScheduleTrigger: Sendable, Equatable {
    /// Five fields: minute hour day-of-month month day-of-week.
    public let cron: String
    /// The IANA zone the cron reads in; nil = UTC.
    public let tz: String?

    public init(cron: String, tz: String? = nil) {
        self.cron = cron
        self.tz = tz
    }
}

/// `kind: "event"` — fires when an `events` row with this name lands.
public struct EventTrigger: Sendable, Equatable {
    /// The events row name (`issue.created`, `deploy:prod`, …).
    public let name: String

    public init(name: String) {
        self.name = name
    }
}

public enum TriggerWhen: Sendable, Equatable {
    case schedule(ScheduleTrigger)
    case event(EventTrigger)

    // MARK: - Tolerant parse

    /// Parse the stored/synced JSON string. ANY malformation — unknown kind,
    /// a missing cron or name, non-object JSON — reads as nil ("no trigger").
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
            guard let cron = object["cron"] as? String, !cron.isEmpty else { return nil }
            let tz = (object["tz"] as? String).flatMap { $0.isEmpty ? nil : $0 }
            return .schedule(ScheduleTrigger(cron: cron, tz: tz))
        case "event":
            guard let name = object["name"] as? String, !name.isEmpty else { return nil }
            return .event(EventTrigger(name: name))
        default:
            return nil
        }
    }
}

// Encodable so an agent's `triggers` element (`AgentTriggerInput`) embeds the
// when-part's keys beside its id and enabled flag.
extension TriggerWhen: Encodable {
    private enum WireKeys: String, CodingKey {
        case kind, cron, tz, name
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: WireKeys.self)
        switch self {
        case let .schedule(s):
            try c.encode("schedule", forKey: .kind)
            try c.encode(s.cron, forKey: .cron)
            try c.encodeIfPresent(s.tz, forKey: .tz)
        case let .event(e):
            try c.encode("event", forKey: .kind)
            try c.encode(e.name, forKey: .name)
        }
    }
}
