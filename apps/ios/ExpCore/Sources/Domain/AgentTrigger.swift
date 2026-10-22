import Foundation

// NEW-2: a team AGENT definition carries its triggers (`agents.triggers`
// jsonb, contract `agentTrigger*`): each element = a stable id + the enabled
// flag beside its WHEN-part (`TriggerWhen`: cron schedule or event name).
// The SERVER fires them; nothing on a device or a phone ever does.
//
// Reads are TOLERANT: an element without an id or with an unreadable
// when-part is skipped, never a throw. Writes are a WHOLE-ARRAY replace
// (`agents.create({triggers})`, `agents.update({triggers})` after) —
// `AgentTriggerInput` is one element of that array.

/// One stored trigger of an agent.
public struct AgentTrigger: Identifiable, Sendable, Equatable {
    public let id: String
    /// A paused trigger keeps its config.
    public let enabled: Bool
    public let when: TriggerWhen

    public init(id: String, enabled: Bool = true, when: TriggerWhen) {
        self.id = id
        self.enabled = enabled
        self.when = when
    }

    /// The Triggers row's text (`AgentSettings.triggerSummary`).
    public var summary: String { AgentSettings.triggerSummary(self) }

    // MARK: - Tolerant parse

    /// The readable triggers of the stored/synced JSON array string, in
    /// order. Anything else — nil, malformed JSON, a non-array — is empty.
    public static func parseList(_ raw: String?) -> [AgentTrigger] {
        guard let raw, !raw.isEmpty,
              let data = raw.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data),
              let entries = json as? [Any]
        else { return [] }
        return entries.compactMap { entry in
            (entry as? [String: Any]).flatMap(parse(object:))
        }
    }

    /// ONE stored trigger. nil without a string id or with an unreadable
    /// when-part.
    public static func parse(object: [String: Any]) -> AgentTrigger? {
        guard let when = TriggerWhen.parse(object: object),
              let id = object["id"] as? String, !id.isEmpty
        else { return nil }
        // Only an explicit `false` pauses: a missing flag is an enabled
        // trigger. JSONSerialization hands numbers and booleans back as
        // NSNumber alike, and a `0` is not a pause.
        var enabled = true
        if let number = object["enabled"] as? NSNumber,
           CFGetTypeID(number) == CFBooleanGetTypeID() {
            enabled = number.boolValue
        }
        return AgentTrigger(id: id, enabled: enabled, when: when)
    }
}

// MARK: - Wire encoding

/// One element of an agent's `triggers` write. The server replaces the WHOLE
/// array: to add, send the existing triggers plus one WITHOUT `id` (the
/// server mints it); to edit or toggle, send the array with that element
/// changed (its `id` kept); to delete, send the array without it.
public struct AgentTriggerInput: Sendable, Equatable {
    /// nil = a new trigger.
    public var id: String?
    public var enabled: Bool
    public var when: TriggerWhen

    public init(id: String? = nil, enabled: Bool = true, when: TriggerWhen) {
        self.id = id
        self.enabled = enabled
        self.when = when
    }

    /// A stored trigger as it is sent back unchanged.
    public init(_ trigger: AgentTrigger) {
        self.init(id: trigger.id, enabled: trigger.enabled, when: trigger.when)
    }
}

extension AgentTriggerInput: Encodable {
    private enum Keys: String, CodingKey {
        case id, enabled
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        try c.encodeIfPresent(id, forKey: .id)
        try c.encode(enabled, forKey: .enabled)
        // The when-part's keys land in the SAME object.
        try when.encode(to: encoder)
    }
}

public extension Array where Element == AgentTrigger {
    /// The whole-array write that ADDS `input` (sent without an id).
    func adding(_ input: AgentTriggerInput) -> [AgentTriggerInput] {
        var added = input
        added.id = nil
        return map(AgentTriggerInput.init) + [added]
    }

    /// The whole-array write that swaps the element `id` for `input` (its id
    /// kept), every other trigger untouched and in place.
    func replacing(id: String, with input: AgentTriggerInput) -> [AgentTriggerInput] {
        map { trigger in
            guard trigger.id == id else { return AgentTriggerInput(trigger) }
            var replaced = input
            replaced.id = id
            return replaced
        }
    }

    /// The whole-array write that flips ONE trigger's enabled flag.
    func settingEnabled(id: String, _ enabled: Bool) -> [AgentTriggerInput] {
        map { trigger in
            var input = AgentTriggerInput(trigger)
            if trigger.id == id { input.enabled = enabled }
            return input
        }
    }

    /// The whole-array write that DELETES the element `id`.
    func removing(id: String) -> [AgentTriggerInput] {
        filter { $0.id != id }.map(AgentTriggerInput.init)
    }
}

public extension TriggerWhen {
    /// `kind: "schedule"` (else an event trigger).
    var isSchedule: Bool {
        if case .schedule = self { return true }
        return false
    }
}
