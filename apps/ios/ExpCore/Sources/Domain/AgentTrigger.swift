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
//
// #EXP-1846: a whole-array write must never ERASE what this build can't
// read (a newer kind, e.g. the app agents' `record`/`webhook` of #EXP-1848):
// `unreadableList` keeps those objects raw, every write appends them back
// unchanged (`AgentTriggerInput(unreadable:)`), and a readable trigger keeps
// its unknown extra keys (`extra`, e.g. `prompt`/`run`) under the edited ones.

/// One stored trigger object as-is (a key → its JSON value).
public typealias AgentTriggerObject = [String: JSONWireValue]

/// One stored trigger of an agent.
public struct AgentTrigger: Identifiable, Sendable, Equatable {
    public let id: String
    /// A paused trigger keeps its config.
    public let enabled: Bool
    public let when: TriggerWhen
    /// #EXP-1846: the Check in preset this schedule is (an
    /// `AgentSettings.assistantSchedules` id); nil = a listed trigger.
    /// Every whole-array write carries it back, or the tag is lost.
    public let preset: String?
    /// The stored keys this build doesn't model, sent back on every write.
    public let extra: AgentTriggerObject

    public init(
        id: String,
        enabled: Bool = true,
        when: TriggerWhen,
        preset: String? = nil,
        extra: AgentTriggerObject = [:]
    ) {
        self.id = id
        self.enabled = enabled
        self.when = when
        self.preset = preset
        self.extra = extra
    }

    /// The keys an `AgentTriggerInput` writes itself: never kept in `extra`.
    static let modelledKeys: Set<String> = ["id", "enabled", "preset", "kind", "cron", "tz", "name"]

    /// The Triggers row's text (`AgentSettings.triggerSummary`).
    public var summary: String { AgentSettings.triggerSummary(self) }

    // MARK: - Tolerant parse

    /// The readable triggers of the stored/synced JSON array string, in
    /// order. Anything else — nil, malformed JSON, a non-array — is empty.
    public static func parseList(_ raw: String?) -> [AgentTrigger] {
        elements(raw).compactMap { entry, object in
            guard var trigger = entry.flatMap(parse(object:)) else { return nil }
            if let object {
                trigger = trigger.with(extra: object.filter { !modelledKeys.contains($0.key) })
            }
            return trigger
        }
    }

    /// The stored trigger OBJECTS this build cannot read (`parse` = nil), in
    /// order, raw: every whole-array write appends them back unchanged.
    public static func unreadableList(_ raw: String?) -> [AgentTriggerObject] {
        elements(raw).compactMap { entry, object in
            guard let entry, let object, parse(object: entry) == nil else { return nil }
            return object
        }
    }

    /// Each array element as a dictionary and as its raw JSON object (nil
    /// when not an object). Anything but an array = none.
    private static func elements(_ raw: String?) -> [(entry: [String: Any]?, object: AgentTriggerObject?)] {
        guard let raw, !raw.isEmpty,
              let data = raw.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data),
              let entries = json as? [Any]
        else { return [] }
        let values = (try? JSONDecoder().decode([JSONWireValue].self, from: data)) ?? []
        return entries.enumerated().map { index, entry in
            var object: AgentTriggerObject?
            if values.count == entries.count, case let .object(value) = values[index] { object = value }
            return (entry as? [String: Any], object)
        }
    }

    func with(extra: AgentTriggerObject) -> AgentTrigger {
        AgentTrigger(id: id, enabled: enabled, when: when, preset: preset, extra: extra)
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
        let preset = (object["preset"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        return AgentTrigger(id: id, enabled: enabled, when: when, preset: preset)
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
    /// #EXP-1846: the Check in tag, sent back on every write.
    public var preset: String?
    /// The stored keys this build doesn't model, written under the others.
    public var extra: AgentTriggerObject
    /// A stored object this build can't read, sent back AS-IS (`when` and
    /// the rest unused). Never a row.
    public private(set) var unreadable: AgentTriggerObject?

    public init(
        id: String? = nil,
        enabled: Bool = true,
        when: TriggerWhen,
        preset: String? = nil,
        extra: AgentTriggerObject = [:]
    ) {
        self.id = id
        self.enabled = enabled
        self.when = when
        self.preset = preset
        self.extra = extra
        unreadable = nil
    }

    /// A stored trigger as it is sent back unchanged (preset + extra keys).
    public init(_ trigger: AgentTrigger) {
        self.init(
            id: trigger.id, enabled: trigger.enabled, when: trigger.when,
            preset: trigger.preset, extra: trigger.extra
        )
    }

    /// An unreadable stored object (`AgentTrigger.unreadableList`), sent back
    /// byte for byte.
    public init(unreadable object: AgentTriggerObject) {
        self.init(when: .event(EventTrigger(name: "")))
        id = nil
        unreadable = object
    }
}

extension AgentTriggerInput: Encodable {
    private enum Keys: String, CodingKey {
        case id, enabled, preset
    }

    private struct AnyKey: CodingKey {
        let stringValue: String
        var intValue: Int? { nil }
        init(stringValue: String) { self.stringValue = stringValue }
        init?(intValue: Int) { nil }
    }

    public func encode(to encoder: Encoder) throws {
        if let unreadable {
            try unreadable.encode(to: encoder)
            return
        }
        // The extra keys first; the modelled ones are never among them.
        var rest = encoder.container(keyedBy: AnyKey.self)
        for (key, value) in extra where !AgentTrigger.modelledKeys.contains(key) {
            try rest.encode(value, forKey: AnyKey(stringValue: key))
        }
        var c = encoder.container(keyedBy: Keys.self)
        try c.encodeIfPresent(id, forKey: .id)
        try c.encode(enabled, forKey: .enabled)
        try c.encodeIfPresent(preset, forKey: .preset)
        // The when-part's keys land in the SAME object.
        try when.encode(to: encoder)
    }
}

public extension Array where Element == AgentTrigger {
    /// The whole-array write of these triggers, then `unreadable` unchanged.
    func inputs(keeping unreadable: [AgentTriggerObject] = []) -> [AgentTriggerInput] {
        map(AgentTriggerInput.init) + unreadable.map(AgentTriggerInput.init(unreadable:))
    }

    /// The whole-array write that ADDS `input` (sent without an id).
    func adding(_ input: AgentTriggerInput, keeping unreadable: [AgentTriggerObject] = []) -> [AgentTriggerInput] {
        var added = input
        added.id = nil
        return map(AgentTriggerInput.init) + [added] + unreadable.map(AgentTriggerInput.init(unreadable:))
    }

    /// The whole-array write that swaps the element `id` for `input` (its id
    /// kept), every other trigger untouched and in place.
    func replacing(
        id: String,
        with input: AgentTriggerInput,
        keeping unreadable: [AgentTriggerObject] = []
    ) -> [AgentTriggerInput] {
        map { trigger in
            guard trigger.id == id else { return AgentTriggerInput(trigger) }
            var replaced = input
            replaced.id = id
            // The editor never names a preset or the extra keys: the stored ones stay.
            if replaced.preset == nil { replaced.preset = trigger.preset }
            replaced.extra = trigger.extra.merging(input.extra) { _, new in new }
            return replaced
        } + unreadable.map(AgentTriggerInput.init(unreadable:))
    }

    /// The whole-array write that flips ONE trigger's enabled flag.
    func settingEnabled(id: String, _ enabled: Bool, keeping unreadable: [AgentTriggerObject] = []) -> [AgentTriggerInput] {
        map { trigger in
            var input = AgentTriggerInput(trigger)
            if trigger.id == id { input.enabled = enabled }
            return input
        } + unreadable.map(AgentTriggerInput.init(unreadable:))
    }

    /// The whole-array write that DELETES the element `id`.
    func removing(id: String, keeping unreadable: [AgentTriggerObject] = []) -> [AgentTriggerInput] {
        filter { $0.id != id }.inputs(keeping: unreadable)
    }
}

public extension TriggerWhen {
    /// `kind: "schedule"` (else an event trigger).
    var isSchedule: Bool {
        if case .schedule = self { return true }
        return false
    }
}
